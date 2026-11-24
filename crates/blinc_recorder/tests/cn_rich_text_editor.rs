#![cfg(feature = "headless")]
//! The rich text editor takes its colours from the cn stylesheet: the document text, markers,
//! quote bar, divider, code, selection and toolbar; a fixed palette still colours the text.

use blinc_core::{Brush, Color};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

const W: u32 = 420;
const H: u32 = 420;

fn session(scheme: ColorScheme) -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(W, H).ok()?;
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
    let Some(mut ui) = session(ColorScheme::Light) else {
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
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(12.0))
            .flex_col()
            .gap(30.0)
            .child(rich_text_editor(&state, RichTextTheme::default(), 380.0))
            .child(rich_text_editor(&fixed, RichTextTheme::dark(), 380.0))
    });
    ui.leave().settle();

    let toolbar = one(&ui, "blinc-rte-toolbar");
    assert!(close(
        background(&ui, toolbar),
        theme.color(ColorToken::SurfaceElevated)
    ));
    assert!(close(
        background(&ui, one(&ui, "blinc-rich-text-selection")),
        theme.color(ColorToken::Selection)
    ));
    let button = one(&ui, "blinc-rte-button");
    let ((got, bg), want) = (ink(&mut ui, button, false), token(ColorToken::TextPrimary));
    assert!(drawn_in(got, want, bg), "toolbar label {got:?} over {bg:?}");
    assert!(close(
        background(&ui, one(&ui, "blinc-rte-swatch--default")),
        theme.color(ColorToken::TextPrimary)
    ));

    // The dark palette's near-white text, on the light theme.
    let documents = all(&ui, "blinc-rich-text");
    let (brightest, _) = ink(&mut ui, documents[1], true);
    assert!(
        brightest.iter().all(|&c| c > 220),
        "fixed palette text {brightest:?}"
    );
    ui.assert_snapshot("layout_rich_text_rules");
}
