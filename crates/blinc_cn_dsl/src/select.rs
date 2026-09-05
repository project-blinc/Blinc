//! `cn.Select` — pick one from a list that drops down.

use std::cell::OnceCell;
use std::sync::Mutex;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

use crate::option::CnOption;

/// `cn.Select(value = signal) { cn.Option(label = "…") }` — pick one,
/// from a list that stays out of the way until asked for.
///
/// ```dsl,ignore
/// signal plan: string = "free"
///
/// cn.Select(value = plan, label = "Plan", placeholder = "Pick one") {
///     cn.Option(value = "free", label = "Free")
///     cn.Option(value = "pro", label = "Pro")
///     cn.Option(value = "team", label = "Team", disabled = true)
/// }
/// ```
///
/// `value` binds both ways: picking writes the signal, and writing the
/// signal moves the selection. Each choice names itself, so the list
/// reads its children rather than being told about them separately. A
/// child that is not a `cn.Option` is dropped with a warning: the
/// widget draws its own rows and has nowhere to put a loose element.
///
/// Prefer `cn.RadioGroup` when the choices are few and worth showing at
/// once; a select trades visibility for space.
#[extern_widget(namespace = "cn", name = "Select")]
pub struct CnSelect {
    /// Which choice is picked. Bind a signal to drive it from
    /// elsewhere.
    pub value: Reactive<String>,
    /// Shown on the trigger while nothing is picked.
    pub placeholder: String,
    /// Text above the trigger.
    pub label: String,
    /// `small` / `medium` (default) / `large`.
    pub size: String,
    /// Dim the trigger and take the whole list out of reach.
    pub disabled: bool,
    /// Trigger width in pixels. Unset lets it size to its content.
    pub w: f64,
    #[children]
    pub children: Mutex<Vec<Box<dyn ElementBuilder>>>,
    /// Tells this select's state from another's when two would
    /// otherwise look identical. See `instance_key`.
    pub key: String,
    /// Built once, consuming `children`.
    #[skip]
    shell: OnceCell<blinc_cn::SelectBuilder>,
}

impl CnSelect {
    fn get_or_build(&self) -> &blinc_cn::SelectBuilder {
        ::blinc_layout::build_once::build_once(&self.shell, || self.make())
    }

    /// What tells this select's open/highlight state from another's.
    ///
    /// Not the call site: the id the DSL would key on reads as zero for
    /// a widget with children, because nothing pushes one around a
    /// lowered component call yet. So the select is named by what the
    /// author wrote, which is stable across rebuilds and differs
    /// wherever two selects differ.
    fn instance_key(&self, options: &[String]) -> String {
        instance_key(
            &self.key,
            &crate::bridge::signal_key(&self.value),
            &self.label,
            &self.placeholder,
            options,
        )
    }

    fn make(&self) -> blinc_cn::SelectBuilder {
        let children = std::mem::take(&mut *self.children.lock().expect("children mutex"));
        let state = crate::bridge::string_state(&self.value);

        let options: Vec<&CnOption> = children
            .iter()
            .filter_map(|c| c.as_any().and_then(|a| a.downcast_ref::<CnOption>()))
            .collect();
        if options.len() != children.len() {
            tracing::warn!(
                "cn.Select: a child is not a cn.Option — dropped; wrap it in \
                 cn.Option(label = \"…\") to give it a row",
            );
        }

        let values: Vec<String> = options.iter().map(|o| o.option_value()).collect();

        let mut b =
            blinc_cn::SelectBuilder::with_key(self.instance_key(&values), &state).size(self.size());
        if !self.placeholder.is_empty() {
            b = b.placeholder(self.placeholder.clone());
        }
        if !self.label.is_empty() {
            b = b.label(self.label.clone());
        }
        if self.disabled {
            b = b.disabled(true);
        }
        if self.w > 0.0 {
            b = b.w(self.w as f32);
        }
        for option in options {
            let (value, label) = (option.option_value(), option.option_label());
            b = if option.disabled {
                b.option_disabled(value, label)
            } else {
                b.option(value, label)
            };
        }
        b
    }

    fn size(&self) -> blinc_cn::SelectSize {
        match self.size.as_str() {
            "small" | "sm" => blinc_cn::SelectSize::Small,
            "large" | "lg" => blinc_cn::SelectSize::Large,
            "" | "medium" | "md" => blinc_cn::SelectSize::Medium,
            other => {
                tracing::warn!(size = %other, "cn.Select: unknown size");
                blinc_cn::SelectSize::Medium
            }
        }
    }
}

/// The instance name for a select, given what the author wrote.
///
/// Split out so the collision rules can be tested without building a
/// widget.
fn instance_key(
    explicit: &str,
    signal: &str,
    label: &str,
    placeholder: &str,
    options: &[String],
) -> String {
    if !explicit.is_empty() {
        return format!("cn-select-{explicit}");
    }
    format!(
        "cn-select-{signal}-{label}-{placeholder}-{}",
        options.join(",")
    )
}

impl ElementBuilder for CnSelect {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> blinc_layout::RenderProps {
        self.get_or_build().render_props()
    }

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
    use super::instance_key;

    fn opts(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    /// Two selects over one signal, differing only in their label. A
    /// shared name would have one's open list track the other's.
    #[test]
    fn selects_differing_only_in_label_get_different_names() {
        assert_ne!(
            instance_key("", "sig1", "Plan", "", &opts(&["a"])),
            instance_key("", "sig1", "Tier", "", &opts(&["a"])),
        );
    }

    #[test]
    fn selects_differing_in_options_get_different_names() {
        assert_ne!(
            instance_key("", "sig1", "", "", &opts(&["a", "b"])),
            instance_key("", "sig1", "", "", &opts(&["a", "c"])),
        );
    }

    /// An explicit key wins outright, for two selects that really are
    /// identical.
    #[test]
    fn an_explicit_key_overrides_everything_else() {
        assert_eq!(
            instance_key("mine", "sig1", "Plan", "pick", &opts(&["a"])),
            instance_key("mine", "sig2", "Tier", "choose", &opts(&["b"])),
        );
    }
}
