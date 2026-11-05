use crate::demos::{DemoState, bound_text};
use blinc_cn as cn;
use blinc_core::reactive::computed;
use blinc_layout::prelude::*;

pub fn card_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div().class("demo-card-width").child(
        cn::card()
            .w_full()
            .class("demo-workspace-card")
            .child(
                cn::card_header()
                    .title("Website redesign")
                    .description("Team project"),
            )
            .child(cn::card_content().child(text(
                "Navigation, forms and feedback, ready for design review.",
            )))
            .child(
                cn::card_footer().child(
                    div()
                        .w_full()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            cn::avatar_group()
                                .child(cn::avatar().fallback("AL"))
                                .child(cn::avatar().fallback("MJ"))
                                .child(cn::avatar().fallback("TK")),
                        )
                        .child(
                            cn::badge("In review").icon(
                                svg(blinc_icons::to_svg_with_stroke(
                                    blinc_icons::icons::CLOCK,
                                    12.0,
                                    2.0,
                                ))
                                .square(12.0),
                            ),
                        ),
                ),
            ),
    )
}

pub fn badge_demo(_: &DemoState, _: (f32, f32)) -> Div {
    let mut samples = div().class("demo-wide").class("demo-badges");
    for (label, style) in [
        ("Soft", cn::BadgeStyle::Soft),
        ("Solid", cn::BadgeStyle::Solid),
        ("Outline", cn::BadgeStyle::Outline),
    ] {
        let mut row = div().class("demo-row").class("demo-badge-row");
        for (name, variant) in [
            ("In review", cn::BadgeVariant::Default),
            ("Draft", cn::BadgeVariant::Secondary),
            ("Shipped", cn::BadgeVariant::Success),
            ("Pending", cn::BadgeVariant::Warning),
            ("Blocked", cn::BadgeVariant::Destructive),
        ] {
            let mut badge = cn::badge(name).variant(variant).style(style);
            if name == "Shipped" {
                badge = badge.icon(
                    svg(blinc_icons::to_svg_with_stroke(
                        blinc_icons::icons::CHECK,
                        12.0,
                        2.0,
                    ))
                    .square(12.0),
                );
            }
            row = row.child(badge);
        }
        samples = samples.child(
            div()
                .class("demo-badge-group")
                .child(text(label).class("demo-caption"))
                .child(row),
        );
    }
    samples
}

pub fn alert_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div()
        .class("demo-wide")
        .child(cn::alert("Your changes have been saved.").variant(cn::AlertVariant::Success))
        .child(
            cn::alert_box()
                .variant(cn::AlertVariant::Warning)
                .title("A little heads up")
                .description("Your trial ends in seven days."),
        )
}

pub fn avatar_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div()
        .class("demo-stack")
        .class("demo-stack--center")
        .child(
            cn::avatar()
                .fallback("AL")
                .online()
                .size(cn::AvatarSize::Large),
        )
        .child(
            cn::avatar_group()
                .max(3)
                .child(cn::avatar().fallback("AL"))
                .child(cn::avatar().fallback("JD"))
                .child(cn::avatar().fallback("MK"))
                .child(cn::avatar().fallback("RS")),
        )
}

pub fn separator_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div()
        .class("demo-stack")
        .child(text("Account settings").class("demo-label"))
        .child(cn::separator())
        .child(
            div()
                .class("demo-row")
                .child(text("Profile"))
                .child(cn::separator().vertical().h(20.0))
                .child(text("Security")),
        )
}

pub fn aspect_ratio_demo(_: &DemoState, _: (f32, f32)) -> Div {
    let mut grid = div().class("demo-aspect-presets");
    for (label, ratio) in [
        ("1:1", 1.0),
        ("16:9", 16.0 / 9.0),
        ("4:3", 4.0 / 3.0),
        ("21:9", 21.0 / 9.0),
        ("3:2", 1.5),
        ("2:3", 2.0 / 3.0),
        ("9:16", 9.0 / 16.0),
        ("3:4", 0.75),
    ] {
        grid = grid.child(
            div().class("demo-aspect-sample").child(
                cn::aspect_ratio(ratio).w(84.0).child(
                    div()
                        .class("demo-ratio")
                        .child(text(label).class("demo-ratio-label")),
                ),
            ),
        );
    }
    grid
}

pub fn kbd_demo(_: &DemoState, _: (f32, f32)) -> Div {
    let mut shortcuts = div().class("demo-shortcuts");
    for (action, key) in [
        ("Search commands", "K"),
        ("New document", "N"),
        ("Save changes", "S"),
        ("Preferences", ","),
    ] {
        shortcuts = shortcuts.child(
            div().class("demo-shortcut-row").child(text(action)).child(
                div()
                    .class("demo-shortcut-keys")
                    .child(cn::kbd("⌘"))
                    .child(cn::kbd(key)),
            ),
        );
    }
    shortcuts.child(text("Use Ctrl on Windows and Linux").class("demo-caption"))
}

pub fn icon_demo(_: &DemoState, _: (f32, f32)) -> Div {
    use blinc_icons::icons;
    let mut grid = div().class("demo-icon-grid");
    for (name, source) in [
        ("Search", icons::SEARCH),
        ("Heart", icons::HEART),
        ("Settings", icons::SETTINGS),
        ("Check", icons::CHECK),
        ("Folder", icons::FOLDER),
        ("File", icons::FILE),
        ("User", icons::USER),
        ("Bell", icons::BELL),
        ("Calendar", icons::CALENDAR),
        ("Clock", icons::CLOCK),
        ("Sun", icons::SUN),
        ("Moon", icons::MOON),
    ] {
        grid = grid.child(
            div()
                .class("demo-icon-cell")
                .child(cn::icon(source).size(cn::IconSize::Medium))
                .child(text(name).class("demo-caption")),
        );
    }
    grid
}

pub fn typography_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div()
        .class("demo-wide")
        .child(cn::h2("Ideas deserve a voice."))
        .child(cn::p(
            "A shared type scale makes every screen feel connected.",
        ))
        .child(cn::muted("Supporting copy uses the same theme tokens."))
        .child(cn::inline_code("let idea = signal(\"Hello\");"))
}

pub fn table_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div().class("demo-wide").child(
        cn::table()
            .child(
                cn::table_header().child(
                    cn::table_row()
                        .child(cn::table_head("Project"))
                        .child(cn::table_head("Status")),
                ),
            )
            .child(
                cn::table_body()
                    .child(
                        cn::table_row()
                            .child(cn::table_cell().child(text("Website")))
                            .child(cn::table_cell().child(text("Published"))),
                    )
                    .child(
                        cn::table_row()
                            .child(cn::table_cell().child(text("Mobile app")))
                            .child(cn::table_cell().child(text("In review"))),
                    )
                    .child(
                        cn::table_row()
                            .child(cn::table_cell().child(text("Design system")))
                            .child(cn::table_cell().child(text("Draft"))),
                    ),
            ),
    )
}

pub fn tree_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let choice = values.choice;
    div()
        .class("demo-stack")
        .child(
            cn::tree_view()
                .with_guides()
                .node("src", "src", |node| {
                    node.expanded().child("main", "main.rs", |node| node).child(
                        "ui",
                        "ui.rs",
                        |node| node,
                    )
                })
                .node("assets", "assets", |node| {
                    node.child("theme", "theme.css", |node| node)
                })
                .on_select(move |key| choice.set(key.to_string())),
        )
        .child(
            bound_text(computed(move |g| {
                g.get(choice)
                    .filter(|v| v != "rust")
                    .map(|v| format!("Selected: {v}"))
                    .unwrap_or_else(|| "Select a file".into())
            }))
            .class("demo-caption"),
        )
}

pub fn skeleton_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div()
        .class("demo-stack")
        .child(
            div()
                .class("demo-row")
                .child(cn::skeleton_circle(44.0))
                .child(
                    div()
                        .class("demo-skeleton-text")
                        .child(cn::skeleton().w(160.0).h(14.0))
                        .child(cn::skeleton().w(112.0).h(12.0)),
                ),
        )
        .child(cn::skeleton().w_full().h(80.0))
}

pub fn spinner_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div()
        .class("demo-stack")
        .class("demo-stack--center")
        .child(cn::spinner().size(cn::SpinnerSize::Large))
        .child(text("Preparing your workspace").class("demo-caption"))
}

pub fn scroll_area_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div().class("demo-stack").child(
        cn::scroll_area().h(208.0).child(
            div()
                .class("demo-scroll-content")
                .children((1..=16).map(|i| text(format!("Chapter {i:02}")).class("demo-list-row"))),
        ),
    )
}

pub fn resizable_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div().class("demo-wide").h(200.0).child(
        cn::resizable_group()
            .horizontal()
            .panel(
                cn::resizable_panel()
                    .default_size(150.0)
                    .min_size(80.0)
                    .child(div().class("demo-resize-panel").child(text("Files"))),
            )
            .panel(
                cn::resizable_panel().flex_grow().min_size(80.0).child(
                    div()
                        .class("demo-resize-panel")
                        .child(text("Your workspace")),
                ),
            ),
    )
}

pub fn collapsible_demo(values: &DemoState, _: (f32, f32)) -> Div {
    div().class("demo-stack").child(cn::collapsible_section(
        "Project details",
        values.confirmed,
        || {
            div()
                .class("demo-collapsible-copy")
                .child(text("Design, build and ship with a shared language."))
        },
    ))
}

pub fn accordion_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div().class("demo-wide").child(
        cn::accordion()
            .item("start", "What is Blinc?", || {
                div().child(text("A native UI framework built with Rust."))
            })
            .item("style", "Can I make it my own?", || {
                div().child(text(
                    "Use CSS classes and theme variables to restyle every part.",
                ))
            }),
    )
}
