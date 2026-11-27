#![cfg(feature = "headless")]
//! An element with an id inside a stateful draws what the state callback
//! returns, through the state passes that pointer events run.

use blinc_core::{Brush, Color};
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_theme::{BlincTheme, ColorScheme};

fn color(ui: &Headless, selector: &str) -> Option<Color> {
    let node = ui.find(selector).expect("node");
    let render = ui.tree().get_render_node(node).expect("render node");
    match render.props.background {
        Some(Brush::Solid(color)) => Some(color),
        _ => None,
    }
}

#[test]
fn an_element_with_an_id_follows_its_stateful() {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return;
    }
    let mut ui = Headless::new(300, 100).expect("session");
    ui.theme(BlincTheme::bundle(), ColorScheme::Light);
    ui.mount(|_| {
        div()
            .w(300.0)
            .h(100.0)
            .child(stateful::<ButtonState>().on_state(|ctx| {
                let color = match ctx.state() {
                    ButtonState::Hovered => Color::BLUE,
                    _ => Color::RED,
                };
                div()
                    .w(200.0)
                    .h(60.0)
                    .child(div().id("swatch").w(80.0).h(40.0).bg(color))
            }))
    });
    ui.leave().settle();
    assert_eq!(color(&ui, "#swatch"), Some(Color::RED));

    ui.move_to(10.0, 10.0).settle();
    assert_eq!(color(&ui, "#swatch"), Some(Color::BLUE));

    // A second move inside runs the state pass again over the updated node.
    ui.move_to(20.0, 20.0).settle();
    assert_eq!(color(&ui, "#swatch"), Some(Color::BLUE));

    ui.leave().settle();
    assert_eq!(color(&ui, "#swatch"), Some(Color::RED));
}
