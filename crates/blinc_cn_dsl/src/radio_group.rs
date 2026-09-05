//! `cn.RadioGroup` — a set of options, one picked at a time.

use std::cell::OnceCell;
use std::sync::Mutex;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

use crate::radio::CnRadio;

/// `cn.RadioGroup(value = signal) { cn.Radio(label = "…") }` — pick one.
///
/// ```dsl,ignore
/// signal plan: string = "free"
///
/// cn.RadioGroup(value = plan, label = "Plan", layout = "horizontal") {
///     cn.Radio(value = "free", label = "Free")
///     cn.Radio(value = "pro", label = "Pro")
///     cn.Radio(value = "team", label = "Team", disabled = true)
/// }
/// ```
///
/// `value` binds both ways: picking an option writes the signal, and
/// writing the signal moves the dial. Each option names itself, so the
/// group reads its children rather than being told about them
/// separately. A child that is not a `cn.Radio` is dropped with a
/// warning: the group draws its own dials and has nowhere to put a
/// loose element.
#[extern_widget(namespace = "cn", name = "RadioGroup")]
pub struct CnRadioGroup {
    /// Which option is picked. Bind a signal to drive it from
    /// elsewhere.
    pub value: Reactive<String>,
    /// Text above the options.
    pub label: String,
    /// `small` / `medium` (default) / `large`.
    pub size: String,
    /// `vertical` (default) or `horizontal`.
    pub layout: String,
    /// Dim every option and take the whole group out of reach.
    pub disabled: bool,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Names this group when two of them would otherwise look
    /// identical. Only needed for a genuine duplicate: see
    /// `group_key`.
    pub key: String,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<blinc_cn::RadioGroup>,
}

impl CnRadioGroup {
    fn get_or_build(&self) -> &blinc_cn::RadioGroup {
        ::blinc_layout::build_once::build_once(&self.shell, || self.make())
    }

    /// What tells this group's per-option state from another group's.
    ///
    /// Not the call site: the id the DSL would key on reads as zero for
    /// a widget with children, because nothing pushes one around a
    /// lowered component call yet. So the group is named by what the
    /// author wrote instead, which is stable across rebuilds and
    /// differs wherever two groups differ.
    ///
    /// Two groups identical in every one of these respects share their
    /// options' hover state, and hovering one dial scales both. `key`
    /// is the way out of that.
    fn group_key(&self, options: &[String]) -> String {
        group_key(
            &self.key,
            &crate::bridge::signal_key(&self.value),
            &self.label,
            &self.size,
            &self.layout,
            options,
        )
    }

    fn make(&self) -> blinc_cn::RadioGroup {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));
        let state = crate::bridge::string_state(&self.value);

        let options: Vec<String> = children
            .iter()
            .filter_map(|c| c.as_any().and_then(|a| a.downcast_ref::<CnRadio>()))
            .map(|o| o.option_value())
            .collect();

        let mut b = blinc_cn::radio_group(&state)
            .key(self.group_key(&options))
            .size(self.size())
            .layout(self.layout());
        if !self.label.is_empty() {
            b = b.label(self.label.clone());
        }
        if self.disabled {
            b = b.disabled(true);
        }

        for child in children {
            let Some(option) = child.as_any().and_then(|any| any.downcast_ref::<CnRadio>()) else {
                tracing::warn!(
                    "cn.RadioGroup: child is not a cn.Radio — dropped; wrap it in \
                     cn.Radio(label = \"…\") to give it a dial",
                );
                continue;
            };
            let (value, label) = (option.option_value(), option.option_label());
            b = if option.disabled {
                b.option_disabled(value, label)
            } else {
                b.option(value, label)
            };
        }

        b.build_component()
    }

    fn size(&self) -> blinc_cn::RadioSize {
        match self.size.as_str() {
            "small" | "sm" => blinc_cn::RadioSize::Small,
            "large" | "lg" => blinc_cn::RadioSize::Large,
            _ => blinc_cn::RadioSize::Medium,
        }
    }

    fn layout(&self) -> blinc_cn::RadioLayout {
        match self.layout.as_str() {
            "horizontal" | "row" => blinc_cn::RadioLayout::Horizontal,
            "" | "vertical" | "column" => blinc_cn::RadioLayout::Vertical,
            other => {
                tracing::warn!(layout = %other, "cn.RadioGroup: unknown layout");
                blinc_cn::RadioLayout::Vertical
            }
        }
    }
}

impl ElementBuilder for CnRadioGroup {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    /// The shell's children, which now hold the dials.
    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    // MUST forward — see `gotcha_element_builder_trait_forwarding`.
    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.get_or_build().event_handlers()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.get_or_build().element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }

    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.get_or_build().element_type_id()
    }
}

/// See [`CnRadioGroup::group_key`]. Free-standing so the naming can be
/// checked without building a widget.
fn group_key(
    explicit: &str,
    signal: &str,
    label: &str,
    size: &str,
    layout: &str,
    options: &[String],
) -> String {
    if !explicit.is_empty() {
        return format!("cn-radio-group-{explicit}");
    }
    format!(
        "cn-radio-group-{signal}-{label}-{size}-{layout}-{}",
        options.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::group_key;

    fn opts(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    /// The two groups from the playground: one signal, one option set,
    /// different size and label. They must not land on one name.
    ///
    /// They did, twice. First keyed by option value alone, then by a
    /// call-site id that reads as zero for a widget with children — so
    /// hovering an option in one group drove the other's.
    #[test]
    fn groups_sharing_a_signal_are_still_told_apart() {
        let a = group_key("", "sig7", "Plan", "", "", &opts(&["free", "pro"]));
        let b = group_key(
            "",
            "sig7",
            "",
            "small",
            "horizontal",
            &opts(&["free", "pro"]),
        );
        assert_ne!(a, b);
    }

    /// Stable across rebuilds: the same group named the same way twice
    /// keeps its options' state.
    #[test]
    fn the_same_group_keeps_its_name() {
        let args = || group_key("", "sig7", "Plan", "", "", &opts(&["free", "pro"]));
        assert_eq!(args(), args());
    }

    /// Groups alike in every respect share their options' hover state,
    /// which is what `key` is for.
    #[test]
    fn an_explicit_key_separates_identical_groups() {
        let same = opts(&["free", "pro"]);
        let a = group_key("", "sig7", "Plan", "", "", &same);
        let b = group_key("", "sig7", "Plan", "", "", &same);
        assert_eq!(a, b, "identical groups do collide by default");

        let named = group_key("left", "sig7", "Plan", "", "", &same);
        assert_ne!(named, a, "and `key` is the way out");
    }
}
