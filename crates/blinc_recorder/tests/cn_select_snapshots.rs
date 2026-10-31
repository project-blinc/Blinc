#![cfg(feature = "headless")]
//! How a cn select trigger and a dropdown menu's default trigger look idle,
//! hovered, pressed, open and disabled. Their fills and borders are rules, so
//! each state is checked against the token or tint its rule names.

use blinc_cn::{combobox, dropdown_menu, popover, select};
use blinc_core::Color;
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::div::div;
use blinc_recorder::{Golden, Headless};
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

const W: u32 = 260;
const H: u32 = 120;

fn session() -> Option<Headless> {
    session_sized(W, H)
}

fn session_sized(w: u32, h: u32) -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(w, h).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    Some(ui)
}

fn mount_select(ui: &mut Headless, disabled: bool) {
    let value = State::new(
        signal::<String>(String::new()),
        global_graph(),
        global_dirty_flag(),
    );
    ui.mount(move |_| {
        div().w(W as f32).h(H as f32).p(4.0).child(
            select(&value)
                .option("a", "Apple")
                .option("b", "Banana")
                .placeholder("Fruit")
                .w(180.0)
                .disabled(disabled),
        )
    });
    ui.leave().settle();
}

fn props(ui: &Headless, class: &str) -> (Option<Color>, Option<Color>) {
    let node = ui.find(class).expect("the trigger");
    let p = &ui.tree().get_render_node(node).unwrap().props;
    let fill = match &p.background {
        Some(blinc_core::Brush::Solid(c)) => Some(*c),
        _ => None,
    };
    (fill, p.border_color)
}

fn same(a: Option<Color>, b: Color) -> bool {
    a.is_some_and(|a| {
        (a.r - b.r).abs() < 0.02 && (a.g - b.g).abs() < 0.02 && (a.b - b.b).abs() < 0.02
    })
}

fn alpha(a: Option<Color>) -> f32 {
    a.map_or(-1.0, |c| c.a)
}

#[test]
fn the_select_trigger_follows_its_rules() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    mount_select(&mut ui, false);
    let (fill, border) = props(&ui, ".cn-select-trigger");
    assert!(alpha(fill) < 0.01, "idle fill is not clear: {fill:?}");
    assert!(same(border, theme.color(ColorToken::Border)));
    ui.assert_snapshot("cn_select_idle");

    ui.hover(".cn-select-trigger").settle();
    let (fill, _) = props(&ui, ".cn-select-trigger");
    assert!((alpha(fill) - 0.05).abs() < 0.02, "hover tint: {fill:?}");

    ui.press(".cn-select-trigger").settle();
    let (fill, _) = props(&ui, ".cn-select-trigger");
    assert!((alpha(fill) - 0.10).abs() < 0.02, "press tint: {fill:?}");
    ui.release().settle();

    let (_, border) = props(&ui, ".cn-select-trigger");
    assert!(
        same(border, theme.color(ColorToken::BorderHover)),
        "an open select did not darken its border: {border:?}"
    );
    ui.assert_snapshot("cn_select_open");
}

#[test]
fn a_disabled_select_does_not_respond() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    mount_select(&mut ui, true);
    let (fill, border) = props(&ui, ".cn-select-trigger");
    assert!(same(fill, theme.color(ColorToken::InputBgDisabled)));
    assert!(same(border, theme.color(ColorToken::BorderSecondary)));
    ui.assert_snapshot("cn_select_disabled");
    ui.hover(".cn-select-trigger").settle();
    assert_eq!(props(&ui, ".cn-select-trigger"), (fill, border));
}

#[test]
fn the_dropdown_trigger_tints_on_hover_and_press() {
    let Some(mut ui) = session() else { return };
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .p(4.0)
            .child(dropdown_menu("Actions").item("Edit", || {}))
    });
    ui.leave().settle();
    let (fill, _) = props(&ui, ".cn-dropdown-trigger");
    assert!(alpha(fill) < 0.01, "idle fill is not clear: {fill:?}");
    ui.assert_snapshot("cn_dropdown_trigger_idle");

    ui.hover(".cn-dropdown-trigger").settle();
    let (fill, _) = props(&ui, ".cn-dropdown-trigger");
    assert!((alpha(fill) - 0.05).abs() < 0.02, "hover tint: {fill:?}");

    ui.press(".cn-dropdown-trigger").settle();
    let (fill, _) = props(&ui, ".cn-dropdown-trigger");
    assert!((alpha(fill) - 0.10).abs() < 0.02, "press tint: {fill:?}");
}

fn mount_combobox(ui: &mut Headless, disabled: bool) {
    let value = State::new(
        signal::<String>(String::new()),
        global_graph(),
        global_dirty_flag(),
    );
    ui.mount(move |_| {
        div().w(W as f32).h(H as f32).p(4.0).child(
            combobox(&value)
                .option("a", "Apple")
                .option("b", "Banana")
                .placeholder("Fruit")
                .w(180.0)
                .disabled(disabled),
        )
    });
    ui.leave().settle();
}

#[test]
fn the_combobox_trigger_follows_its_rules() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    mount_combobox(&mut ui, false);
    let (fill, border) = props(&ui, ".cn-combobox-trigger");
    assert!(same(fill, theme.color(ColorToken::Surface)));
    assert!(same(border, theme.color(ColorToken::Border)));
    ui.assert_snapshot("cn_combobox_idle");

    ui.hover(".cn-combobox-trigger").settle();
    let (_, border) = props(&ui, ".cn-combobox-trigger");
    assert!(same(border, theme.color(ColorToken::BorderHover)));

    ui.click(".cn-combobox-trigger").settle();
    assert!(
        ui.find(".cn-combobox-content").is_some(),
        "the list did not open"
    );
    let (_, border) = props(&ui, ".cn-combobox-trigger");
    assert!(
        same(border, theme.color(ColorToken::BorderFocus)),
        "an open combobox did not take the focus border: {border:?} (hover {:?}, focus {:?}, idle {:?})",
        theme.color(ColorToken::BorderHover),
        theme.color(ColorToken::BorderFocus),
        theme.color(ColorToken::Border)
    );
}

#[test]
fn a_disabled_combobox_does_not_respond() {
    let Some(mut ui) = session() else { return };
    let theme = ThemeState::get();
    mount_combobox(&mut ui, true);
    let (fill, border) = props(&ui, ".cn-combobox-trigger");
    assert!(same(fill, theme.color(ColorToken::InputBgDisabled)));
    assert!(same(border, theme.color(ColorToken::BorderSecondary)));
    ui.hover(".cn-combobox-trigger").settle();
    assert_eq!(props(&ui, ".cn-combobox-trigger"), (fill, border));
}

#[test]
fn a_long_combobox_list_stays_inside_its_popup() {
    let Some(mut ui) = session_sized(W, 420) else {
        return;
    };
    let value = State::new(
        signal::<String>(String::new()),
        global_graph(),
        global_dirty_flag(),
    );
    ui.mount(move |_| {
        let mut c = combobox(&value).placeholder("Framework").w(180.0);
        for i in 0..10 {
            c = c.option(format!("v{i}"), format!("Option {i}"));
        }
        div().w(W as f32).h(420.0).p(4.0).child(c)
    });
    ui.leave().settle();
    ui.click(".cn-combobox-trigger").settle();
    assert!(ui.find(".cn-combobox-content").is_some(), "the list opened");
    // The search field is focused and its caret blinks on the wall clock.
    let frame = ui.snapshot().expect("snapshot");
    Golden::in_crate()
        .tolerance(24)
        .check("cn_combobox_long_list", &frame);

    // Scrolled, the rows stay below the search field.
    ui.move_to(60.0, 200.0);
    ui.scroll(0.0, -60.0).settle();
    let frame = ui.snapshot().expect("snapshot");
    Golden::in_crate()
        .tolerance(24)
        .check("cn_combobox_long_list_scrolled", &frame);
}

fn open_overlays() -> usize {
    // Not counting one that is still playing its exit.
    blinc_layout::overlay_state::overlay_stack()
        .lock()
        .unwrap()
        .iter_bottom_up()
        .filter(|e| !e.exiting)
        .count()
}

#[test]
fn clicking_the_dropdown_trigger_again_closes_the_menu() {
    let Some(mut ui) = session() else { return };
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .p(4.0)
            .child(dropdown_menu("Actions").item("Edit", || {}))
    });
    ui.leave().settle();
    let before = open_overlays();
    ui.click(".cn-dropdown-trigger").settle();
    assert_eq!(open_overlays(), before + 1, "the first click did not open");
    ui.click(".cn-dropdown-trigger").settle();
    assert_eq!(open_overlays(), before, "the second click reopened it");
    ui.click(".cn-dropdown-trigger").settle();
    assert_eq!(open_overlays(), before + 1, "it would not open again");
}

#[test]
fn clicking_the_popover_trigger_again_closes_it() {
    let Some(mut ui) = session() else { return };
    ui.mount(move |_| {
        div().w(W as f32).h(H as f32).p(4.0).child(
            popover(|_open| div().class("pop-trigger").w(80.0).h(24.0))
                .content(|| div().w(60.0).h(40.0)),
        )
    });
    ui.leave().settle();
    let before = open_overlays();
    ui.click(".pop-trigger").settle();
    assert_eq!(open_overlays(), before + 1, "the first click did not open");
    ui.click(".pop-trigger").settle();
    assert_eq!(open_overlays(), before, "the second click reopened it");
}

/// The rotation, in degrees, painted on the first `.cn-chevron` under `parent`.
fn chevron_degrees(ui: &Headless, parent: &str) -> f32 {
    let node = ui.find(parent).expect("the trigger");
    let chevron = ui
        .tree()
        .layout_tree
        .children(node)
        .into_iter()
        .find(|c| {
            ui.tree()
                .get_render_node(*c)
                .is_some_and(|n| n.props.transform.is_some())
                || ui
                    .tree()
                    .element_registry()
                    .class_to_nodes_index()
                    .get("cn-chevron")
                    .is_some_and(|set| set.contains(c))
        })
        .expect("a caret");
    match ui
        .tree()
        .get_render_node(chevron)
        .and_then(|n| n.props.transform.clone())
    {
        Some(blinc_core::Transform::Affine2D(a)) => {
            a.elements[1].atan2(a.elements[0]).to_degrees().abs()
        }
        _ => 0.0,
    }
}

/// Opening a trigger turns its caret through the angles in between, a frame at
/// a time, and closing turns it back the same way.
fn assert_caret_spins(ui: &mut Headless, trigger: &str) {
    assert!(chevron_degrees(ui, trigger) < 1.0, "caret starts upright");
    ui.click(trigger);
    let mut seen = Vec::new();
    for _ in 0..30 {
        ui.advance(16);
        seen.push(chevron_degrees(ui, trigger));
    }
    assert!(
        seen.iter().any(|d| *d > 10.0 && *d < 170.0),
        "the caret flipped without easing: {seen:?}"
    );
    assert!(
        (seen[seen.len() - 1] - 180.0).abs() < 1.0,
        "the caret did not end inverted: {seen:?}"
    );
}

#[test]
fn the_select_caret_spins_open() {
    let Some(mut ui) = session_sized(W, 240) else {
        return;
    };
    let value = State::new(
        signal::<String>(String::new()),
        global_graph(),
        global_dirty_flag(),
    );
    ui.mount(move |_| {
        div().w(W as f32).h(240.0).p(4.0).child(
            select(&value)
                .option("a", "Apple")
                .placeholder("Fruit")
                .w(180.0),
        )
    });
    ui.leave().settle();
    assert_caret_spins(&mut ui, ".cn-select-trigger");
}

#[test]
fn the_combobox_caret_spins_open() {
    let Some(mut ui) = session_sized(W, 240) else {
        return;
    };
    let value = State::new(
        signal::<String>(String::new()),
        global_graph(),
        global_dirty_flag(),
    );
    ui.mount(move |_| {
        div().w(W as f32).h(240.0).p(4.0).child(
            combobox(&value)
                .option("a", "Apple")
                .placeholder("Fruit")
                .w(180.0),
        )
    });
    ui.leave().settle();
    assert_caret_spins(&mut ui, ".cn-combobox-trigger");
}

#[test]
fn the_accordion_caret_spins_open() {
    let Some(mut ui) = session_sized(W, 240) else {
        return;
    };
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(240.0)
            .p(4.0)
            .child(blinc_cn::accordion().item("one", "One", || div().h(20.0)))
    });
    ui.leave().settle();
    assert_caret_spins(&mut ui, ".cn-accordion-trigger");
}

#[test]
fn the_dropdown_caret_spins_open() {
    let Some(mut ui) = session_sized(W, 240) else {
        return;
    };
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(240.0)
            .p(4.0)
            .child(dropdown_menu("Actions").item("Edit", || {}))
    });
    ui.leave().settle();
    assert_caret_spins(&mut ui, ".cn-dropdown-trigger");
}
