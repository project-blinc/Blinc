//! Image Layer Test
//!
//! Tests the rendering order of images vs primitives (paths, backgrounds).
//! This helps debug z-order issues where images may render above/below other elements.
//!
//! **Solution for rendering elements ON TOP of images:**
//! Use `.foreground()` on any element that needs to render above images.
//! The render order is: Background primitives → Images → Foreground primitives
//!
//! Run with: cargo run -p blinc_app --example image_layer_test --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Image Layer Test".to_string(),
        width: 800,
        height: 600,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(_ctx: &WindowedContext) -> impl ElementBuilder {
    let image_path = "crates/blinc_app/examples/assets/avatar.jpg";

    div()
        .w_full()
        .h_full()
        .bg(Color::rgb(0.1, 0.1, 0.15))
        .flex_col()
        .gap(8.0)
        .p(20.0)
        .child(
            text("Image Layer Test - Testing image vs primitive z-order")
                .size(24.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE),
        )
        .child(
            div()
                .flex_row()
                .gap(16.0)
                .child(test_case_1(image_path))
                .child(test_case_2(image_path))
                .child(test_case_3(image_path))
                .child(test_case_4(image_path)),
        )
        .child(
            div()
                .flex_row()
                .gap(16.0)
                .child(test_case_5(image_path))
                .child(test_case_6(image_path))
                .child(test_border_no_image()),
        )
}

/// Test: Border without image - verify border works in general
fn test_border_no_image() -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(4.0)
        .child(text("7: Border test (small circles)").size(14.0).color(Color::WHITE))
        .child(
            div()
                .flex_row()
                .gap(8.0)
                .items_end()
                // Tiny 6px circle - like avatar status indicator
                // Normal (no foreground)
                .child(
                    div()
                        .w(6.0)
                        .h(6.0)
                        .bg(Color::GREEN)
                        .border(1.0, Color::WHITE)
                        .rounded(3.0),
                )
                // Tiny 6px - .foreground()
                .child(
                    div()
                        .w(6.0)
                        .h(6.0)
                        .bg(Color::GREEN)
                        .border(1.0, Color::WHITE)
                        .rounded(3.0)
                        .foreground(),
                )
                // 10px circle - medium size
                .child(
                    div()
                        .w(10.0)
                        .h(10.0)
                        .bg(Color::GREEN)
                        .border(1.0, Color::WHITE)
                        .rounded(5.0),
                )
                // 10px - .foreground()
                .child(
                    div()
                        .w(10.0)
                        .h(10.0)
                        .bg(Color::GREEN)
                        .border(1.0, Color::WHITE)
                        .rounded(5.0)
                        .foreground(),
                )
                // 30px circle - larger
                .child(
                    div()
                        .w(30.0)
                        .h(30.0)
                        .bg(Color::GREEN)
                        .border(2.0, Color::WHITE)
                        .rounded(15.0),
                )
                // 30px - .foreground()
                .child(
                    div()
                        .w(30.0)
                        .h(30.0)
                        .bg(Color::GREEN)
                        .border(2.0, Color::WHITE)
                        .rounded(15.0)
                        .foreground(),
                ),
        )
        .child(
            text("Pairs: 6px, 10px, 30px - normal vs .foreground()")
                .size(11.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.6)),
        )
}

/// Test 1: Image with border directly on img()
fn test_case_1(src: &str) -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(4.0)
        .child(text("1: Border on img()").size(14.0).color(Color::WHITE))
        .child(
            img(src)
                .size(100.0, 100.0)
                .cover()
                .rounded(12.0)
                .border(4.0, Color::RED),
        )
        .child(
            text("Red border directly on image")
                .size(11.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.6)),
        )
}

/// Test 2: Image with sibling overlay using .foreground()
fn test_case_2(src: &str) -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(4.0)
        .child(text("2: Sibling + .foreground()").size(14.0).color(Color::WHITE))
        .child(
            div()
                .w(100.0)
                .h(100.0)
                .relative()
                .child(img(src).size(100.0, 100.0).cover())
                .child(
                    div()
                        .w(30.0)
                        .h(30.0)
                        .bg(Color::GREEN)
                        .rounded(15.0)
                        .absolute()
                        .bottom(4.0)
                        .right(4.0)
                        .foreground(), // Required for sibling overlay on images
                ),
        )
        .child(
            text("Green circle ON TOP (uses .foreground())")
                .size(11.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.6)),
        )
}

/// Test 3: Image with sibling overlay div using foreground layer + border
fn test_case_3(src: &str) -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(4.0)
        .child(text("3: .foreground() + border").size(14.0).color(Color::WHITE))
        .child(
            div()
                .w(100.0)
                .h(100.0)
                .relative()
                .child(img(src).size(100.0, 100.0).cover())
                .child(
                    div()
                        .w(30.0)
                        .h(30.0)
                        .bg(Color::GREEN)
                        .border(2.0, Color::WHITE)
                        .rounded(15.0)
                        .absolute()
                        .bottom(4.0)
                        .right(4.0)
                        .foreground(), // Use foreground layer
                ),
        )
        .child(
            text("Green circle + white border, on TOP")
                .size(11.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.6)),
        )
}

/// Test 4: Stack with image first, overlay second
fn test_case_4(src: &str) -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(4.0)
        .child(text("4: Stack (image first)").size(14.0).color(Color::WHITE))
        .child(
            stack()
                .w(100.0)
                .h(100.0)
                .child(img(src).size(100.0, 100.0).cover())
                .child(
                    div()
                        .w(30.0)
                        .h(30.0)
                        .bg(Color::YELLOW)
                        .rounded(15.0)
                        .absolute()
                        .bottom(4.0)
                        .right(4.0),
                ),
        )
        .child(
            text("Yellow circle should be ON TOP (stack order)")
                .size(11.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.6)),
        )
}

/// Test 5: Plain div background under image
fn test_case_5(src: &str) -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(4.0)
        .child(text("5: Bg div under image").size(14.0).color(Color::WHITE))
        .child(
            div()
                .w(100.0)
                .h(100.0)
                .bg(Color::MAGENTA)
                .rounded(8.0)
                .flex_row()
                .items_center()
                .justify_center()
                .child(img(src).size(80.0, 80.0).cover().rounded(8.0)),
        )
        .child(
            text("Magenta bg should show as border around image")
                .size(11.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.6)),
        )
}

/// Test 6: Text over image (wrapped in div for positioning)
fn test_case_6(src: &str) -> impl ElementBuilder {
    div()
        .flex_col()
        .gap(4.0)
        .child(text("6: Text over image").size(14.0).color(Color::WHITE))
        .child(
            div()
                .w(100.0)
                .h(100.0)
                .relative()
                .child(img(src).size(100.0, 100.0).cover().rounded(8.0))
                .child(
                    div()
                        .absolute()
                        .bottom(8.0)
                        .left(8.0)
                        .child(
                            text("HELLO")
                                .size(16.0)
                                .weight(FontWeight::Bold)
                                .color(Color::WHITE),
                        ),
                ),
        )
        .child(
            text("Text should appear ON TOP of image")
                .size(11.0)
                .color(Color::rgba(1.0, 1.0, 1.0, 0.6)),
        )
}
