//! `cn.InputOTP` — a one-time code, one slot per character.

use std::cell::OnceCell;

use blinc_dsl_core::{Reactive, extern_widget};
use blinc_layout::div::ElementBuilder;

/// `cn.InputOTP(value = signal, length = 6.0)` — six boxes, one code.
///
/// ```dsl,ignore
/// signal code: string = ""
///
/// cn.InputOTP(value = code, length = 6.0, numeric_only = true,
///             on_complete = || submit())
/// ```
///
/// `value` binds both ways: typing fills the signal one character per
/// slot, and writing the signal fills the slots. Focus walks forward on
/// each keystroke and back on backspace; a click lands on the first
/// empty slot rather than wherever the pointer happened to be.
#[extern_widget(namespace = "cn", name = "InputOTP")]
pub struct CnInputOTP {
    /// The code as typed so far. Bind a signal to read or prefill it.
    pub value: Reactive<String>,
    /// How many slots. Omitted reads as six.
    pub length: f64,
    /// Digits only.
    pub numeric_only: bool,
    /// Show `•` instead of the character.
    pub masked: bool,
    /// Non-interactive, dimmed.
    pub disabled: bool,
    /// Error styling on every slot.
    pub error: bool,
    /// `small` / `medium` (default) / `large`.
    pub size: String,
    /// Insert a `•` separator every N slots. Omitted keeps the cn
    /// default of three.
    pub group_every: f64,
    /// Fired once when the last slot is filled. Zero when omitted.
    pub on_complete: i64,
    /// Names this widget when two of them would otherwise look
    /// identical. Only needed for a genuine duplicate: see
    /// [`Self::otp_key`].
    pub key: String,
    /// Lazy-constructed cn widget. Same caching rationale as
    /// `CnButton::built`.
    #[skip]
    built: OnceCell<blinc_cn::InputOtpBuilder>,
}

impl CnInputOTP {
    fn get_or_build(&self) -> &blinc_cn::InputOtpBuilder {
        ::blinc_layout::build_once::build_once(&self.built, || self.to_cn_widget())
    }

    /// What tells this widget's group state from another's.
    ///
    /// Named by what the author wrote rather than the call site — the
    /// id a DSL widget would key on is unreliable, and every instance
    /// is built from the one line below regardless. Two OTPs alike in
    /// every respect share state; `key` is the way out.
    fn otp_key(&self) -> String {
        if !self.key.is_empty() {
            return format!("cn-input-otp-{}", self.key);
        }
        format!(
            "cn-input-otp-{}-{}-{}",
            crate::bridge::signal_key(&self.value),
            self.length(),
            self.size,
        )
    }

    fn length(&self) -> usize {
        if self.length >= 1.0 {
            self.length as usize
        } else {
            6
        }
    }

    fn to_cn_widget(&self) -> blinc_cn::InputOtpBuilder {
        let state = crate::bridge::string_state(&self.value);
        let mut b = blinc_cn::input_otp(&state, self.length())
            .key(self.otp_key())
            .size(self.size());

        if self.numeric_only {
            b = b.numeric_only(true);
        }
        if self.masked {
            b = b.masked(true);
        }
        if self.disabled {
            b = b.disabled(true);
        }
        if self.error {
            b = b.error(true);
        }
        if self.group_every >= 1.0 {
            b = b.group_every(self.group_every as usize);
        }
        if self.on_complete != 0 {
            // Zyntax mints a zero-arg `extern "C" fn()` for the DSL
            // closure and hands it across as `i64` — same shape as
            // `cn.Button`'s `on_click`. The code itself travels through
            // the bound signal, so the closure takes no arguments.
            let complete_ptr = self.on_complete;
            b = b.on_complete(move |_code| {
                type ClosureFn = extern "C" fn();
                // SAFETY: minted by the DSL lowering for exactly this
                // dispatch; non-zero only when the author wrote one.
                let func: ClosureFn = unsafe { std::mem::transmute(complete_ptr) };
                func();
            });
        }
        b
    }

    fn size(&self) -> blinc_cn::InputSize {
        match self.size.as_str() {
            "small" | "sm" => blinc_cn::InputSize::Small,
            "large" | "lg" => blinc_cn::InputSize::Large,
            _ => blinc_cn::InputSize::Medium,
        }
    }
}

impl ElementBuilder for CnInputOTP {
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
}
