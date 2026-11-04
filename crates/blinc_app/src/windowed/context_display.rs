//! `WindowedContext` display helpers (DPI) and the stylesheet API.

use super::*;

impl WindowedContext {
    // =========================================================================
    // DPI-Related Helpers
    // =========================================================================

    /// Get the physical window width (for advanced use cases)
    ///
    /// Most users should use `ctx.width` which is in logical pixels.
    /// Physical dimensions are only needed when directly interfacing
    /// with GPU surfaces or platform-specific code.
    pub fn physical_width(&self) -> f32 {
        self.physical_width
    }

    /// Get the physical window height (for advanced use cases)
    pub fn physical_height(&self) -> f32 {
        self.physical_height
    }

    /// Check if the UI is ready (has completed at least one rebuild)
    ///
    /// This is useful for triggering animations after the first UI build,
    /// when motion bindings have been registered with the renderer.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn my_component(ctx: &WindowedContext) -> impl ElementBuilder {
    ///     let progress = ctx.use_animated_value_for("progress", 0.0, SpringConfig::gentle());
    ///
    ///     // Only trigger animation after UI is ready
    ///     let triggered = ctx.use_state_keyed("triggered", || false);
    ///     if ctx.is_ready() && !triggered.get() {
    ///         triggered.set(true);
    ///         progress.lock().unwrap().set_target(100.0);
    ///     }
    ///
    ///     // ... build UI ...
    /// }
    /// ```
    pub fn is_ready(&self) -> bool {
        self.rebuild_count > 0
    }

    /// Safe area inset from the top (status bar, notch)
    pub fn safe_top(&self) -> f32 {
        self.safe_area.0
    }

    /// Safe area inset from the right
    pub fn safe_right(&self) -> f32 {
        self.safe_area.1
    }

    /// Safe area inset from the bottom (home indicator)
    pub fn safe_bottom(&self) -> f32 {
        self.safe_area.2
    }

    /// Safe area inset from the left
    pub fn safe_left(&self) -> f32 {
        self.safe_area.3
    }

    /// Content width excluding safe area insets
    pub fn safe_width(&self) -> f32 {
        self.width - self.safe_area.1 - self.safe_area.3
    }

    /// Content height excluding safe area insets
    pub fn safe_height(&self) -> f32 {
        self.height - self.safe_area.0 - self.safe_area.2
    }

    /// Open a new window with the given configuration.
    ///
    /// The window is created asynchronously on the next event loop tick.
    /// Only available on desktop platforms.
    ///
    /// # Example
    ///
    /// ```ignore
    /// ctx.open_window(WindowConfig::new("Settings").size(400, 300));
    /// ```
    pub fn open_window(&self, config: WindowConfig) {
        if let Some(ref open_fn) = self.open_window_fn {
            open_fn(config);
        } else {
            tracing::warn!(
                "open_window() called but no window creation callback is set (not on desktop?)"
            );
        }
    }

    /// Set the callback for opening new windows (called by the desktop runner)
    pub(crate) fn set_open_window_fn(&mut self, f: Arc<dyn Fn(WindowConfig) + Send + Sync>) {
        self.open_window_fn = Some(f);
    }

    /// Set per-window action callbacks (called by the desktop runner)
    pub(crate) fn set_window_actions(
        &mut self,
        close: Arc<dyn Fn() + Send + Sync>,
        drag: Arc<dyn Fn() + Send + Sync>,
        minimize: Arc<dyn Fn() + Send + Sync>,
        maximize: Arc<dyn Fn() + Send + Sync>,
    ) {
        self.close_fn = Some(close);
        self.drag_fn = Some(drag);
        self.minimize_fn = Some(minimize);
        self.maximize_fn = Some(maximize);
    }

    /// Close THIS window. Safe to call from any click handler.
    pub fn close(&self) {
        if let Some(ref f) = self.close_fn {
            f();
        }
    }

    /// Start dragging THIS window (for custom title bars).
    pub fn drag(&self) {
        if let Some(ref f) = self.drag_fn {
            f();
        }
    }

    /// Minimize THIS window.
    pub fn minimize(&self) {
        if let Some(ref f) = self.minimize_fn {
            f();
        }
    }

    /// Maximize/restore THIS window.
    pub fn maximize(&self) {
        if let Some(ref f) = self.maximize_fn {
            f();
        }
    }

    /// Get a cloneable close callback for THIS window.
    /// Use this to capture the close action in event handler closures.
    pub fn close_callback(&self) -> Arc<dyn Fn() + Send + Sync> {
        self.close_fn.clone().unwrap_or_else(|| Arc::new(|| {}))
    }

    /// Get a cloneable drag callback for THIS window.
    pub fn drag_callback(&self) -> Arc<dyn Fn() + Send + Sync> {
        self.drag_fn.clone().unwrap_or_else(|| Arc::new(|| {}))
    }

    /// Get a cloneable minimize callback for THIS window.
    pub fn minimize_callback(&self) -> Arc<dyn Fn() + Send + Sync> {
        self.minimize_fn.clone().unwrap_or_else(|| Arc::new(|| {}))
    }

    /// Get a cloneable maximize callback for THIS window.
    pub fn maximize_callback(&self) -> Arc<dyn Fn() + Send + Sync> {
        self.maximize_fn.clone().unwrap_or_else(|| Arc::new(|| {}))
    }

    /// Register a callback to run once after the UI is ready
    ///
    /// The callback will be executed after the first UI rebuild completes,
    /// when motion bindings have been registered with the renderer.
    /// This is the recommended way to trigger initial animations.
    ///
    /// Callbacks are executed once and then discarded. If `is_ready()` is
    /// already true, the callback will run on the next frame.
    ///
    /// # Example
    ///
    /// ```ignore
    /// fn my_component(ctx: &WindowedContext) -> impl ElementBuilder {
    ///     let progress = ctx.use_animated_value_for("progress", 0.0, SpringConfig::gentle());
    ///
    ///     // Register animation to trigger when UI is ready
    ///     let progress_clone = progress.clone();
    ///     ctx.on_ready(move || {
    ///         if let Ok(mut value) = progress_clone.lock() {
    ///             value.set_target(100.0);
    ///         }
    ///     });
    ///
    ///     // ... build UI ...
    /// }
    /// ```
    /// Register a callback to run once when the UI is ready (context-level).
    ///
    /// **Note:** For element-specific callbacks, prefer using the query API:
    /// ```ignore
    /// ctx.query_element("my-element").on_ready(|bounds| {
    ///     // Triggered once after element is laid out
    /// });
    /// ```
    /// The query-based approach uses stable string IDs that survive tree rebuilds.
    ///
    /// This context-level callback runs after the first rebuild completes.
    /// If called after the UI is already ready, executes immediately.
    pub fn on_ready<F>(&self, callback: F)
    where
        F: FnOnce() + Send + Sync + 'static,
    {
        // If already ready, execute immediately
        if self.rebuild_count > 0 {
            callback();
            return;
        }
        // Otherwise queue for execution after first rebuild
        if let Ok(mut callbacks) = self.ready_callbacks.lock() {
            callbacks.push(Box::new(callback));
        }
    }

    // =========================================================================
    // CSS Stylesheet API
    // =========================================================================

    /// Add inline CSS to the application stylesheet.
    ///
    /// Multiple calls cascade — later rules override earlier ones.
    /// Stylesheets are visual-only: they update render props on existing nodes
    /// and trigger redraws. They never cause tree rebuilds.
    pub fn add_css(&mut self, css: &str) {
        // Store raw CSS for reparsing on theme changes
        self.css_sources.push(css.to_string());

        // Seed parser with theme variables + any previously defined CSS variables
        let mut external_vars = blinc_theme::ThemeState::try_get()
            .map(|t| t.to_css_variable_map())
            .unwrap_or_default();
        if let Some(existing) = &self.stylesheet {
            for (k, v) in existing.variables() {
                external_vars.insert(k.clone(), v.clone());
            }
        }
        match blinc_layout::css_parser::Stylesheet::parse_with_variables(css, &external_vars) {
            Ok(sheet) => self.add_stylesheet(sheet),
            Err(e) => {
                tracing::warn!("Failed to parse CSS: {}", e);
            }
        }
    }

    /// Load and add a `.css` file to the application stylesheet.
    ///
    /// Multiple calls cascade — later rules override earlier ones.
    pub fn load_css(&mut self, path: &str) {
        match std::fs::read_to_string(path) {
            Ok(css) => self.add_css(&css),
            Err(e) => {
                tracing::warn!("Failed to load CSS file '{}': {}", path, e);
            }
        }
    }

    /// Add a pre-parsed stylesheet to the application.
    ///
    /// Multiple calls cascade — later rules override earlier ones.
    pub fn add_stylesheet(&mut self, sheet: blinc_layout::css_parser::Stylesheet) {
        match self.stylesheet.as_mut() {
            Some(existing) => {
                // Cascade: merge into existing (Arc::make_mut for COW)
                Arc::make_mut(existing).merge(sheet);
            }
            None => {
                self.stylesheet = Some(Arc::new(sheet));
            }
        }
        // Publish to global so stateful widgets (buttons, etc.) can read CSS
        // overrides during tree construction, before set_stylesheet_arc() runs
        if let Some(ref stylesheet) = self.stylesheet {
            blinc_layout::css_parser::set_active_stylesheet(std::sync::Arc::clone(stylesheet));
        }
    }

    /// Add a component's own stylesheet.
    ///
    /// Its class rules sit below every application rule, so an app
    /// stylesheet themes a component without matching its specificity.
    pub fn add_component_stylesheet(&mut self, sheet: blinc_layout::css_parser::Stylesheet) {
        self.add_stylesheet(sheet.into_component());
    }

    /// Drop every accumulated stylesheet and reset `rebuild_count` so a
    /// subsequent rebuild looks like the very first one.
    ///
    /// Called by the hot-reload runner before re-invoking the user's UI
    /// closure under the freshly-applied subsecond patch. Without this:
    ///
    /// - `add_css` cascades, so deleted rules from the pre-patch run
    ///   would linger in the merged stylesheet.
    /// - `css_sources` would grow unboundedly across every patch.
    /// - The common `if ctx.rebuild_count == 0 { ctx.add_css(...) }`
    ///   guard in app code would skip re-registration entirely, so
    ///   even live rules wouldn't refresh.
    ///
    /// Also drops the global `ACTIVE_STYLESHEET` so widgets reaching
    /// for CSS overrides during the rebuild don't see a stale Arc.
    /// Outside hot-reload this would throw away cascaded user state;
    /// the method is `#[doc(hidden)]` and only invoked from the
    /// hot-reload trigger.
    #[doc(hidden)]
    pub fn reset_for_hot_reload(&mut self) {
        self.stylesheet = None;
        self.css_sources.clear();
        self.rebuild_count = 0;
        blinc_layout::css_parser::clear_active_stylesheet();
    }

    /// Reparse all stored CSS sources with fresh theme variables.
    ///
    /// Called automatically when the theme color scheme changes to ensure
    /// CSS `var()` and `theme()` references resolve to the new colors.
    pub fn reparse_css(&mut self) {
        if self.css_sources.is_empty() {
            return;
        }

        tracing::debug!(
            "Reparsing {} CSS sources with updated theme variables",
            self.css_sources.len()
        );

        // Clear existing stylesheet
        self.stylesheet = None;

        // Reparse each CSS source with fresh theme variables
        for css in self.css_sources.clone() {
            let mut external_vars = blinc_theme::ThemeState::try_get()
                .map(|t| t.to_css_variable_map())
                .unwrap_or_default();
            if let Some(existing) = &self.stylesheet {
                for (k, v) in existing.variables() {
                    external_vars.insert(k.clone(), v.clone());
                }
            }
            match blinc_layout::css_parser::Stylesheet::parse_with_variables(&css, &external_vars) {
                Ok(sheet) => self.add_stylesheet(sheet),
                Err(e) => {
                    tracing::warn!("Failed to reparse CSS on theme change: {}", e);
                }
            }
        }
    }

    /// Set a style for an element by ID.
    ///
    /// This is the Rust-native alternative to `add_css()`. Use with `css_style!` or `style!`
    /// macros to define styles in Rust syntax that are applied automatically to matching
    /// elements — just like CSS stylesheets.
    ///
    /// # Example
    ///
    /// ```ignore
    /// ctx.set_style("card", css_style! {
    ///     background: Color::BLUE;
    ///     border-radius: 12.0;
    ///     box-shadow: md;
    /// });
    ///
    /// // Then just give the element an ID:
    /// div().id("card").w(200.0).h(100.0)
    /// ```
    pub fn set_style(&mut self, id: &str, style: blinc_layout::element_style::ElementStyle) {
        match self.stylesheet.as_mut() {
            Some(existing) => {
                Arc::make_mut(existing).insert(id, style);
            }
            None => {
                let mut sheet = blinc_layout::css_parser::Stylesheet::new();
                sheet.insert(id, style);
                self.stylesheet = Some(Arc::new(sheet));
            }
        }
    }

    /// Set a state-specific style for an element by ID.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_layout::css_parser::ElementState;
    ///
    /// ctx.set_style("button", style! { bg: Color::BLUE, rounded: 8.0 });
    /// ctx.set_state_style("button", ElementState::Hover, style! {
    ///     bg: Color::from_hex(0x2563EB),
    ///     shadow_md,
    /// });
    /// ```
    pub fn set_state_style(
        &mut self,
        id: &str,
        state: blinc_layout::css_parser::ElementState,
        style: blinc_layout::element_style::ElementStyle,
    ) {
        match self.stylesheet.as_mut() {
            Some(existing) => {
                Arc::make_mut(existing).insert_with_state(id, state, style);
            }
            None => {
                let mut sheet = blinc_layout::css_parser::Stylesheet::new();
                sheet.insert_with_state(id, state, style);
                self.stylesheet = Some(Arc::new(sheet));
            }
        }
    }
}
