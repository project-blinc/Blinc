use crate::demos::{DemoState, bound_text};
use blinc_cn as cn;
use blinc_core::reactive::computed;
use blinc_layout::prelude::*;

pub fn code_block_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div().class("demo-wide").child(
        code("fn main() {\n    let message = \"Hello, Blinc!\";\n    println!(\"{message}\");\n}")
            .syntax(SyntaxConfig::new(RustHighlighter::new()))
            .line_numbers(true)
            .font_size(13.0)
            .w_full(),
    )
}

pub fn code_editor_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let name = values.name;
    div()
        .class("demo-wide")
        .child(
            code_editor(&values.code)
                .syntax(SyntaxConfig::new(RustHighlighter::new()))
                .line_numbers(true)
                .font_size(13.0)
                .search_enabled(true)
                .on_change(move |text| name.set(text.to_string()))
                .w_full()
                .h(216.0),
        )
        .child(text("Edit Rust · ⌘/Ctrl F to find").class("demo-caption"))
}

pub fn rich_text_editor_demo(values: &DemoState, viewport: (f32, f32)) -> Div {
    div()
        .class("demo-wide")
        .child(blinc_layout::widgets::rich_text_editor::rich_text_editor(
            &values.document,
            blinc_layout::widgets::rich_text_editor::RichTextTheme::default(),
            (viewport.0 - 80.0).clamp(1.0, 380.0),
        ))
}

pub fn markdown_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div().class("demo-wide").child(markdown("## From notes to interfaces\n\nWrite **clear ideas** with *emphasis*, `inline code` and lists.\n\n- Native rendering\n- Shared typography\n- Your theme"))
}

pub fn virtual_list_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div().class("demo-wide").child(
        blinc_layout::widgets::virtual_list::virtual_list(10_000, |index| {
            div()
                .class("demo-virtual-row")
                .child(text(format!("Message {:05}", index + 1)))
        })
        .estimated_item_height(40.0)
        .window_size(24)
        .w_full()
        .h(216.0)
        .class("demo-virtual-list")
        .into_div(),
    )
}

pub fn gestures_demo(values: &DemoState, _: (f32, f32)) -> Div {
    use blinc_layout::widgets::gesture::GestureExt;
    let message = values.choice;
    div()
        .class("demo-stack")
        .child(
            div()
                .class("demo-gesture")
                .id("demo-action")
                .child(text("Tap, hold or swipe"))
                .on_tap(move |_| message.set("Tapped".into()))
                .on_long_press(move |_| message.set("Long press".into()))
                .on_swipe(move |direction, _| message.set(format!("Swiped {direction:?}"))),
        )
        .child(
            bound_text(computed(move |g| {
                let message = g.get(message).unwrap_or_default();
                if message == "rust" {
                    "Try an interaction".into()
                } else {
                    message
                }
            }))
            .class("demo-caption"),
        )
}

pub fn motion_demo(values: &DemoState, _: (f32, f32)) -> Div {
    use blinc_animation::AnimationPreset;
    use blinc_layout::motion::StaggerConfig;
    let visible = values.confirmed;
    div()
        .class("demo-wide")
        .class("demo-activity")
        .child(
            div()
                .class("demo-activity-heading")
                .child(text("Workspace activity").class("demo-label"))
                .child(
                    cn::button("Toggle feed")
                        .variant(cn::ButtonVariant::Outline)
                        .on_click(move |_| visible.set(!visible.get()))
                        .build_component()
                        .id("demo-action"),
                ),
        )
        .child(div().class("demo-activity-feed").show_or(
            visible,
            || {
                motion()
                    .id("demo-activity-motion")
                    .gap(8.0)
                    .stagger(StaggerConfig::new(90, AnimationPreset::fade_in(320)))
                    .exit_animation(AnimationPreset::fade_out(180))
                    .children(
                        [
                            (
                                "Design review",
                                "Maya shared the updated layouts",
                                "2 min ago",
                            ),
                            (
                                "Build passed",
                                "All checks are ready for review",
                                "8 min ago",
                            ),
                            (
                                "Release notes",
                                "Alex added the latest changes",
                                "15 min ago",
                            ),
                        ]
                        .map(|(title, detail, time)| {
                            div()
                                .class("demo-activity-item")
                                .child(text(title).class("demo-label"))
                                .child(text(detail).class("demo-caption"))
                                .child(text(time).class("demo-activity-time"))
                        }),
                    )
            },
            || {
                div()
                    .class("demo-activity-empty")
                    .child(text("You're all caught up.").class("demo-caption"))
            },
        ))
}

pub fn lists_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div()
        .class("demo-wide")
        .child(
            ul().child(li().child(text("A clear idea")))
                .child(li().child(text("A thoughtful interface"))),
        )
        .child(
            ol().child(li().child(text("Prototype")))
                .child(li().child(text("Build and share"))),
        )
}

pub fn blockquote_demo(_: &DemoState, _: (f32, f32)) -> Div {
    div().class("demo-wide").child(
        blockquote()
            .child(text("Make the important things easy to understand."))
            .child(text("A little clarity goes a long way.").class("demo-caption")),
    )
}

pub fn link_demo(values: &DemoState, _: (f32, f32)) -> Div {
    let clicked = values.confirmed;
    div()
        .class("demo-stack")
        .class("demo-stack--center")
        .child(
            link(
                "Read the guide",
                "https://blinc.rs/components/installation/",
            )
            .underline_on_hover()
            .on_click(move |_, _| clicked.set(true))
            .id("demo-action"),
        )
        .child(
            bound_text(computed(move |g| {
                if g.get(clicked).unwrap_or(false) {
                    "Link activated"
                } else {
                    "Links can handle application navigation."
                }
                .into()
            }))
            .class("demo-caption"),
        )
}

pub fn svg_demo(_: &DemoState, _: (f32, f32)) -> Div {
    let illustration = include_str!("../../assets/fennec-fox.svg");
    let mut samples = div().class("demo-svg-samples");
    for (size, label, class) in [
        (48.0, "48 px", "demo-svg-small"),
        (96.0, "96 px", "demo-svg-medium"),
        (160.0, "160 px", "demo-svg-large"),
    ] {
        samples = samples.child(
            div()
                .class("demo-svg-sample")
                .child(
                    div().class("demo-svg-artwork").child(
                        svg(illustration)
                            .size(size, size * 179.2 / 240.0)
                            .class(class),
                    ),
                )
                .child(text(label).class("demo-caption")),
        );
    }
    div()
        .class("demo-wide")
        .child(samples)
        .child(text("One vector · three sizes · theme-aware tint").class("demo-caption"))
}
