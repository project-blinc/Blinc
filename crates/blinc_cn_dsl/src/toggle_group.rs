//! `cn.ToggleGroup` — a bar of options, one picked at a time.

use std::cell::OnceCell;
use std::sync::Mutex;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

use crate::toggle::CnToggle;

/// `cn.ToggleGroup(value = signal) { cn.Toggle(label = "…") }` — pick
/// one, as a row of chips rather than dials.
///
/// ```dsl,ignore
/// signal align: string = "left"
///
/// cn.ToggleGroup(value = align, variant = "outline") {
///     cn.Toggle(value = "left", label = "Left")
///     cn.Toggle(value = "center", label = "Center")
///     cn.Toggle(value = "right", label = "Right")
/// }
/// ```
///
/// The children are ordinary `cn.Toggle`s. A toggle already knows how
/// to be a chip; inside a group it gives up only its own `pressed`,
/// because the group owns one selection for the whole set, and takes
/// the group's `variant` and `size` so a bar cannot come out
/// mismatched. Its `value` says which option it is, defaulting to the
/// label.
///
/// `value` binds both ways: picking an option writes the signal, and
/// writing the signal moves the selection. A child that is not a
/// `cn.Toggle` is dropped with a warning: the group draws its own chips
/// and has nowhere to put a loose element.
///
/// Clicking the selected option is a no-op, matching shadcn's
/// `type="single"`. A standalone `cn.Toggle` can turn itself off.
#[extern_widget(namespace = "cn", name = "ToggleGroup")]
pub struct CnToggleGroup {
    /// Which option is picked. Bind a signal to drive it from
    /// elsewhere.
    pub value: Reactive<String>,
    /// `default` (default) or `outline`, which borders the off state.
    pub variant: String,
    /// `small` / `medium` (default) / `large`.
    pub size: String,
    /// Dim every option and take the whole group out of reach.
    pub disabled: bool,
    /// Space between chips, in pixels.
    pub gap: f64,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Names this group when two of them would otherwise look
    /// identical. Only needed for a genuine duplicate: see
    /// [`Self::group_key`].
    pub key: String,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<blinc_cn::ToggleGroupBuilder>,
}

impl CnToggleGroup {
    fn get_or_build(&self) -> &blinc_cn::ToggleGroupBuilder {
        ::blinc_layout::build_once::build_once(&self.shell, || self.make())
    }

    /// What tells this group's per-item state from another group's.
    ///
    /// Not the call site: the id the DSL would key on reads as zero for
    /// a widget with children, because nothing pushes one around a
    /// lowered component call yet. Worse here than in Rust, where the
    /// key comes from `#[track_caller]` — every DSL group shares the
    /// one FFI thunk, so without this they would all collide.
    ///
    /// Two groups identical in every one of these respects share their
    /// items' hover state, and hovering one chip scales both. `key` is
    /// the way out of that.
    fn group_key(&self, options: &[String]) -> String {
        group_key(
            &self.key,
            &crate::bridge::signal_key(&self.value),
            &self.variant,
            &self.size,
            options,
        )
    }

    fn make(&self) -> blinc_cn::ToggleGroupBuilder {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));
        let state = crate::bridge::string_state(&self.value);

        let items: Vec<&CnToggle> = children
            .iter()
            .filter_map(|c| c.as_any().and_then(|a| a.downcast_ref::<CnToggle>()))
            .collect();
        if items.len() != children.len() {
            tracing::warn!(
                "cn.ToggleGroup: a child is not a cn.Toggle — dropped; wrap it in \
                 cn.Toggle(label = \"…\") to give it a chip",
            );
        }

        let options: Vec<String> = items.iter().map(|i| i.option_value()).collect();

        let mut b = blinc_cn::toggle_group(&state)
            .key(self.group_key(&options))
            .variant(self.variant())
            .size(self.size());
        if self.disabled {
            b = b.disabled(true);
        }
        if self.gap > 0.0 {
            b = b.gap(self.gap as f32);
        }
        for item in items {
            b = b.item(item.to_cn_item());
        }
        b
    }

    fn variant(&self) -> blinc_cn::ToggleVariant {
        match self.variant.as_str() {
            "outline" => blinc_cn::ToggleVariant::Outline,
            "" | "default" => blinc_cn::ToggleVariant::Default,
            other => {
                tracing::warn!(variant = %other, "cn.ToggleGroup: unknown variant");
                blinc_cn::ToggleVariant::Default
            }
        }
    }

    fn size(&self) -> blinc_cn::ToggleSize {
        match self.size.as_str() {
            "small" | "sm" => blinc_cn::ToggleSize::Small,
            "large" | "lg" => blinc_cn::ToggleSize::Large,
            "" | "medium" | "md" => blinc_cn::ToggleSize::Medium,
            other => {
                tracing::warn!(size = %other, "cn.ToggleGroup: unknown size");
                blinc_cn::ToggleSize::Medium
            }
        }
    }
}

/// The instance name for a group, given what the author wrote.
///
/// Split out so the collision rules can be tested without building a
/// widget.
fn group_key(
    explicit: &str,
    signal: &str,
    variant: &str,
    size: &str,
    options: &[String],
) -> String {
    if !explicit.is_empty() {
        return format!("cn-toggle-group-{explicit}");
    }
    format!(
        "cn-toggle-group-{signal}-{variant}-{size}-{}",
        options.join(",")
    )
}

impl ElementBuilder for CnToggleGroup {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

    /// The shell's children, which now hold the chips.
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

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }
}

#[cfg(test)]
mod tests {
    use super::group_key;

    fn opts(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    /// Two groups over one signal and one option set, differing only in
    /// size. Sharing a name would make hovering a chip in one scale the
    /// other, which is what happened to the radios.
    #[test]
    fn groups_differing_only_in_size_get_different_names() {
        assert_ne!(
            group_key("", "sig1", "outline", "small", &opts(&["a", "b"])),
            group_key("", "sig1", "outline", "large", &opts(&["a", "b"])),
        );
    }

    #[test]
    fn groups_differing_in_options_get_different_names() {
        assert_ne!(
            group_key("", "sig1", "", "", &opts(&["a", "b"])),
            group_key("", "sig1", "", "", &opts(&["a", "c"])),
        );
    }

    /// An explicit key wins outright, which is the escape hatch for two
    /// groups that really are identical.
    #[test]
    fn an_explicit_key_overrides_everything_else() {
        assert_eq!(
            group_key("mine", "sig1", "outline", "small", &opts(&["a"])),
            group_key("mine", "sig2", "default", "large", &opts(&["b"])),
        );
    }

    /// Identical groups still collide, which is why `key` exists.
    #[test]
    fn identical_groups_share_a_name() {
        assert_eq!(
            group_key("", "sig1", "", "", &opts(&["a"])),
            group_key("", "sig1", "", "", &opts(&["a"])),
        );
    }
}
