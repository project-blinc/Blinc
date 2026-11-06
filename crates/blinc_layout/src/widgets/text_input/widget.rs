//! The `TextInput` element: its builder and how it is built into a tree.

use super::*;

/// The class whose rules in [`crate::ua_styles::UA_STYLES`] give a text input
/// its default look.
pub const DEFAULT_CLASS: &str = "blinc-input";

struct AccessibleNameEffect(blinc_core::reactive::EffectId);

impl Drop for AccessibleNameEffect {
    fn drop(&mut self) {
        blinc_core::reactive::dispose_host_effect(self.0);
    }
}

/// A single-line text field.
///
/// Built once. Its handlers edit the shared [`TextInputData`] and bump its
/// revision; what it shows (the text, the selection, the caret, the scroll,
/// and the fill and border for its state) is bound to that revision, so
/// typing, clicking and dragging patch the field in place.
pub struct TextInput {
    /// The element as configured so far: layout calls and the handlers land
    /// on it as they are made. The content is added when it is first built.
    inner: std::cell::RefCell<Div>,
    built: std::cell::OnceCell<Div>,
    data: SharedTextInputData,
    config: Arc<Mutex<TextInputConfig>>,
    /// Callback invoked when text value changes
    on_change_callback: Option<OnChangeCallback>,
    /// Caller's handle onto this field, if bound.
    element_ref: Option<crate::selector::DivRef>,
    accessibility_label: Option<crate::binding::Reactive<String>>,
}

impl TextInput {
    /// Bind an [`crate::selector::InputRef`] to this field.
    ///
    /// The value half is handed over now — the field is built from
    /// state its caller already holds, so nothing has to wait for a
    /// render. The element half is resolved by the renderer when the
    /// field is built, the same as any other bound ref.
    pub fn bind(mut self, input_ref: &crate::selector::InputRef) -> Self {
        input_ref.bind_data(&self.data);
        self.element_ref = Some(input_ref.element().clone());
        self
    }

    /// Create a text input with externally-managed data state
    pub fn new(data: SharedTextInputData) -> Self {
        let config = Arc::new(Mutex::new(TextInputConfig::default()));
        if let Ok(mut d) = data.lock() {
            d.mounted = true;
        }
        let mut inner = Self::create_inner_with_handlers(Arc::clone(&data), Arc::clone(&config))
            .class(DEFAULT_CLASS);

        // HTML input behaviour in flex layouts: stretch to the parent's
        // width by default, and `min_w(0)` so the field can shrink below its
        // content. The inner clip container does the clipping; clipping here
        // would cut into the rounded border.
        {
            let cfg = config.lock().unwrap();
            if cfg.use_full_width {
                inner = inner.w_full();
            }
            inner = inner.h(cfg.height).min_w(0.0);
        }

        Self {
            inner: std::cell::RefCell::new(inner),
            built: std::cell::OnceCell::new(),
            data,
            config,
            on_change_callback: None,
            element_ref: None,
            accessibility_label: None,
        }
    }

    /// The element, finished on first use: the content and the bound looks
    /// added to what the builder calls made.
    fn element(&self) -> &Div {
        crate::build_once::build_once(&self.built, || {
            let outer = self.inner.take();
            let cfg = self.config.lock().unwrap().clone();
            assemble(outer, &self.data, &cfg)
        })
    }

    /// Name announced by assistive technology, independent of the current value.
    pub fn accessibility_label(mut self, label: impl crate::binding::IntoReactive<String>) -> Self {
        self.accessibility_label = Some(label.into_reactive());
        self
    }

    pub fn w(mut self, px: f32) -> Self {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.width = px;
        }
        self.inner = std::cell::RefCell::new(self.inner.take().w(px));
        self
    }

    pub fn w_full(mut self) -> Self {
        self.config.lock().unwrap().use_full_width = true;
        self.inner = std::cell::RefCell::new(self.inner.take().w_full());
        self
    }

    pub fn min_w(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().min_w(px));
        self
    }

    pub fn h(mut self, px: f32) -> Self {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.height = px;
        }
        self.inner = std::cell::RefCell::new(self.inner.take().h(px));
        self
    }

    /// The text shown while the field is empty: a string, Signal or
    /// Computed. A bound value is followed in place.
    pub fn placeholder(self, text: impl crate::binding::IntoReactive<String>) -> Self {
        match text.into_reactive() {
            crate::binding::Reactive::Const(p) => {
                self.config.lock().unwrap().placeholder = p.clone();
                if let Ok(mut d) = self.data.lock() {
                    d.placeholder = p;
                }
            }
            bound => {
                let data = Arc::clone(&self.data);
                blinc_core::reactive::effect(move |g| {
                    set_placeholder(&data, bound.read(g).unwrap_or_default());
                });
            }
        }
        self
    }

    /// Whether the field shows as invalid whatever its value, as a field
    /// with an error does: a bool, Signal or Computed. The box carries
    /// `data-invalid` while it is.
    pub fn invalid(self, invalid: impl crate::binding::IntoReactive<bool>) -> Self {
        match invalid.into_reactive() {
            crate::binding::Reactive::Const(on) => set_invalid(&self.data, on),
            bound => {
                let data = Arc::clone(&self.data);
                blinc_core::reactive::effect(move |g| {
                    set_invalid(&data, bound.read(g).unwrap_or(false));
                });
            }
        }
        self
    }

    pub fn input_type(self, input_type: InputType) -> Self {
        if let Ok(mut d) = self.data.lock() {
            d.input_type = input_type;
        }
        self
    }

    /// Set horizontal text alignment inside the field. `Left` (default)
    /// renders the value flush against `padding_x`. `Center` centres
    /// the value within the available clip area — canonical for
    /// number / OTP / code inputs where the field is fixed-width.
    pub fn text_align(self, align: blinc_core::TextAlign) -> Self {
        if let Ok(mut cfg) = self.config.lock() {
            cfg.text_align = align;
        }
        self
    }

    /// Set the internal horizontal padding inside the field (left and
    /// right spacers). Default is 12 px each side, which gives a
    /// roomy form-input feel. Number / OTP / code inputs that want a
    /// tight cell hugging the centred value should drop this to
    /// 4–6 px so the visible field hugs the text instead of carrying
    /// 24 px of fixed dead space.
    pub fn padding_x(self, px: f32) -> Self {
        if let Ok(mut cfg) = self.config.lock() {
            cfg.padding_x = px.max(0.0);
        }
        self
    }

    /// Register a stepper hook fired on ↑ / ↓ / `+` / `−` keys while
    /// the field is focused. The argument is `+1` for increment,
    /// `-1` for decrement. When this is set, the keys are
    /// *consumed* — the default behaviour (character insertion for
    /// `+` / `−`, no-op for arrows on a single-line field) is
    /// skipped. Used by `cn::number_input` to wire keyboard
    /// stepping to the bound `State<f64>`.
    pub fn on_step<F>(self, callback: F) -> Self
    where
        F: Fn(i32) + Send + Sync + 'static,
    {
        if let Ok(mut d) = self.data.lock() {
            d.on_step_callback = Some(Arc::new(callback));
        }
        self
    }

    /// Called when Enter is pressed in the field, with whether Shift was
    /// held. Without one, Enter does nothing in a single-line field.
    pub fn on_submit<F>(self, callback: F) -> Self
    where
        F: Fn(bool) + Send + Sync + 'static,
    {
        if let Ok(mut d) = self.data.lock() {
            d.on_submit_callback = Some(Arc::new(callback));
        }
        self
    }

    /// Override Backspace on an empty field.
    pub fn on_backspace_empty<F>(self, callback: F) -> Self
    where
        F: Fn() + Send + Sync + 'static,
    {
        if let Ok(mut d) = self.data.lock() {
            d.on_backspace_empty_callback = Some(Arc::new(callback));
        }
        self
    }

    /// Override Cmd/Ctrl+V paste handling.
    pub fn on_paste_override<F>(self, callback: F) -> Self
    where
        F: Fn(&str) -> bool + Send + Sync + 'static,
    {
        if let Ok(mut d) = self.data.lock() {
            d.on_paste_override_callback = Some(Arc::new(callback));
        }
        self
    }

    /// Override pointer focus handling. Return `true` to consume focus.
    pub fn on_focus_request<F>(self, callback: F) -> Self
    where
        F: Fn() -> bool + Send + Sync + 'static,
    {
        if let Ok(mut d) = self.data.lock() {
            d.on_focus_request_callback = Some(Arc::new(callback));
        }
        self
    }

    /// Whether the field is disabled: a bool, Signal or Computed. A bound
    /// value is followed in place.
    pub fn disabled(self, disabled: impl crate::binding::IntoReactive<bool>) -> Self {
        match disabled.into_reactive() {
            crate::binding::Reactive::Const(on) => set_disabled(&self.data, on),
            bound => {
                let data = Arc::clone(&self.data);
                blinc_core::reactive::effect(move |g| {
                    set_disabled(&data, bound.read(g).unwrap_or(false));
                });
            }
        }
        self
    }

    pub fn masked(self, masked: bool) -> Self {
        if let Ok(mut d) = self.data.lock() {
            d.masked = masked;
        }
        self
    }

    pub fn max_length(self, max: usize) -> Self {
        if let Ok(mut d) = self.data.lock() {
            d.constraints.max_length = Some(max);
        }
        self
    }

    /// Set the font size for the text input (default: 16.0)
    pub fn text_size(self, size: f32) -> Self {
        self.config.lock().unwrap().font_size = size;
        self
    }

    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.lock().unwrap().corner_radius = radius;
        self.inner = std::cell::RefCell::new(self.inner.take().rounded(radius));
        self
    }

    pub fn border(mut self, width: f32, color: blinc_core::Color) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().border(width, color));
        self
    }

    pub fn border_color(mut self, color: blinc_core::Color) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().border_color(color));
        self
    }

    pub fn border_width(mut self, width: f32) -> Self {
        self.config.lock().unwrap().border_width = width;
        self.inner = std::cell::RefCell::new(self.inner.take().border_width(width));
        self
    }

    pub fn shadow_sm(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().shadow_sm());
        self
    }

    pub fn shadow_md(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().shadow_md());
        self
    }

    pub fn flex_grow(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().flex_grow());
        self
    }

    /// Set the CSS element ID for stylesheet matching
    ///
    /// When set, the TextInput will query the active stylesheet for
    /// styles matching `#id`, `#id:hover`, `#id:focus`, `#id:disabled`,
    /// and `#id::placeholder`.
    pub fn id(mut self, id: &str) -> Self {
        if let Ok(mut d) = self.data.lock() {
            d.css_element_id = Some(id.to_string());
        }
        self.inner = std::cell::RefCell::new(self.inner.take().id(id));
        self
    }

    /// Add a CSS class name for selector matching
    pub fn class(mut self, name: &str) -> Self {
        if let Ok(mut d) = self.data.lock() {
            d.css_classes.push(blinc_core::intern::intern(name));
        }
        self.inner = std::cell::RefCell::new(self.inner.take().class(name));
        self
    }

    /// Give the field `name` as a class while `condition` holds: a bool,
    /// Signal or Computed, followed in place.
    pub fn class_when(
        mut self,
        name: &str,
        condition: impl crate::binding::IntoReactive<bool>,
    ) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().class_when(name, condition));
        self
    }

    // ========== Box Colours ==========
    //
    // How the box looks while hovered, focused, invalid or disabled is the
    // `.blinc-input` rules (`:hover`, `:focus`, `[data-invalid]`, `:disabled`);
    // restyle it with rules of your own. These two set the idle values the way
    // inline style does, so a rule of the component layer does not replace them.

    /// Set the idle border color.
    pub fn idle_border_color(mut self, color: Color) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().border_color(color));
        self
    }

    /// Set the idle background color.
    pub fn idle_bg_color(mut self, color: Color) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().bg(color));
        self
    }

    // ========== Text Color Configuration ==========

    /// Set the text color
    pub fn text_color(self, color: Color) -> Self {
        self.config.lock().unwrap().text_color = color;
        self
    }

    /// Set the placeholder text color
    pub fn placeholder_color(self, color: Color) -> Self {
        self.config.lock().unwrap().placeholder_color = color;
        self
    }

    /// Set the cursor color
    pub fn cursor_color(self, color: Color) -> Self {
        self.config.lock().unwrap().cursor_color = color;
        self
    }

    /// Set the selection highlight color
    pub fn selection_color(self, color: Color) -> Self {
        self.config.lock().unwrap().selection_color = color;
        self
    }

    /// Set the callback to be invoked when the text value changes
    ///
    /// The callback receives the new text value as a string slice.
    /// This is called after insert or delete operations modify the text.
    ///
    /// # Example
    ///
    /// ```ignore
    /// text_input(&data)
    ///     .on_change(|new_value| {
    ///         println!("Text changed to: {}", new_value);
    ///     })
    /// ```
    pub fn on_change<F>(mut self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        let cb: OnChangeCallback = Arc::new(callback);
        self.on_change_callback = Some(Arc::clone(&cb));
        // Store in TextInputData so it can be accessed in event handlers
        if let Ok(mut d) = self.data.lock() {
            d.on_change_callback = Some(cb);
        }
        self
    }
}

/// Create a text input widget
/// By default, uses the config's default width (200px).
/// Use .w_full() to fill parent width, or .w() to set explicit width.
pub fn text_input(data: &SharedTextInputData) -> TextInput {
    // TextInput::new() sets default width from config (200px)
    TextInput::new(Arc::clone(data))
}

impl ElementBuilder for TextInput {
    fn bound_element_ref(&self) -> Option<&crate::selector::DivRef> {
        self.element_ref.as_ref()
    }

    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        let node = self.element().build(tree);
        if let Ok(mut d) = self.data.lock() {
            d.node_id = Some(node);
        }
        node
    }

    fn render_props(&self) -> RenderProps {
        self.element().render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.element().children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        crate::div::ElementTypeId::Div
    }

    fn ime_binding(&self) -> Option<crate::ime::ImeBinding> {
        Some(super::ime::binding(&self.data))
    }

    fn accessibility_binding(&self) -> Option<crate::accessibility::AccessibilityBinding> {
        use crate::accessibility::{AccessibilityAction, AccessibilityBinding, AccessibilityValue};
        let data = self.data.clone();
        let label = self.accessibility_label.clone();
        let name_effect = label.as_ref().and_then(|source| {
            if matches!(source, crate::binding::Reactive::Const(_)) {
                return None;
            }
            let source = source.clone();
            let effect = blinc_core::reactive::effect(move |graph| {
                match &source {
                    crate::binding::Reactive::Bound(value) => {
                        graph.get(value.signal());
                    }
                    crate::binding::Reactive::Computed(value) => {
                        value.try_get();
                    }
                    crate::binding::Reactive::Const(_) => {}
                }
                crate::request_redraw();
            });
            Some(Arc::new(AccessibleNameEffect(effect.id())))
        });
        let edit_data = self.data.clone();
        let input = crate::selector::InputRef::new();
        input.bind_data(&self.data);
        Some(AccessibilityBinding {
            read: Arc::new(move || {
                let _keep_alive = &name_effect;
                let label = label.as_ref().and_then(|v| v.now());
                let d = data.lock().unwrap();
                AccessibilityValue {
                    label,
                    value: d.display_text(),
                    placeholder: d.placeholder.clone(),
                    secure: d.masked,
                    focused: d.visual.is_focused(),
                    selection: {
                        let offset = |index| {
                            if d.masked {
                                index
                            } else {
                                d.value.chars().take(index).map(char::len_utf16).sum()
                            }
                        };
                        let anchor = d.selection_start.unwrap_or(d.cursor);
                        Some([offset(anchor.min(d.cursor)), offset(anchor.max(d.cursor))])
                    },
                }
            }),
            action: Arc::new(move |action| match action {
                AccessibilityAction::Focus | AccessibilityAction::Activate => {
                    input.focus();
                    true
                }
                AccessibilityAction::SetValue(_) | AccessibilityAction::ReplaceText { .. } => {
                    let (callback, value) = {
                        let mut d = edit_data.lock().unwrap();
                        if d.disabled {
                            return false;
                        }
                        match action {
                            AccessibilityAction::SetValue(value) => {
                                d.select_all();
                                d.insert(value);
                            }
                            AccessibilityAction::ReplaceText { start, end, text } => {
                                let exposed = d.display_text();
                                let index = |offset| {
                                    let mut units = 0;
                                    for (i, c) in exposed.chars().enumerate() {
                                        if units == offset {
                                            return Some(i);
                                        }
                                        units += c.len_utf16();
                                    }
                                    (units == offset).then_some(exposed.chars().count())
                                };
                                let (Some(start), Some(end)) = (index(*start), index(*end)) else {
                                    return false;
                                };
                                if start > end {
                                    return false;
                                }
                                d.cursor = end;
                                d.selection_start = Some(start);
                                d.insert(text);
                            }
                            _ => unreachable!(),
                        }
                        (d.on_change_callback.clone(), d.value.clone())
                    };
                    if let Some(callback) = callback {
                        callback(&value);
                    }
                    super::focus::notify_text_input(&edit_data);
                    true
                }
                _ => false,
            }),
        })
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("input")
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(self.element())
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        ElementBuilder::layout_style(self.element())
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.element().element_classes()
    }

    fn element_id(&self) -> Option<&str> {
        ElementBuilder::element_id(self.element())
    }

    fn layout_bounds_storage(&self) -> Option<crate::renderer::LayoutBoundsStorage> {
        // Updated after layout, so the scroll can be worked out against the
        // field's real width.
        if let Ok(data) = self.data.lock() {
            Some(Arc::clone(&data.layout_bounds_storage))
        } else {
            None
        }
    }

    fn layout_bounds_callback(&self) -> Option<crate::renderer::LayoutBoundsCallback> {
        // A new width can scroll the text differently.
        let data = Arc::clone(&self.data);
        Some(Arc::new(move |_bounds| notify_text_input(&data)))
    }
}

/// Disable or enable a field and have what it shows follow.
fn set_placeholder(data: &SharedTextInputData, text: String) {
    let changed = match data.lock() {
        Ok(mut d) if d.placeholder != text => {
            d.placeholder = text;
            true
        }
        _ => false,
    };
    if changed {
        super::focus::notify_text_input(data);
    }
}

fn set_invalid(data: &SharedTextInputData, on: bool) {
    let changed = match data.lock() {
        Ok(mut d) if d.invalid != on => {
            d.invalid = on;
            true
        }
        _ => false,
    };
    if changed {
        super::focus::notify_text_input(data);
    }
}

fn set_disabled(data: &SharedTextInputData, on: bool) {
    let changed = match data.lock() {
        Ok(mut d) if d.disabled != on => {
            d.disabled = on;
            if on {
                if d.visual.is_focused() {
                    decrement_focus_count();
                }
                d.visual = TextFieldState::Disabled;
            } else if d.visual == TextFieldState::Disabled {
                d.visual = TextFieldState::Idle;
            }
            true
        }
        _ => false,
    };
    if changed {
        super::focus::notify_text_input(data);
    }
}
