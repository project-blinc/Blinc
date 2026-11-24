#![cfg(feature = "headless")]
//! Which cn components carry a shadow once the stylesheet is applied. The
//! stylesheet gives shadows to the filled button variants, the outline variant, the
//! card, the switch thumb, the kbd, the slider thumb and the accordion; the
//! rest are flat by design.

use blinc_cn::{ButtonVariant, accordion, button, card, kbd, slider, switch};
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(320, 200).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    Some(ui)
}

fn shadows(ui: &Headless, class: &str) -> usize {
    let node = ui.find(class).unwrap_or_else(|| panic!("no `{class}`"));
    // `box-shadow: none` is a single clear layer.
    ui.tree()
        .get_render_node(node)
        .unwrap()
        .props
        .shadow
        .iter()
        .filter(|s| s.color.a > 0.0)
        .count()
}

fn flag(on: bool) -> State<bool> {
    State::new(signal::<bool>(on), global_graph(), global_dirty_flag())
}

#[test]
fn buttons_carry_the_shadow_their_variant_names() {
    let Some(mut ui) = session() else { return };
    ui.mount(|_| {
        div()
            .w(320.0)
            .h(200.0)
            .flex_col()
            .gap(2.0)
            .child(button("Primary"))
            .child(button("Outline").variant(ButtonVariant::Outline))
            .child(button("Ghost").variant(ButtonVariant::Ghost))
    });
    ui.leave().settle();
    assert!(shadows(&ui, ".cn-button--primary") > 0, "primary is flat");
    assert!(shadows(&ui, ".cn-button--outline") > 0, "outline is flat");
    assert_eq!(shadows(&ui, ".cn-button--ghost"), 0);
}

#[test]
fn surfaces_and_thumbs_carry_a_shadow() {
    let Some(mut ui) = session() else { return };
    let on = flag(true);
    let level = State::new(signal::<f32>(0.5), global_graph(), global_dirty_flag());
    ui.mount(move |_| {
        div()
            .w(320.0)
            .h(200.0)
            .flex_col()
            .gap(2.0)
            .child(card())
            .child(switch(on.signal()))
            .child(kbd("K"))
            .child(slider(level.signal()))
            .child(accordion())
    });
    ui.leave().settle();
    for class in [
        ".cn-card",
        ".cn-switch-thumb",
        ".cn-kbd",
        ".cn-slider-thumb",
        ".cn-accordion",
    ] {
        assert!(shadows(&ui, class) > 0, "`{class}` is flat");
    }
}

#[test]
fn triggers_carry_the_small_shadow_until_disabled() {
    let Some(mut ui) = session() else { return };
    let value = |s: &str| {
        State::new(
            signal::<String>(s.into()),
            global_graph(),
            global_dirty_flag(),
        )
    };
    let (a, b) = (value(""), value(""));
    ui.mount(move |_| {
        div()
            .w(320.0)
            .h(300.0)
            .flex_col()
            .gap(2.0)
            .child(blinc_cn::select(a.signal()).option("a", "A"))
            .child(blinc_cn::combobox(b.signal()).option("a", "A"))
            .child(blinc_cn::dropdown_menu("Menu").item("x", || {}))
    });
    ui.leave().settle();
    for class in [
        ".cn-select-trigger",
        ".cn-combobox-trigger",
        ".cn-dropdown-trigger",
    ] {
        assert!(shadows(&ui, class) > 0, "`{class}` is flat");
    }

    let (a, b) = (value(""), value(""));
    ui.mount(move |_| {
        div()
            .w(320.0)
            .h(300.0)
            .flex_col()
            .gap(2.0)
            .child(blinc_cn::select(a.signal()).option("a", "A").disabled(true))
            .child(
                blinc_cn::combobox(b.signal())
                    .option("a", "A")
                    .disabled(true),
            )
    });
    ui.leave().settle();
    for class in [".cn-select-trigger", ".cn-combobox-trigger"] {
        assert_eq!(
            shadows(&ui, class),
            0,
            "a disabled `{class}` kept its shadow"
        );
    }
}

#[test]
fn form_fields_carry_the_small_shadow_until_disabled() {
    let Some(mut ui) = session() else { return };
    let text = blinc_layout::widgets::text_input::text_input_data();
    let area = blinc_layout::widgets::text_area::text_area_state();
    let n = State::new(signal::<f64>(3.0), global_graph(), global_dirty_flag());
    let (t2, a2, n2) = (text.clone(), area.clone(), n.clone());
    ui.mount(move |_| {
        div()
            .w(320.0)
            .h(300.0)
            .flex_col()
            .gap(2.0)
            .child(blinc_cn::input(&t2))
            .child(blinc_cn::textarea(&a2))
            .child(blinc_cn::number_input(n2.signal()))
    });
    ui.leave().settle();
    for class in [".cn-input", ".cn-textarea", ".cn-number-input-group"] {
        assert!(shadows(&ui, class) > 0, "`{class}` is flat");
    }

    ui.mount(move |_| {
        div()
            .w(320.0)
            .h(300.0)
            .flex_col()
            .gap(2.0)
            .child(blinc_cn::input(&text).disabled(true))
            .child(blinc_cn::textarea(&area).disabled(true))
            .child(blinc_cn::number_input(n.signal()).disabled(true))
    });
    ui.leave().settle();
    for class in [".cn-input", ".cn-textarea", ".cn-number-input-group"] {
        assert_eq!(
            shadows(&ui, class),
            0,
            "a disabled `{class}` kept its shadow"
        );
    }
}
