#![cfg(feature = "headless")]
//! Links, blockquotes, rules, list markers, task boxes and tables take their
//! colours from the layout stylesheet; a colour given to the widget wins.

use blinc_core::{Brush, Color};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::text::text;
use blinc_layout::widgets::{
    TableBuilder, blockquote, hr, hr_color, li, link, task_item, td_text, th_text, tr, ul,
};
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
    ui.theme(HybridTheme::bundle(), scheme);
    Some(ui)
}

fn mount(ui: &mut Headless) {
    ui.mount(|_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(12.0))
            .flex_col()
            .gap(2.0)
            .child(link("Documentation", "https://example.com").font_size(24.0))
            .child(blockquote().child(text("Quoted")))
            .child(hr())
            .child(hr_color(Color::rgb(1.0, 0.0, 0.0)))
            .child(ul().child(li().child(text("Item"))))
            .child(task_item(true).child(text("Done")))
            .child(task_item(false).child(text("Open")))
            .child(
                TableBuilder::new()
                    .headers(&["Name", "Role"])
                    .row(&["Ada", "Engineer"])
                    .row(&["Grace", "Admiral"])
                    .striped(true)
                    .build(),
            )
            .child(
                tr().child(th_text("Header").size(24.0))
                    .child(td_text("Cell").size(24.0)),
            )
    });
    ui.leave().settle();
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

fn border(ui: &Headless, node: LayoutNodeId) -> Option<Color> {
    ui.tree().get_render_node(node).unwrap().props.border_color
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

#[test]
fn text_takes_its_rule_colour_in_both_schemes() {
    for (scheme, dark) in [(ColorScheme::Dark, true), (ColorScheme::Light, false)] {
        let Some(mut ui) = session(scheme) else {
            return;
        };
        mount(&mut ui);
        let ths = all(&ui, "blinc-th");
        let tds = all(&ui, "blinc-td");
        for (what, node, want) in [
            ("link", one(&ui, "blinc-link"), ColorToken::TextLink),
            ("th", *ths.last().unwrap(), ColorToken::TextPrimary),
            ("td", *tds.last().unwrap(), ColorToken::TextSecondary),
        ] {
            let ((got, bg), want) = (ink(&mut ui, node, dark), token(want));
            assert!(
                drawn_in(got, want, bg),
                "{scheme:?} {what} drawn {got:?} over {bg:?}, want {want:?}"
            );
        }
    }
}

#[test]
fn fills_and_borders_follow_their_rules() {
    let Some(mut ui) = session(ColorScheme::Dark) else {
        return;
    };
    let theme = ThemeState::get();
    mount(&mut ui);

    let quote = one(&ui, "blinc-blockquote");
    assert!(close(
        background(&ui, quote),
        theme.color(ColorToken::SurfaceOverlay)
    ));
    let left = ui
        .tree()
        .get_render_node(quote)
        .unwrap()
        .props
        .border_sides
        .left;
    assert!(close(
        left.map(|side| side.color),
        theme.color(ColorToken::Border)
    ));

    let rules = all(&ui, "blinc-hr");
    assert!(close(
        background(&ui, rules[0]),
        theme.color(ColorToken::Border)
    ));
    // A colour given to the widget is inline and wins over the rule.
    assert!(close(background(&ui, rules[1]), Color::rgb(1.0, 0.0, 0.0)));
    let b = ui.tree().get_absolute_bounds(rules[1]).unwrap();
    let drawn = ui
        .snapshot()
        .unwrap()
        .get_pixel((b.x + b.width / 2.0) as u32, b.y as u32)
        .unwrap();
    assert!(drawn[0] > 200 && drawn[1] < 60, "red rule drawn {drawn:?}");

    let marker = one(&ui, "blinc-list-marker");
    let ((got, bg), want) = (ink(&mut ui, marker, true), token(ColorToken::TextTertiary));
    assert!(drawn_in(got, want, bg), "marker {got:?} over {bg:?}");

    let boxes = all(&ui, "blinc-task-box");
    let tertiary = theme.color(ColorToken::TextTertiary);
    assert!(close(background(&ui, boxes[0]), tertiary));
    assert!(close(border(&ui, boxes[1]), tertiary));
    assert!(!close(background(&ui, boxes[1]), tertiary));

    assert!(close(
        background(&ui, one(&ui, "blinc-thead")),
        theme.color(ColorToken::SurfaceOverlay)
    ));
    let alt = all(&ui, "blinc-tr-alt");
    assert_eq!(alt.len(), 1, "only the odd body row is striped");
    assert!(close(
        background(&ui, alt[0]),
        theme.color(ColorToken::AccentSubtle)
    ));
    ui.assert_snapshot("layout_widget_rules");
}

#[test]
fn a_hovered_link_turns_the_hover_colour() {
    let Some(mut ui) = session(ColorScheme::Dark) else {
        return;
    };
    mount(&mut ui);
    let node = one(&ui, "blinc-link");
    let b = ui.tree().get_absolute_bounds(node).unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0)
        .settle();
    let ((got, bg), want) = (ink(&mut ui, node, true), token(ColorToken::PrimaryHover));
    assert!(drawn_in(got, want, bg), "hovered link {got:?} over {bg:?}");
}

fn mount_code(ui: &mut Headless) {
    ui.mount(|_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(12.0))
            .flex_col()
            .gap(2.0)
            .child(
                blinc_layout::widgets::code("let answer = 42;\nlet next = answer + 1;")
                    .line_numbers(true)
                    .font_size(22.0)
                    .w_full(),
            )
    });
    ui.leave().settle();
}

/// Plain code takes the code rules: the fill, the text by inheritance, the
/// line numbers.
#[test]
fn plain_code_takes_the_code_rules_in_both_schemes() {
    for (scheme, dark) in [(ColorScheme::Dark, true), (ColorScheme::Light, false)] {
        let Some(mut ui) = session(scheme) else {
            return;
        };
        mount_code(&mut ui);
        let block = one(&ui, "blinc-code");
        assert!(close(
            background(&ui, block),
            ThemeState::get().color(ColorToken::Surface)
        ));
        let number = one(&ui, "blinc-code-line-number");
        for (what, node, want) in [
            ("code", block, ColorToken::TextPrimary),
            ("line number", number, ColorToken::TextTertiary),
        ] {
            let ((got, bg), want) = (ink(&mut ui, node, dark), token(want));
            assert!(
                drawn_in(got, want, bg),
                "{scheme:?} {what} drawn {got:?} over {bg:?}, want {want:?}"
            );
        }
    }
}

/// A selection in the editor is the `.blinc-code-selection` rule's fill; a
/// syntax theme keeps its own background.
#[test]
fn the_editor_takes_selection_and_syntax_colours() {
    use blinc_layout::syntax::{RustHighlighter, SyntaxConfig, SyntaxHighlighter};
    use blinc_layout::widgets::code::{code_editor, code_editor_state};
    use blinc_layout::widgets::text_area::TextPosition;

    let Some(mut ui) = session(ColorScheme::Dark) else {
        return;
    };
    let theme = ThemeState::get();
    let plain = code_editor_state("hello world");
    {
        let mut d = plain.lock().unwrap();
        d.selection_start = Some(TextPosition::new(0, 0));
        d.cursor = TextPosition::new(0, 5);
    }
    let rust = code_editor_state("fn main() {}");
    let (p, r) = (plain.clone(), rust.clone());
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .flex_col()
            .child(code_editor(&p).w(300.0).h(80.0))
            .child(
                code_editor(&r)
                    .syntax(SyntaxConfig::new(RustHighlighter::new()))
                    .w(300.0)
                    .h(80.0),
            )
    });
    ui.leave().settle();

    let selection = one(&ui, "blinc-code-selection");
    assert!(close(
        background(&ui, selection),
        theme.color(ColorToken::Selection)
    ));
    let editors = all(&ui, "blinc-code");
    assert_eq!(editors.len(), 2);
    assert!(close(
        background(&ui, editors[0]),
        theme.color(ColorToken::Surface)
    ));
    assert!(close(
        background(&ui, editors[1]),
        RustHighlighter::new().background_color()
    ));
    ui.assert_snapshot("layout_code_rules");
}
