#![cfg(feature = "media")]
//! Media controls take their colours from the layout stylesheet.

use std::rc::Rc;
use std::sync::Arc;

use blinc_core::{Color, DrawCommand, RecordingContext, Size};
use blinc_layout::CanvasBounds;
use blinc_layout::renderer::{ElementType, RenderTree};
use blinc_layout::widgets::media::MediaControls;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

struct Stub {
    live: bool,
}

impl blinc_media::Player for Stub {
    fn play(&self) {}
    fn pause(&self) {}
    fn stop(&self) {}
    fn seek(&self, _: u64) {}
    fn position_ms(&self) -> u64 {
        30_000
    }
    fn duration_ms(&self) -> u64 {
        60_000
    }
    fn volume(&self) -> f32 {
        0.5
    }
    fn set_volume(&self, _: f32) {}
    fn is_playing(&self) -> bool {
        false
    }
    fn is_live(&self) -> bool {
        self.live
    }
}

/// The controls for `player`, styled by the layout stylesheet.
fn styled(live: bool) -> RenderTree {
    static INIT: std::sync::Once = std::sync::Once::new();
    // Pinned: `init_default` follows the machine's colour scheme.
    INIT.call_once(|| ThemeState::init(HybridTheme::bundle(), ColorScheme::Dark));
    let vars = ThemeState::get().to_css_variable_map();
    let sheet = blinc_layout::css_parser::Stylesheet::parse_with_variables(
        blinc_layout::ua_styles::UA_STYLES,
        &vars,
    )
    .expect("the stylesheet parses with the theme's variables");
    let controls = MediaControls::new(Rc::new(Stub { live })).into_div();
    let mut tree = RenderTree::from_element(&controls);
    tree.set_stylesheet_arc(Arc::new(sheet));
    tree.apply_stylesheet_base_styles();
    tree.compute_layout(400.0, 60.0);
    tree
}

/// The render props of the first node carrying `class`.
fn props(tree: &RenderTree, class: &str) -> blinc_layout::element::RenderProps {
    let registry = tree.element_registry();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(n) = stack.pop() {
        if registry.has_class(n, class) {
            return tree.get_render_node(n).unwrap().props.clone();
        }
        stack.extend(tree.layout_tree.children(n));
    }
    panic!("no .{class}");
}

fn fill(tree: &RenderTree, class: &str) -> Option<Color> {
    match props(tree, class).background {
        Some(blinc_core::Brush::Solid(c)) => Some(c),
        _ => None,
    }
}

fn near(a: Option<Color>, b: Color) -> bool {
    a.is_some_and(|a| {
        (a.r - b.r).abs() < 0.01 && (a.g - b.g).abs() < 0.01 && (a.b - b.b).abs() < 0.01
    })
}

fn text(tree: &RenderTree, class: &str) -> Option<Color> {
    props(tree, class)
        .text_color
        .map(|[r, g, b, a]| Color::rgba(r, g, b, a))
}

/// The width and colour of each bar the canvas under `class` draws at `width`.
fn bars(tree: &RenderTree, class: &str, width: f32) -> Vec<(f32, Color)> {
    let registry = tree.element_registry();
    let mut stack = vec![(tree.root().unwrap(), false)];
    while let Some((node, inside)) = stack.pop() {
        let inside = inside || registry.has_class(node, class);
        if let Some(ElementType::Canvas(canvas)) =
            tree.get_render_node(node).map(|n| &n.element_type)
            && inside
        {
            let mut ctx = RecordingContext::new(Size::new(width, 12.0));
            let bounds = CanvasBounds {
                x: 0.0,
                y: 0.0,
                width,
                height: 12.0,
            };
            (canvas.render_fn.as_ref().expect("a paint"))(&mut ctx, bounds);
            return ctx
                .commands()
                .iter()
                .filter_map(|command| match command {
                    DrawCommand::FillRect {
                        rect,
                        brush: blinc_core::Brush::Solid(color),
                        ..
                    } => Some((rect.width(), *color)),
                    _ => None,
                })
                .collect();
        }
        stack.extend(
            tree.layout_tree
                .children(node)
                .into_iter()
                .map(|child| (child, inside)),
        );
    }
    panic!("no canvas under .{class}");
}

#[test]
fn controls_take_their_colours_from_rules() {
    let tree = styled(false);
    let theme = ThemeState::get();
    assert!(near(
        fill(&tree, "blinc-media-play-btn"),
        theme.color(ColorToken::SurfaceElevated)
    ));
    // The seek bar is drawn: the track across it, and the half that has played.
    let seek = bars(&tree, "blinc-media-seek", 200.0);
    for (width, token) in [
        (200.0, ColorToken::SurfaceElevated),
        (100.0, ColorToken::Primary),
    ] {
        assert!(
            seek.iter()
                .any(|(w, c)| (w - width).abs() < 0.5 && near(Some(*c), theme.color(token))),
            "no {token:?} bar {width} wide in {seek:?}"
        );
    }
    // Half the volume: the columns up to it stand out, and the rest are dim.
    let columns = bars(&tree, "blinc-media-vol-slider", 50.0);
    assert_eq!(columns.len(), 20);
    for (index, (_, color)) in columns.iter().enumerate() {
        let token = if index < 10 {
            ColorToken::TextSecondary
        } else {
            ColorToken::SurfaceElevated
        };
        assert!(near(Some(*color), theme.color(token)), "column {index}");
    }
    assert!(near(
        text(&tree, "blinc-media-time"),
        theme.color(ColorToken::TextTertiary)
    ));
}

#[test]
fn a_live_stream_shows_the_live_badge() {
    let tree = styled(true);
    assert!(near(
        fill(&tree, "blinc-media-live-badge"),
        ThemeState::get().color(ColorToken::Error)
    ));
}
