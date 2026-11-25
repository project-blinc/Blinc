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
const WIDE: u32 = 720;
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
    // Five marks, four alignments, quote, insert table, link and close, and the colour picker's
    // swatch beside them.
    assert_eq!(all(&ui, "blinc-rte-button").len(), 13);
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
    selected_editor_styled(placement, "")
}

/// [`selected_editor`] under the application stylesheet `css`.
fn selected_editor_styled(
    placement: blinc_cn::components::rich_text_editor::ToolbarPlacement,
    css: &str,
) -> Option<(
    Headless,
    blinc_cn::components::rich_text_editor::RichTextState,
)> {
    use blinc_cn::components::rich_text_editor::{
        DocPosition, RichDocument, RichTextTheme, Selection, rich_text_editor, rich_text_state,
    };
    // Wide enough that the floating toolbar's controls fit without scrolling.
    let mut ui = session_sized(ColorScheme::Light, WIDE)?;
    if !css.is_empty() {
        ui.css(css);
    }
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
    let bold = tool(&ui, "Bold");
    press(&mut ui, bold);
    let span = first_span(&state);
    assert!(span.bold && span.end == 6, "{span:?}");
    assert_eq!(
        heard.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "formatting from the toolbar is one change"
    );
    let pressed = |ui: &Headless, label| {
        ui.tree()
            .element_registry()
            .has_class(tool(ui, label), "blinc-rte-button--active")
    };
    assert!(pressed(&ui, "Bold"), "Bold shows on");
    assert!(pressed(&ui, "Align left") && !pressed(&ui, "Italic"));
    // The press kept the selection, so the toolbar is still there.
    assert!(
        state
            .lock()
            .unwrap()
            .selection
            .is_some_and(|s| !s.is_empty())
    );
}

/// The toolbar button named `label`.
fn tool(ui: &Headless, label: &str) -> LayoutNodeId {
    let registry = ui.tree().element_registry().clone();
    all(ui, "blinc-rte-button")
        .into_iter()
        .find(|n| registry.attribute(*n, "aria-label").as_deref() == Some(label))
        .unwrap_or_else(|| panic!("no {label} button"))
}

/// Opens the link popover from the floating toolbar's link button.
fn open_link_popover(ui: &mut Headless) {
    let link = tool(ui, "Link");
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
    let close = tool(&ui, "Close");
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
    let bold = tool(&ui, "Bold");
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
    let bold = tool(&ui, "Bold");
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
    let right = tool(&ui, "Scroll right");
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
        let chevron = tool(ui, if right { "Scroll right" } else { "Scroll left" });
        press(ui, chevron);
        ui.advance(1500);
        ui.leave().settle();
    };
    assert_eq!(fades(&ui), (false, true), "at rest");
    step(&mut ui, true);
    assert_eq!(fades(&ui), (true, true), "in the middle");
    for _ in 0..4 {
        step(&mut ui, true);
    }
    assert_eq!(fades(&ui), (true, false), "at the end");
    step(&mut ui, false);
    assert_eq!(fades(&ui), (true, true), "in the middle again");
    for _ in 0..4 {
        step(&mut ui, false);
    }
    assert_eq!(fades(&ui), (false, true), "back at the start");
}

/// Text is drawn where its block's alignment puts it, bold and code included, and a click lands on
/// the character under it.
#[test]
fn aligned_text_is_drawn_and_clicked_where_it_is_placed() {
    use blinc_cn::components::rich_text_editor::{
        BlockAlign, RichDocument, RichTextTheme, ToolbarPlacement, rich_text_editor,
        rich_text_state,
    };
    let Some(mut ui) = session_sized(ColorScheme::Light, WIDE) else {
        return;
    };
    let text = "Plain, **bold words** and `code`.";
    let long = "A justified paragraph with **bold**, *italic* and `code` runs reaches both edges of the editor on every line but its last.";
    let mut doc = RichDocument::from_markdown(
        &format!("{text}\n\n{text}\n\n{text}\n\n# Heading\n\n- an item\n\n{long}"),
        Color::TRANSPARENT,
    );
    let aligns = [
        BlockAlign::Start,
        BlockAlign::Center,
        BlockAlign::End,
        BlockAlign::Center,
        BlockAlign::End,
        BlockAlign::Justify,
    ];
    for (block, align) in doc.blocks.iter_mut().zip(aligns) {
        block.align = align;
    }
    let state = rich_text_state(doc);
    let e = state.clone();
    ui.mount(move |_| {
        div().w(WIDE as f32).h(H as f32).p(5.0).child(
            div().w(440.0).child(rich_text_editor(
                &e,
                RichTextTheme::default()
                    .toolbar(ToolbarPlacement::Hidden)
                    .padding(20.0),
                400.0,
            )),
        )
    });
    ui.leave().settle();
    let origin = ui
        .tree()
        .get_absolute_bounds(one(&ui, "blinc-rich-text"))
        .unwrap();
    let lines: Vec<(usize, f32, f32)> = state
        .lock()
        .unwrap()
        .line_index
        .iter()
        .map(|g| (g.start.block, g.y, g.height))
        .collect();
    let frame = ui.snapshot().unwrap();
    // The leftmost and rightmost dark pixels of a line, from the editor's left edge.
    let ink = |(_, y, h): (usize, f32, f32)| {
        let rows = (origin.y + y) as u32..(origin.y + y + h) as u32;
        let dark = |x: u32| {
            rows.clone().any(|y| {
                let p = frame.get_pixel(x, y).unwrap();
                p[0] as u32 + p[1] as u32 + (p[2] as u32) < 380
            })
        };
        let columns = origin.x as u32..(origin.x + 400.0) as u32;
        let first = columns.clone().find(|x| dark(*x)).unwrap();
        let last = columns.rev().find(|x| dark(*x)).unwrap();
        (first as f32 - origin.x, last as f32 - origin.x)
    };
    let of = |block: usize| lines.iter().copied().filter(move |l| l.0 == block);
    let (start, center, end) = (
        ink(of(0).next().unwrap()),
        ink(of(1).next().unwrap()),
        ink(of(2).next().unwrap()),
    );
    assert!(start.0 < 3.0, "start {start:?}");
    assert!(end.1 > 395.0, "end {end:?}");
    let width = start.1 - start.0;
    assert!(
        (center.0 - (400.0 - center.1)).abs() < 3.0,
        "centre {center:?}"
    );
    for other in [center, end] {
        assert!(
            (other.1 - other.0 - width).abs() < 2.0,
            "{other:?} as wide as {start:?}"
        );
    }
    let justified: Vec<_> = of(5).map(ink).collect();
    assert!(justified.len() >= 2, "{justified:?}");
    let (last, full) = justified.split_last().unwrap();
    for line in full {
        assert!(line.0 < 3.0 && line.1 > 394.0, "justified {line:?}");
    }
    assert!(last.1 < 390.0, "the last line was stretched: {last:?}");

    // A click just inside the first character of the line at the end.
    let (_, y, h) = of(2).next().unwrap();
    ui.move_to(origin.x + end.0 + 1.0, origin.y + y + h / 2.0);
    ui.mouse_down();
    ui.mouse_up();
    ui.settle();
    let cursor = state.lock().unwrap().cursor;
    assert_eq!((cursor.block, cursor.col), (2, 0));
    let (x, ..) = state.lock().unwrap().cursor_geometry().unwrap();
    assert!(
        (x - end.0).abs() < 3.0,
        "the caret is at {x}, the text at {end:?}"
    );
    // A click between two letters in the middle of another line.
    let (_, y, h) = of(1).next().unwrap();
    state
        .lock()
        .unwrap()
        .set_cursor(blinc_cn::components::rich_text_editor::DocPosition::new(
            1, 0, 12,
        ));
    let (x, ..) = state.lock().unwrap().cursor_geometry().unwrap();
    // Not the second press of a double click.
    state.lock().unwrap().last_click_time = None;
    ui.move_to(origin.x + x, origin.y + y + h / 2.0);
    ui.mouse_down();
    ui.mouse_up();
    ui.settle();
    let cursor = state.lock().unwrap().cursor;
    assert_eq!((cursor.block, cursor.col), (1, 12));
    ui.leave();
    ui.assert_snapshot("cn_rich_text_alignment");
}

#[test]
fn an_alignment_button_aligns_the_selected_blocks() {
    use blinc_cn::components::rich_text_editor::{BlockAlign, ToolbarPlacement};
    let Some((mut ui, state)) = selected_editor(ToolbarPlacement::Fixed) else {
        return;
    };
    let align = |s: &blinc_cn::components::rich_text_editor::RichTextState| {
        s.lock().unwrap().document.blocks[0].align
    };
    for (label, expected) in [
        ("Align centre", BlockAlign::Center),
        ("Align right", BlockAlign::End),
        ("Justify", BlockAlign::Justify),
        ("Align left", BlockAlign::Start),
    ] {
        let button = tool(&ui, label);
        press(&mut ui, button);
        assert_eq!(align(&state), expected);
        let on = ui
            .tree()
            .element_registry()
            .has_class(tool(&ui, label), "blinc-rte-button--active");
        assert!(on, "{label} does not show on");
    }
    assert!(state.lock().unwrap().undo());
    assert_eq!(align(&state), BlockAlign::Justify);
}

/// A checklist's boxes tick on a press and follow undo, without rebuilding their block, so the
/// box's own transition runs.
#[test]
fn checklist_boxes_tick_in_place_and_follow_undo() {
    use blinc_cn::components::rich_text_editor::{
        BlockKind, DocPosition, RichDocument, RichTextTheme, ToolbarPlacement, rich_text_editor,
        rich_text_state,
    };
    let Some(mut ui) = session(ColorScheme::Light) else {
        return;
    };
    let state = rich_text_state(RichDocument::from_markdown(
        "- [ ] first\n- [x] second\n  - [ ] nested\n- bullet\n  - inner\n\n1. one\n   - x\n2. two",
        Color::TRANSPARENT,
    ));
    state.lock().unwrap().set_cursor(DocPosition::new(3, 0, 2));
    let e = state.clone();
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .p(5.0)
            .child(div().w(380.0).child(rich_text_editor(
                &e,
                RichTextTheme::default().toolbar(ToolbarPlacement::Hidden),
                380.0,
            )))
    });
    ui.leave().settle();
    let boxes = all(&ui, "blinc-rte-task");
    assert_eq!(boxes.len(), 3);
    let checked = |block: usize| match state.lock().unwrap().document.blocks[block].kind {
        BlockKind::TaskItem { checked } => checked,
        ref kind => panic!("{kind:?}"),
    };
    let shown = |ui: &Headless| {
        let registry = ui.tree().element_registry().clone();
        all(ui, "blinc-rte-task")
            .into_iter()
            .map(|n| {
                let mut stack = vec![n];
                while let Some(n) = stack.pop() {
                    if registry.attribute(n, "checked").is_some() {
                        return true;
                    }
                    stack.extend(ui.tree().layout_tree.children(n));
                }
                false
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(shown(&ui), [false, true, false]);
    let before = all(&ui, "blinc-rte-task");
    press(&mut ui, boxes[0]);
    assert!(checked(0));
    assert_eq!(shown(&ui), [true, true, false]);
    assert_eq!(
        all(&ui, "blinc-rte-task"),
        before,
        "a tick rebuilt its block"
    );
    assert_eq!(
        state.lock().unwrap().cursor,
        DocPosition::new(3, 0, 2),
        "the press moved the caret"
    );

    // Undo, through the editor's shortcut path, unticks the box shown.
    ui.move_to(200.0, 200.0);
    let text = one(&ui, "blinc-rich-text");
    press(&mut ui, text);
    assert!(state.lock().unwrap().undo());
    blinc_cn::components::rich_text_editor::RichTextStateExt::on_change(&state, |_| {});
    ui.key(39);
    ui.settle();
    assert!(!checked(0));
    assert_eq!(shown(&ui), [false, true, false]);
    ui.leave();
    ui.assert_snapshot("cn_rich_text_lists");
}

#[test]
fn the_list_menu_makes_lists_of_the_selected_blocks() {
    use blinc_cn::components::rich_text_editor::{BlockKind, ToolbarPlacement};
    let Some((mut ui, state)) = selected_editor(ToolbarPlacement::Fixed) else {
        return;
    };
    for (item, kind) in [
        (0, BlockKind::BulletItem),
        (1, BlockKind::NumberedItem),
        (2, BlockKind::TaskItem { checked: false }),
        // The kind it already has makes it a paragraph again.
        (2, BlockKind::Paragraph),
    ] {
        let trigger = one(&ui, "cn-dropdown-trigger");
        press(&mut ui, trigger);
        let items = all(&ui, "cn-dropdown-item");
        assert_eq!(items.len(), 3, "the list menu did not open");
        press(&mut ui, items[item]);
        assert_eq!(state.lock().unwrap().document.blocks[0].kind, kind);
    }
}

/// An editor over `markdown` with a fixed toolbar, focused with the cursor at `at`.
fn fixed_editor(
    markdown: &str,
    at: blinc_cn::components::rich_text_editor::DocPosition,
) -> Option<(
    Headless,
    blinc_cn::components::rich_text_editor::RichTextState,
)> {
    use blinc_cn::components::rich_text_editor::{
        RichDocument, RichTextTheme, ToolbarPlacement, rich_text_editor, rich_text_state,
    };
    let mut ui = session_sized(ColorScheme::Light, WIDE)?;
    let state = rich_text_state(RichDocument::from_markdown(markdown, Color::TRANSPARENT));
    {
        let mut d = state.lock().unwrap();
        d.set_focus(true);
        d.cursor = at;
    }
    let editor = state.clone();
    ui.mount(move |_| {
        div()
            .w(WIDE as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(12.0))
            .child(rich_text_editor(
                &editor,
                RichTextTheme::default().toolbar(ToolbarPlacement::Fixed),
                WIDE as f32 - 24.0,
            ))
    });
    ui.settle();
    Some((ui, state))
}

const TABLE: &str = "Costs\n\n| Item | Each | Total |\n| --- | :---: | ---: |\n\
    | A long name that has to wrap inside its own cell of the table | 2 | 40 |\n\
    | Short | **3** | 9 |\n\nafter";

#[test]
fn a_table_is_drawn_in_cells_and_clicked_there() {
    use blinc_cn::components::rich_text_editor::DocPosition;
    let Some((mut ui, state)) = fixed_editor(TABLE, DocPosition::new(1, 1, 2)) else {
        return;
    };
    ui.leave().settle();
    ui.assert_snapshot("cn_rich_text_table");
    let table = ui
        .tree()
        .get_absolute_bounds(one(&ui, "blinc-rich-text-table"))
        .unwrap();
    // A press in the middle of each cell puts the cursor in it, below the text too.
    let column = table.width / 3.0;
    let rows = {
        let d = state.lock().unwrap();
        let mut tops: Vec<(f32, f32)> = d
            .line_index
            .iter()
            .filter_map(|g| g.cell.map(|[_, y, _, h]| (y, h)))
            .collect();
        tops.dedup();
        tops
    };
    assert_eq!(rows.len(), 3);
    assert!(rows[1].1 > rows[2].1, "the wrapped row is taller: {rows:?}");
    let top = rows[0].0;
    for (r, (y, h)) in rows.iter().enumerate() {
        for c in 0..3 {
            ui.move_to(
                table.x + (c as f32 + 0.5) * column,
                table.y + y - top + h - 2.0,
            );
            ui.mouse_down();
            ui.mouse_up();
            ui.settle();
            let mut d = state.lock().unwrap();
            assert_eq!((d.cursor.block, d.cursor.line), (1, r * 3 + c));
            // Each press starts a click of its own.
            d.last_click_time = None;
        }
    }
}

#[test]
fn keys_move_between_cells() {
    use blinc_cn::components::rich_text_editor::DocPosition;
    const TAB: u32 = 9;
    const ENTER: u32 = 13;
    const UP: u32 = 38;
    const DOWN: u32 = 40;
    let Some((mut ui, state)) = fixed_editor(TABLE, DocPosition::new(0, 0, 0)) else {
        return;
    };
    let at = |state: &blinc_cn::components::rich_text_editor::RichTextState| {
        let d = state.lock().unwrap();
        (d.cursor.block, d.cursor.line)
    };
    let put = |line| {
        state
            .lock()
            .unwrap()
            .move_cursor(DocPosition::new(1, line, 0), false)
    };
    // The focused editor hears keys once pressed.
    let b = ui
        .tree()
        .get_absolute_bounds(one(&ui, "blinc-rich-text"))
        .unwrap();
    ui.move_to(b.x + 3.0, b.y + 8.0);
    ui.mouse_down();
    ui.mouse_up();
    ui.settle();
    assert_eq!(at(&state), (0, 0));
    // Down, into the table and out of it, through the wrapped cell's two lines.
    let mut seen = Vec::new();
    for _ in 0..5 {
        ui.key(DOWN);
        ui.settle();
        seen.push(at(&state));
    }
    assert_eq!(seen, [(1, 0), (1, 3), (1, 3), (1, 6), (2, 0)]);
    for want in [(1, 6), (1, 3), (1, 3), (1, 0), (0, 0)] {
        ui.key(UP);
        ui.settle();
        assert_eq!(at(&state), want);
    }
    // The middle column is kept.
    put(1);
    ui.key(DOWN);
    ui.settle();
    assert_eq!(at(&state), (1, 4));
    ui.key(DOWN);
    ui.settle();
    assert_eq!(at(&state), (1, 7));
    // Enter goes down, and from the last row out.
    put(5);
    ui.key(ENTER);
    ui.settle();
    assert_eq!(at(&state), (1, 8));
    ui.key(ENTER);
    ui.settle();
    assert_eq!(at(&state), (2, 0));
    // Tab goes on, back with Shift, and makes a row past the last cell.
    put(0);
    ui.key(TAB);
    ui.settle();
    assert_eq!(at(&state), (1, 1));
    ui.key_with_shift(TAB, true);
    ui.settle();
    assert_eq!(at(&state), (1, 0));
    put(8);
    ui.key(TAB);
    ui.settle();
    assert_eq!(at(&state), (1, 9));
    assert_eq!(state.lock().unwrap().document.blocks[1].lines.len(), 12);
    assert!(state.lock().unwrap().undo());
    assert_eq!(state.lock().unwrap().document.blocks[1].lines.len(), 9);
}

#[test]
fn the_grid_inserts_a_table_and_its_menu_changes_it() {
    use blinc_cn::components::rich_text_editor::{BlockAlign, BlockKind, DocPosition};
    let Some((mut ui, state)) = fixed_editor("only", DocPosition::new(0, 0, 4)) else {
        return;
    };
    let insert = tool(&ui, "Insert table");
    press(&mut ui, insert);
    let square = |ui: &Headless, label: &str| {
        let registry = ui.tree().element_registry().clone();
        all(ui, "blinc-rte-grid-square")
            .into_iter()
            .find(|n| registry.attribute(*n, "aria-label").as_deref() == Some(label))
            .unwrap_or_else(|| panic!("no {label}"))
    };
    let pick = square(&ui, "2 by 3 table");
    let b = ui.tree().get_absolute_bounds(pick).unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0);
    ui.settle();
    let lit = {
        let registry = ui.tree().element_registry().clone();
        all(&ui, "blinc-rte-grid-square")
            .into_iter()
            .filter(|n| registry.attribute(*n, "data-within").is_some())
            .count()
    };
    assert_eq!(lit, 6, "the squares of the size under the pointer are lit");
    press(&mut ui, pick);
    ui.advance(400);
    ui.settle();
    assert!(all(&ui, "blinc-rte-grid").is_empty(), "a pick closes it");
    let shape = || {
        let d = state.lock().unwrap();
        let table = &d.document.blocks[1];
        (
            table.kind.table_columns().unwrap(),
            table.lines.len(),
            d.cursor.block,
            d.cursor.line,
        )
    };
    assert_eq!(shape(), (3, 6, 1, 0));
    assert_eq!(state.lock().unwrap().document.blocks.len(), 3);
    // In a table the menu takes the place of the lists, and columns are not justified.
    assert!(all(&ui, "blinc-rte-list").is_empty());
    let registry = ui.tree().element_registry().clone();
    assert!(
        !all(&ui, "blinc-rte-button")
            .iter()
            .any(|n| registry.attribute(*n, "aria-label").as_deref() == Some("Justify"))
    );
    let centre = tool(&ui, "Align centre");
    press(&mut ui, centre);
    assert_eq!(
        state.lock().unwrap().document.blocks[1].kind,
        BlockKind::Table {
            columns: vec![BlockAlign::Center, BlockAlign::Start, BlockAlign::Start]
        }
    );
    let choose = |ui: &mut Headless, item: usize| {
        let trigger = one(ui, "cn-dropdown-trigger");
        press(ui, trigger);
        let items = all(ui, "cn-dropdown-item");
        assert_eq!(items.len(), 7, "the table menu did not open");
        press(ui, items[item]);
        ui.advance(400);
        ui.settle();
    };
    choose(&mut ui, 1);
    assert_eq!(shape(), (3, 9, 1, 3), "a row below");
    choose(&mut ui, 3);
    assert_eq!(shape(), (4, 12, 1, 5), "a column after");
    choose(&mut ui, 5);
    assert_eq!(shape(), (3, 9, 1, 4), "the column gone");
    choose(&mut ui, 4);
    assert_eq!(shape().1, 6, "the row gone");
    choose(&mut ui, 6);
    let d = state.lock().unwrap();
    assert_eq!(d.document.blocks[1].kind, BlockKind::Paragraph);
}

#[test]
fn the_caret_moves_through_wrapped_lines() {
    use blinc_cn::components::rich_text_editor::DocPosition;
    const END: u32 = 35;
    const DOWN: u32 = 40;
    let prose = "words that wrap ".repeat(16);
    let Some((mut ui, state)) = fixed_editor(&prose, DocPosition::new(0, 0, 0)) else {
        return;
    };
    let b = ui
        .tree()
        .get_absolute_bounds(one(&ui, "blinc-rich-text"))
        .unwrap();
    ui.move_to(b.x + 1.0, b.y + 8.0);
    ui.mouse_down();
    ui.mouse_up();
    ui.settle();
    let starts: Vec<usize> = {
        let d = state.lock().unwrap();
        d.line_index.iter().map(|g| g.start.col).collect()
    };
    assert!(starts.len() > 2, "the text wraps: {starts:?}");
    for &start in &starts[1..] {
        ui.key(DOWN);
        ui.settle();
        assert_eq!(state.lock().unwrap().cursor.col, start);
    }
    // Pressed past a wrapped line's end, and sent there by a key, the caret stays on the line.
    state
        .lock()
        .unwrap()
        .move_cursor(DocPosition::new(0, 0, 0), false);
    ui.key(END);
    ui.settle();
    let end = state.lock().unwrap().cursor;
    assert_eq!(end.col, starts[1] - 1);
    state.lock().unwrap().last_click_time = None;
    ui.move_to(b.x + b.width - 1.0, b.y + 8.0);
    ui.mouse_down();
    ui.mouse_up();
    ui.settle();
    assert_eq!(state.lock().unwrap().cursor, end);
}

/// The last row with ink between two columns, in a box `top` to `bottom`.
fn last_inked_row(
    frame: &blinc_recorder::CapturedFrame,
    (left, right): (f32, f32),
    (top, bottom): (f32, f32),
) -> Option<u32> {
    (top as u32..bottom as u32).rev().find(|&y| {
        (left as u32..right as u32).any(|x| {
            let p = frame.get_pixel(x, y).unwrap();
            p[0].min(p[1]).min(p[2]) < 195
        })
    })
}

#[test]
fn the_runs_of_a_line_share_a_baseline() {
    use blinc_cn::components::rich_text_editor::DocPosition;
    // Letters without descenders, so the last inked row is the baseline.
    for markdown in [
        "nnnn **nnnn** *nnnn* ***nnnn***",
        "nnnn **nnnn** `nnnn` *nnnn*",
        "# nnnn `nnnn` *nnnn*",
        "> nnnn **nnnn** `nnnn`",
        "- nnnn **nnnn** `nnnn`",
        "| nnnn | `nnnn` |\n|---|---|\n| nnnn | **nnnn** |\n| nnnn | `nnnn` nnnn *nnnn* |",
    ] {
        let Some((mut ui, state)) = fixed_editor(markdown, DocPosition::new(0, 0, 0)) else {
            return;
        };
        state.lock().unwrap().set_focus(false);
        ui.leave().settle();
        let b = ui
            .tree()
            .get_absolute_bounds(one(&ui, "blinc-rich-text"))
            .unwrap();
        let frame = ui.snapshot().unwrap();
        let d = state.lock().unwrap();
        let mut rows: std::collections::BTreeMap<u32, Vec<(String, Option<u32>)>> =
            Default::default();
        for g in &d.line_index {
            for run in g.runs.iter().filter(|r| !r.text.trim().is_empty()) {
                let x = b.x + g.x + run.x_in_line;
                let y = b.y + g.y;
                let row = last_inked_row(&frame, (x, x + run.width), (y, y + g.height));
                rows.entry(g.y as u32)
                    .or_default()
                    .push((run.text.clone(), row));
            }
        }
        for runs in rows.values() {
            assert!(
                runs.iter()
                    .all(|(_, row)| *row == runs[0].1 && row.is_some()),
                "{markdown:?}: {runs:?}"
            );
        }
    }
}

#[test]
fn the_faces_of_a_rich_text_share_a_baseline() {
    let Some(mut ui) = session_sized(ColorScheme::Light, WIDE) else {
        return;
    };
    const SIZES: [f32; 6] = [12.0, 13.0, 14.0, 15.0, 18.0, 24.0];
    ui.mount(|_| {
        let mut column = div().w(WIDE as f32).h(H as f32).flex_col();
        for size in SIZES {
            column = column.child(div().h(48.0).child(
                blinc_layout::rich_text::rich_text("nnnn<b>nnnn</b><i>nnnn</i>").size(size),
            ));
        }
        column
    });
    ui.settle();
    let frame = ui.snapshot().unwrap();
    for (i, size) in SIZES.iter().enumerate() {
        let (top, bottom) = (i as f32 * 48.0, (i + 1) as f32 * 48.0);
        let ink: Vec<u32> = (0..WIDE)
            .filter_map(|x| last_inked_row(&frame, (x as f32, x as f32 + 1.0), (top, bottom)))
            .collect();
        // Every stem ends on one row.
        let row = ink.iter().max().unwrap();
        let on_it = ink.iter().filter(|r| *r == row).count();
        assert!(
            on_it as f32 > ink.len() as f32 * 0.3,
            "size {size}: {on_it} of {} on row {row}",
            ink.len()
        );
        let higher = (
            ink.iter().take(ink.len() / 4).max(),
            ink.iter().rev().take(ink.len() / 4).max(),
        );
        assert_eq!(higher, (Some(row), Some(row)), "size {size}");
    }
}

#[test]
fn a_stylesheet_restyles_both_toolbars() {
    use blinc_cn::components::rich_text_editor::ToolbarPlacement;
    for (placement, class) in [
        (ToolbarPlacement::Floating, "blinc-rte-floating-toolbar"),
        (ToolbarPlacement::Fixed, "blinc-rte-fixed-toolbar"),
    ] {
        let rule = format!(
            ".{class} {{ background: #ff0000; border: 3px solid #0000ff; border-radius: 0px;
               box-shadow: none; }}"
        );
        let Some((mut ui, _)) = selected_editor_styled(placement, &rule) else {
            return;
        };
        ui.leave().settle();
        ui.advance(500);
        ui.settle();
        let b = ui.tree().get_absolute_bounds(one(&ui, class)).unwrap();
        let frame = ui.snapshot().unwrap();
        // Along the left edge, at the corner too: the corner is square.
        let body = b.y + b.height - 2.0;
        let at = |x: f32, y: f32| {
            let p = frame.get_pixel(x as u32, y as u32).unwrap();
            [p[0], p[1], p[2]]
        };
        assert_eq!(at(b.x + 1.0, body), [0, 0, 255], "{placement:?} border");
        assert_eq!(at(b.x + 4.0, body - 4.0), [255, 0, 0], "{placement:?} fill");
        let below = at(b.x + b.width / 2.0, b.y + b.height + 3.0);
        let page = at(b.x + b.width / 2.0, b.y + b.height + 60.0);
        assert_eq!(below, page, "{placement:?} shadow");
    }
}

#[test]
fn the_quote_button_shows_a_quote_and_toggles_it() {
    use blinc_cn::components::rich_text_editor::{
        BlockKind, DocPosition, RichDocument, RichTextTheme, ToolbarPlacement, rich_text_editor,
        rich_text_state,
    };
    let Some(mut ui) = session_sized(ColorScheme::Light, WIDE) else {
        return;
    };
    let state = rich_text_state(RichDocument::from_markdown(
        "plain\n\n> quoted",
        Color::TRANSPARENT,
    ));
    state.lock().unwrap().set_focus(true);
    let editor = state.clone();
    ui.mount(move |_| {
        div()
            .w(WIDE as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(12.0))
            .child(rich_text_editor(
                &editor,
                RichTextTheme::default().toolbar(ToolbarPlacement::Fixed),
                WIDE as f32 - 24.0,
            ))
    });
    ui.settle();
    let pressed = |ui: &Headless| {
        ui.tree()
            .element_registry()
            .has_class(tool(ui, "Quote"), "blinc-rte-button--active")
    };
    assert!(!pressed(&ui), "a paragraph is not a quote");
    // Into the quote, from the keyboard, where the toolbar follows the cursor.
    state
        .lock()
        .unwrap()
        .move_cursor(DocPosition::new(1, 0, 2), false);
    ui.key(39);
    ui.settle();
    assert!(pressed(&ui), "the quote shows pressed");
    let quote = tool(&ui, "Quote");
    press(&mut ui, quote);
    assert_eq!(
        state.lock().unwrap().document.blocks[1].kind,
        BlockKind::Paragraph
    );
    assert!(!pressed(&ui));
    let quote = tool(&ui, "Quote");
    press(&mut ui, quote);
    assert_eq!(
        state.lock().unwrap().document.blocks[1].kind,
        BlockKind::Quote
    );
    assert!(pressed(&ui));
}
