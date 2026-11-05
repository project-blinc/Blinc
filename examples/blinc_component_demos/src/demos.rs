use blinc_cn as cn;
use blinc_cn::{
    ButtonVariant, ChartBand, area_chart, bar_chart, button, comparison_bar_chart, dialog,
    histogram, input, line_chart, pie_chart, progress, select, spark_line, threshold_line_chart,
};
use blinc_core::reactive::{Signal, computed, signal};
use blinc_layout::{
    binding::IntoReactive,
    prelude::*,
    widgets::text_input::{SharedTextInputData, text_input_data},
};

/// One session owns its bindings; remounting creates a fresh session.
#[derive(Clone)]
pub struct DemoState {
    pub clicks: Signal<u32>,
    pub disabled: Signal<bool>,
    pub value: Signal<f32>,
    pub choice: Signal<String>,
    pub name: Signal<String>,
    pub confirmed: Signal<bool>,
    pub input: SharedTextInputData,
    pub number: Signal<f64>,
    pub range: Signal<std::ops::RangeInclusive<f64>>,
    pub page: Signal<usize>,
    pub date: Signal<Option<cn::CalendarDay>>,
    pub time: Signal<cn::TimeOfDay>,
    pub color: Signal<blinc_core::Color>,
    pub multiline: blinc_layout::widgets::text_area::SharedTextAreaState,
    pub code: blinc_layout::widgets::code::SharedCodeEditorState,
    pub audio: std::rc::Rc<std::cell::OnceCell<std::rc::Rc<blinc_media::AudioPlayer>>>,
    pub video: std::rc::Rc<std::cell::OnceCell<std::rc::Rc<blinc_media::VideoPlayer>>>,
    pub document: blinc_layout::widgets::rich_text_editor::RichTextState,
}

impl Default for DemoState {
    fn default() -> Self {
        Self {
            clicks: signal(0),
            disabled: signal(false),
            value: signal(42.0),
            choice: signal("rust".to_string()),
            name: signal(String::new()),
            confirmed: signal(false),
            input: text_input_data(),
            number: signal(42.0),
            range: signal(20.0..=80.0),
            page: signal(1),
            date: signal(Some(cn::CalendarDay::new(2026, 11, 4))),
            time: signal(cn::TimeOfDay::new(14, 30, 0).unwrap()),
            color: signal(blinc_core::Color::from_hex(0x7da8ff)),
            multiline: blinc_layout::widgets::text_area::text_area_state(),
            code: code_editor_state(
                "fn main() {\n    let theme = \"light\";\n    println!(\"Theme: {theme}\");\n}\n",
            ),
            audio: Default::default(),
            video: Default::default(),
            document: blinc_layout::widgets::rich_text_editor::rich_text_state(
                blinc_layout::widgets::rich_text_editor::RichDocument::from_markdown(
                    "## Project notes\n\nBuild interfaces that feel natural.",
                    blinc_core::Color::TRANSPARENT,
                ),
            ),
        }
    }
}

pub fn build(slug: &str, state: &DemoState, viewport: (f32, f32)) -> Div {
    let content = match slug {
        "button" => button_demo(state),
        "input" => input_demo(state),
        "select" => select_demo(state),
        "dialog" => dialog_demo(state),
        "progress" => progress_demo(state, viewport),
        "line-chart" | "charts" => chart_demo(state, viewport),
        "area-chart" => area_demo(state, viewport),
        "stacked-area-chart" => stacked_area_demo(state, viewport),
        "bar-chart" => bar_demo(state, viewport),
        "stacked-bar-chart" => stacked_bar_demo(state, viewport),
        "horizontal-bar-chart" => horizontal_bar_demo(state, viewport),
        "pie-chart" => pie_demo(state, viewport),
        "donut-chart" => donut_demo(state, viewport),
        "sparkline" => sparkline_demo(state, viewport),
        "threshold-chart" => threshold_demo(state, viewport),
        "histogram" => histogram_demo(state, viewport),
        "comparison-chart" => comparison_demo(state, viewport),

        _ => crate::gallery::build(slug, state, viewport),
    };
    div()
        .class("catalog-demo")
        .class(&format!("demo--{slug}"))
        .child(content)
}

pub fn button_demo(state: &DemoState) -> Div {
    let clicks = state.clicks;
    div()
        .class("demo-wide")
        .class("demo-stack--center")
        .child(
            button("Click me")
                .disabled(state.disabled)
                .on_click(move |_| clicks.set(clicks.get() + 1))
                .build_component()
                .id("demo-action"),
        )
        .child(
            bound_text(computed(move |graph| {
                format!("Clicked {} times", graph.get(clicks).unwrap_or(0))
            }))
            .class("demo-caption"),
        )
        .child(
            div()
                .class("demo-row")
                .child(
                    cn::button("Secondary")
                        .variant(cn::ButtonVariant::Secondary)
                        .disabled(state.disabled),
                )
                .child(
                    cn::button("Outline")
                        .variant(cn::ButtonVariant::Outline)
                        .disabled(state.disabled),
                )
                .child(
                    cn::button("Ghost")
                        .variant(cn::ButtonVariant::Ghost)
                        .disabled(state.disabled),
                ),
        )
        .child(
            div()
                .class("demo-row")
                .child(
                    cn::button("Remove")
                        .variant(cn::ButtonVariant::Destructive)
                        .disabled(state.disabled),
                )
                .child(
                    cn::button("Documentation")
                        .variant(cn::ButtonVariant::Link)
                        .disabled(state.disabled),
                ),
        )
}

pub fn input_demo(state: &DemoState) -> Div {
    let name = state.name;
    div()
        .class("demo-stack")
        .child(
            input(&state.input)
                .label("Your name")
                .placeholder("Ada Lovelace")
                .disabled(state.disabled)
                .on_change(move |value| name.set(value.to_string())),
        )
        .child(
            bound_text(computed(move |graph| {
                let value = graph.get(name).unwrap_or_default();
                if value.is_empty() {
                    "Say hello.".to_string()
                } else {
                    format!("Hello, {value}.")
                }
            }))
            .class("demo-caption"),
        )
}

pub fn select_demo(state: &DemoState) -> Div {
    let choice = state.choice;
    div()
        .class("demo-stack")
        .child(
            select(choice)
                .label("Language")
                .disabled(state.disabled)
                .option("rust", "Rust")
                .option("typescript", "TypeScript")
                .option("python", "Python"),
        )
        .child(
            bound_text(computed(move |graph| {
                format!("Selected: {}", graph.get(choice).unwrap_or_default())
            }))
            .class("demo-caption"),
        )
}

pub fn dialog_demo(state: &DemoState) -> Div {
    let confirmed = state.confirmed;
    div()
        .class("demo-stack")
        .class("demo-stack--center")
        .child(
            button("Open dialog")
                .variant(ButtonVariant::Outline)
                .disabled(state.disabled)
                .on_click(move |_| {
                    dialog()
                        .title("Publish this draft?")
                        .description("Your changes are ready to share.")
                        .confirm_text("Publish")
                        .cancel_text("Keep editing")
                        .on_confirm(move || confirmed.set(true))
                        .show();
                })
                .build_component()
                .id("demo-action"),
        )
        .child(
            bound_text(computed(move |graph| {
                if graph.get(confirmed).unwrap_or(false) {
                    "Published.".to_string()
                } else {
                    "Draft saved.".to_string()
                }
            }))
            .class("demo-caption"),
        )
}

pub fn progress_demo(state: &DemoState, viewport: (f32, f32)) -> Div {
    let value = state.value;
    div()
        .class("demo-stack")
        .class("demo-stack--center")
        .child(text("Uploading your project").class("demo-label"))
        .child(progress(value).w((viewport.0 - 48.0).clamp(1.0, 256.0)))
        .child(
            bound_text(computed(move |graph| {
                format!("{:.0}% complete", graph.get(value).unwrap_or(0.0))
            }))
            .class("demo-caption"),
        )
        .child(
            button("Advance")
                .variant(ButtonVariant::Outline)
                .disabled(state.disabled)
                .on_click(move |_| value.set((value.get() + 10.0).min(100.0)))
                .build_component()
                .id("demo-action"),
        )
}

pub fn chart_demo(_state: &DemoState, viewport: (f32, f32)) -> Div {
    div().class("demo-chart").child(
        line_chart()
            .width((viewport.0 - 48.0).clamp(1.0, 420.0))
            .height((viewport.1 - 120.0).clamp(40.0, 200.0))
            .series("Visitors", &[24.0, 38.0, 32.0, 56.0, 48.0, 72.0])
            .series("Returning", &[12.0, 18.0, 16.0, 24.0, 28.0, 40.0])
            .labels(
                ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
                    .map(String::from)
                    .to_vec(),
            )
            .dots(),
    )
}

pub fn area_demo(_state: &DemoState, viewport: (f32, f32)) -> Div {
    div().class("demo-chart").child(
        area_chart()
            .width((viewport.0 - 48.0).clamp(1.0, 420.0))
            .height((viewport.1 - 120.0).clamp(40.0, 200.0))
            .series("Desktop", &[186.0, 305.0, 237.0, 273.0, 209.0, 214.0])
            .series("Mobile", &[80.0, 200.0, 120.0, 190.0, 130.0, 140.0])
            .labels(
                ["Jan", "Feb", "Mar", "Apr", "May", "Jun"]
                    .map(String::from)
                    .to_vec(),
            ),
    )
}

pub fn stacked_area_demo(_state: &DemoState, viewport: (f32, f32)) -> Div {
    div().class("demo-chart").child(
        area_chart()
            .width((viewport.0 - 48.0).clamp(1.0, 420.0))
            .height((viewport.1 - 120.0).clamp(40.0, 200.0))
            .series("Desktop", &[186.0, 305.0, 237.0, 273.0, 209.0, 214.0])
            .series("Mobile", &[80.0, 200.0, 120.0, 190.0, 130.0, 140.0])
            .labels(
                ["Jan", "Feb", "Mar", "Apr", "May", "Jun"]
                    .map(String::from)
                    .to_vec(),
            )
            .stacked(),
    )
}

pub fn bar_demo(_state: &DemoState, viewport: (f32, f32)) -> Div {
    div().class("demo-chart").child(
        bar_chart()
            .width((viewport.0 - 48.0).clamp(1.0, 420.0))
            .height((viewport.1 - 120.0).clamp(40.0, 200.0))
            .series("Desktop", &[186.0, 305.0, 237.0, 273.0, 209.0, 214.0])
            .series("Mobile", &[80.0, 200.0, 120.0, 190.0, 130.0, 140.0])
            .labels(
                ["Jan", "Feb", "Mar", "Apr", "May", "Jun"]
                    .map(String::from)
                    .to_vec(),
            ),
    )
}

pub fn stacked_bar_demo(_state: &DemoState, viewport: (f32, f32)) -> Div {
    div().class("demo-chart").child(
        bar_chart()
            .width((viewport.0 - 48.0).clamp(1.0, 420.0))
            .height((viewport.1 - 120.0).clamp(40.0, 200.0))
            .series("Desktop", &[186.0, 305.0, 237.0, 273.0, 209.0, 214.0])
            .series("Mobile", &[80.0, 200.0, 120.0, 190.0, 130.0, 140.0])
            .labels(
                ["Jan", "Feb", "Mar", "Apr", "May", "Jun"]
                    .map(String::from)
                    .to_vec(),
            )
            .stacked(),
    )
}

pub fn horizontal_bar_demo(_state: &DemoState, viewport: (f32, f32)) -> Div {
    div().class("demo-chart").child(
        bar_chart()
            .width((viewport.0 - 48.0).clamp(1.0, 420.0))
            .height((viewport.1 - 120.0).clamp(40.0, 200.0))
            .series("Desktop", &[186.0, 305.0, 237.0, 273.0, 209.0, 214.0])
            .series("Mobile", &[80.0, 200.0, 120.0, 190.0, 130.0, 140.0])
            .labels(
                ["Jan", "Feb", "Mar", "Apr", "May", "Jun"]
                    .map(String::from)
                    .to_vec(),
            )
            .horizontal(),
    )
}

pub fn pie_demo(_state: &DemoState, viewport: (f32, f32)) -> Div {
    div().class("demo-pie").child(
        pie_chart()
            .slice("Chrome", 275.0)
            .slice("Safari", 200.0)
            .slice("Firefox", 90.0)
            .size(
                (viewport.0 - 48.0)
                    .min(viewport.1 - 100.0)
                    .clamp(80.0, 200.0),
            ),
    )
}

pub fn donut_demo(_state: &DemoState, viewport: (f32, f32)) -> Div {
    div().class("demo-pie").child(
        pie_chart()
            .slice("Chrome", 275.0)
            .slice("Safari", 200.0)
            .slice("Firefox", 90.0)
            .size(
                (viewport.0 - 48.0)
                    .min(viewport.1 - 100.0)
                    .clamp(80.0, 200.0),
            )
            .donut(0.6)
            .total("Visitors"),
    )
}

pub fn sparkline_demo(_state: &DemoState, viewport: (f32, f32)) -> Div {
    div()
        .class("demo-stack")
        .class("demo-stack--center")
        .child(text("Weekly trend").class("demo-label"))
        .child(
            spark_line(vec![12.0, 18.0, 15.0, 24.0, 20.0, 32.0, 38.0])
                .width((viewport.0 - 48.0).clamp(1.0, 256.0))
                .height(80.0)
                .filled(),
        )
        .child(text("A compact view of the same series").class("demo-caption"))
}

pub fn threshold_demo(_state: &DemoState, viewport: (f32, f32)) -> Div {
    div().class("demo-chart").child(
        threshold_line_chart()
            .values(vec![12.0, 18.0, 15.0, 22.0, 14.0, 28.0, 19.0])
            .band(ChartBand::new(0.0, 16.67).label("60 fps"))
            .band(ChartBand::new(16.67, 33.33).label("30 fps"))
            .baseline(16.67)
            .width((viewport.0 - 48.0).clamp(1.0, 420.0))
            .height((viewport.1 - 120.0).clamp(40.0, 200.0))
            .format(|value| format!("{value:.1} ms")),
    )
}

pub fn histogram_demo(_state: &DemoState, viewport: (f32, f32)) -> Div {
    let values = (0..80)
        .map(|i| ((i * 17 % 31) as f64) / 5.0)
        .collect::<Vec<_>>();
    div().class("demo-chart").child(
        histogram(values)
            .bins(12)
            .marker(4.0)
            .height((viewport.1 - 120.0).clamp(40.0, 200.0)),
    )
}

pub fn comparison_demo(_state: &DemoState, viewport: (f32, f32)) -> Div {
    div().class("demo-chart").child(
        comparison_bar_chart()
            .item("Render", 12.5, 9.8)
            .item("Layout", 3.2, 4.1)
            .item("Paint", 8.4, 7.2)
            .threshold(10.0)
            .height((viewport.1 - 120.0).clamp(40.0, 200.0))
            .format(|value| format!("{value:.1} ms")),
    )
}

pub(crate) fn bound_text(value: impl IntoReactive<String>) -> Text {
    Text::bound(value.into_reactive())
}
