//! Classes that come and go on a live node.
//!
//! A class added with `Div::class_when` follows a signal. The stylesheet's
//! rules for it are applied as layers over what the node is without them (the
//! snapshot the state passes also keep), so taking the class away puts the
//! node back by dropping the layer and applying what is left on the snapshot.
//!
//! The rules re-evaluated are those of the node and of its descendants, so
//! `.switch:checked > .thumb` follows the switch. A rule that depends on the
//! class from a sibling selector, or through `:not`, waits for the next full
//! style pass. A node whose rules give it a `transition` eases to what the
//! change gives it, as it does for `:hover`, and a change that gives it a
//! different `animation` starts that animation.

use std::collections::HashSet;
use std::sync::Arc;

use crate::css_parser::{ComplexSelector, SelectorPart};
use crate::element_style::ElementStyle;
use crate::tree::LayoutNodeId;

use super::super::RenderTree;

/// Where a rule sits in the cascade: its tier, then its specificity, then its
/// place in the stylesheet.
type Order = (bool, (u32, u32, u32), usize);

/// The rules a class brought, each with where it sits in the cascade.
type Rules = Vec<(Order, ElementStyle)>;

/// The classes of one node that follow a signal.
#[derive(Default)]
pub(crate) struct DynamicClasses {
    /// The ones the node has now.
    wanted: HashSet<Arc<str>>,
    /// The ones whose rules are applied, in the order they were, with the
    /// rules each added.
    applied: Vec<(Arc<str>, Rules)>,
    /// Rules this node gained because an ancestor gained a class, by the
    /// ancestor and the class.
    inherited: Vec<((LayoutNodeId, Arc<str>), Rules)>,
}

impl DynamicClasses {
    /// Every rule applied for a class, and those an ancestor's class brought,
    /// in cascade order whichever class came first.
    pub(crate) fn layers(&self) -> impl Iterator<Item = &ElementStyle> {
        let mut all: Vec<&(Order, ElementStyle)> = self
            .applied
            .iter()
            .flat_map(|(_, styles)| styles.iter())
            .chain(self.inherited.iter().flat_map(|(_, styles)| styles.iter()))
            .collect();
        all.sort_by_key(|(order, _)| *order);
        all.into_iter().map(|(_, style)| style)
    }
}

impl RenderTree {
    /// Take up the classes that follow a signal on the nodes of this tree:
    /// have each node match what its sources hold now. Run at the end of the
    /// passes that apply base styles, which is when the nodes are new or have
    /// been rebuilt.
    ///
    /// `within` limits it to the nodes a subtree pass styled: a node outside it keeps what it
    /// has, so a change it is easing in the same update is not applied twice, unseen.
    pub(crate) fn sync_dynamic_classes(&mut self, within: Option<&HashSet<LayoutNodeId>>) {
        let _tree_scope = self.layout_tree.enter_scope();
        let toggles = crate::binding::with_registry(|registry| registry.class_toggles());
        for (node, infos) in toggles {
            if !self.render_nodes.contains_key(&node)
                || within.is_some_and(|set| !set.contains(&node))
            {
                continue;
            }
            let entry = self.dynamic_classes.entry(node).or_default();
            for info in infos {
                if info.current() {
                    entry.wanted.insert(info.class);
                } else {
                    entry.wanted.remove(&info.class);
                }
            }
            self.reconcile_dynamic_classes(node, false);
        }
    }

    /// A class gained or lost: have the node match it, in place.
    pub(crate) fn set_dynamic_class(&mut self, node: LayoutNodeId, class: Arc<str>, on: bool) {
        let entry = self.dynamic_classes.entry(node).or_default();
        if on {
            entry.wanted.insert(class);
        } else {
            entry.wanted.remove(&class);
        }
        self.reconcile_dynamic_classes(node, true);
    }

    /// `ease` is whether this is a change a viewer sees (a signal moved), as
    /// against the state a node is built in.
    fn reconcile_dynamic_classes(&mut self, node: LayoutNodeId, ease: bool) {
        // With no stylesheet there are no rules to apply, but the classes and
        // attributes are still the element's, for queries and selectors.
        let stylesheet = self.stylesheet.clone().unwrap_or_default();
        let Some(entry) = self.dynamic_classes.get(&node) else {
            return;
        };
        let applied: HashSet<Arc<str>> = entry.applied.iter().map(|(c, _)| Arc::clone(c)).collect();
        let to_remove: Vec<Arc<str>> = applied.difference(&entry.wanted).cloned().collect();
        let to_add: Vec<Arc<str>> = entry.wanted.difference(&applied).cloned().collect();

        for class in to_remove {
            self.remove_class_everywhere(node, &class, &stylesheet, ease);
        }
        for class in to_add {
            self.add_class_everywhere(node, class, &stylesheet, ease);
        }
    }

    /// The node's descendants, nearest first.
    fn descendants_of(&self, node: LayoutNodeId) -> Vec<LayoutNodeId> {
        let mut out = Vec::new();
        let mut queue = std::collections::VecDeque::from(self.layout_tree.children(node));
        while let Some(next) = queue.pop_front() {
            out.push(next);
            queue.extend(self.layout_tree.children(next));
        }
        out
    }

    /// The `transition` the rules matching a node now give it, if any.
    fn matching_transition(
        &self,
        node: LayoutNodeId,
        stylesheet: &crate::css_parser::Stylesheet,
    ) -> Option<crate::css_parser::CssTransitionSet> {
        let rules = stylesheet.complex_rules();
        self.matching_base_rules(node, stylesheet)
            .into_iter()
            .filter(|i| rules[*i].1.transition.is_some())
            .max_by_key(|i| (Self::rule_order(&rules[*i].0, &rules[*i].1), *i))
            .and_then(|i| rules[i].1.transition.clone())
    }

    /// The `animation` the stateless rules matching a node now give it, if
    /// any: the latest in the cascade among those that set one.
    fn matching_animation(
        &self,
        node: LayoutNodeId,
        stylesheet: &crate::css_parser::Stylesheet,
    ) -> Option<crate::css_parser::CssAnimation> {
        let none: HashSet<LayoutNodeId> = HashSet::new();
        let rules = stylesheet.complex_rules();
        rules
            .iter()
            .enumerate()
            .filter(|(_, (selector, style))| style.animation.is_some() && !selector.has_state())
            .filter(|(_, (selector, _))| {
                self.complex_selector_matches(selector, node, &none, &none, None)
            })
            .max_by_key(|(i, (selector, style))| (Self::rule_order(selector, style), *i))
            .and_then(|(_, (_, style))| style.animation.clone())
    }

    /// Start the animations the stateless rules give nodes that play none,
    /// whatever the rule keys on: an attribute, a class in a compound, a
    /// descendant.
    pub(crate) fn start_rule_animations(&mut self) {
        let Some(stylesheet) = self.stylesheet.clone() else {
            return;
        };
        if !stylesheet
            .complex_rules()
            .iter()
            .any(|(selector, style)| style.animation.is_some() && !selector.has_state())
        {
            return;
        }
        let nodes: Vec<LayoutNodeId> = self.render_nodes.keys().copied().collect();
        for node in nodes {
            let Some(stable) = self.stable_id(node) else {
                continue;
            };
            let Some(animation) = self.matching_animation(node, &stylesheet) else {
                continue;
            };
            // One the node plays already stays, unless a rule now names
            // another (a rebuilt node whose attribute changed meanwhile).
            let playing = self
                .css_anim_store
                .lock()
                .unwrap()
                .animations
                .get(&stable)
                .map(|a| a.name.clone());
            match playing {
                Some(Some(name)) if name != animation.name => {}
                Some(_) => continue,
                None => {}
            }
            self.start_css_animation_from(node, &animation);
        }
    }

    /// Apply a bound write to a node's layout style, easing what it changes
    /// by the `transition` the node's rules give it, as a change of rule is.
    pub(crate) fn eased_layout_write(
        &mut self,
        node: LayoutNodeId,
        write: crate::stateful::TaffyStyleWrite,
    ) {
        let apply = |tree: &mut Self| {
            if let Some(mut style) = tree.layout_tree.get_style(node) {
                write(&mut style);
                tree.layout_tree.set_style(node, style);
            }
        };
        let spec = self
            .stylesheet
            .clone()
            .and_then(|sheet| self.matching_transition(node, &sheet));
        let Some(spec) = spec else {
            apply(self);
            return;
        };
        let before = self.snapshot_before_keyframe_properties(node);
        apply(self);
        if let (Some(before), Some(after)) = (before, self.snapshot_keyframe_properties(node)) {
            self.detect_and_start_transitions(node, &before, &after, &spec);
        }
    }

    /// Run `change` on a node, and start the transitions its `transition`
    /// asks for between what it was and what it is, and the animation the
    /// change gives it.
    fn eased(
        &mut self,
        node: LayoutNodeId,
        stylesheet: &crate::css_parser::Stylesheet,
        ease: bool,
        change: impl FnOnce(&mut Self),
    ) {
        let spec = ease
            .then(|| self.matching_transition(node, stylesheet))
            .flatten();
        let before = spec
            .as_ref()
            .and_then(|_| self.snapshot_before_keyframe_properties(node));
        let animation_before = ease
            .then(|| self.matching_animation(node, stylesheet))
            .flatten();
        change(self);
        if let (Some(before), Some(spec)) = (before, spec)
            && let Some(after) = self.snapshot_keyframe_properties(node)
        {
            self.detect_and_start_transitions(node, &before, &after, &spec);
        }
        if ease {
            let animation_after = self.matching_animation(node, stylesheet);
            let name =
                |a: &Option<crate::css_parser::CssAnimation>| a.as_ref().map(|a| a.name.clone());
            if name(&animation_after) != name(&animation_before) {
                match animation_after {
                    Some(animation) => {
                        self.start_css_animation_from(node, &animation);
                    }
                    None => self.stop_css_animation(node),
                }
            }
        }
    }

    fn add_class_everywhere(
        &mut self,
        node: LayoutNodeId,
        class: Arc<str>,
        stylesheet: &crate::css_parser::Stylesheet,
        ease: bool,
    ) {
        // Only a rule that names the class in a compound before its subject can change what a
        // descendant matches; with none, the descendants are left alone.
        let reach = rules_naming(stylesheet, &class, Position::Ancestor);
        let descendants = if reach.is_empty() {
            Vec::new()
        } else {
            self.descendants_of(node)
        };
        let before: Vec<Vec<usize>> = descendants
            .iter()
            .map(|d| self.matching_rules_among(*d, &reach))
            .collect();

        let own = node;
        let class_for_node = Arc::clone(&class);
        self.eased(own, stylesheet, ease, |tree| {
            tree.add_class_layer(own, class_for_node, stylesheet)
        });

        // The classes registered on the node have changed, so the rules its
        // descendants match may have.
        let rules = stylesheet.complex_rules();
        for (descendant, before) in descendants.into_iter().zip(before) {
            let after = self.matching_rules_among(descendant, &reach);
            let added: Vec<usize> = after
                .iter()
                .copied()
                .filter(|i| !before.contains(i))
                .collect();
            if added.is_empty() {
                continue;
            }
            let all_before = self.matched_before(descendant, stylesheet, &before, &after, &added);
            let styles = Self::layer_rules(&all_before, added, rules);
            self.eased(descendant, stylesheet, ease, |tree| {
                tree.keep_snapshots(descendant);
                tree.dynamic_classes
                    .entry(descendant)
                    .or_default()
                    .inherited
                    .push(((node, Arc::clone(&class)), styles));
                tree.refold_node(descendant);
            });
        }
    }

    fn remove_class_everywhere(
        &mut self,
        node: LayoutNodeId,
        class: &Arc<str>,
        stylesheet: &crate::css_parser::Stylesheet,
        ease: bool,
    ) {
        let key = (node, Arc::clone(class));
        let descendants: Vec<LayoutNodeId> = self
            .descendants_of(node)
            .into_iter()
            .filter(|d| {
                self.dynamic_classes
                    .get(d)
                    .is_some_and(|e| e.inherited.iter().any(|(k, _)| *k == key))
            })
            .collect();

        let own = node;
        let class_for_node = Arc::clone(class);
        self.eased(own, stylesheet, ease, |tree| {
            tree.remove_class_layer(own, &class_for_node)
        });
        for descendant in descendants {
            self.eased(descendant, stylesheet, ease, |tree| {
                if let Some(entry) = tree.dynamic_classes.get_mut(&descendant) {
                    entry.inherited.retain(|(k, _)| *k != key);
                }
                tree.refold_node(descendant);
            });
        }
    }

    /// What the node is without any class's rules, kept so it can be put back.
    fn keep_snapshots(&mut self, node: LayoutNodeId) {
        if !self.base_styles.contains_key(&node)
            && let Some(render_node) = self.render_nodes.get(&node)
        {
            self.base_styles.insert(node, render_node.props.clone());
        }
        if !self.base_taffy_styles.contains_key(&node)
            && let Some(style) = self.layout_tree.get_style(node)
        {
            self.base_taffy_styles.insert(node, style);
        }
    }

    /// The indices of the base (stateless) rules that match the node now.
    fn matching_base_rules(
        &self,
        node: LayoutNodeId,
        stylesheet: &crate::css_parser::Stylesheet,
    ) -> Vec<usize> {
        let none: HashSet<LayoutNodeId> = HashSet::new();
        stylesheet
            .complex_rules()
            .iter()
            .enumerate()
            .filter(|(_, (selector, _))| !selector.has_state())
            .filter(|(_, (selector, _))| {
                self.complex_selector_matches(selector, node, &none, &none, None)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Which of the `rules` (indices into the stylesheet's rules) match the node now.
    fn matching_rules_among(
        &self,
        node: LayoutNodeId,
        rules: &[(usize, &ComplexSelector)],
    ) -> Vec<usize> {
        let none: HashSet<LayoutNodeId> = HashSet::new();
        rules
            .iter()
            .filter(|(_, selector)| {
                self.complex_selector_matches(selector, node, &none, &none, None)
            })
            .map(|(i, _)| *i)
            .collect()
    }

    /// Every base rule the node matched before a class changed, from what it matches now: the
    /// rules the change could reach were `before` and are `after`, and `added` it gained.
    fn matched_before(
        &self,
        node: LayoutNodeId,
        stylesheet: &crate::css_parser::Stylesheet,
        before: &[usize],
        after: &[usize],
        added: &[usize],
    ) -> Vec<usize> {
        let mut all: Vec<usize> = self
            .matching_base_rules(node, stylesheet)
            .into_iter()
            .filter(|i| !added.contains(i))
            .collect();
        all.extend(before.iter().copied().filter(|i| !after.contains(i)));
        all.sort_unstable();
        all
    }

    /// The layer a class brings: the rules it `added`, and with them every
    /// rule the node matched `before` that sits later in the cascade than the
    /// first of those. The layer goes over the snapshot, which already holds
    /// the earlier rules, so a later one is carried in it to stay on top.
    fn layer_rules(
        before: &[usize],
        added: Vec<usize>,
        rules: &[(crate::css_parser::ComplexSelector, ElementStyle)],
    ) -> Rules {
        let order = |i: usize| {
            let tier = Self::rule_order(&rules[i].0, &rules[i].1);
            (tier.0, tier.1, i)
        };
        let Some(first) = added.iter().map(|i| order(*i)).min() else {
            return Vec::new();
        };
        added
            .into_iter()
            .chain(before.iter().copied().filter(|i| order(*i) > first))
            .map(|i| (order(i), rules[i].1.clone()))
            .collect()
    }

    fn set_registered_classes(&self, node: LayoutNodeId, classes: Vec<Arc<str>>) {
        if classes.is_empty() {
            self.element_registry.clear_classes(node);
        } else {
            self.element_registry.register_classes(node, classes);
        }
    }

    fn add_class_layer(
        &mut self,
        node: LayoutNodeId,
        class: Arc<str>,
        stylesheet: &crate::css_parser::Stylesheet,
    ) {
        // Only a rule that names the class in its subject can start or stop matching the node.
        let reach = rules_naming(stylesheet, &class, Position::Subject);
        let before = self.matching_rules_among(node, &reach);

        let mut classes = self.element_registry.get_classes(node).unwrap_or_default();
        if !classes.contains(&class) {
            classes.push(Arc::clone(&class));
        }
        self.set_registered_classes(node, classes);

        let after = self.matching_rules_among(node, &reach);
        let added: Vec<usize> = after
            .iter()
            .copied()
            .filter(|i| !before.contains(i))
            .collect();
        let styles = if added.is_empty() {
            Vec::new()
        } else {
            let all_before = self.matched_before(node, stylesheet, &before, &after, &added);
            Self::layer_rules(&all_before, added, stylesheet.complex_rules())
        };

        // What the node is without the class's rules, kept so it can be put
        // back. It is kept already when a state rule has applied to the node.
        self.keep_snapshots(node);

        self.dynamic_classes
            .entry(node)
            .or_default()
            .applied
            .push((class, styles));
        // Folded with the node's other layers, so the cascade orders them
        // and not the order the classes arrived in.
        self.refold_node(node);
    }

    fn remove_class_layer(&mut self, node: LayoutNodeId, class: &Arc<str>) {
        let mut classes = self.element_registry.get_classes(node).unwrap_or_default();
        classes.retain(|c| c != class);
        self.set_registered_classes(node, classes);

        if let Some(entry) = self.dynamic_classes.get_mut(&node) {
            entry.applied.retain(|(c, _)| c != class);
        }
        self.refold_node(node);
    }

    /// The node as it is under the snapshot with every layer on it: the
    /// classes' rules, then the state rules that apply.
    pub(super) fn refold_node(&mut self, node: LayoutNodeId) {
        let class_layers: Vec<ElementStyle> = self
            .dynamic_classes
            .get(&node)
            .map(|entry| entry.layers().cloned().collect())
            .unwrap_or_default();
        let state_layers = self.state_layers.get(&node).cloned().unwrap_or_default();

        if let (Some(base), Some(render_node)) = (
            self.base_styles.get(&node),
            self.render_nodes.get_mut(&node),
        ) {
            let mut props = base.clone();
            for style in class_layers.iter().chain(state_layers.iter()) {
                Self::apply_element_style_to_props(&mut props, style);
            }
            render_node.props = props;
        }
        if let Some(base) = self.base_taffy_styles.get(&node) {
            let mut style = base.clone();
            for layer in class_layers
                .iter()
                .chain(state_layers.iter())
                .filter(|l| l.has_layout_props())
            {
                Self::apply_element_style_to_taffy(&mut style, layer);
            }
            self.layout_tree.set_style(node, style);
        }
    }

    /// Forget what is applied for a node, as the passes do when they rebuild it.
    pub(crate) fn forget_dynamic_classes(&mut self, node: LayoutNodeId) {
        self.dynamic_classes.remove(&node);
    }

    /// Take off the registry the classes whose rules a node holds as a layer, so a pass that
    /// matches rules against the registry sees the node as its snapshot is. What it returns
    /// goes back with [`Self::restore_classes`].
    pub(super) fn withhold_dynamic_classes(&self) -> Vec<(LayoutNodeId, Vec<Arc<str>>)> {
        let mut withheld = Vec::new();
        for (node, entry) in &self.dynamic_classes {
            // A class that brought no rule changes no match, and may be one the node has anyway.
            let layered: Vec<&Arc<str>> = entry
                .applied
                .iter()
                .filter(|(_, rules)| !rules.is_empty())
                .map(|(class, _)| class)
                .collect();
            let Some(all) = self.element_registry.get_classes(*node) else {
                continue;
            };
            if layered.is_empty() || !all.iter().any(|c| layered.contains(&c)) {
                continue;
            }
            let kept: Vec<Arc<str>> = all
                .iter()
                .filter(|c| !layered.contains(c))
                .cloned()
                .collect();
            self.set_registered_classes(*node, kept);
            withheld.push((*node, all));
        }
        withheld
    }

    /// Put back what [`Self::withhold_dynamic_classes`] took.
    pub(super) fn restore_classes(&self, withheld: Vec<(LayoutNodeId, Vec<Arc<str>>)>) {
        for (node, classes) in withheld {
            self.set_registered_classes(node, classes);
        }
    }
}

/// Where in a selector a class is named.
#[derive(Clone, Copy, PartialEq)]
enum Position {
    /// In the compound the rule applies to.
    Subject,
    /// In a compound before it, as an ancestor or sibling.
    Ancestor,
}

/// The stateless rules that name `class` at `position`, with their indices. An attribute is
/// held as a `name=value` class, so a rule naming the attribute names it. A rule with
/// `:not`, `:is`, `:has` or a state read from attributes there is counted, since it may
/// depend on it.
fn rules_naming<'a>(
    stylesheet: &'a crate::css_parser::Stylesheet,
    class: &str,
    position: Position,
) -> Vec<(usize, &'a ComplexSelector)> {
    let attribute = class.split('=').next().unwrap_or(class);
    let names = |part: &SelectorPart| match part {
        SelectorPart::Class(c) => c == class,
        SelectorPart::Attribute { name, .. } => name == attribute,
        SelectorPart::Not(_) | SelectorPart::Is(_) | SelectorPart::Has(_) => true,
        // `:checked`, `:disabled` and the like read the element's attributes.
        SelectorPart::State(state) => !state.is_pointer_driven(),
        _ => false,
    };
    stylesheet
        .complex_rules()
        .iter()
        .enumerate()
        .filter(|(_, (selector, _))| !selector.has_state())
        .filter(|(_, (selector, _))| {
            let last = selector.segments.len().saturating_sub(1);
            selector
                .segments
                .iter()
                .enumerate()
                .any(|(i, (compound, _))| {
                    let here = if i == last {
                        Position::Subject
                    } else {
                        Position::Ancestor
                    };
                    here == position && compound.parts.iter().any(names)
                })
        })
        .map(|(i, (selector, _))| (i, selector))
        .collect()
}
