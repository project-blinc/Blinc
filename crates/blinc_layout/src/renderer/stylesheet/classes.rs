//! Classes that come and go on a live node.
//!
//! A class added with `Div::class_when` follows a signal. The stylesheet's
//! rules for it are applied as layers over what the node is without them (the
//! snapshot the state passes also keep), so taking the class away puts the
//! node back by dropping the layer and applying what is left on the snapshot.
//!
//! Only the rules whose subject is the node itself are re-evaluated. A rule
//! about another element that depends on this class (a descendant or sibling
//! selector) waits for the next full style pass.

use std::collections::HashSet;
use std::sync::Arc;

use crate::element_style::ElementStyle;
use crate::tree::LayoutNodeId;

use super::super::RenderTree;

/// The classes of one node that follow a signal.
#[derive(Default)]
pub(crate) struct DynamicClasses {
    /// The ones the node has now.
    wanted: HashSet<Arc<str>>,
    /// The ones whose rules are applied, in the order they were, with the
    /// rules each added.
    applied: Vec<(Arc<str>, Vec<ElementStyle>)>,
}

impl DynamicClasses {
    /// Every rule applied for a class, in order.
    pub(crate) fn layers(&self) -> impl Iterator<Item = &ElementStyle> {
        self.applied.iter().flat_map(|(_, styles)| styles.iter())
    }
}

impl RenderTree {
    /// Take up the classes that follow a signal on the nodes of this tree:
    /// have each node match what its sources hold now. Run at the end of the
    /// passes that apply base styles, which is when the nodes are new or have
    /// been rebuilt.
    pub(crate) fn sync_dynamic_classes(&mut self) {
        let toggles = crate::binding::with_registry(|registry| registry.class_toggles());
        for (node, infos) in toggles {
            if !self.render_nodes.contains_key(&node) {
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
            self.reconcile_dynamic_classes(node);
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
        self.reconcile_dynamic_classes(node);
    }

    fn reconcile_dynamic_classes(&mut self, node: LayoutNodeId) {
        let Some(stylesheet) = self.stylesheet.clone() else {
            return;
        };
        let Some(entry) = self.dynamic_classes.get(&node) else {
            return;
        };
        let applied: HashSet<Arc<str>> = entry.applied.iter().map(|(c, _)| Arc::clone(c)).collect();
        let to_remove: Vec<Arc<str>> = applied.difference(&entry.wanted).cloned().collect();
        let to_add: Vec<Arc<str>> = entry.wanted.difference(&applied).cloned().collect();

        for class in &to_remove {
            self.remove_class_layer(node, class);
        }
        for class in to_add {
            self.add_class_layer(node, class, &stylesheet);
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
        let before = self.matching_base_rules(node, stylesheet);

        let mut classes = self.element_registry.get_classes(node).unwrap_or_default();
        if !classes.contains(&class) {
            classes.push(Arc::clone(&class));
        }
        self.set_registered_classes(node, classes);

        let after = self.matching_base_rules(node, stylesheet);
        let mut added: Vec<usize> = after.into_iter().filter(|i| !before.contains(i)).collect();
        // Lowest specificity first, so the more specific rule is applied last.
        let rules = stylesheet.complex_rules();
        added.sort_by_key(|i| (Self::selector_specificity(&rules[*i].0), *i));
        let styles: Vec<ElementStyle> = added.into_iter().map(|i| rules[i].1.clone()).collect();

        // What the node is without the class's rules, kept so it can be put
        // back. It is kept already when a state rule has applied to the node.
        if !self.base_styles.contains_key(&node) {
            if let Some(render_node) = self.render_nodes.get(&node) {
                self.base_styles.insert(node, render_node.props.clone());
            }
        }
        if !self.base_taffy_styles.contains_key(&node) {
            if let Some(style) = self.layout_tree.get_style(node) {
                self.base_taffy_styles.insert(node, style);
            }
        }

        self.apply_layers(node, styles.iter());
        self.dynamic_classes
            .entry(node)
            .or_default()
            .applied
            .push((class, styles));
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
    fn refold_node(&mut self, node: LayoutNodeId) {
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

    /// Apply layers to the node as it is.
    fn apply_layers<'a>(
        &mut self,
        node: LayoutNodeId,
        layers: impl Iterator<Item = &'a ElementStyle>,
    ) {
        let layers: Vec<&ElementStyle> = layers.collect();
        if let Some(render_node) = self.render_nodes.get_mut(&node) {
            for style in &layers {
                Self::apply_element_style_to_props(&mut render_node.props, style);
            }
        }
        if layers.iter().any(|l| l.has_layout_props()) {
            if let Some(mut style) = self.layout_tree.get_style(node) {
                for layer in layers.iter().filter(|l| l.has_layout_props()) {
                    Self::apply_element_style_to_taffy(&mut style, layer);
                }
                self.layout_tree.set_style(node, style);
            }
        }
    }

    /// Forget what is applied for a node, as the passes do when they rebuild it.
    pub(crate) fn forget_dynamic_classes(&mut self, node: LayoutNodeId) {
        self.dynamic_classes.remove(&node);
    }
}
