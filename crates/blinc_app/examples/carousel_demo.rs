//! Carousel Demo - Selector API Showcase
//!
//! Demonstrates the new selector API features:
//! - ScrollRef for programmatic scroll control
//! - Element IDs for targeting elements
//! - scroll_to() to scroll to elements by ID
//! - ScrollOptions with ScrollBlock::Center for centering cards
//!
//! Run with: cargo run -p blinc_app --example carousel_demo --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::{Color, State, Transform};
use blinc_layout::prelude::NoState;
use blinc_layout::selector::{ScrollBehavior, ScrollBlock, ScrollOptions, ScrollRef};
use blinc_layout::units::px;

const CARD_COUNT: usize = 5;
const CARD_WIDTH: f32 = 280.0;
const CARD_HEIGHT: f32 = 360.0;
const CARD_GAP: f32 = 20.0;
const VIEWPORT_WIDTH: f32 = 400.0;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let config = WindowConfig {
        title: "Carousel Demo - Selector API".to_string(),
        width: 600,
        height: 800,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    // Create a ScrollRef for programmatic scroll control
    let scroll_ref = ctx.use_scroll_ref("carousel_scroll");

    let current_index = ctx.use_state_keyed("current_index", || 0usize);

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .flex_col()
        .items_center()
        .justify_center()
        .p(10.0)
        .gap(8.0)
        // Title
        .child(
            div()
                .flex_col()
                .items_center()
                .gap(4.0)
                .child(
                    h1("Carousel Demo")
                        .size(36.0)
                        .weight(FontWeight::Bold)
                        .color(Color::WHITE),
                )
                .child(
                    text("Showcasing ScrollRef and Element IDs")
                        .size(16.0)
                        .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
                ),
        )
        .child(
            scroll()
                .justify_center()
                .w_full()
                .h(ctx.height - 200.0)
                .child(
                    div()
                        .w_full()
                        .h_fit()
                        .flex_col()
                        .gap(10.0)
                        .justify_center()
                        .child(build_carousel(ctx, &scroll_ref, &current_index))
                        .child(build_carousel_dots(ctx, &scroll_ref, &current_index))
                        .child(build_info_section()),
                ),
        )
}

fn build_carousel(
    _ctx: &WindowedContext,
    scroll_ref: &ScrollRef,
    current_index: &State<usize>,
) -> impl ElementBuilder {
    // Card data - static, cards don't change
    let cards = [
        (
            "Welcome",
            "This carousel demonstrates the new selector API",
            Color::rgba(0.4, 0.6, 1.0, 1.0),
        ),
        (
            "ScrollRef",
            "Programmatic scroll control via scroll_ref.scroll_to()",
            Color::rgba(0.6, 0.4, 1.0, 1.0),
        ),
        (
            "Element IDs",
            "Each card has an ID like 'card-0', 'card-1', etc.",
            Color::rgba(1.0, 0.4, 0.6, 1.0),
        ),
        (
            "Smooth Scroll",
            "Click dots below to smoothly scroll to cards",
            Color::rgba(0.4, 1.0, 0.6, 1.0),
        ),
        (
            "Centered",
            "Cards center in viewport using ScrollBlock::Center",
            Color::rgba(1.0, 0.6, 0.4, 1.0),
        ),
    ];

    // Only the indicator needs to update reactively
    let current_index_clone = current_index.clone();

    div()
        .flex_col()
        .items_center()
        .gap(12.0)
        // Carousel container with rounded corners and shadow
        .child(
            div()
                .w(VIEWPORT_WIDTH)
                .h(CARD_HEIGHT + 40.0)
                .bg(Color::rgba(0.12, 0.12, 0.16, 1.0))
                .rounded(20.0)
                .shadow_lg()
                .overflow_clip()
                .items_center()
                .justify_center()
                .child(
                    // Horizontal scroll container - cards are STATIC inside
                    scroll()
                        .bind(scroll_ref)
                        .direction(ScrollDirection::Horizontal)
                        .w(VIEWPORT_WIDTH)
                        .h(CARD_HEIGHT + 20.0)
                        .items_start()
                        .justify_start() // Ensure content starts at beginning
                        .child(
                            // Cards container - STATIC, no stateful needed
                            div()
                                .flex_row()
                                .gap(CARD_GAP)
                                .items_start()
                                // Padding to center first and last cards in viewport
                                // px() gives raw pixels, sp() gives scaled spacing units
                                .padding_x(px((VIEWPORT_WIDTH - CARD_WIDTH) / 2.0))
                                .children(
                                    cards
                                        .iter()
                                        .enumerate()
                                        .map(|(i, (title, desc, accent))| {
                                            build_card(i, title, desc, *accent)
                                        })
                                        .collect::<Vec<_>>(),
                                ),
                        ),
                ),
        )
        // Current card indicator - ONLY this updates reactively
        .child(
            stateful::<NoState>()
                .deps([current_index.signal_id()])
                .on_state(move |_ctx| {
                    let current = current_index_clone.get();
                    div().child(
                        text(&format!("Card {} of {}", current + 1, CARD_COUNT))
                            .size(14.0)
                            .color(Color::rgba(0.5, 0.5, 0.6, 1.0)),
                    )
                }),
        )
}

fn build_card(index: usize, title: &str, description: &str, accent: Color) -> impl ElementBuilder {
    div()
        // Set element ID for this card - key for the selector API!
        .id(format!("card-{}", index))
        .w(CARD_WIDTH)
        .h(CARD_HEIGHT)
        .bg(Color::rgba(0.18, 0.18, 0.22, 1.0))
        .rounded(16.0)
        .shadow_md()
        .flex_col()
        .p(4.0)
        .gap(4.0)
        // Accent bar at top
        .child(div().w_full().h(4.0).bg(accent).rounded(2.0))
        // Card number badge
        .child(
            div()
                .w(40.0)
                .h(40.0)
                .bg(accent.with_alpha(0.2))
                .rounded(20.0)
                .items_center()
                .justify_center()
                .child(
                    text(&format!("{}", index + 1))
                        .size(18.0)
                        .weight(FontWeight::Bold)
                        .color(accent),
                ),
        )
        // Title
        .child(
            h3(title)
                .size(24.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE),
        )
        // Description
        .child(
            text(description)
                .size(14.0)
                .color(Color::rgba(0.7, 0.7, 0.75, 1.0))
                .line_height(1.5),
        )
        // Spacer
        .child(div().flex_grow())
        // ID display
        .child(
            div()
                .bg(Color::rgba(0.1, 0.1, 0.14, 1.0))
                .rounded(8.0)
                .px(12.0)
                .py(8.0)
                .child(
                    text(&format!("id=\"card-{}\"", index))
                        .size(12.0)
                        .color(Color::rgba(0.5, 0.8, 0.5, 1.0)),
                ),
        )
}

fn build_carousel_dots(
    ctx: &WindowedContext,
    scroll_ref: &ScrollRef,
    current_index: &State<usize>,
) -> impl ElementBuilder {
    div()
        .w_full()
        .flex_row()
        .gap(12.0)
        .justify_center()
        .items_center()
        .children(
            (0..CARD_COUNT)
                .map(|i| build_dot(ctx, i, scroll_ref, current_index))
                .collect::<Vec<_>>(),
        )
}

fn build_dot(
    _ctx: &WindowedContext,
    index: usize,
    scroll_ref: &ScrollRef,
    current_index: &State<usize>,
) -> impl ElementBuilder {
    let current_index_signal = current_index.signal_id();
    let current_index_clone = current_index.clone();
    let current_index_for_click = current_index.clone();
    // Clone scroll_ref for the click handler
    let scroll_ref_clone = scroll_ref.clone();

    stateful::<ButtonState>()
        .initial(ButtonState::Idle)
        // Bind to current_index signal so we re-render when selection changes
        .deps([current_index_signal])
        .on_state(move |ctx| {
            let state = ctx.state();
            // Read current selection inside on_state - will be fresh on each refresh
            let current_val = current_index_clone.get();
            let is_current = index == current_val;

            let base_color = if is_current {
                Color::rgba(0.4, 0.6, 1.0, 1.0)
            } else {
                Color::rgba(0.3, 0.3, 0.4, 1.0)
            };

            let color = match state {
                ButtonState::Idle => base_color,
                ButtonState::Hovered => base_color.with_alpha(0.8),
                ButtonState::Pressed => base_color.with_alpha(0.6),
                ButtonState::Disabled => base_color.with_alpha(0.3),
            };

            let transform = if state == ButtonState::Hovered && !is_current {
                Transform::scale(1.1, 1.1)
            } else if is_current {
                Transform::scale(2.0, 1.0)
            } else {
                Transform::scale(1.0, 1.0)
            };

            div()
                .h(12.0)
                .rounded(6.0)
                .w(12.0)
                .bg(color)
                .transform(transform)
        })
        .on_click(move |_| {
            // Use the selector API to scroll to the card by ID!
            let card_id = format!("card-{}", index);
            tracing::info!("Scrolling to {}", card_id);

            // Update current index signal
            current_index_for_click.set(index);

            // Scroll with smooth animation and center the card
            scroll_ref_clone.scroll_to_with_options(
                &card_id,
                ScrollOptions {
                    behavior: ScrollBehavior::Smooth,
                    block: ScrollBlock::Center,
                    ..Default::default()
                },
            );
        })
}

fn build_info_section() -> impl ElementBuilder {
    let example_code = r#"let scroll_ref = ScrollRef::new();

scroll()
    .bind(&scroll_ref)
    .child(
        div().id("card-0").child(...)
    )

// Scroll to element by ID
scroll_ref.scroll_to("card-0");"#;

    div()
        .max_w(500.0)
        .bg(Color::rgba(0.12, 0.12, 0.16, 0.8))
        .rounded(12.0)
        .p(10.0)
        .flex_col()
        .gap(6.0)
        .child(
            h4("How it works")
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE),
        )
        .child(
            code(example_code)
                .syntax(SyntaxConfig::new(RustHighlighter::new()))
                .font_size(12.0)
                .w_full(),
        )
}
