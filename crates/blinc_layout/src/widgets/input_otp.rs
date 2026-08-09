//! One-time-passcode input built from linked single-character
//! [`text_input`](mod@super::text_input) slots.
//!
//! The widget owns value synchronization, focus movement, backspace
//! rewind, and paste distribution. Styling wrappers can reuse the same
//! slot wiring via [`wire_otp_slot`].

use std::sync::Arc;

use blinc_core::State;

use crate::div::{Div, ElementBuilder, ElementTypeId, div};
use crate::element::RenderProps;
use crate::key::InstanceKey;
use crate::stateful::{NoState, stateful_with_key};
use crate::tree::{LayoutNodeId, LayoutTree};
use crate::widgets::text_input::{
    InputType, SharedTextInputData, TextInput, blur_text_input_deferred, focus_text_input_deferred,
    text_input, text_input_data,
};

/// Reactive OTP configuration bound to a [`State<String>`] holding the
/// joined value (e.g. `"12"` while typing a 6-digit code).
#[derive(Clone)]
pub struct InputOtpConfig {
    value: State<String>,
    /// Number of slots. Fixed for the widget's lifetime.
    pub length: usize,
    /// When `true`, non-digit characters are filtered out of both
    /// typed input and pasted text.
    pub numeric_only: bool,
    on_change: Option<crate::widgets::TextCallback>,
    on_complete: Option<crate::widgets::TextCallback>,
}

fn joined_value(slots: &[SharedTextInputData]) -> String {
    slots
        .iter()
        .map(|s| s.lock().map(|d| d.value.clone()).unwrap_or_default())
        .collect()
}

fn filter_char(c: char, numeric_only: bool) -> bool {
    if numeric_only {
        c.is_ascii_digit()
    } else {
        true
    }
}

fn first_empty_slot(slots: &[SharedTextInputData]) -> usize {
    slots
        .iter()
        .position(|slot| slot.lock().map(|d| d.value.is_empty()).unwrap_or(true))
        .unwrap_or(slots.len())
}

/// Re-sync slots from the bound value.
///
/// The bound [`State<String>`] is the source of truth, including while a
/// slot is focused.
#[doc(hidden)]
pub fn sync_slots_from_value(slots: &[SharedTextInputData], value: &str) {
    let mut chars = value.chars();
    for slot in slots {
        let target = chars.next().map(|c| c.to_string()).unwrap_or_default();
        if let Ok(mut d) = slot.lock() {
            if d.value != target {
                d.cursor = target.chars().count();
                d.value = target;
                d.selection_start = None;
            }
        }
    }
}

/// Wire the auto-advance / backspace-rewind / paste-distribution FSM
/// onto a single slot's `text_input` builder.
///
/// Shared by the layout and cn OTP wrappers.
#[doc(hidden)]
pub fn wire_otp_slot(
    field: TextInput,
    index: usize,
    slots: &Arc<Vec<SharedTextInputData>>,
    numeric_only: bool,
    value: &State<String>,
    on_change: Option<crate::widgets::TextCallback>,
    on_complete: Option<crate::widgets::TextCallback>,
) -> TextInput {
    let length = slots.len();
    let mut field = field;

    {
        let slots = slots.clone();
        field = field.on_focus_request(move || {
            if index >= slots.len() {
                return false;
            }
            let current_is_empty = slots[index]
                .lock()
                .map(|d| d.value.is_empty())
                .unwrap_or(true);
            if !current_is_empty {
                return false;
            }
            let first_empty = first_empty_slot(&slots);
            if index > first_empty && first_empty < slots.len() {
                // The click drives this slot's FSM to `Focused` via
                // Stateful's auto POINTER_DOWN handler, which fires
                // AFTER this callback within the same dispatch. Defer the
                // blur so it runs after that, clearing the outline this
                // bounced-past slot would otherwise keep — it's not the
                // tracked focus, so nothing else would ever blur it.
                blur_text_input_deferred(&slots[index]);
                focus_text_input_deferred(&slots[first_empty]);
                return true;
            }
            false
        });
    }

    {
        let slots = slots.clone();
        let value_state = value.clone();
        let on_change = on_change.clone();
        let on_complete = on_complete.clone();
        field = field.on_change(move |text| {
            // `InputType::Integer` admits `-`; OTP numeric mode does not.
            if let Some(c) = text.chars().next() {
                if !filter_char(c, numeric_only) {
                    if let Ok(mut d) = slots[index].lock() {
                        d.value.clear();
                        d.cursor = 0;
                        d.selection_start = None;
                    }
                    return;
                }
            }
            let joined = joined_value(&slots);
            let previous = value_state.get();
            let value_changed = joined != previous;
            if value_changed {
                // Only fire on_complete on the incomplete-to-complete transition.
                let was_complete = previous.chars().count() == length;
                value_state.set(joined.clone());
                sync_slots_from_value(&slots, &joined);
                if let Some(ref cb) = on_change {
                    cb(&joined);
                }
                if !was_complete && joined.chars().count() == length {
                    if let Some(ref cb) = on_complete {
                        cb(&joined);
                    }
                }
            }
            if value_changed && !text.is_empty() {
                let next = first_empty_slot(&slots).min(length - 1);
                focus_text_input_deferred(&slots[next]);
            }
        });
    }

    if index > 0 {
        let slots = slots.clone();
        let value_state = value.clone();
        let on_change = on_change.clone();
        field = field.on_backspace_empty(move || {
            if let Ok(mut prev) = slots[index - 1].lock() {
                prev.value.clear();
                prev.cursor = 0;
                prev.selection_start = None;
            }
            focus_text_input_deferred(&slots[index - 1]);
            let joined = joined_value(&slots);
            if joined != value_state.get() {
                value_state.set(joined.clone());
                sync_slots_from_value(&slots, &joined);
                if let Some(ref cb) = on_change {
                    cb(&joined);
                }
            }
        });
    }

    {
        let slots = slots.clone();
        let value_state = value.clone();
        let on_change = on_change.clone();
        let on_complete = on_complete.clone();
        field = field.on_paste_override(move |clip| {
            let chars: Vec<char> = clip
                .chars()
                .filter(|c| filter_char(*c, numeric_only))
                .take(length - index)
                .collect();
            if chars.is_empty() {
                return true;
            }
            for (offset, c) in chars.iter().enumerate() {
                let slot_idx = index + offset;
                if let Ok(mut d) = slots[slot_idx].lock() {
                    d.value = c.to_string();
                    d.cursor = 1;
                    d.selection_start = None;
                }
            }
            let joined = joined_value(&slots);
            let previous = value_state.get();
            if joined != previous {
                let was_complete = previous.chars().count() == length;
                value_state.set(joined.clone());
                sync_slots_from_value(&slots, &joined);
                if let Some(ref cb) = on_change {
                    cb(&joined);
                }
                if !was_complete && joined.chars().count() == length {
                    if let Some(ref cb) = on_complete {
                        cb(&joined);
                    }
                }
            }
            let next = first_empty_slot(&slots).min(length - 1);
            focus_text_input_deferred(&slots[next]);
            true
        });
    }

    field
}

/// OTP input widget.
pub struct InputOtp {
    inner: Div,
}

/// Lazy builder for [`InputOtp`].
pub struct InputOtpBuilder {
    key: InstanceKey,
    config: InputOtpConfig,
    masked: bool,
    disabled: bool,
    gap: f32,
    css_element_id: Option<String>,
    css_classes: Vec<String>,
    built: std::cell::OnceCell<InputOtp>,
}

impl InputOtpBuilder {
    #[track_caller]
    pub fn new(value: &State<String>, length: usize) -> Self {
        Self {
            key: InstanceKey::new("input_otp"),
            config: InputOtpConfig {
                value: value.clone(),
                length,
                numeric_only: false,
                on_change: None,
                on_complete: None,
            },
            masked: false,
            disabled: false,
            gap: 8.0,
            css_element_id: None,
            css_classes: Vec::new(),
            built: std::cell::OnceCell::new(),
        }
    }

    pub fn numeric_only(mut self, numeric_only: bool) -> Self {
        self.config.numeric_only = numeric_only;
        self
    }

    /// Render each slot's content as `•` instead of the literal
    /// character. Passed straight through to every slot's `text_input`.
    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Gap between slots in logical px. Default `8.0`.
    pub fn gap(mut self, px: f32) -> Self {
        self.gap = px;
        self
    }

    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.css_element_id = Some(id.into());
        self
    }

    pub fn class(mut self, name: impl Into<String>) -> Self {
        self.css_classes.push(name.into());
        self
    }

    pub fn on_change<F>(mut self, handler: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.config.on_change = Some(Arc::new(handler));
        self
    }

    /// Fires once, the instant the value reaches `length` characters
    /// (won't re-fire on further edits unless the value drops below
    /// `length` and refills).
    pub fn on_complete<F>(mut self, handler: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        self.config.on_complete = Some(Arc::new(handler));
        self
    }

    fn get_or_build(&self) -> &InputOtp {
        self.built.get_or_init(|| InputOtp::from_builder(self))
    }
}

impl InputOtp {
    fn from_builder(b: &InputOtpBuilder) -> Self {
        let length = b.config.length;
        let numeric_only = b.config.numeric_only;

        // Preserve prefilled bound values on first paint.
        let initial = b.config.value.get();
        let mut initial_chars = initial.chars();
        let slots: Vec<SharedTextInputData> = (0..length)
            .map(|_| {
                let data = text_input_data();
                if let Ok(mut d) = data.lock() {
                    if let Some(c) = initial_chars.next() {
                        d.value = c.to_string();
                        d.cursor = 1;
                    }
                    d.input_type = if numeric_only {
                        InputType::Integer
                    } else {
                        InputType::Text
                    };
                    d.masked = b.masked;
                    d.disabled = b.disabled;
                    d.constraints.max_length = Some(1);
                }
                data
            })
            .collect();
        let slots = Arc::new(slots);

        // `slots` is captured outside the signal-driven rebuild so
        // cursor/focus state survives each render.
        let group_key = b.key.derive("group");
        let value_for_group = b.config.value.clone();
        let value_dep = b.config.value.clone();
        let masked = b.masked;
        let disabled = b.disabled;
        let gap = b.gap;
        let on_change = b.config.on_change.clone();
        let on_complete = b.config.on_complete.clone();
        let css_element_id = b.css_element_id.clone();
        let css_classes = b.css_classes.clone();

        let row = stateful_with_key::<NoState>(&group_key)
            .deps([value_dep.signal_id()])
            .on_state(move |_ctx| {
                sync_slots_from_value(&slots, &value_for_group.get());

                let mut row = div().flex_row().items_center().gap(gap);

                for i in 0..length {
                    let field = text_input(&slots[i])
                        .input_type(if numeric_only {
                            InputType::Integer
                        } else {
                            InputType::Text
                        })
                        .text_align(blinc_core::TextAlign::Center)
                        .max_length(1)
                        .masked(masked)
                        .disabled(disabled)
                        .class("cn-input-otp-slot");

                    let field = wire_otp_slot(
                        field,
                        i,
                        &slots,
                        numeric_only,
                        &value_for_group,
                        on_change.clone(),
                        on_complete.clone(),
                    );

                    row = row.child(field);
                }

                for class in &css_classes {
                    row = row.class(class.as_str());
                }
                if let Some(ref id) = css_element_id {
                    row = row.id(id.as_str());
                }

                row.class("cn-input-otp")
            });

        Self {
            inner: div().h_fit().w_fit().child(row),
        }
    }
}

impl ElementBuilder for InputOtp {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.inner.element_type_id()
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("input_otp")
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }

    fn element_classes(&self) -> &[Arc<str>] {
        self.inner.element_classes()
    }
}

impl ElementBuilder for InputOtpBuilder {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().children_builders()
    }

    fn element_type_id(&self) -> ElementTypeId {
        self.get_or_build().element_type_id()
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("input_otp")
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.get_or_build().layout_style()
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        self.get_or_build().event_handlers()
    }

    fn element_id(&self) -> Option<&str> {
        self.get_or_build().element_id()
    }

    fn element_classes(&self) -> &[Arc<str>] {
        self.get_or_build().element_classes()
    }
}

/// Create a one-time-passcode input with `length` slots, bound to a
/// reactive [`State<String>`] holding the joined value.
///
/// ```ignore
/// let code = ctx.use_state_keyed("code", || String::new());
/// input_otp(&code, 6).numeric_only(true)
/// ```
#[track_caller]
pub fn input_otp(value: &State<String>, length: usize) -> InputOtpBuilder {
    InputOtpBuilder::new(value, length)
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, atomic::AtomicBool};

    use blinc_core::events::event_types;
    use blinc_core::{ReactiveGraph, State};

    use crate::div::ElementBuilder;
    use crate::event_handler::EventContext;
    use crate::stateful::TextFieldState;
    use crate::tree::LayoutNodeId;
    use crate::widgets::text_input::{
        blur_all_text_inputs, focus_text_input, process_pending_input_focus,
    };

    use super::*;

    /// Focus is process-global, so these take turns.
    ///
    /// Poison is ignored deliberately: `.unwrap()` here turned one
    /// failing assertion into every other test in the module reporting
    /// a lock error instead of its own result, which hides which test
    /// actually broke.
    static OTP_FOCUS_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn test_state(initial: impl Into<String>) -> State<String> {
        let graph = Arc::new(std::sync::Mutex::new(ReactiveGraph::new()));
        let signal = graph.lock().unwrap().create_signal(initial.into());
        State::new(signal, graph, Arc::new(AtomicBool::new(false)))
    }

    fn reset_focus_for_test() {
        blur_all_text_inputs();
        process_pending_input_focus();
    }

    fn stateful_is_focused(slot: &SharedTextInputData) -> bool {
        slot.lock()
            .unwrap()
            .stateful_state
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .state
            .is_focused()
    }

    #[test]
    fn sync_slots_from_value_clears_focused_slot() {
        let slot = text_input_data();
        {
            let mut data = slot.lock().unwrap();
            data.value = "7".to_string();
            data.cursor = 1;
            data.selection_start = Some(0);
            data.constraints.max_length = Some(1);
            data.visual = TextFieldState::Focused;
        }

        sync_slots_from_value(std::slice::from_ref(&slot), "");

        let mut data = slot.lock().unwrap();
        assert_eq!(data.value, "");
        assert_eq!(data.cursor, 0);
        assert_eq!(data.selection_start, None);

        data.insert("8");
        assert_eq!(data.value, "8");
    }

    #[test]
    fn sync_slots_from_value_canonicalizes_sparse_slots() {
        let slots: Vec<_> = (0..4).map(|_| text_input_data()).collect();
        {
            let mut data = slots[3].lock().unwrap();
            data.value = "7".to_string();
            data.cursor = 1;
        }

        let value = joined_value(&slots);
        assert_eq!(value, "7");

        sync_slots_from_value(&slots, &value);

        assert_eq!(slots[0].lock().unwrap().value, "7");
        assert_eq!(slots[3].lock().unwrap().value, "");
    }

    #[test]
    fn focusing_future_empty_slot_redirects_to_first_empty_slot() {
        let _guard = OTP_FOCUS_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        reset_focus_for_test();
        blinc_theme::ThemeState::init_default();

        let value = test_state("");
        let slots = Arc::new(vec![text_input_data(), text_input_data()]);
        let _first = text_input(&slots[0]);
        let second = wire_otp_slot(
            text_input(&slots[1]).max_length(1),
            1,
            &slots,
            false,
            &value,
            None,
            None,
        );
        let ctx = EventContext::new(event_types::POINTER_DOWN, LayoutNodeId::default());

        second.event_handlers().unwrap().dispatch(&ctx);
        process_pending_input_focus();
        process_pending_input_focus();

        assert!(slots[0].lock().unwrap().visual.is_focused());
        assert!(!slots[1].lock().unwrap().visual.is_focused());
    }

    // A redirected click still drove the slot's FSM to `Focused` via
    // Stateful's auto POINTER_DOWN handler (the `:focus` outline is
    // painted from the FSM, not `data.visual`), and nothing blurred it
    // because it was never the tracked focus — so the outline ring
    // stuck. Assert the FSM, not just `visual`.
    #[test]
    fn redirected_click_does_not_leave_slot_fsm_focused() {
        let _guard = OTP_FOCUS_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        reset_focus_for_test();
        blinc_theme::ThemeState::init_default();

        let value = test_state("");
        let slots = Arc::new(vec![
            text_input_data(),
            text_input_data(),
            text_input_data(),
        ]);
        let mut tree = LayoutTree::new();
        let fields: Vec<_> = (0..slots.len())
            .map(|i| {
                wire_otp_slot(
                    text_input(&slots[i]).max_length(1),
                    i,
                    &slots,
                    false,
                    &value,
                    None,
                    None,
                )
            })
            .collect();
        for field in &fields {
            field.build(&mut tree);
        }

        // Click slot 2 while every slot is empty: focus bounces to the
        // first empty slot (0), and slot 2 must not keep a focus ring.
        let ctx = EventContext::new(event_types::POINTER_DOWN, LayoutNodeId::default());
        fields[2].event_handlers().unwrap().dispatch(&ctx);
        process_pending_input_focus();
        process_pending_input_focus();

        assert!(
            !stateful_is_focused(&slots[2]),
            "redirected slot kept its FSM Focused (outline ring persists)"
        );
        assert!(
            stateful_is_focused(&slots[0]),
            "focus should have bounced to the first empty slot"
        );
    }

    #[test]
    fn typing_into_full_slot_does_not_advance_focus() {
        let _guard = OTP_FOCUS_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        reset_focus_for_test();
        blinc_theme::ThemeState::init_default();

        let value = test_state("7");
        let slots = Arc::new(vec![text_input_data(), text_input_data()]);
        {
            let mut data = slots[0].lock().unwrap();
            data.value = "7".to_string();
            data.cursor = 1;
            data.visual = TextFieldState::Focused;
        }

        let field = wire_otp_slot(
            text_input(&slots[0]).max_length(1),
            0,
            &slots,
            false,
            &value,
            None,
            None,
        );
        let ctx =
            EventContext::new(event_types::TEXT_INPUT, LayoutNodeId::default()).with_key_char('8');

        field.event_handlers().unwrap().dispatch(&ctx);

        assert_eq!(slots[0].lock().unwrap().value, "7");
        assert!(!slots[1].lock().unwrap().visual.is_focused());
    }

    #[test]
    fn typing_into_empty_slot_advances_focus() {
        let _guard = OTP_FOCUS_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        reset_focus_for_test();
        blinc_theme::ThemeState::init_default();

        let value = test_state("");
        let slots = Arc::new(vec![text_input_data(), text_input_data()]);
        slots[0].lock().unwrap().visual = TextFieldState::Focused;
        let _next = text_input(&slots[1]);

        let field = wire_otp_slot(
            text_input(&slots[0]).max_length(1),
            0,
            &slots,
            false,
            &value,
            None,
            None,
        );
        let ctx =
            EventContext::new(event_types::TEXT_INPUT, LayoutNodeId::default()).with_key_char('7');

        field.event_handlers().unwrap().dispatch(&ctx);
        process_pending_input_focus();
        process_pending_input_focus();

        assert_eq!(slots[0].lock().unwrap().value, "7");
        assert!(slots[1].lock().unwrap().visual.is_focused());
    }

    #[test]
    fn auto_advance_blurs_previous_slot_stateful_visuals() {
        let _guard = OTP_FOCUS_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        reset_focus_for_test();
        blinc_theme::ThemeState::init_default();

        let value = test_state("");
        let slots = Arc::new(vec![
            text_input_data(),
            text_input_data(),
            text_input_data(),
        ]);
        let mut tree = LayoutTree::new();
        let fields: Vec<_> = (0..slots.len())
            .map(|index| {
                wire_otp_slot(
                    text_input(&slots[index]).max_length(1),
                    index,
                    &slots,
                    true,
                    &value,
                    None,
                    None,
                )
            })
            .collect();
        for field in &fields {
            field.build(&mut tree);
        }

        focus_text_input(&slots[0]);
        let input_one =
            EventContext::new(event_types::TEXT_INPUT, LayoutNodeId::default()).with_key_char('1');
        fields[0].event_handlers().unwrap().dispatch(&input_one);
        process_pending_input_focus();

        let input_two =
            EventContext::new(event_types::TEXT_INPUT, LayoutNodeId::default()).with_key_char('2');
        fields[1].event_handlers().unwrap().dispatch(&input_two);
        process_pending_input_focus();

        assert!(!slots[0].lock().unwrap().visual.is_focused());
        assert!(!slots[1].lock().unwrap().visual.is_focused());
        assert!(slots[2].lock().unwrap().visual.is_focused());
        assert!(!stateful_is_focused(&slots[0]));
        assert!(!stateful_is_focused(&slots[1]));
        assert!(stateful_is_focused(&slots[2]));
    }

    #[test]
    fn broadcast_text_input_does_not_fill_next_slot_with_same_character() {
        let _guard = OTP_FOCUS_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        reset_focus_for_test();
        blinc_theme::ThemeState::init_default();

        let value = test_state("");
        let slots = Arc::new(vec![text_input_data(), text_input_data()]);
        slots[0].lock().unwrap().visual = TextFieldState::Focused;

        let first = wire_otp_slot(
            text_input(&slots[0]).max_length(1),
            0,
            &slots,
            false,
            &value,
            None,
            None,
        );
        let second = wire_otp_slot(
            text_input(&slots[1]).max_length(1),
            1,
            &slots,
            false,
            &value,
            None,
            None,
        );
        let ctx =
            EventContext::new(event_types::TEXT_INPUT, LayoutNodeId::default()).with_key_char('2');

        first.event_handlers().unwrap().dispatch(&ctx);
        second.event_handlers().unwrap().dispatch(&ctx);

        assert_eq!(slots[0].lock().unwrap().value, "2");
        assert_eq!(slots[1].lock().unwrap().value, "");
    }

    #[test]
    fn on_complete_fires_once_on_the_incomplete_to_complete_transition() {
        let _guard = OTP_FOCUS_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        reset_focus_for_test();
        blinc_theme::ThemeState::init_default();

        let value = test_state("");
        let slots = Arc::new(vec![text_input_data(), text_input_data()]);
        slots[0].lock().unwrap().value = "7".to_string();
        slots[0].lock().unwrap().cursor = 1;
        slots[1].lock().unwrap().visual = TextFieldState::Focused;

        let complete_calls = Arc::new(Mutex::new(Vec::new()));
        let spy = Arc::clone(&complete_calls);
        let on_complete: crate::widgets::TextCallback =
            Arc::new(move |v: &str| spy.lock().unwrap().push(v.to_string()));

        let field = wire_otp_slot(
            text_input(&slots[1]).max_length(1),
            1,
            &slots,
            false,
            &value,
            None,
            Some(on_complete),
        );
        let ctx =
            EventContext::new(event_types::TEXT_INPUT, LayoutNodeId::default()).with_key_char('8');

        field.event_handlers().unwrap().dispatch(&ctx);

        assert_eq!(*complete_calls.lock().unwrap(), vec!["78".to_string()]);
    }

    #[test]
    fn on_complete_does_not_refire_when_pasting_over_an_already_complete_code() {
        let _guard = OTP_FOCUS_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        blinc_theme::ThemeState::init_default();

        let value = test_state("12");
        let slots = Arc::new(vec![text_input_data(), text_input_data()]);
        slots[0].lock().unwrap().value = "1".to_string();
        slots[0].lock().unwrap().cursor = 1;
        slots[1].lock().unwrap().value = "2".to_string();
        slots[1].lock().unwrap().cursor = 1;

        let complete_calls = Arc::new(Mutex::new(Vec::new()));
        let spy = Arc::clone(&complete_calls);
        let on_complete: crate::widgets::TextCallback =
            Arc::new(move |v: &str| spy.lock().unwrap().push(v.to_string()));

        let _field = wire_otp_slot(
            text_input(&slots[0]).max_length(1),
            0,
            &slots,
            false,
            &value,
            None,
            Some(on_complete),
        );
        let paste_override = slots[0]
            .lock()
            .unwrap()
            .on_paste_override_callback
            .clone()
            .expect("wire_otp_slot should register a paste override");

        // Replace one complete code with another — never drops below length.
        paste_override("34");

        assert_eq!(joined_value(&slots), "34");
        assert!(complete_calls.lock().unwrap().is_empty());
    }
}
