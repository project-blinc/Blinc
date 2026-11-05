//! Stream deterministic, full-resolution frames without retaining an animation in RAM.
mod cursor;
mod recording_config;
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
    if selected.is_none() {
        let executable = std::env::current_exe()?;
        let mut failed = Vec::new();
        // A fixture owns its reactive graph, overlay stack, media and GPU session.
        for slug in SLUGS {
            if !std::process::Command::new(&executable)
                .arg(&output)
                .arg(slug)
                .status()?
                .success()
            {
                failed.push(*slug);
            }
        }
        if !failed.is_empty() {
            return Err(format!("recordings failed: {}", failed.join(", ")).into());
        }
        return Ok(());
    }
    let mut errors = Vec::new();
    for &slug in SLUGS
        .iter()
        .filter(|slug| selected.as_deref().is_none_or(|s| s == **slug))
    {
        if let Err(error) = capture(&output, slug) {
            eprintln!("FAILED {slug}: {error}");
            errors.push(slug);
        }
    }
    if !errors.is_empty() {
        return Err(format!("recordings failed: {}", errors.join(", ")).into());
    }
    Ok(())
}

fn capture(output: &std::path::Path, slug: &str) -> Result<(), Box<dyn Error>> {
    println!("capturing {slug}…");
    let dir = output.join(slug);
    fs::create_dir_all(dir.join("frames"))?;
    let config = recording_config::Fixture::load(slug);
    let viewport = &config.viewport;
    let (width, height) = (viewport.width, viewport.height);
    let (pixel_width, pixel_height) = (
        (width * viewport.scale) as u16,
        (height * viewport.scale) as u16,
    );
    let mut ui = Headless::with_scale(width, height, viewport.scale as f64)?;
    ui.theme(theme_bundle(), ColorScheme::Dark).css(DEMO_CSS);
    for font in FONTS {
        ui.load_font_data(font.to_vec());
    }
    let state = DemoState::default();
    let fixture = state.clone();
    let name = slug.to_string();
    ui.mount(move |ctx| build(&name, &fixture, (ctx.width, ctx.height)))
        .settle();
    if !config.animated() {
        let frame = ui.snapshot()?;
        ScreenshotExporter::save_png(&frame, dir.join("poster.png"))?;
        ScreenshotExporter::save_png(&frame, dir.join("frames/000.png"))?;
        println!("captured {slug}: {width}×{height} @2x, static preview");
        return Ok(());
    }
    let mut gif = gif::Encoder::new(
        File::create(dir.join("preview.gif"))?,
        pixel_width,
        pixel_height,
        &[],
    )?;
    gif.set_repeat(gif::Repeat::Infinite)?;
    let plot = is_chart(slug).then(|| {
        let node = ui.find(".cn-chart-plot").expect("chart plot");
        ui.tree().get_absolute_bounds(node).expect("plot bounds")
    });
    let chart_at = |u: f32, v: f32| {
        let bounds = plot.expect("chart plot");
        (bounds.x + bounds.width * u, bounds.y + bounds.height * v)
    };
    let interaction = config.recording.as_ref();
    let pointer = config
        .capabilities
        .iter()
        .any(|v| v == "pointer" || v == "keyboard");
    let frames = interaction.and_then(|s| s.frames).unwrap_or(48);
    let target = if !pointer {
        (0.0, 0.0)
    } else if let Some(script) = interaction {
        centre(&ui, &script.target, script.target_index)?
    } else {
        match slug {
            "input" => ui.centre(".cn-input"),
            "select" => ui.centre(".cn-select-trigger"),
            "pie-chart" | "donut-chart" => chart_at(0.83, 0.35),
            "horizontal-bar-chart" | "comparison-chart" => chart_at(0.45, 0.2),
            value if is_chart(value) => chart_at(0.25, 0.58),
            _ => ui.centre("#demo-action"),
        }
    };
    let mut second = if !pointer || interaction.is_some() {
        target
    } else {
        match slug {
            "pie-chart" | "donut-chart" => chart_at(0.25, 0.72),
            "horizontal-bar-chart" | "comparison-chart" => chart_at(0.65, 0.8),
            value if is_chart(value) => chart_at(0.78, 0.28),
            "button" | "input" => target,
            "progress" => ui.centre(".cn-progress"),
            _ => (target.0 + 12.0, target.1 - 2.0),
        }
    };
    let single_target = match slug {
        "button" => Some("#demo-action"),
        "input" => Some(".cn-input"),
        _ => None,
    };
    let (mut visits, mut hovered_before) = (0, false);
    let target = if slug == "code-editor" {
        let bounds = ui
            .tree()
            .get_absolute_bounds(ui.find(".blinc-code").unwrap())
            .unwrap();
        let mut code = state.code.lock().unwrap();
        let column = code.lines[1]
            .find("light")
            .ok_or("code example must contain light")?;
        let x = code.config.gutter_width + code.config.padding + code.measure_chars(column) + 0.1;
        (
            bounds.x + x,
            bounds.y + code.config.padding + 1.5 * code.config.font_size * code.config.line_height,
        )
    } else {
        target
    };
    for index in 0..frames {
        if let Some(script) = interaction {
            if index == 20 {
                if let Some(selector) = &script.secondary {
                    second = centre(&ui, selector, script.secondary_index)?;
                } else if script.kind == "drag" {
                    second = (target.0 + 80.0, target.1);
                }
            }
        }
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
        let (x, y) = if let Some(script) = interaction {
            if script.secondary.is_some() || script.kind == "drag" {
                recording_motion::position(slug, index, target, second)
            } else {
                recording_motion::position(
                    "button",
                    if frames > 48 {
                        if index < frames - 12 {
                            index.min(35)
                        } else {
                            36 + index - (frames - 12)
                        }
                    } else {
                        index
                    },
                    target,
                    target,
                )
            }
        } else {
            recording_motion::position(slug, index, target, second)
        };
        ui.move_to(x, y);
        let first_click = interaction.map_or(pointer && !is_chart(slug), |s| {
            pointer && !matches!(s.kind.as_str(), "hover" | "scroll" | "context")
        });
        let second_click = interaction.map_or(matches!(slug, "select" | "dialog"), |s| {
            s.secondary.is_some() && s.kind != "hover"
        });
        if let Some(script) = interaction {
            if script.kind == "context" && index == 16 {
                ui.right_click(&script.target);
            }
            if script.kind == "scroll" && (16..32).contains(&index) {
                ui.scroll(0.0, -18.0);
            }
            if script.kind == "type" {
                if index == 18 && slug == "code-editor" {
                    for _ in 0..5 {
                        ui.key_with_shift(39, true);
                    }
                }
                if index == 18 && slug == "rich-text-editor" {
                    ui.key(40).key(35).key(13);
                }
                if index >= 22 && (index - 22) % 2 == 0 {
                    if let Some(ch) = script
                        .text
                        .as_deref()
                        .ok_or("typing recording needs text")?
                        .chars()
                        .nth((index - 22) / 2)
                    {
                        ui.type_text(&ch.to_string());
                    }
                }
            }
        }
        let drag = interaction.is_some_and(|s| s.kind == "drag");
        if (index == 14 && first_click) || (index == 29 && second_click) {
            ui.mouse_down();
        }
        if (index == if drag { 28 } else { 16 } && first_click) || (index == 31 && second_click) {
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
        if slug == "video-player" {
            std::thread::sleep(std::time::Duration::from_millis(60));
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
        let annotation = if (width, height) == (560, 320) {
            cursor::annotation(x, y, pressed, pulse)
        } else {
            cursor::annotation_in((width as f32, height as f32), x, y, pressed, pulse)
        };
        let frame = if pointer {
            ui.snapshot_with_overlay(&annotation)?
        } else {
            ui.snapshot()?
        };
        ScreenshotExporter::save_png(&frame, dir.join(format!("frames/{index:03}.png")))?;
        if index == if is_chart(slug) { 8 } else { 0 } {
            ScreenshotExporter::save_png(&frame, dir.join("poster.png"))?;
        }
        let mut rgba = frame.data;
        let mut gif_frame = gif::Frame::from_rgba_speed(pixel_width, pixel_height, &mut rgba, 20);
        gif_frame.delay = 6;
        gif.write_frame(&gif_frame)?;
    }
    if single_target.is_some() {
        assert_eq!(visits, 1, "one recorded visit to {slug}");
    }
    match slug {
        "checkbox" | "switch" | "toggle" | "popover" => {
            assert!(state.confirmed.get(), "recorded toggle")
        }
        "radio-group" => {
            assert_eq!(state.choice.get(), "typescript", "recorded choice")
        }
        "toggle-group" => assert_eq!(state.choice.get(), "left", "recorded alignment"),
        "number-input" => assert_eq!(state.number.get(), 43.0, "recorded stepper"),
        "textarea" => assert_eq!(
            state.name.get(),
            "A clearer way to build UI.",
            "recorded textarea"
        ),
        "input-otp" => assert_eq!(state.name.get(), "123456", "recorded OTP"),
        "code-editor" => assert_eq!(
            state.code.lock().unwrap().value(),
            "fn main() {\n    let theme = \"dark\";\n    println!(\"Theme: {theme}\");\n}",
            "recorded code edit"
        ),
        "rich-text-editor" => assert_eq!(
            state.document.lock().unwrap().document.plain_text(),
            "Project notes\nBuild interfaces that feel natural.\nKeep the details clear.",
            "recorded rich text edit"
        ),
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
    if let Some(player) = state.audio.get() {
        player.stop();
    }
    if let Some(player) = state.video.get() {
        player.stop();
    }
    println!("recorded {slug}: {width}×{height} @2x, {frames} frames");
    Ok(())
}

fn centre(ui: &Headless, selector: &str, index: usize) -> Result<(f32, f32), Box<dyn Error>> {
    if !selector.starts_with('.') {
        let node = ui
            .find(selector)
            .ok_or_else(|| format!("missing recording target {selector}"))?;
        let b = ui
            .tree()
            .get_absolute_bounds(node)
            .ok_or("target has no bounds")?;
        return Ok((b.x + b.width * 0.5, b.y + b.height * 0.5));
    }
    let registry = ui.tree().element_registry().class_to_nodes_index();
    let mut bounds = registry
        .get(selector.trim_start_matches('.'))
        .into_iter()
        .flatten()
        .filter_map(|id| ui.tree().get_absolute_bounds(*id))
        .filter(|b| b.width > 0.0 && b.height > 0.0)
        .collect::<Vec<_>>();
    bounds.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    let b = bounds
        .get(index)
        .ok_or_else(|| format!("missing recording target {selector}[{index}]"))?;
    Ok((b.x + b.width * 0.5, b.y + b.height * 0.5))
}
