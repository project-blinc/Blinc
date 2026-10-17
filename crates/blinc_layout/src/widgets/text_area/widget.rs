//! The `TextArea` element: its builder and how it is built into a tree.

use super::*;

/// Ready-to-use text area element
///
/// Built once: what it shows is bound to its state's revision, so typing,
/// focus, hover, selection and scrolling patch it in place.
///
/// Usage: `text_area(&state).rows(4).w(400.0).rounded(12.0)`
pub struct TextArea {
    /// The element as configured so far: layout calls and the handlers land
    /// on it as they are made. The content is added when it is first built.
    inner: std::cell::RefCell<Div>,
    built: std::cell::OnceCell<Div>,
    /// Text area state (content, cursor, etc.)
    state: SharedTextAreaState,
    /// Text area configuration
    config: Arc<Mutex<TextAreaConfig>>,
    /// Caller's handle onto this field, if bound.
    element_ref: Option<crate::selector::DivRef>,
}

impl TextArea {
    /// Bind a [`crate::selector::TextareaRef`] to this field.
    ///
    /// The state half is handed over now — the field is built from
    /// state its caller already holds — and the element half is
    /// resolved by the renderer when the field is built.
    pub fn bind(mut self, textarea_ref: &crate::selector::TextareaRef) -> Self {
        textarea_ref.bind_state(&self.state);
        self.element_ref = Some(textarea_ref.element().clone());
        self
    }

    /// Create a new text area with shared state
    pub fn new(state: &SharedTextAreaState) -> Self {
        let config = Arc::new(Mutex::new(TextAreaConfig::default()));
        let (default_width, default_height) = {
            let cfg = config.lock().unwrap();
            (cfg.effective_width(), cfg.effective_height())
        };
        // A field with a revision is one a deferred focus can land on.
        if let Ok(mut d) = state.lock() {
            d.revision
                .get_or_insert_with(|| blinc_core::reactive::signal(0u64));
            d.config = Some(Arc::clone(&config));
        }

        // HTML-like flex behaviour: `min_w(0)` lets the field shrink below
        // its content in a flex container; the height is always explicit.
        // The inner scroll clips, so this does not: clipping here would cut
        // into the rounded border.
        let inner = Self::create_inner_with_handlers(Arc::clone(state), Arc::clone(&config))
            .w(default_width)
            .h(default_height)
            .min_w(0.0);

        let textarea = Self {
            inner: std::cell::RefCell::new(inner),
            built: std::cell::OnceCell::new(),
            state: Arc::clone(state),
            config,
            element_ref: None,
        };
        textarea.update_scroll_dimensions();
        textarea
    }

    /// The element, finished on first use: the content and the bound looks
    /// added to what the builder calls made.
    fn element(&self) -> &Div {
        crate::build_once::build_once(&self.built, || {
            let outer = self.inner.take();
            let cfg = self.config.lock().unwrap().clone();
            assemble(outer, &self.state, &cfg)
        })
    }

    /// Set placeholder text
    pub fn placeholder(mut self, text: impl Into<String>) -> Self {
        let placeholder = text.into();
        self.config.lock().unwrap().placeholder = placeholder.clone();
        if let Ok(mut s) = self.state.lock() {
            s.placeholder = placeholder;
        }
        self
    }

    /// Update cached scroll dimensions from config
    /// This must be called whenever config values that affect scroll calculation change
    fn update_scroll_dimensions(&self) {
        let cfg = self.config.lock().unwrap();
        let line_height = cfg.font_size * cfg.line_height;
        let viewport_height = cfg.effective_height() - cfg.padding_y * 2.0 - cfg.border_width * 2.0;
        let viewport_width = cfg.effective_width() - cfg.padding_x * 2.0 - cfg.border_width * 2.0;
        drop(cfg);

        if let Ok(mut s) = self.state.lock() {
            s.line_height = line_height;
            s.viewport_height = viewport_height;
            // Update scroll physics viewport dimensions
            if let Ok(mut physics) = s.scroll_physics.lock() {
                physics.viewport_height = viewport_height;
                physics.viewport_width = viewport_width;
            }
        }
    }

    /// Set number of visible rows (like HTML textarea rows attribute)
    pub fn rows(mut self, rows: usize) -> Self {
        let height = {
            let mut cfg = self.config.lock().unwrap();
            cfg.rows = Some(rows);
            cfg.effective_height()
        };
        self.inner = std::cell::RefCell::new(self.inner.take().h(height));
        self.update_scroll_dimensions();
        self
    }

    /// Set number of visible columns (like HTML textarea cols attribute)
    pub fn cols(mut self, cols: usize) -> Self {
        let width = {
            let mut cfg = self.config.lock().unwrap();
            cfg.cols = Some(cols);
            cfg.width_is_explicit = true;
            cfg.effective_width()
        };
        self.inner = std::cell::RefCell::new(self.inner.take().w(width));
        self
    }

    /// Set both rows and cols
    pub fn text_size(mut self, rows: usize, cols: usize) -> Self {
        let (width, height) = {
            let mut cfg = self.config.lock().unwrap();
            cfg.rows = Some(rows);
            cfg.cols = Some(cols);
            cfg.width_is_explicit = true;
            (cfg.effective_width(), cfg.effective_height())
        };
        self.inner = std::cell::RefCell::new(self.inner.take().w(width).h(height));
        self.update_scroll_dimensions();
        self
    }

    /// Set font size
    pub fn font_size(mut self, size: f32) -> Self {
        self.config.lock().unwrap().font_size = size;
        self.update_scroll_dimensions();
        self
    }

    /// Set disabled state
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.config.lock().unwrap().disabled = disabled;
        if let Ok(mut s) = self.state.lock() {
            s.disabled = disabled;
            if disabled {
                s.visual = TextFieldState::Disabled;
            }
        }
        self
    }

    /// Set maximum length
    pub fn max_length(mut self, max: usize) -> Self {
        self.config.lock().unwrap().max_length = max;
        self
    }

    /// Enable or disable text wrapping
    ///
    /// When wrapping is enabled (default), long lines wrap to the next visual line.
    /// When disabled, text scrolls horizontally instead.
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.config.lock().unwrap().wrap = wrap;
        self
    }

    /// Disable text wrapping (alias for `.wrap(false)`)
    pub fn no_wrap(self) -> Self {
        self.wrap(false)
    }

    // =========================================================================
    // Builder methods that return Self (shadow Div methods for fluent API)
    // =========================================================================

    pub fn w(mut self, px: f32) -> Self {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.width = px;
            cfg.cols = None;
            cfg.width_is_explicit = true;
        }
        self.inner = std::cell::RefCell::new(self.inner.take().w(px));
        self
    }

    pub fn h(mut self, px: f32) -> Self {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.height = px;
            cfg.rows = None;
        }
        self.inner = std::cell::RefCell::new(self.inner.take().h(px));
        self.update_scroll_dimensions();
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        {
            let mut cfg = self.config.lock().unwrap();
            cfg.width = w;
            cfg.height = h;
            cfg.cols = None;
            cfg.rows = None;
            cfg.width_is_explicit = true;
        }
        self.inner = std::cell::RefCell::new(self.inner.take().size(w, h));
        self.update_scroll_dimensions();
        self
    }

    pub fn square(mut self, size: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().square(size));
        self
    }

    pub fn w_full(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().w_full());
        self
    }

    pub fn h_full(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().h_full());
        self
    }

    pub fn w_fit(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().w_fit());
        self
    }

    pub fn h_fit(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().h_fit());
        self
    }

    pub fn min_w(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().min_w(px));
        self
    }

    pub fn p(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().p(px));
        self
    }

    pub fn px(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().px(px));
        self
    }

    pub fn py(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().py(px));
        self
    }

    pub fn m(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().m(px));
        self
    }

    pub fn mx(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().mx(px));
        self
    }

    pub fn my(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().my(px));
        self
    }

    pub fn gap(mut self, px: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().gap(px));
        self
    }

    pub fn flex_row(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().flex_row());
        self
    }

    pub fn flex_col(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().flex_col());
        self
    }

    pub fn flex_grow(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().flex_grow());
        self
    }

    pub fn items_center(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().items_center());
        self
    }

    pub fn items_start(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().items_start());
        self
    }

    pub fn items_end(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().items_end());
        self
    }

    pub fn justify_center(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().justify_center());
        self
    }

    pub fn justify_start(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().justify_start());
        self
    }

    pub fn justify_end(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().justify_end());
        self
    }

    pub fn justify_between(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().justify_between());
        self
    }

    pub fn bg(mut self, color: impl Into<blinc_core::Brush>) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().background(color));
        self
    }

    pub fn rounded(mut self, radius: f32) -> Self {
        self.config.lock().unwrap().corner_radius = radius;
        self.inner = std::cell::RefCell::new(self.inner.take().rounded(radius));
        self
    }

    /// Set the CSS element ID for stylesheet matching
    ///
    /// When set, the TextArea will query the active stylesheet for
    /// styles matching `#id`, `#id:hover`, `#id:focus`, `#id:disabled`,
    /// and `#id::placeholder`.
    pub fn id(mut self, id: &str) -> Self {
        if let Ok(mut d) = self.state.lock() {
            d.css_element_id = Some(id.to_string());
        }
        self.inner = std::cell::RefCell::new(self.inner.take().id(id));
        self
    }

    /// Add a CSS class name for selector matching
    pub fn class(mut self, name: &str) -> Self {
        if let Ok(mut d) = self.state.lock() {
            d.css_classes.push(blinc_core::intern::intern(name));
        }
        self.inner = std::cell::RefCell::new(self.inner.take().class(name));
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
        self.inner = std::cell::RefCell::new(self.inner.take().border_width(width));
        self
    }

    pub fn shadow(mut self, shadow: blinc_core::Shadow) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().shadow(shadow));
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

    pub fn shadow_lg(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().shadow_lg());
        self
    }

    pub fn transform(mut self, transform: blinc_core::Transform) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().transform(transform));
        self
    }

    pub fn opacity(mut self, opacity: f32) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().opacity(opacity));
        self
    }

    pub fn overflow_clip(mut self) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().overflow_clip());
        self
    }

    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.inner = std::cell::RefCell::new(self.inner.take().child(child));
        self
    }

    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator,
        I::Item: ElementBuilder + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().children(children));
        self
    }

    // Event handlers
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_click(handler));
        self
    }

    pub fn on_hover_enter<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_hover_enter(handler));
        self
    }

    pub fn on_hover_leave<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_hover_leave(handler));
        self
    }

    pub fn on_mouse_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_mouse_down(handler));
        self
    }

    pub fn on_mouse_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_mouse_up(handler));
        self
    }

    pub fn on_focus<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_focus(handler));
        self
    }

    pub fn on_blur<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_blur(handler));
        self
    }

    pub fn on_key_down<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_key_down(handler));
        self
    }

    pub fn on_key_up<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_key_up(handler));
        self
    }

    pub fn on_scroll<F>(mut self, handler: F) -> Self
    where
        F: Fn(&crate::event_handler::EventContext) + Send + Sync + 'static,
    {
        self.inner = std::cell::RefCell::new(self.inner.take().on_scroll(handler));
        self
    }

    /// Called after every edit with the new text.
    ///
    /// Complements [`Self::on_change_signal`], which only pokes the
    /// stateful dep graph: this one carries the text, which is what a
    /// caller keeping an external value (a DSL signal, a form model) in
    /// sync actually needs.
    pub fn on_change<F>(self, callback: F) -> Self
    where
        F: Fn(&str) + Send + Sync + 'static,
    {
        if let Ok(mut state) = self.state.lock() {
            state.on_change = Some(Arc::new(callback));
        }
        self
    }

    /// Set a signal ID to be notified when text content changes
    ///
    /// When the text area content is modified (typing, deletion, paste, etc.),
    /// this signal will be triggered via `check_stateful_deps`, causing any
    /// stateful elements that depend on this signal to rebuild.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let markdown_state = ctx.use_state_keyed("editor", || {
    ///     Arc::new(Mutex::new(TextAreaState::new()))
    /// });
    ///
    /// // Editor panel
    /// text_area(&markdown_state.get())
    ///     .on_change_signal(markdown_state.signal_id())
    ///
    /// // Preview panel - will rebuild when text changes
    /// stateful(preview_state)
    ///     .deps(&[markdown_state.signal_id()])
    ///     .on_state(|_, container| { ... })
    /// ```
    pub fn on_change_signal(self, signal_id: SignalId) -> Self {
        // Store the signal ID in the TextAreaState
        if let Ok(mut state) = self.state.lock() {
            state.set_change_signal(signal_id);
        }
        self
    }
}

/// Create a ready-to-use multi-line text area
///
/// The text area inherits ALL Div methods, so you have full layout control.
///
/// # Example
///
/// ```ignore
/// let state = text_area_state_with_placeholder("Enter message...");
/// text_area(&state)
///     .rows(4)
///     .w(400.0)
///     .rounded(12.0)
///     .shadow_sm()
/// ```
/// Create a text area widget
/// By default, width inherits from parent (w_full). Use .w() to set explicit width.
pub fn text_area(state: &SharedTextAreaState) -> TextArea {
    TextArea::new(state).w_full()
}

impl ElementBuilder for TextArea {
    fn bound_element_ref(&self) -> Option<&crate::selector::DivRef> {
        self.element_ref.as_ref()
    }

    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        let node = self.element().build(tree);
        if let Ok(mut d) = self.state.lock() {
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

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("textarea")
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
        // Return the layout bounds storage from the state so it gets updated after layout
        if let Ok(data) = self.state.lock() {
            Some(Arc::clone(&data.layout_bounds_storage))
        } else {
            None
        }
    }

    fn layout_bounds_callback(&self) -> Option<crate::renderer::LayoutBoundsCallback> {
        // A new size wraps the lines differently and moves the viewport.
        let config = Arc::clone(&self.config);
        let state = Arc::clone(&self.state);
        Some(Arc::new(move |bounds| {
            let mut changed = false;
            if let Ok(mut cfg) = config.lock() {
                let width_changed = (cfg.width - bounds.width).abs() > 1.0;
                let height_changed = (cfg.height - bounds.height).abs() > 1.0;
                if width_changed {
                    cfg.width = bounds.width;
                }
                if height_changed {
                    cfg.height = bounds.height;
                    let viewport_height =
                        bounds.height - cfg.padding_y * 2.0 - cfg.border_width * 2.0;
                    if let Ok(data) = state.lock() {
                        if let Ok(mut p) = data.scroll_physics.lock() {
                            p.viewport_height = viewport_height;
                        }
                    }
                }
                changed = width_changed || height_changed;
            }
            if changed {
                notify_text_area(&state);
            }
        }))
    }
}
