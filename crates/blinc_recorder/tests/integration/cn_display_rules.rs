#![cfg(feature = "headless")]
//! Display pieces take their colours from rules: card titles and
//! descriptions, keyboard keys, separators, field messages and resizable
//! handles, in both schemes.

use blinc_cn::{card, card_header, input, kbd, resizable_group, resizable_panel, separator};
use blinc_core::{Brush, Color};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::text::text;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

const W: u32 = 420;
const H: u32 = 360;

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

fn mount(ui: &mut Headless) {
    let field = blinc_layout::widgets::text_input::text_input_data();
    let described = blinc_layout::widgets::text_input::text_input_data();
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(12.0))
            .flex_col()
            .gap(3.0)
            .child(card().child(card_header().title("Title").description("Description")))
            .child(kbd("Esc"))
            .child(separator())
            .child(input(&field).w(200.0).error("Required"))
            .child(input(&described).w(200.0).description("Optional"))
            .child(
                div().w(200.0).h(40.0).child(
                    resizable_group()
                        .horizontal()
                        .key("display_rules")
                        .panel(resizable_panel().default_size(100.0).child(text("a")))
                        .panel(resizable_panel().default_size(100.0).child(text("b"))),
                ),
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
    (best.unwrap(), px(b.x as u32 + 2, b.y as u32 + 2))
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
fn text_parts_take_their_rule_colours_in_both_schemes() {
    for (scheme, dark) in [(ColorScheme::Dark, true), (ColorScheme::Light, false)] {
        let Some(mut ui) = session(scheme) else {
            return;
        };
        mount(&mut ui);
        for (class, want) in [
            ("cn-card-title", ColorToken::TextPrimary),
            ("cn-card-description", ColorToken::TextSecondary),
            ("cn-kbd", ColorToken::TextSecondary),
            ("cn-field-error", ColorToken::Error),
            ("cn-field-description", ColorToken::TextTertiary),
        ] {
            let node = one(&ui, class);
            let ((got, bg), want) = (ink(&mut ui, node, dark), token(want));
            assert!(
                drawn_in(got, want, bg),
                "{scheme:?} .{class} text {got:?} over {bg:?}, want {want:?}"
            );
        }
    }
}

#[test]
fn fills_follow_their_rules() {
    let Some(mut ui) = session(ColorScheme::Dark) else {
        return;
    };
    let theme = ThemeState::get();
    mount(&mut ui);
    let key = one(&ui, "cn-kbd");
    assert!(close(
        background(&ui, key),
        theme.color(ColorToken::Surface)
    ));
    let border = ui.tree().get_render_node(key).unwrap().props.border_color;
    assert!(close(border, theme.color(ColorToken::Border)));
    assert!(close(
        background(&ui, one(&ui, "cn-separator")),
        theme.color(ColorToken::Border)
    ));
    let line = one(&ui, "cn-resizable-line");
    assert!(close(
        background(&ui, line),
        theme.color(ColorToken::Border)
    ));
    ui.assert_snapshot("cn_display_rules");
}

#[test]
fn a_dragged_handle_turns_primary() {
    let Some(mut ui) = session(ColorScheme::Dark) else {
        return;
    };
    let theme = ThemeState::get();
    mount(&mut ui);
    let handle = one(&ui, "cn-resizable-handle");
    let b = ui.tree().get_absolute_bounds(handle).unwrap();
    ui.move_to(b.x + b.width / 2.0, b.y + b.height / 2.0)
        .mouse_down();
    ui.move_to(b.x + b.width / 2.0 + 10.0, b.y + b.height / 2.0);
    ui.settle();
    let registry = ui.tree().element_registry();
    assert_eq!(
        registry.attribute(handle, "data-state").as_deref(),
        Some("dragging")
    );
    let line = one(&ui, "cn-resizable-line");
    assert!(close(
        background(&ui, line),
        theme.color(ColorToken::Primary)
    ));
    ui.release().settle();
    assert_eq!(
        ui.tree().element_registry().attribute(handle, "data-state"),
        None
    );
}

/// A toast's fill, border and text are rules; a success toast colours its
/// border and icon, a default one keeps the plain border.
#[test]
fn a_toast_takes_its_variant_from_rules() {
    let Some(mut ui) = session(ColorScheme::Dark) else {
        return;
    };
    let theme = ThemeState::get();
    ui.mount(|_| div().w(W as f32).h(H as f32));
    ui.leave().settle();

    let handle = blinc_cn::toast_success("Saved")
        .description("Your changes are saved.")
        .show();
    ui.settle();
    let toast = one(&ui, "cn-toast");
    assert_eq!(
        ui.tree()
            .element_registry()
            .attribute(toast, "data-variant")
            .as_deref(),
        Some("success")
    );
    assert!(close(
        background(&ui, toast),
        theme.color(ColorToken::Surface)
    ));
    let border = ui.tree().get_render_node(toast).unwrap().props.border_color;
    assert!(close(border, theme.color(ColorToken::Success)));
    for (class, want) in [
        ("cn-toast-icon", ColorToken::Success),
        ("cn-toast-title", ColorToken::TextPrimary),
        ("cn-toast-description", ColorToken::TextSecondary),
    ] {
        let node = one(&ui, class);
        let ((got, bg), want) = (ink(&mut ui, node, true), token(want));
        assert!(
            drawn_in(got, want, bg),
            ".{class} drawn {got:?} over {bg:?}, want {want:?}"
        );
    }
    ui.assert_snapshot("cn_toast_success");
    handle.dismiss();
    ui.settle();

    blinc_cn::toast("Note").show();
    ui.settle();
    let toast = one(&ui, "cn-toast");
    let border = ui.tree().get_render_node(toast).unwrap().props.border_color;
    assert!(close(border, theme.color(ColorToken::Border)));
}

/// An avatar's fallback, status dot and group ring are rules.
#[test]
fn an_avatar_takes_its_colours_from_rules() {
    let Some(mut ui) = session(ColorScheme::Dark) else {
        return;
    };
    let theme = ThemeState::get();
    ui.mount(|_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .padding(blinc_layout::units::px(12.0))
            .flex_col()
            .gap(3.0)
            .child(blinc_cn::avatar().fallback("JD").busy())
            .child(
                blinc_cn::avatar_group()
                    .max(2)
                    .child(blinc_cn::avatar().fallback("A"))
                    .child(blinc_cn::avatar().fallback("B"))
                    .child(blinc_cn::avatar().fallback("C")),
            )
    });
    ui.leave().settle();

    let avatars = all(&ui, "cn-avatar");
    let first = avatars[0];
    assert!(close(
        background(&ui, first),
        theme.color(ColorToken::Surface)
    ));
    let ((got, bg), want) = (ink(&mut ui, first, true), token(ColorToken::TextPrimary));
    assert!(drawn_in(got, want, bg), "initials {got:?} over {bg:?}");
    let dot = one(&ui, "cn-avatar-status");
    assert_eq!(
        ui.tree()
            .element_registry()
            .attribute(dot, "data-status")
            .as_deref(),
        Some("busy")
    );
    assert!(close(background(&ui, dot), theme.color(ColorToken::Error)));

    // The grouped avatars and the "+1" are ringed by the group's rule.
    let ring = theme.color(ColorToken::BorderSecondary);
    let grouped = avatars[1];
    let border = ui
        .tree()
        .get_render_node(grouped)
        .unwrap()
        .props
        .border_color;
    assert!(close(border, ring), "grouped avatar ring {border:?}");
    let more = one(&ui, "cn-avatar-group__more");
    assert!(close(
        background(&ui, more),
        theme.color(ColorToken::SurfaceElevated)
    ));
    let border = ui.tree().get_render_node(more).unwrap().props.border_color;
    assert!(close(border, ring));
    ui.assert_snapshot("cn_avatar_rules");
}

/// An icon takes the text colour around it, the document's when nothing
/// around it sets one; a token given to it wins.
#[test]
fn an_icon_takes_the_colour_around_it() {
    let Some(mut ui) = session(ColorScheme::Dark) else {
        return;
    };
    let check = blinc_cn::prelude::icons::CHECK;
    ui.mount(move |_| {
        div()
            .w(W as f32)
            .h(H as f32)
            .flex_row()
            .gap(4.0)
            .child(blinc_cn::icon(check).size_px(32.0))
            .child(
                div()
                    .class("cn-field-error")
                    .child(blinc_cn::icon(check).size_px(32.0)),
            )
            .child(
                blinc_cn::icon(check)
                    .size_px(32.0)
                    .color(ColorToken::Success),
            )
    });
    ui.leave().settle();
    let icons = all(&ui, "cn-icon");
    for (i, want) in [
        ColorToken::TextPrimary,
        ColorToken::Error,
        ColorToken::Success,
    ]
    .into_iter()
    .enumerate()
    {
        let ((got, bg), want) = (ink(&mut ui, icons[i], true), token(want));
        assert!(
            drawn_in(got, want, bg),
            "icon {i} drawn {got:?} over {bg:?}, want {want:?}"
        );
    }
}
