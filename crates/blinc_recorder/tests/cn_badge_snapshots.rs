#![cfg(feature = "headless")]
//! Every badge style and variant, with a static label and a bound one.

use blinc_cn::{BadgeStyle, BadgeVariant, badge};
use blinc_core::reactive::{State, global_dirty_flag, global_graph, signal};
use blinc_layout::binding::Reactive;
use blinc_layout::div::div;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 120;
const H: u32 = 40;

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

const VARIANTS: [(&str, BadgeVariant); 5] = [
    ("default", BadgeVariant::Default),
    ("secondary", BadgeVariant::Secondary),
    ("success", BadgeVariant::Success),
    ("warning", BadgeVariant::Warning),
    ("destructive", BadgeVariant::Destructive),
];
const STYLES: [(&str, BadgeStyle); 3] = [
    ("soft", BadgeStyle::Soft),
    ("solid", BadgeStyle::Solid),
    ("outline", BadgeStyle::Outline),
];

#[test]
fn every_style_and_variant() {
    let Some(mut ui) = session() else { return };
    for (sname, style) in STYLES {
        for (vname, variant) in VARIANTS {
            ui.mount(move |_| {
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .items_center()
                    .justify_center()
                    .child(badge("Status").variant(variant).style(style))
            });
            ui.settle();
            ui.assert_snapshot(&format!("cn_badge_{sname}_{vname}"));
        }
    }
}

#[test]
fn a_bound_label_looks_the_same() {
    let Some(mut ui) = session() else { return };
    for (sname, style) in STYLES {
        for (vname, variant) in VARIANTS {
            let label = State::new(
                signal::<String>("Status".to_string()),
                global_graph(),
                global_dirty_flag(),
            );
            ui.mount(move |_| {
                div()
                    .w(W as f32)
                    .h(H as f32)
                    .items_center()
                    .justify_center()
                    .child(
                        badge("")
                            .variant(variant)
                            .style(style)
                            .reactive_label(Reactive::Bound(label.clone())),
                    )
            });
            ui.settle();
            ui.assert_snapshot(&format!("cn_badge_bound_{sname}_{vname}"));
        }
    }
}
