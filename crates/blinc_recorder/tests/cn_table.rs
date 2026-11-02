#![cfg(feature = "headless")]
//! A table's text takes the theme's colours, in dark as in light, and a
//! selected row follows its signal in place.

use blinc_cn::cn;
use blinc_core::reactive::{Signal, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::prelude::*;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, ColorToken, HybridTheme, ThemeState};

const W: u32 = 360;
const H: u32 = 220;

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

fn mount(ui: &mut Headless, chosen: Signal<bool>) {
    ui.mount(move |_| {
        div().w(W as f32).h(H as f32).p(3.0).child(
            cn::table()
                .w_full()
                .child(
                    cn::table_header().child(
                        cn::table_row()
                            .child(cn::table_head("Invoice"))
                            .child(cn::table_head("Amount")),
                    ),
                )
                .child(
                    cn::table_body()
                        .child(
                            cn::table_row()
                                .child(cn::table_cell().child(text("INV001")))
                                .child(cn::table_cell().child(text("$250.00"))),
                        )
                        .child(
                            cn::table_row()
                                .selected(chosen)
                                .child(cn::table_cell().child(text("INV002")))
                                .child(cn::table_cell().child(text("$1,200.00"))),
                        ),
                )
                .child(
                    cn::table_footer().child(
                        cn::table_row()
                            .child(cn::table_cell().child(text("Total")))
                            .child(cn::table_cell().child(text("$1,450.00"))),
                    ),
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

/// The colour of the brightest pixel (dark scheme) or darkest (light) in a
/// node's box: its text.
fn ink(ui: &mut Headless, node: LayoutNodeId, dark: bool) -> [u8; 3] {
    let b = ui.tree().get_absolute_bounds(node).unwrap();
    let frame = ui.snapshot().unwrap();
    let mut best: Option<[u8; 3]> = None;
    for y in b.y as u32..(b.y + b.height) as u32 {
        for x in b.x as u32..(b.x + b.width) as u32 {
            let p = frame.get_pixel(x, y).unwrap();
            let px = [p[0], p[1], p[2]];
            let sum = |c: [u8; 3]| c.iter().map(|v| *v as u32).sum::<u32>();
            let better = best.is_none_or(|b| {
                if dark {
                    sum(px) > sum(b)
                } else {
                    sum(px) < sum(b)
                }
            });
            if better {
                best = Some(px);
            }
        }
    }
    best.unwrap()
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 24)
}

fn token(t: ColorToken) -> [u8; 3] {
    let c = ThemeState::get().color(t);
    [
        (c.r * 255.0) as u8,
        (c.g * 255.0) as u8,
        (c.b * 255.0) as u8,
    ]
}

#[test]
fn text_takes_the_theme_colours_in_both_schemes() {
    for (scheme, dark) in [(ColorScheme::Dark, true), (ColorScheme::Light, false)] {
        let Some(mut ui) = session(scheme) else {
            return;
        };
        mount(&mut ui, signal(false));
        let cells = all(&ui, "cn-table-cell");
        let heads = all(&ui, "cn-table-head");
        assert_eq!(cells.len(), 6, "cells carry their class");
        let primary = token(ColorToken::TextPrimary);
        let secondary = token(ColorToken::TextSecondary);
        for (name, node) in [("body", cells[0]), ("footer", cells[4])] {
            let got = ink(&mut ui, node, dark);
            assert!(
                near(got, primary),
                "{scheme:?} {name} text {got:?}, want {primary:?}"
            );
        }
        let got = ink(&mut ui, heads[0], dark);
        assert!(
            near(got, secondary),
            "{scheme:?} header text {got:?}, want {secondary:?}"
        );
    }
}

#[test]
fn a_selected_row_follows_its_signal() {
    let Some(mut ui) = session(ColorScheme::Dark) else {
        return;
    };
    let chosen = signal(false);
    mount(&mut ui, chosen);
    let row = all(&ui, "cn-table-row")[2];
    let state = |ui: &Headless| ui.tree().element_registry().attribute(row, "data-state");
    assert_eq!(state(&ui), None);
    chosen.set(true);
    ui.settle();
    assert_eq!(state(&ui).as_deref(), Some("selected"));
    chosen.set(false);
    ui.settle();
    assert_eq!(state(&ui), None);
}
