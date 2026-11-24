#![cfg(feature = "headless")]
//! Field text follows its signals in place: an input's and a textarea's
//! placeholder, description and error, and the placeholder of a select, a
//! combobox and a number input. A non-empty error shows in place of the
//! description and marks the box `data-invalid`, which colours its border.

use blinc_core::reactive::{Signal, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::renderer::ElementType;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(480, 480).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    Some(ui)
}

fn shown(ui: &Headless, content: &str) -> bool {
    let tree = ui.tree();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
            && t.content == content
        {
            return true;
        }
        stack.extend(tree.layout_tree.children(id));
    }
    false
}

fn invalid(ui: &Headless, node: LayoutNodeId) -> bool {
    ui.tree()
        .element_registry()
        .attribute(node, "data-invalid")
        .is_some()
}

fn text(v: &str) -> Signal<String> {
    signal(v.to_string())
}

/// An error and a description on a field whose box is `class`: the
/// description shows until the error is non-empty, which then shows in its
/// place and marks the box invalid; clearing the error undoes both.
fn error_swaps_with_description(ui: &mut Headless, class: &str, error: Signal<String>, hint: &str) {
    let field = ui.find(class).unwrap_or_else(|| panic!("no {class}"));
    assert!(
        shown(ui, hint) && !invalid(ui, field),
        "{class} starts plain"
    );

    error.set("Required".to_string());
    ui.settle();
    assert!(shown(ui, "Required"), "{class} error not shown");
    assert!(!shown(ui, hint), "{class} description still shown");
    assert!(invalid(ui, field), "{class} box not marked invalid");
    let border = ui.tree().get_render_node(field).unwrap().props.border_color;
    let want = ThemeState::get().color(ColorToken::BorderError);
    assert!(
        border.is_some_and(|c| (c.r - want.r).abs() < 0.02 && (c.g - want.g).abs() < 0.02),
        "{class} border {border:?}, want {want:?}"
    );

    error.set(String::new());
    ui.settle();
    assert!(
        shown(ui, hint) && !shown(ui, "Required"),
        "{class} error not cleared"
    );
    assert!(!invalid(ui, field), "{class} still invalid");
}

#[test]
fn an_input_follows_its_text_signals() {
    let Some(mut ui) = session() else { return };
    let data = blinc_layout::widgets::text_input::text_input_data();
    let (placeholder, desc, error) = (text("Your name"), text("As on your passport"), text(""));
    ui.mount(move |_| {
        div().w(480.0).h(480.0).child(
            blinc_cn::input(&data)
                .w(300.0)
                .placeholder(placeholder)
                .description(desc)
                .error(error),
        )
    });
    ui.leave().settle();

    assert!(shown(&ui, "Your name"));
    placeholder.set("Full name".to_string());
    ui.settle();
    assert!(shown(&ui, "Full name"), "the placeholder did not follow");

    desc.set("As on your ID".to_string());
    ui.settle();
    assert!(
        shown(&ui, "As on your ID"),
        "the description did not follow"
    );
    error_swaps_with_description(&mut ui, ".cn-input", error, "As on your ID");
}

#[test]
fn a_textarea_follows_its_text_signals() {
    let Some(mut ui) = session() else { return };
    let state = blinc_layout::widgets::text_area::text_area_state();
    let (placeholder, error) = (text("Tell us more"), text(""));
    ui.mount(move |_| {
        div().w(480.0).h(480.0).child(
            blinc_cn::textarea(&state)
                .w(300.0)
                .placeholder(placeholder)
                .description("Optional")
                .error(error),
        )
    });
    ui.leave().settle();

    assert!(shown(&ui, "Tell us more"));
    placeholder.set("Anything else?".to_string());
    ui.settle();
    assert!(
        shown(&ui, "Anything else?"),
        "the placeholder did not follow"
    );
    error_swaps_with_description(&mut ui, ".cn-textarea", error, "Optional");
}

#[test]
fn choice_fields_follow_their_placeholders() {
    let Some(mut ui) = session() else { return };
    let (pick, search) = (text("Pick one"), text("Find one"));
    ui.mount(move |_| {
        div()
            .w(480.0)
            .h(480.0)
            .flex_col()
            .gap(4.0)
            .child(
                blinc_cn::select(signal(String::new()))
                    .option("a", "A")
                    .placeholder(pick),
            )
            .child(
                blinc_cn::combobox(signal(String::new()))
                    .option("a", "A")
                    .placeholder(search),
            )
    });
    ui.leave().settle();

    for (sig, from, to) in [
        (pick, "Pick one", "Choose"),
        (search, "Find one", "Search fruit"),
    ] {
        assert!(shown(&ui, from), "{from:?} not shown");
        sig.set(to.to_string());
        ui.settle();
        assert!(shown(&ui, to), "{from:?} did not become {to:?}");
    }
}
