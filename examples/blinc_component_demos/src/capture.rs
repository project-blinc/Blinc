//! Stream deterministic, full-resolution frames without retaining an animation in RAM.
mod cursor;
mod recording_motion;

use blinc_component_demos::{
    DEMO_CSS, FONTS, SLUGS,
    demos::{DemoState, build},
    is_chart, theme_bundle,
};
use blinc_recorder::{Headless, ScreenshotExporter};
use blinc_theme::ColorScheme;
use std::{
    error::Error,
    fs::{self, File},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(
        args.next()
            .ok_or("usage: capture-components OUTPUT [SLUG]")?,
    );
    let selected = args.next();
    if let Some(slug) = selected.as_deref() {
        if !SLUGS.contains(&slug) {
            return Err(format!("unknown component: {slug}").into());
        }
    }
    for &slug in SLUGS
        .iter()
        .filter(|slug| selected.as_deref().is_none_or(|s| s == **slug))
    {
        capture(&output, slug)?;
    }
    Ok(())
}

fn capture(output: &std::path::Path, slug: &str) -> Result<(), Box<dyn Error>> {
    let dir = output.join(slug);
    fs::create_dir_all(dir.join("frames"))?;
    let mut ui = Headless::with_scale(560, 320, 2.0)?;
    ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
    for font in FONTS {
        ui.load_font_data(font.to_vec());
    }
    let state = DemoState::default();
    let fixture = state.clone();
    let name = slug.to_string();
    ui.mount(move |ctx| build(&name, &fixture, (ctx.width, ctx.height)))
        .settle();
    if slug == "sparkline" {
        let frame = ui.snapshot()?;
        ScreenshotExporter::save_png(&frame, dir.join("poster.png"))?;
        ScreenshotExporter::save_png(&frame, dir.join("frames/000.png"))?;
        println!("captured {slug}: 560×320 @2x, static preview");
        return Ok(());
    }
    let mut gif = gif::Encoder::new(File::create(dir.join("preview.gif"))?, 1120, 640, &[])?;
    gif.set_repeat(gif::Repeat::Infinite)?;
    let plot = is_chart(slug).then(|| {
        let node = ui.find(".cn-chart-plot").expect("chart plot");
        ui.tree().get_absolute_bounds(node).expect("plot bounds")
    });
    let chart_at = |u: f32, v: f32| {
        let bounds = plot.expect("chart plot");
        (bounds.x + bounds.width * u, bounds.y + bounds.height * v)
    };
    let target = match slug {
        "input" => ui.centre(".cn-input"),
        "select" => ui.centre(".cn-select-trigger"),
        "pie-chart" | "donut-chart" => chart_at(0.83, 0.35),
        "horizontal-bar-chart" | "comparison-chart" => chart_at(0.45, 0.2),
        value if is_chart(value) => chart_at(0.25, 0.58),
        _ => ui.centre("#demo-action"),
    };
    let mut second = match slug {
        "pie-chart" | "donut-chart" => chart_at(0.25, 0.72),
        "horizontal-bar-chart" | "comparison-chart" => chart_at(0.65, 0.8),
        value if is_chart(value) => chart_at(0.78, 0.28),
        "button" | "input" => target,
        "progress" => ui.centre(".cn-progress"),
        _ => (target.0 + 12.0, target.1 - 2.0),
    };
    let single_target = match slug {
        "button" => Some("#demo-action"),
        "input" => Some(".cn-input"),
        _ => None,
    };
    let (mut visits, mut hovered_before) = (0, false);
    for index in 0..48 {
        if index == 20 && slug == "select" {
            let nodes = ui.tree().element_registry().class_to_nodes_index();
            let mut options = nodes
                .get("cn-select-item")
                .ok_or("select options missing")?
                .iter()
                .filter_map(|id| ui.tree().get_absolute_bounds(*id))
                .collect::<Vec<_>>();
            options.sort_by(|a, b| a.y.total_cmp(&b.y));
            let option = options.get(1).ok_or("second option missing")?;
            second = (
                option.x + option.width * 0.5,
                option.y + option.height * 0.5,
            );
        }
        if index == 20 && slug == "dialog" {
            second = ui.centre(".cn-button--primary");
        }
        let (x, y) = recording_motion::position(slug, index, target, second);
        ui.move_to(x, y);
        let first_click = !is_chart(slug);
        let second_click = matches!(slug, "select" | "dialog");
        if (index == 14 && first_click) || (index == 29 && second_click) {
            ui.mouse_down();
        }
        if (index == 16 && first_click) || (index == 31 && second_click) {
            ui.mouse_up();
        }
        if slug == "input" && (20..23).contains(&index) {
            let node = ui.find(".cn-input").expect("input");
            let mut event = blinc_layout::event_handler::EventContext::new(
                blinc_core::events::event_types::TEXT_INPUT,
                node,
            )
            .with_key_char(['A', 'd', 'a'][index - 20]);
            event.stable_id = ui.tree().stable_id(node).expect("input stable id");
            ui.tree().handler_registry().dispatch(&event);
        }
        if slug == "progress" && (16..36).contains(&index) {
            state
                .value
                .set((52.0 + (index - 16) as f32 * 2.6).min(100.0));
        }
        ui.advance(60);
        if let Some(selector) = single_target {
            let hovered = ui.is_hovered(selector);
            if hovered && !hovered_before {
                visits += 1;
            }
            hovered_before = hovered;
        }
        let pulse_start = if index >= 29 && second_click { 29 } else { 14 };
        let age = (index as f32 - pulse_start as f32) / 9.0;
        let pulse = (first_click && (0.0..1.0).contains(&age)).then_some(age);
        let pressed = (first_click && (14..16).contains(&index))
            || (second_click && (29..31).contains(&index));
        let frame = ui.snapshot_with_overlay(&cursor::annotation(x, y, pressed, pulse))?;
        ScreenshotExporter::save_png(&frame, dir.join(format!("frames/{index:03}.png")))?;
        if index == if is_chart(slug) { 8 } else { 0 } {
            ScreenshotExporter::save_png(&frame, dir.join("poster.png"))?;
        }
        let mut rgba = frame.data;
        let mut gif_frame = gif::Frame::from_rgba_speed(1120, 640, &mut rgba, 20);
        gif_frame.delay = 6;
        gif.write_frame(&gif_frame)?;
    }
    if single_target.is_some() {
        assert_eq!(visits, 1, "one recorded visit to {slug}");
    }
    match slug {
        "button" => assert_eq!(state.clicks.get(), 1, "recorded button click"),
        "input" => assert_eq!(state.name.get(), "Ada", "recorded text input"),
        "select" => assert_eq!(
            state.choice.get(),
            "typescript",
            "recorded option selection"
        ),
        "dialog" => assert!(state.confirmed.get(), "recorded dialog confirmation"),
        "progress" => assert_eq!(state.value.get(), 100.0, "recorded progress"),
        _ => {}
    }
    println!("recorded {slug}: 560×320 @2x, 48 frames with a glowing cursor");
    Ok(())
}
