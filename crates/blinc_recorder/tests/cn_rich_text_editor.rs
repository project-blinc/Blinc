#![cfg(feature = "headless")]
//! The rich text editor takes its colours from the cn stylesheet: the document text, markers,
//! quote bar, divider, code, selection and toolbar; a fixed palette still colours the text.

use blinc_core::{Brush, Color};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

const W: u32 = 420;
/// The width of the toolbar tests' window.
const WIDE: u32 = 520;
const H: u32 = 420;

fn session(scheme: ColorScheme) -> Option<Headless> {
    session_sized(scheme, W)
}

/// A session `width` wide.
fn session_sized(scheme: ColorScheme, width: u32) -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(width, H).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        scheme,
    );
    Some(ui)
}

/// Every node with `class`, in document order.
fn all(ui: &Headless, class: &str) -> Vec<LayoutNodeId> {
    let tree = ui.tree();
    let registry = tree.element_registry();
    let mut found = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        if registry.has_class(node, class) {
            found.push(node);
        }
        let mut children = tree.layout_tree.children(node);
        children.reverse();
        stack.extend(children);
    }
    found
}

fn one(ui: &Headless, class: &str) -> LayoutNodeId {
    *all(ui, class)
        .first()
        .unwrap_or_else(|| panic!("no .{class}"))
}

fn background(ui: &Headless, node: LayoutNodeId) -> Option<Color> {
    match &ui.tree().get_render_node(node).unwrap().props.background {
        Some(Brush::Solid(c)) => Some(*c),
        _ => None,
    }
}

fn close(a: Option<Color>, b: Color) -> bool {
    a.is_some_and(|a| {
        (a.r - b.r).abs() < 0.02 && (a.g - b.g).abs() < 0.02 && (a.b - b.b).abs() < 0.02
    })
}

/// The colour of the text drawn in a node's box, its brightest pixel in the
/// dark scheme or its darkest in the light, and the box's own background.
fn ink(ui: &mut Headless, node: LayoutNodeId, dark: bool) -> ([u8; 3], [u8; 3]) {
    let b = ui.tree().get_absolute_bounds(node).unwrap();
    let frame = ui.snapshot().unwrap();
    let px = |x: u32, y: u32| {
        let p = frame.get_pixel(x, y).unwrap();
        [p[0], p[1], p[2]]
    };
    let sum = |c: [u8; 3]| c.iter().map(|v| *v as u32).sum::<u32>();
    let mut best: Option<[u8; 3]> = None;
    for y in b.y as u32..(b.y + b.height) as u32 {
        for x in b.x as u32..(b.x + b.width) as u32 {
            let p = px(x, y);
            if best.is_none_or(|b| {
                if dark {
                    sum(p) > sum(b)
                } else {
                    sum(p) < sum(b)
                }
            }) {
                best = Some(p);
            }
        }
    }
    (best.unwrap(), px(b.x as u32 + 1, b.y as u32 + 1))
}

fn token(t: ColorToken) -> [u8; 3] {
    let c = ThemeState::get().color(t);
    [
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
    ]
}

/// Whether `px` is `want` drawn over `bg` at one coverage for every channel,
/// mostly covered: small glyphs never reach full coverage.
fn drawn_in(px: [u8; 3], want: [u8; 3], bg: [u8; 3]) -> bool {
    let coverage: Vec<f32> = (0..3)
        .filter(|&i| want[i].abs_diff(bg[i]) > 30)
        .map(|i| (px[i] as f32 - bg[i] as f32) / (want[i] as f32 - bg[i] as f32))
        .collect();
    let (lo, hi) = coverage
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), c| (lo.min(*c), hi.max(*c)));
    !coverage.is_empty() && lo > 0.7 && hi < 1.1 && hi - lo < 0.08
}

const PROSE: &str = "Plain prose here\n\n- listed\n\n> quoted\n\n---\n\nsome `code` inline";

/// A rich text editor following the theme: the document text, markers,
/// quote bar, divider and code take their rules' colours.
#[test]
fn rich_text_follows_the_theme_in_both_schemes() {
    use blinc_cn::components::rich_text_editor::{
        RichDocument, RichTextTheme, rich_text_editor, rich_text_state,
    };
    for (scheme, dark) in [(ColorScheme::Dark, true), (ColorScheme::Light, false)] {
        let Some(mut ui) = session(scheme) else {
            return;
        };
        let theme = ThemeState::get();
        let state = rich_text_state(RichDocument::from_markdown(PROSE, Color::TRANSPARENT));
        ui.mount(move |_| {
            div()
                .w(W as f32)
                .h(H as f32)
                .padding(blinc_layout::units::px(12.0))
                .child(rich_text_editor(&state, RichTextTheme::default(), 380.0))
        });
        ui.leave().settle();

        let rules = |ui: &Headless, class| background(ui, one(ui, class));
        assert!(close(
            rules(&ui, "blinc-rich-text-quote-bar"),
            theme.color(ColorToken::BorderSecondary)
        ));
        assert!(close(
            rules(&ui, "blinc-rich-text-divider"),
            theme.color(ColorToken::Border)
        ));
        // The document's most contrasting ink is its body text.
        for (what, node, want) in [
            (
                "prose",
                one(&ui, "blinc-rich-text"),
                ColorToken::TextPrimary,
            ),
            (
                "marker",
                one(&ui, "blinc-rich-text-marker"),
                ColorToken::TextTertiary,
            ),
            (
                "code",
                one(&ui, "blinc-rich-text-code"),
                ColorToken::Warning,
            ),
        ] {
            let ((got, bg), want) = (ink(&mut ui, node, dark), token(want));
            assert!(
                drawn_in(got, want, bg),
                "{scheme:?} {what} drawn {got:?} over {bg:?}, want {want:?}"
            );
        }
    }
}

/// The selection toolbar takes its rules' colours; a fixed palette from
/// `RichTextTheme::dark()` still colours the text.
#[test]
fn the_rich_text_toolbar_and_a_fixed_palette() {
    use blinc_cn::components::rich_text_editor::{
        DocPosition, RichDocument, RichTextTheme, Selection, rich_text_editor, rich_text_state,
    };
    let Some(mut ui) = session_sized(ColorScheme::Light, WIDE) else {
        return;
    };
    let theme = ThemeState::get();
    let state = rich_text_state(RichDocument::from_markdown(
        "Select this text",
        Color::TRANSPARENT,
    ));
    {
        let mut d = state.lock().unwrap();
        d.focused = true;
        d.selection = Some(Selection {
            anchor: DocPosition::new(0, 0, 0),
            head: DocPosition::new(0, 0, 6),
        });
    }
    let fixed = rich_text_state(RichDocument::from_markdown(
        "Fixed palette",
        Color::TRANSPARENT,
    ));
    ui.mount(move |_| {
        div()
            .w(WIDE as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(12.0))
            .flex_col()
            .gap(30.0)
            .child(rich_text_editor(&state, RichTextTheme::default(), 380.0))
            .child(rich_text_editor(&fixed, RichTextTheme::dark(), 380.0))
    });
    ui.leave().settle();

    // The toolbar is a notch filled with the elevated surface, its peak toward the selection.
    let toolbar = one(&ui, "blinc-rte-toolbar");
    let b = ui.tree().get_absolute_bounds(toolbar).unwrap();
    let fill = ui
        .snapshot()
        .unwrap()
        .get_pixel((b.x + 4.0) as u32, (b.y + b.height * 0.6) as u32)
        .unwrap();
    let want = theme.color(ColorToken::SurfaceElevated);
    assert!(
        close(
            Some(Color::rgb(
                fill[0] as f32 / 255.0,
                fill[1] as f32 / 255.0,
                fill[2] as f32 / 255.0
            )),
            want
        ),
        "toolbar fill {fill:?}"
    );
    assert!(close(
        background(&ui, one(&ui, "blinc-rich-text-selection")),
        theme.color(ColorToken::Selection)
    ));
    // Five marks, link and close, and the colour picker's swatch beside them.
    assert_eq!(all(&ui, "blinc-rte-button").len(), 7);
    assert_eq!(all(&ui, "blinc-rte-color").len(), 1);

    // The dark palette's near-white text, on the light theme.
    let documents = all(&ui, "blinc-rich-text");
    let (brightest, _) = ink(&mut ui, documents[1], true);
    assert!(
        brightest.iter().all(|&c| c > 220),
        "fixed palette text {brightest:?}"
    );
    ui.assert_snapshot("layout_rich_text_rules");
}

/// A press on a node, at its centre.
fn press(ui: &mut Headless, node: LayoutNodeId) {
    let b = ui.tree().get_absolute_bounds(node).unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0);
    ui.mouse_down();
    ui.mouse_up();
    ui.settle();
}

/// An editor over "Select this text", with "Select" selected.
fn selected_editor(
    placement: blinc_cn::components::rich_text_editor::ToolbarPlacement,
) -> Option<(
    Headless,
    blinc_cn::components::rich_text_editor::RichTextState,
)> {
    use blinc_cn::components::rich_text_editor::{
        DocPosition, RichDocument, RichTextTheme, Selection, rich_text_editor, rich_text_state,
    };
    // Wide enough that the floating toolbar's controls fit without scrolling.
    let mut ui = session_sized(ColorScheme::Light, WIDE)?;
    let state = rich_text_state(RichDocument::from_markdown(
        "Select this text",
        Color::TRANSPARENT,
    ));
    {
        let mut d = state.lock().unwrap();
        d.set_focus(true);
        d.selection = Some(Selection {
            anchor: DocPosition::new(0, 0, 0),
            head: DocPosition::new(0, 0, 6),
        });
        d.cursor = DocPosition::new(0, 0, 6);
    }
    let editor = state.clone();
    ui.mount(move |_| {
        div()
            .w(WIDE as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(12.0))
            // Room above the selection for the floating toolbar (4 px units).
            .pt(20.0)
            .child(rich_text_editor(
                &editor,
                RichTextTheme::default().toolbar(placement),
                WIDE as f32 - 24.0,
            ))
    });
    ui.settle();
    Some((ui, state))
}

fn first_span(
    state: &blinc_cn::components::rich_text_editor::RichTextState,
) -> blinc_layout::styled_text::TextSpan {
    state.lock().unwrap().document.blocks[0].lines[0].spans[0].clone()
}

#[test]
fn the_floating_toolbar_bolds_the_selection_and_shows_the_mark_on() {
    use blinc_cn::components::rich_text_editor::ToolbarPlacement;
    let Some((mut ui, state)) = selected_editor(ToolbarPlacement::Floating) else {
        return;
    };
    let heard = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = heard.clone();
    blinc_cn::components::rich_text_editor::RichTextStateExt::on_change(&state, move |_| {
        count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    });
    let bold = all(&ui, "blinc-rte-button")[0];
    press(&mut ui, bold);
    let span = first_span(&state);
    assert!(span.bold && span.end == 6, "{span:?}");
    assert_eq!(
        heard.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "formatting from the toolbar is one change"
    );
    assert_eq!(
        all(&ui, "blinc-rte-button--active").len(),
        1,
        "Bold shows on"
    );
    // The press kept the selection, so the toolbar is still there.
    assert!(
        state
            .lock()
            .unwrap()
            .selection
            .is_some_and(|s| !s.is_empty())
    );
}

/// Opens the link popover from the floating toolbar's link button.
fn open_link_popover(ui: &mut Headless) {
    // Bold, italic, underline, strikethrough, code, then link.
    let link = all(ui, "blinc-rte-button")[5];
    press(ui, link);
    assert_eq!(
        all(ui, "blinc-rte-link").len(),
        1,
        "the link popover did not open"
    );
    ui.advance(16);
}

#[test]
fn the_link_popover_links_the_selection_on_enter() {
    use blinc_cn::components::rich_text_editor::ToolbarPlacement;
    let Some((mut ui, state)) = selected_editor(ToolbarPlacement::Floating) else {
        return;
    };
    open_link_popover(&mut ui);
    // The selection fills the Text field, so the Link field has the focus.
    ui.type_text("https://blinc.rs");
    ui.key(13);
    ui.settle();
    let span = first_span(&state);
    assert_eq!(
        span.link_url.as_deref(),
        Some("https://blinc.rs"),
        "{span:?}"
    );
    assert_eq!(span.end, 6, "{span:?}");
    assert!(
        all(&ui, "blinc-rte-link").is_empty(),
        "the popover stayed open"
    );
}

#[test]
fn the_link_popover_renames_the_linked_text() {
    use blinc_cn::components::rich_text_editor::ToolbarPlacement;
    let Some((mut ui, state)) = selected_editor(ToolbarPlacement::Floating) else {
        return;
    };
    open_link_popover(&mut ui);
    ui.type_text("https://blinc.rs");
    // The Text field holds the selection; replace it.
    let fields = all(&ui, "cn-input");
    press(&mut ui, fields[0]);
    for _ in 0.."Select".len() {
        ui.key(8);
    }
    ui.type_text("Pick");
    ui.key(13);
    ui.settle();
    let d = state.lock().unwrap();
    let line = &d.document.blocks[0].lines[0];
    assert_eq!(line.text, "Pick this text");
    let span = &line.spans[0];
    assert_eq!(
        (span.end, span.link_url.as_deref()),
        (4, Some("https://blinc.rs")),
        "{span:?}"
    );
}

#[test]
fn the_close_button_collapses_the_selection() {
    use blinc_cn::components::rich_text_editor::ToolbarPlacement;
    let Some((mut ui, state)) = selected_editor(ToolbarPlacement::Floating) else {
        return;
    };
    let close = *all(&ui, "blinc-rte-button").last().unwrap();
    press(&mut ui, close);
    assert!(state.lock().unwrap().selection.is_none());
    assert!(all(&ui, "blinc-rte-toolbar").is_empty());
}

#[test]
fn the_fixed_toolbar_sits_above_the_text_and_marks_what_is_typed_next() {
    use blinc_cn::components::rich_text_editor::{DocPosition, ToolbarPlacement};
    let Some((mut ui, state)) = selected_editor(ToolbarPlacement::Fixed) else {
        return;
    };
    assert!(
        all(&ui, "blinc-rte-toolbar").is_empty(),
        "no floating toolbar"
    );
    let bar = one(&ui, "blinc-rte-fixed-toolbar");
    let text = one(&ui, "blinc-rich-text");
    let (bb, tb) = (
        ui.tree().get_absolute_bounds(bar).unwrap(),
        ui.tree().get_absolute_bounds(text).unwrap(),
    );
    assert!(
        bb.y + bb.height <= tb.y + 0.5,
        "toolbar {bb:?} above text {tb:?}"
    );
    assert_eq!(all(&ui, "blinc-rte-heading").len(), 1);

    // A click on the text, below the toolbar, puts the caret where it lands.
    ui.move_to(tb.x + 2.0, tb.y + 6.0);
    ui.mouse_down();
    ui.mouse_up();
    ui.settle();
    assert_eq!(state.lock().unwrap().cursor, DocPosition::new(0, 0, 0));

    // Bold with a collapsed caret marks what is typed next, and the editor keeps the focus.
    let bold = all(&ui, "blinc-rte-button")[0];
    press(&mut ui, bold);
    assert!(state.lock().unwrap().focused);
    ui.type_text("x");
    ui.settle();
    let span = first_span(&state);
    assert!(span.bold && span.end == 1, "{span:?}");
}

#[test]
fn a_toolbar_tooltip_is_centred_over_its_button() {
    use blinc_cn::components::rich_text_editor::ToolbarPlacement;
    let Some((mut ui, _state)) = selected_editor(ToolbarPlacement::Fixed) else {
        return;
    };
    let bold = all(&ui, "blinc-rte-button")[0];
    let b = ui.tree().get_absolute_bounds(bold).unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0);
    ui.advance(600);
    ui.settle();
    let tip = one(&ui, "cn-tooltip");
    let t = ui.tree().get_absolute_bounds(tip).unwrap();
    let (button, tooltip) = (b.x + b.width / 2.0, t.x + t.width / 2.0);
    assert!(
        (button - tooltip).abs() < 1.0,
        "tooltip centred at {tooltip}, button at {button}: {t:?} {b:?}"
    );
}

#[test]
fn a_dragged_selection_shows_the_floating_toolbar_in_a_scroll() {
    use blinc_cn::components::rich_text_editor::{
        RichDocument, RichTextTheme, rich_text_editor, rich_text_state,
    };
    let Some(mut ui) = session(ColorScheme::Dark) else {
        return;
    };
    let state = rich_text_state(RichDocument::from_markdown(
        "## Floating toolbar\n\nSelect some text to **format** it.",
        Color::TRANSPARENT,
    ));
    let s = state.clone();
    let other = rich_text_state(RichDocument::from_markdown(
        "## Fixed toolbar\n\nThe toolbar stays above the text.",
        Color::TRANSPARENT,
    ));
    ui.mount(move |_| {
        use blinc_cn::components::rich_text_editor::ToolbarPlacement;
        let example = |name: &str, editor: blinc_layout::div::Div| {
            div()
                .flex_col()
                .h_fit()
                .gap_px(8.0)
                .child(blinc_cn::cn::label(name))
                .child(editor)
        };
        blinc_layout::widgets::scroll::scroll()
            .w(W as f32)
            .h(H as f32)
            .viewport_cull(true)
            .child(
                div()
                    .w_full()
                    .p(6.0)
                    .flex_col()
                    .gap(4.0)
                    .child(div().h(60.0))
                    .child(
                        div()
                            .w_full()
                            .h_fit()
                            .rounded(12.0)
                            .border(1.0, Color::rgb(0.3, 0.3, 0.3))
                            .p(4.0)
                            .child(
                                div()
                                    .flex_row()
                                    .flex_wrap()
                                    .items_start()
                                    .gap_px(24.0)
                                    .child(example(
                                        "Floating toolbar",
                                        rich_text_editor(&s, RichTextTheme::default(), 380.0),
                                    ))
                                    .child(example(
                                        "Fixed toolbar",
                                        rich_text_editor(
                                            &other,
                                            RichTextTheme::default()
                                                .toolbar(ToolbarPlacement::Fixed),
                                            380.0,
                                        ),
                                    )),
                            ),
                    ),
            )
    });
    ui.leave().settle();
    let text = all(&ui, "blinc-rich-text")[0];
    let b = ui.tree().get_absolute_bounds(text).unwrap();
    // Drag across the start of the paragraph, the second line.
    let y = b.y + b.height - 10.0;
    ui.move_to(b.x + 2.0, y);
    ui.mouse_down();
    ui.move_to(b.x + 60.0, y);
    ui.move_to(b.x + 120.0, y);
    ui.mouse_up();
    ui.frame(16);
    let selection = state.lock().unwrap().selection;
    assert!(
        selection.is_some_and(|s| !s.is_empty()),
        "no selection: {selection:?}"
    );
    assert_eq!(
        all(&ui, "blinc-rte-toolbar").len(),
        1,
        "no floating toolbar"
    );

    // It fades in: one frame after it appears, the pill is not yet its settled colour.
    let t = ui
        .tree()
        .get_absolute_bounds(one(&ui, "blinc-rte-toolbar"))
        .unwrap();
    let (px, py) = ((t.x + 6.0) as u32, (t.y + t.height * 0.6) as u32);
    let early = ui.snapshot().unwrap().get_pixel(px, py).unwrap();
    ui.settle();
    let settled = ui.snapshot().unwrap().get_pixel(px, py).unwrap();
    assert_ne!(early, settled, "the toolbar appeared without a transition");
}

#[test]
fn a_narrow_fixed_toolbar_scrolls_between_two_buttons() {
    use blinc_cn::components::rich_text_editor::{
        RichDocument, RichTextTheme, ToolbarPlacement, rich_text_editor, rich_text_state,
    };
    let Some(mut ui) = session(ColorScheme::Dark) else {
        return;
    };
    // The scroll buttons ease the controls along, which needs the physics a window ticks.
    ui.scroll_physics();
    let state = rich_text_state(RichDocument::from_markdown("Narrow", Color::TRANSPARENT));
    let s = state.clone();
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .p(3.0)
            .child(div().w(260.0).child(rich_text_editor(
                &s,
                RichTextTheme::default().toolbar(ToolbarPlacement::Fixed),
                260.0,
            )))
    });
    ui.leave().settle();
    let bar = ui
        .tree()
        .get_absolute_bounds(one(&ui, "blinc-rte-fixed-toolbar"))
        .unwrap();
    assert!(bar.width <= 260.5, "the toolbar overflows: {bar:?}");
    // The heading select works inside the scroll.
    let trigger = one(&ui, "cn-select-trigger");
    press(&mut ui, trigger);
    assert_eq!(
        all(&ui, "cn-select-content").len(),
        1,
        "the heading select did not open inside the scroll"
    );
    ui.key(27);
    ui.settle();
    // Scroll left, the marks, link and scroll right; the right one is last.
    let right = *all(&ui, "blinc-rte-button").last().unwrap();
    let row = |ui: &mut Headless| {
        let frame = ui.snapshot().unwrap();
        let y = (bar.y + bar.height / 2.0) as u32;
        (bar.x as u32 + 40..(bar.x + bar.width) as u32 - 40)
            .map(|x| frame.get_pixel(x, y).unwrap())
            .collect::<Vec<_>>()
    };
    let before = row(&mut ui);
    // The scroll eases there: one frame in, the controls have moved but not arrived.
    let r = ui.tree().get_absolute_bounds(right).unwrap();
    ui.move_to(r.x + r.width / 2.0, r.y + r.height / 2.0);
    ui.mouse_down();
    ui.mouse_up();
    ui.frame(16);
    ui.frame(16);
    let moving = row(&mut ui);
    ui.settle();
    let arrived = row(&mut ui);
    assert_ne!(before, arrived, "scrolling right did not move the controls");
    assert_ne!(moving, arrived, "the scroll jumped instead of easing");
}

#[test]
fn the_colour_picker_stays_open_while_it_colours_the_selection() {
    use blinc_cn::components::rich_text_editor::ToolbarPlacement;
    let Some((mut ui, state)) = selected_editor(ToolbarPlacement::Floating) else {
        return;
    };
    let trigger = one(&ui, "cn-color-picker-trigger");
    press(&mut ui, trigger);
    assert_eq!(
        all(&ui, "cn-color-picker-panel").len(),
        1,
        "the picker did not open"
    );
    let preset = all(&ui, "cn-color-picker-preset")[0];
    press(&mut ui, preset);
    assert_eq!(
        all(&ui, "cn-color-picker-panel").len(),
        1,
        "picking a colour closed the picker"
    );
    let span = first_span(&state);
    assert!(span.color.a > 0.0 && span.end == 6, "{span:?}");
}

/// The editor wraps its text to the width its parent gives it, not the width it was built with.
#[test]
fn the_editor_wraps_to_the_width_it_is_given() {
    use blinc_cn::components::rich_text_editor::{
        RichDocument, RichTextTheme, rich_text_editor, rich_text_state,
    };
    let lines_at = |width: f32| -> Option<(usize, f32)> {
        let mut ui = session(ColorScheme::Dark)?;
        let state = rich_text_state(RichDocument::from_markdown(
            "A paragraph long enough to wrap onto more lines in a narrow editor than a wide one.",
            Color::TRANSPARENT,
        ));
        let s = state.clone();
        ui.mount(move |_| {
            div()
                .w(W as f32)
                .h(H as f32)
                .child(
                    div()
                        .w(width)
                        .child(rich_text_editor(&s, RichTextTheme::default(), 380.0)),
                )
        });
        ui.leave().settle();
        let text = ui
            .tree()
            .get_absolute_bounds(one(&ui, "blinc-rich-text"))
            .unwrap();
        let lines = state.lock().unwrap().line_index.len();
        Some((lines, text.width))
    };
    let (Some((narrow, narrow_w)), Some((wide, _))) = (lines_at(180.0), lines_at(400.0)) else {
        return;
    };
    assert!(
        narrow_w <= 180.5,
        "the text is {narrow_w} wide in a 180 editor"
    );
    assert!(narrow > wide, "{narrow} lines at 180, {wide} at 400");
}

#[test]
fn the_heading_select_opens_and_sets_the_block_kind() {
    use blinc_cn::components::rich_text_editor::{BlockKind, ToolbarPlacement};
    let Some((mut ui, state)) = selected_editor(ToolbarPlacement::Fixed) else {
        return;
    };
    // A press on the text gives the editor the focus, as a person would before choosing a kind.
    let text = one(&ui, "blinc-rich-text");
    press(&mut ui, text);
    let trigger = one(&ui, "cn-select-trigger");
    // The window blurs every text input before it routes a press.
    blinc_layout::widgets::blur_all_text_inputs();
    press(&mut ui, trigger);
    assert_eq!(
        all(&ui, "cn-select-content").len(),
        1,
        "the heading select did not open"
    );
    // Paragraph, then Heading 1 to 6.
    let items = all(&ui, "cn-select-item");
    assert_eq!(items.len(), 7);
    press(&mut ui, items[2]);
    assert_eq!(
        state.lock().unwrap().document.blocks[0].kind,
        BlockKind::Heading(2)
    );
}

/// The scroll lane fades only an edge with more beyond it: the first control is clear when the
/// lane is at its start, and the last when it is at its end.
#[test]
fn the_fade_leaves_the_first_and_last_controls_clear() {
    use blinc_cn::components::rich_text_editor::{
        RichDocument, RichTextTheme, ToolbarPlacement, rich_text_editor, rich_text_state,
    };
    let Some(mut ui) = session(ColorScheme::Dark) else {
        return;
    };
    ui.scroll_physics();
    let state = rich_text_state(RichDocument::from_markdown("Narrow", Color::TRANSPARENT));
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .p(3.0)
            .child(div().w(300.0).child(rich_text_editor(
                &state,
                RichTextTheme::default().toolbar(ToolbarPlacement::Fixed),
                300.0,
            )))
    });
    ui.leave().settle();
    // Whether the start and the end of the lane fade.
    let fades = |ui: &Headless| {
        let lane = one(ui, "blinc-rte-lane");
        let fade = ui.tree().get_render_node(lane).unwrap().props.overflow_fade;
        (fade.left > 0.0, fade.right > 0.0)
    };
    let step = |ui: &mut Headless, right: bool| {
        let buttons = all(ui, "blinc-rte-button");
        let chevron = if right {
            *buttons.last().unwrap()
        } else {
            buttons[0]
        };
        press(ui, chevron);
        ui.advance(1500);
        ui.leave().settle();
    };
    assert_eq!(fades(&ui), (false, true), "at rest");
    step(&mut ui, true);
    assert_eq!(fades(&ui), (true, true), "in the middle");
    step(&mut ui, true);
    assert_eq!(fades(&ui), (true, false), "at the end");
    step(&mut ui, false);
    assert_eq!(fades(&ui), (true, true), "in the middle again");
    step(&mut ui, false);
    assert_eq!(fades(&ui), (false, true), "back at the start");
}
