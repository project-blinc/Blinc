#![cfg(feature = "headless")]
//! How a cn number input looks: one bordered piece with a − and a + around a
//! borderless field. The steps tint on hover and press and dim where a step
//! would change nothing; all of it is rules.

use blinc_cn::number_input;
use blinc_core::Color;
use blinc_core::reactive::{Signal, signal};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 200;
const H: u32 = 64;

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(W, H).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    Some(ui)
}

fn mount(ui: &mut Headless, start: f64, min: f64, max: f64) -> Signal<f64> {
    let state = signal(start);
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .items_center()
            .justify_center()
            .child(number_input(state).min(min).max(max).step(1.0).precision(0))
    });
    ui.leave().settle();
    state
}

fn fill(ui: &Headless, class: &str) -> Option<Color> {
    let node = ui.find(class).expect("the element");
    match &ui.tree().get_render_node(node).unwrap().props.background {
        Some(blinc_core::Brush::Solid(c)) => Some(*c),
        _ => None,
    }
}

#[test]
fn the_steps_tint_on_hover_and_press() {
    let Some(mut ui) = session() else { return };
    mount(&mut ui, 5.0, 0.0, 10.0);
    let alpha = |ui: &Headless| fill(ui, ".cn-number-input-step--inc").map_or(-1.0, |c| c.a);
    assert!(alpha(&ui) < 0.01);
    ui.assert_snapshot("cn_number_input_idle");

    ui.hover(".cn-number-input-step--inc").settle();
    assert!(
        (alpha(&ui) - 0.06).abs() < 0.02,
        "hover tint: {}",
        alpha(&ui)
    );
    ui.press(".cn-number-input-step--inc").settle();
    assert!(
        (alpha(&ui) - 0.10).abs() < 0.02,
        "press tint: {}",
        alpha(&ui)
    );
}

#[test]
fn a_step_that_changes_nothing_is_dimmed_and_inert() {
    let Some(mut ui) = session() else { return };
    let state = mount(&mut ui, 10.0, 0.0, 10.0);
    ui.hover(".cn-number-input-step--inc").settle();
    assert!(
        fill(&ui, ".cn-number-input-step--inc").map_or(0.0, |c| c.a) < 0.01,
        "a step at the bound tinted on hover"
    );
    ui.assert_snapshot("cn_number_input_at_max");
    ui.click(".cn-number-input-step--inc").settle();
    assert_eq!(state.get(), 10.0);
    ui.click(".cn-number-input-step--dec").settle();
    assert_eq!(state.get(), 9.0);
}

#[test]
fn the_field_is_clear_and_borderless() {
    let Some(mut ui) = session() else { return };
    mount(&mut ui, 5.0, 0.0, 10.0);
    assert!(
        fill(&ui, ".cn-number-input-field").map_or(0.0, |c| c.a) < 0.01,
        "idle: {:?}",
        fill(&ui, ".cn-number-input-field")
    );
    ui.hover(".cn-number-input-field").settle();
    assert!(
        fill(&ui, ".cn-number-input-field").map_or(0.0, |c| c.a) < 0.01,
        "hover: {:?}",
        fill(&ui, ".cn-number-input-field")
    );
    ui.click(".cn-number-input-field").settle();
    assert!(fill(&ui, ".cn-number-input-field").map_or(0.0, |c| c.a) < 0.01);
}

fn mount_disabled(ui: &mut Headless, start: f64) -> Signal<f64> {
    let state = signal(start);
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .items_center()
            .justify_center()
            .child(number_input(state).min(0.0).max(10.0).disabled(true))
    });
    ui.leave().settle();
    state
}

#[test]
fn the_wheel_steps_the_value_by_whole_steps() {
    let Some(mut ui) = session() else { return };
    let state = mount(&mut ui, 5.0, 0.0, 10.0);
    ui.hover(".cn-number-input-field").settle();

    // One line, up: one step up.
    ui.scroll(0.0, 40.0).settle();
    assert_eq!(state.get(), 6.0);
    // Trackpad deltas accumulate until a whole step.
    ui.scroll(0.0, 15.0).scroll(0.0, 15.0).settle();
    assert_eq!(state.get(), 6.0, "a partial step changed the value");
    ui.scroll(0.0, 15.0).settle();
    assert_eq!(state.get(), 7.0);
    // Down steps down; reversing drops the remainder of the other direction.
    ui.scroll(0.0, 20.0).scroll(0.0, -40.0).settle();
    assert_eq!(state.get(), 6.0);
    // Horizontal gestures do nothing.
    ui.scroll(80.0, 10.0).settle();
    assert_eq!(state.get(), 6.0);
    // The bounds hold.
    for _ in 0..12 {
        ui.scroll(0.0, 40.0);
    }
    ui.settle();
    assert_eq!(state.get(), 10.0);
}

#[test]
fn a_disabled_number_input_ignores_the_wheel() {
    let Some(mut ui) = session() else { return };
    let state = mount_disabled(&mut ui, 5.0);
    ui.hover(".cn-number-input-field").settle();
    ui.scroll(0.0, 80.0).settle();
    assert_eq!(state.get(), 5.0);
}

#[test]
fn a_bound_disabled_follows_its_signal_in_place() {
    let Some(mut ui) = session() else { return };
    let state = signal(5.0);
    let locked = signal(true);
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .items_center()
            .justify_center()
            .child(number_input(state).min(0.0).max(10.0).disabled(locked))
    });
    ui.leave().settle();
    let disabled = |ui: &Headless, selector: &str| {
        let node = ui.find(selector).expect(selector);
        ui.tree().element_registry().has_class(node, "disabled=")
    };
    assert!(disabled(&ui, ".cn-number-input-group"));
    assert!(disabled(&ui, ".cn-number-input-field"));
    ui.click(".cn-number-input-step--inc").settle();
    ui.hover(".cn-number-input-field")
        .scroll(0.0, 40.0)
        .settle();
    assert_eq!(state.get(), 5.0, "a disabled input changed");

    locked.set(false);
    ui.settle();
    assert!(!disabled(&ui, ".cn-number-input-group"));
    assert!(!disabled(&ui, ".cn-number-input-field"));
    ui.click(".cn-number-input-step--inc").settle();
    assert_eq!(state.get(), 6.0, "re-enabled, the step did nothing");
    ui.hover(".cn-number-input-field")
        .scroll(0.0, 40.0)
        .settle();
    assert_eq!(state.get(), 7.0, "re-enabled, the wheel did nothing");
}

/// Each size has its own row height and stepper width, and names itself in
/// `data-size` for rules to key on.
#[test]
fn the_sizes_have_their_own_scale() {
    use blinc_cn::InputSize;
    if !Headless::available() {
        return;
    }
    let Ok(mut ui) = Headless::new(360, 64) else {
        return;
    };
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    let sizes = [InputSize::Small, InputSize::Medium, InputSize::Large];
    ui.mount(move |_| {
        div()
            .w(360.0)
            .h(64.0)
            .flex_row()
            .items_center()
            .justify_center()
            .gap(2.0)
            .children(sizes.into_iter().map(|size| {
                number_input(signal(5.0))
                    .min(0.0)
                    .max(9.0)
                    .precision(0)
                    .size(size)
            }))
    });
    ui.leave().settle();

    let tree = ui.tree();
    let registry = tree.element_registry();
    let mut groups = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(n) = stack.pop() {
        if registry.has_class(n, "cn-number-input-group") {
            groups.push(n);
        }
        let mut kids = tree.layout_tree.children(n);
        kids.reverse();
        stack.extend(kids);
    }
    assert_eq!(groups.len(), 3);
    for (node, (name, row, step)) in
        groups
            .into_iter()
            .zip([("sm", 24.0, 24.0), ("md", 32.0, 32.0), ("lg", 40.0, 36.0)])
    {
        assert_eq!(registry.attribute(node, "data-size").as_deref(), Some(name));
        let b = tree.get_absolute_bounds(node).unwrap();
        assert!((b.height - row).abs() < 0.5, "{name} row {}", b.height);
        let first_step = tree.layout_tree.children(node)[0];
        let s = tree.get_absolute_bounds(first_step).unwrap();
        assert!((s.width - step).abs() < 0.5, "{name} step {}", s.width);
    }
    ui.assert_snapshot("cn_number_input_sizes");
}
