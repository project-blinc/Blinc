//! A canvas asks for frames as its flow loop says, and a clock moves the shader's time.

use blinc_core::{RecordingContext, Size};
use blinc_layout::prelude::*;
use blinc_layout::{FlowClock, FlowLoop, take_animation_tick_request, take_needs_redraw};

fn paint(canvas: &Canvas) {
    let render = canvas.canvas_render_info().expect("a render function");
    let mut ctx = RecordingContext::new(Size::new(100.0, 100.0));
    render(
        &mut ctx,
        blinc_layout::CanvasBounds {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        },
    );
}

fn ticks(canvas: &Canvas) -> bool {
    take_animation_tick_request();
    paint(canvas);
    take_animation_tick_request()
}

fn shader(source: &str) -> blinc_core::FlowShader {
    blinc_layout::flow_shader(source).unwrap()
}

#[test]
fn the_loop_decides_whether_a_canvas_asks_for_frames() {
    let still = shader(
        "@flow s { target: fragment; input uv: builtin(uv); output color = vec4(uv.x, 0.0, 0.0, 1.0); }",
    );
    let moving = shader(
        "@flow m { target: fragment; input time: builtin(time); output color = vec4(sin(time), 0.0, 0.0, 1.0); }",
    );
    assert!(!still.animated);
    assert!(moving.animated);

    // Auto follows whether the shader reads time.
    assert!(!ticks(&canvas(|_, _| {}).flow(still.clone())));
    assert!(ticks(&canvas(|_, _| {}).flow(moving.clone())));
    // Always and Never override it.
    assert!(ticks(
        &canvas(|_, _| {})
            .flow(still.clone())
            .flow_loop(FlowLoop::Always)
    ));
    assert!(!ticks(
        &canvas(|_, _| {})
            .flow(moving.clone())
            .flow_loop(FlowLoop::Never)
    ));
    // While asks the closure each paint.
    let on = std::rc::Rc::new(std::cell::Cell::new(true));
    let flag = on.clone();
    let driven = canvas(|_, _| {})
        .flow(moving.clone())
        .flow_loop(FlowLoop::while_(move || flag.get()));
    assert!(ticks(&driven));
    on.set(false);
    assert!(!ticks(&driven));

    // A clock runs the canvas only while it plays, and a step asks for one repaint.
    let clock = FlowClock::paused();
    let stepped = canvas(|_, _| {})
        .flow(moving)
        .flow_loop(FlowLoop::Clock(clock.clone()));
    assert!(!ticks(&stepped));
    take_needs_redraw();
    clock.step(1.0 / 60.0);
    assert!(take_needs_redraw());
    clock.play();
    assert!(ticks(&stepped));
    clock.pause();
    assert!(!ticks(&stepped));
}
