//! Preview Panel - Live/recorded UI preview

use std::cell::OnceCell;

use blinc_cn::components::select::{SelectSize, select};
use blinc_cn::components::separator::separator;
use blinc_cn::components::switch::{SwitchSize, switch};
use blinc_core::Color;
use blinc_core::context_state::BlincContextState;
use blinc_layout::div::{Div, ElementBuilder, FontWeight};
use blinc_layout::element::RenderProps;
use blinc_layout::event_handler::EventHandlers;
use blinc_layout::prelude::*;
use blinc_layout::tree::{LayoutNodeId, LayoutTree};
use blinc_recorder::TreeSnapshot;
use blinc_theme::{ColorToken, ThemeState};

#[derive(Clone)]
pub struct PreviewConfig {
    pub show_bounds: bool,
    pub show_cursor: bool,
    pub zoom: f32,
}

impl Default for PreviewConfig {
    fn default() -> Self {
        Self {
            show_bounds: false,
            show_cursor: true,
            zoom: 1.0,
        }
    }
}

struct PreviewPanelConfig {
    has_snapshot: bool,
    show_bounds: bool,
    show_cursor: bool,
    zoom: f32,
    cursor_position: Option<(f32, f32)>,
}

struct BuiltPreviewPanel {
    inner: Div,
}

impl BuiltPreviewPanel {
    fn from_config(config: &PreviewPanelConfig) -> Self {
        let theme = ThemeState::get();

        let inner = div()
            .flex_grow()
            .h_full()
            .bg(theme.color(ColorToken::Background))
            .flex_col()
            .child(Self::toolbar(config))
            .child(separator())
            .child(Self::preview_area(config));

        BuiltPreviewPanel { inner }
    }

    fn toolbar(config: &PreviewPanelConfig) -> Div {
        let theme = ThemeState::get();
        let ctx = BlincContextState::get();

        // Get states from context
        let bounds_state = ctx.use_state_keyed("preview_bounds", || config.show_bounds);
        let cursor_state = ctx.use_state_keyed("preview_cursor", || config.show_cursor);
        let zoom_str = format!("{}", (config.zoom * 100.0) as i32);
        let zoom_state = ctx.use_state_keyed("preview_zoom", || zoom_str.clone());

        div()
            .h(44.0)
            .py(2.0)
            .px(12.0)
            .bg(theme.color(ColorToken::SurfaceElevated))
            .flex_row()
            .items_center()
            .justify_between()
            .child(
                text("Preview")
                    .size(13.0)
                    .color(theme.color(ColorToken::TextPrimary))
                    .weight(FontWeight::SemiBold),
            )
            .child(
                div()
                    .flex_row()
                    .items_center()
                    .gap(12.0)
                    .child(
                        switch(bounds_state.signal())
                            .size(SwitchSize::Small)
                            .label("Bounds"),
                    )
                    .child(
                        switch(cursor_state.signal())
                            .size(SwitchSize::Small)
                            .label("Cursor"),
                    )
                    .child(
                        select(zoom_state.signal())
                            .size(SelectSize::Small)
                            .w(80.0)
                            .option("50", "50%")
                            .option("75", "75%")
                            .option("100", "100%")
                            .option("150", "150%")
                            .option("200", "200%"),
                    ),
            )
    }

    fn preview_area(config: &PreviewPanelConfig) -> Div {
        let content = if config.has_snapshot {
            Self::render_preview(config)
        } else {
            Self::render_empty_state()
        };

        div()
            .flex_grow()
            .overflow_clip()
            .items_center()
            .justify_center()
            .p(16.0)
            .child(content)
    }

    fn render_preview(config: &PreviewPanelConfig) -> Div {
        let theme = ThemeState::get();
        let mut preview = div()
            .w(800.0 * config.zoom)
            .h(600.0 * config.zoom)
            .bg(theme.color(ColorToken::Surface))
            .rounded(8.0)
            .border(1.0, theme.color(ColorToken::Border))
            .items_center()
            .justify_center()
            .relative()
            .child(
                text("UI Preview")
                    .size(16.0)
                    .color(theme.color(ColorToken::TextTertiary)),
            );

        if config.show_cursor {
            if let Some((x, y)) = config.cursor_position {
                preview = preview.child(Self::render_cursor(x, y, config.zoom));
            }
        }

        preview
    }

    fn render_cursor(x: f32, y: f32, zoom: f32) -> Div {
        let theme = ThemeState::get();
        div()
            .absolute()
            .left(x * zoom)
            .top(y * zoom)
            .w(10.0)
            .h(10.0)
            .rounded_full()
            .bg(theme.color(ColorToken::Primary))
            .border(2.0, Color::WHITE)
    }

    fn render_empty_state() -> Div {
        let theme = ThemeState::get();
        div()
            .w(400.0)
            .flex_col()
            .items_center()
            .gap(8.0)
            .child(
                text("No Preview Available")
                    .size(16.0)
                    .color(theme.color(ColorToken::TextPrimary)),
            )
            .child(
                text("Load a recording or connect to a running app")
                    .size(13.0)
                    .color(theme.color(ColorToken::TextTertiary)),
            )
    }
}

pub struct PreviewPanel {
    config: PreviewPanelConfig,
    built: OnceCell<BuiltPreviewPanel>,
}

impl PreviewPanel {
    pub fn new(
        snapshot: Option<&TreeSnapshot>,
        config: &PreviewConfig,
        cursor_position: Option<(f32, f32)>,
    ) -> Self {
        Self {
            config: PreviewPanelConfig {
                has_snapshot: snapshot.is_some(),
                show_bounds: config.show_bounds,
                show_cursor: config.show_cursor,
                zoom: config.zoom,
                cursor_position,
            },
            built: OnceCell::new(),
        }
    }

    fn get_or_build(&self) -> &BuiltPreviewPanel {
        self.built
            .get_or_init(|| BuiltPreviewPanel::from_config(&self.config))
    }

    pub fn build(self) -> Div {
        BuiltPreviewPanel::from_config(&self.config).inner
    }
}

impl ElementBuilder for PreviewPanel {
    fn build(&self, tree: &mut LayoutTree) -> LayoutNodeId {
        self.get_or_build().inner.build(tree)
    }

    fn render_props(&self) -> RenderProps {
        self.get_or_build().inner.render_props()
    }

    fn children_builders(&self) -> &[Box<dyn ElementBuilder>] {
        self.get_or_build().inner.children_builders()
    }

    fn event_handlers(&self) -> Option<&EventHandlers> {
        let handlers = self.get_or_build().inner.event_handlers();
        if handlers.is_empty() {
            None
        } else {
            Some(handlers)
        }
    }
}
