//! Link widget for clickable text
//!
//! A styled text element that acts as a hyperlink with hover states,
//! equivalent to `<a>` in HTML. Links are underlined by default and
//! clicking opens the URL in the system browser.
//!
//! # Example
//!
//! ```ignore
//! use blinc_layout::prelude::*;
//!
//! // Default link - clicking opens URL in browser
//! link("Click here", "https://example.com")
//!
//! // Custom click handler (replaces default behavior)
//! link("Custom action", "https://example.com")
//!     .on_click(|url, ctx| println!("Custom handler: {}", url))
//!
//! // Link without underline
//! link("No underline", "https://example.com")
//!     .no_underline()
//!
//! // Link with underline only on hover
//! link("Hover to see underline", "https://example.com")
//!     .underline_on_hover()
//! ```

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use blinc_core::Color;
use blinc_theme::ThemeState;

use crate::div::{Div, ElementBuilder, div};
use crate::element::RenderProps;
use crate::event_handler::EventContext;
use crate::text::text;
use crate::tree::{LayoutNodeId, LayoutTree};

/// Open a URL in the system's default browser
///
/// This is the default action for links when clicked.
/// On platforms without the `open` crate support, this logs a warning.
pub fn open_url(url: &str) {
    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    {
        if let Err(e) = open::that(url) {
            tracing::warn!("Failed to open URL '{}': {}", url, e);
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        tracing::warn!("URL opening not supported on this platform: {}", url);
    }
}

/// Configuration for link styling
#[derive(Clone, Debug)]
pub struct LinkConfig {
    /// Text colour over the `.blinc-link` rule's, when set.
    pub text_color: Option<Color>,
    /// Font size
    pub font_size: f32,
    /// Whether the text is underlined
    pub underline: bool,
    /// Underline under the pointer only
    pub underline_on_hover_only: bool,
}

impl Default for LinkConfig {
    fn default() -> Self {
        Self {
            text_color: None,
            font_size: 16.0,
            underline: true,
            underline_on_hover_only: false,
        }
    }
}

/// What a click runs in place of opening the URL.
type ClickHandler = Arc<dyn Fn(&str, &EventContext) + Send + Sync>;

/// A hyperlink widget
pub struct Link {
    /// Built from the fields below, and rebuilt when one of them changes.
    inner: Div,
    label: String,
    url: String,
    config: LinkConfig,
    handler: Option<ClickHandler>,
    css_element_id: Option<String>,
    css_classes: Vec<Arc<str>>,
}

impl Link {
    /// Create a new link with text and URL
    ///
    /// By default, clicking the link opens the URL in the system browser.
    /// Use `.on_click()` to override this behavior.
    pub fn new(label: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            inner: div(),
            label: label.into(),
            url: url.into(),
            config: LinkConfig::default(),
            handler: None,
            css_element_id: None,
            css_classes: vec![
                blinc_core::intern::intern("blinc-link"),
                blinc_core::intern::intern("tabindex=0"),
            ],
        }
        .rebuilt()
    }

    /// Rebuilds the inner element. `build`, `render_props` and `children_builders` all read it,
    /// so it carries every style of the link.
    fn rebuilt(mut self) -> Self {
        let mut label = text(&self.label).size(self.config.font_size).no_cursor();
        if let Some(color) = self.config.text_color {
            label = label.color(color);
        }
        if self.config.underline {
            label = if self.config.underline_on_hover_only {
                // The stylesheet underlines this class while the link is hovered.
                label.class("blinc-link-hover-underline")
            } else {
                label.underline()
            };
        }

        let url = self.url.clone();
        let handler = self.handler.clone();
        self.inner = div()
            .child(label)
            .cursor_pointer()
            .on_click(move |ctx| match &handler {
                Some(handler) => handler(&url, ctx),
                None => open_url(&url),
            });
        self
    }

    /// Set a custom click handler (receives URL and event context)
    ///
    /// This replaces the default behavior of opening the URL in the browser.
    pub fn on_click<F>(mut self, handler: F) -> Self
    where
        F: Fn(&str, &EventContext) + Send + Sync + 'static,
    {
        self.handler = Some(Arc::new(handler));
        self.rebuilt()
    }

    /// Set the text color
    pub fn text_color(mut self, color: Color) -> Self {
        self.config.text_color = Some(color);
        self.rebuilt()
    }

    /// Set the font size
    pub fn font_size(mut self, size: f32) -> Self {
        self.config.font_size = size;
        self.rebuilt()
    }

    /// Underline the text at all times, or not at all
    pub fn underline(mut self, enabled: bool) -> Self {
        self.config.underline = enabled;
        self.config.underline_on_hover_only = false;
        self.rebuilt()
    }

    /// Remove underline decoration (convenience for `.underline(false)`)
    pub fn no_underline(self) -> Self {
        self.underline(false)
    }

    /// Underline the text only while the pointer is over the link
    pub fn underline_on_hover(mut self) -> Self {
        self.config.underline = true;
        self.config.underline_on_hover_only = true;
        self.rebuilt()
    }

    /// Get the URL
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Set the element ID for CSS selector targeting
    pub fn id(mut self, id: &str) -> Self {
        self.css_element_id = Some(id.to_string());
        self
    }

    /// Add a CSS class for selector matching
    pub fn class(mut self, name: &str) -> Self {
        self.css_classes.push(blinc_core::intern::intern(name));
        self
    }
}

impl Deref for Link {
    type Target = Div;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for Link {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl ElementBuilder for Link {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.inner.children_builders()
    }

    fn element_type_id(&self) -> crate::div::ElementTypeId {
        self.inner.element_type_id()
    }

    fn semantic_type_name(&self) -> Option<&'static str> {
        Some("a")
    }

    fn element_id(&self) -> Option<&str> {
        self.css_element_id.as_deref()
    }

    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        &self.css_classes
    }

    fn event_handlers(&self) -> Option<&crate::event_handler::EventHandlers> {
        ElementBuilder::event_handlers(&self.inner)
    }

    fn layout_style(&self) -> Option<&taffy::Style> {
        self.inner.layout_style()
    }
}

/// Create a link with text and URL
///
/// # Example
///
/// ```ignore
/// link("Click here", "https://example.com")
///     .on_click(|url, _ctx| println!("Opening: {}", url))
/// ```
pub fn link(label: impl Into<String>, url: impl Into<String>) -> Link {
    Link::new(label, url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::CursorStyle;

    fn init_theme() {
        let _ = ThemeState::try_get().unwrap_or_else(|| {
            ThemeState::init_default();
            ThemeState::get()
        });
    }

    #[test]
    fn test_link_creates_element() {
        init_theme();
        let mut tree = LayoutTree::new();
        let lnk = link("Test", "https://example.com");
        lnk.build(&mut tree);
        assert!(!tree.is_empty());
    }

    #[test]
    fn test_link_stores_url() {
        init_theme();
        let lnk = link("Test", "https://example.com");
        assert_eq!(lnk.url(), "https://example.com");
    }

    #[test]
    fn modifiers_compose_and_keep_the_click_handler() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        init_theme();
        let clicks = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&clicks);
        let lnk = link("Test", "https://example.com")
            .on_click(move |url, _| {
                assert_eq!(url, "https://example.com");
                count.fetch_add(1, Ordering::SeqCst);
            })
            .font_size(24.0)
            .text_color(Color::RED)
            .no_underline();

        let label = lnk.children_builders()[0].text_render_info().unwrap();
        assert_eq!(label.content, "Test");
        assert_eq!(label.font_size, 24.0);
        assert_eq!(label.color, [1.0, 0.0, 0.0, 1.0]);
        assert!(!label.underline);

        ElementBuilder::event_handlers(&lnk)
            .unwrap()
            .dispatch(&EventContext::new(
                blinc_core::events::event_types::POINTER_UP,
                LayoutNodeId::default(),
            ));
        assert_eq!(clicks.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_last_underline_setting_applies() {
        init_theme();
        let state = |lnk: Link| {
            let label = &lnk.children_builders()[0];
            (
                label.text_render_info().unwrap().underline,
                label
                    .element_classes()
                    .iter()
                    .any(|class| &**class == "blinc-link-hover-underline"),
            )
        };
        let new = || link("Test", "https://example.com");

        assert_eq!(state(new()), (true, false));
        assert_eq!(state(new().underline_on_hover()), (false, true));
        assert_eq!(
            state(new().underline_on_hover().underline(true)),
            (true, false)
        );
        assert_eq!(
            state(new().underline_on_hover().no_underline()),
            (false, false)
        );
        assert_eq!(
            state(new().no_underline().underline_on_hover()),
            (false, true)
        );
    }

    #[test]
    fn test_link_has_pointer_cursor() {
        init_theme();
        let lnk = link("Test", "https://example.com");
        let props = lnk.render_props();
        assert_eq!(props.cursor, Some(CursorStyle::Pointer));
    }
}
