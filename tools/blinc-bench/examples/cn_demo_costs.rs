#![allow(deprecated, dead_code)]
//! What a frame of the cn demo costs while the interaction a user is doing runs: its indeterminate
//! progress bar, an active scroll, a switch toggled and a slider dragged. The real demo is
//! mounted in a headless session and each scene is scripted a frame at a time, so no window is
//! driven. Run with
//!
//! ```text
//! cargo run --release -p blinc-bench --example cn_demo_costs
//! ```
//!
//! Each line gives the median and 90th percentile of the session's own frame and of the engine's
//! render with its submission (what a window pays), in milliseconds, and the paths the frames
//! took.

#[path = "../../../examples/blinc_app_examples/examples/cn_demo.rs"]
#[allow(dead_code)]
mod cn_demo;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use blinc_recorder::{Headless, MotionPaint};

const W: u32 = 900;
fn height() -> u32 {
    std::env::var("COSTS_H")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(900)
}

#[derive(Clone, Default)]
struct PathLog(Arc<Mutex<Vec<String>>>);

impl tracing::Subscriber for PathLog {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        matches!(
            metadata.target(),
            "blinc_app::frame_timing" | "blinc_app::node_rows"
        )
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        #[derive(Default)]
        struct Fields {
            message: String,
            path: String,
            outcome: String,
            all: String,
        }
        impl tracing::field::Visit for Fields {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                let text = format!("{value:?}").trim_matches('"').to_string();
                match field.name() {
                    "message" => self.message = text.clone(),
                    "path" => self.path = text.clone(),
                    "outcome" => self.outcome = text.clone(),
                    _ => {}
                }
                if field.name() != "message" {
                    self.all.push_str(&format!(" {}={}", field.name(), text));
                }
            }
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let mut seen = self.0.lock().unwrap();
        match fields.message.as_str() {
            "fast_path" => seen.push(fields.path),
            "node_rows" => seen.push(format!("rows:{}", fields.outcome)),
            "p4_breakdown" => seen.push("full".into()),
            "css_patch_gate_failed" => seen.push(format!("gate:{}", fields.all)),
            _ => {}
        }
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

struct Scene<'a> {
    ui: &'a mut Headless,
    log: PathLog,
    session: Vec<f64>,
    render: Vec<f64>,
    paths: BTreeMap<String, usize>,
    gates: BTreeMap<String, usize>,
}

impl<'a> Scene<'a> {
    fn new(ui: &'a mut Headless, log: PathLog) -> Self {
        Self {
            ui,
            log,
            session: Vec::new(),
            render: Vec::new(),
            paths: BTreeMap::new(),
            gates: BTreeMap::new(),
        }
    }

    /// One frame: time passes, the session runs, the engine renders as a window does.
    fn step(&mut self) {
        self.ui.advance(16);
        self.log.0.lock().unwrap().clear();
        let (session, render) = self.ui.paint_timed(true).expect("a frame");
        self.session.push(session);
        self.render.push(render);
        let seen = self.log.0.lock().unwrap();
        let painted = seen
            .iter()
            .rev()
            .find(|e| !e.starts_with("rows:") && !e.starts_with("gate:"))
            .cloned()
            .unwrap_or_else(|| "none".into());
        let rows = seen.iter().rev().find(|e| e.starts_with("rows:")).cloned();
        let tag = match rows {
            Some(rows) => format!("{painted} {rows}"),
            None => painted,
        };
        *self.paths.entry(tag).or_default() += 1;
        if let Some(gate) = seen.iter().rev().find(|e| e.starts_with("gate:")) {
            *self.gates.entry(gate.clone()).or_default() += 1;
        }
    }

    fn report(&self, name: &str) {
        let q = |v: &[f64], p: f64| {
            let mut v = v.to_vec();
            v.sort_by(|a, b| a.total_cmp(b));
            v[((v.len() as f64 * p) as usize).min(v.len() - 1)]
        };
        println!(
            "{name:<34} session {:>6.2} / {:>6.2}   render {:>6.2} / {:>6.2}   {:?}",
            q(&self.session, 0.5),
            q(&self.session, 0.9),
            q(&self.render, 0.5),
            q(&self.render, 0.9),
            self.paths
        );
        for (gate, n) in &self.gates {
            println!("    {n:>3} x {gate}");
        }
    }
}

fn session() -> Option<(Headless, PathLog)> {
    if !Headless::available() {
        return None;
    }
    let mut ui = Headless::with_motion_paint(W, height(), MotionPaint::Vector).ok()?;
    ui.theme(cn_demo::theme_bundle(), blinc_theme::ColorScheme::Dark);
    ui.mount(|ctx| cn_demo::build_ui(ctx));
    ui.settle();
    Some((ui, PathLog::default()))
}

/// The node a selector names. `@indeterminate-bar` is the bar of the first indeterminate progress.
fn find_node(ui: &Headless, selector: &str) -> Option<blinc_layout::tree::LayoutNodeId> {
    if selector != "@indeterminate-bar" {
        return ui.find(selector);
    }
    let tracks: Vec<_> = ui
        .tree()
        .element_registry()
        .class_to_nodes_index()
        .get("cn-progress")
        .map(|set| set.iter().copied().collect())
        .unwrap_or_default();
    // There may be several; the one nearest the top of the page that is on it.
    let mut bars: Vec<_> = tracks
        .into_iter()
        .filter(|n| {
            ui.tree()
                .element_registry()
                .attribute(*n, "data-indeterminate")
                .is_some()
        })
        .filter_map(|track| ui.tree().layout_tree.children(track).into_iter().next())
        .filter_map(|bar| {
            ui.tree()
                .layout_tree
                .get_absolute_bounds(bar)
                .map(|b| (b.y, bar))
        })
        .filter(|(y, _)| *y > 0.0)
        .collect();
    bars.sort_by(|a, b| a.0.total_cmp(&b.0));
    bars.first().map(|(_, bar)| *bar)
}

/// Scroll until the node is inside the window, `false` when it never is.
fn bring_into_view(ui: &mut Headless, selector: &str) -> bool {
    ui.move_to(450.0, 400.0);
    for _ in 0..200 {
        if let Some(node) = find_node(ui, selector)
            && let Some(b) = ui.tree().get_absolute_bounds(node)
            && b.y > 60.0
            && b.y + b.height < height() as f32 - 20.0
        {
            return true;
        }
        ui.scroll(0.0, -120.0);
        ui.advance(16);
        let _ = ui.paint_timed(true);
    }
    false
}

fn rest(ui: &mut Headless) {
    for _ in 0..30 {
        ui.advance(16);
        let _ = ui.paint_timed(true);
    }
}

fn main() {
    let Some((mut ui, log)) = session() else {
        eprintln!("no GPU adapter");
        return;
    };
    let _guard = tracing::subscriber::set_default(log.clone());
    ui.settle();
    rest(&mut ui);

    // `COSTS_ONLY_PAINT=<frames>`: nothing but full paints of the page, for a sampling profiler.
    if let Some(frames) = std::env::var("COSTS_ONLY_PAINT")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        let mut times = Vec::new();
        for _ in 0..frames {
            let (session, render) = ui.paint_timed(false).expect("a frame");
            times.push(session + render);
        }
        times.sort_by(|a, b| a.total_cmp(b));
        println!(
            "full paint: median {:.2} ms, p90 {:.2} ms over {frames} frames",
            times[times.len() / 2],
            times[times.len() * 9 / 10]
        );
        return;
    }

    // Which way a wheel delta scrolls the page, and whether it does.
    if std::env::var("COSTS_SCROLL_DIAG").is_ok() {
        let page = |ui: &Headless| -> f32 {
            let bar = ui.find(".cn-menubar").expect("the menubar");
            std::iter::once(bar)
                .chain(ui.tree().layout_tree.ancestors(bar))
                .map(|n| ui.tree().get_scroll_offset(n).1)
                .fold(0.0, |a: f32, b| if b.abs() > a.abs() { b } else { a })
        };
        ui.move_to(450.0, 500.0);
        println!("page offset at start {}", page(&ui));
        for delta in [-200.0f32, -200.0, 400.0] {
            ui.scroll(0.0, delta);
            ui.advance(16);
            let _ = ui.paint_timed(true);
            println!("after wheel {delta}: page offset {}", page(&ui));
        }
        return;
    }

    // What a frame at rest would have to be redrawn for.
    {
        let tree = ui.tree();
        println!(
            "at rest: css off rows {}, visual {}, layout {}, flip off rows {}, scroll physics {}, scroll stale {}, css active {}",
            tree.css_active_off_rows(),
            tree.has_active_visual_animations(),
            tree.has_active_layout_animations(),
            tree.has_flip_animations_off_rows(),
            tree.has_animating_scroll_physics(),
            tree.scroll_paint_stale(),
            tree.css_has_active(),
        );
        let store = tree.css_anim_store();
        let g = store.lock().unwrap();
        let playing = g.animations.values().filter(|a| a.is_playing).count();
        let trans = g.transitions.values().filter(|a| a.is_playing).count();
        println!(
            "css animations playing {playing} of {}, transitions playing {trans}",
            g.animations.len()
        );
    }

    // The page at rest, nothing moving.
    {
        let mut scene = Scene::new(&mut ui, log.clone());
        for _ in 0..40 {
            scene.step();
        }
        scene.report("top of the page, at rest");
    }

    // The pointer moved along the triggers of the hover menubar, with the page scrolled a little.
    {
        let page = |ui: &Headless| -> f32 {
            let bar = ui.find(".cn-menubar").expect("the menubar");
            std::iter::once(bar)
                .chain(ui.tree().layout_tree.ancestors(bar))
                .map(|n| ui.tree().get_scroll_offset(n).1)
                .fold(0.0, |a: f32, b| if b.abs() > a.abs() { b } else { a })
        };
        let scroll_by: f32 = std::env::var("COSTS_MENUBAR_SCROLL")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        ui.move_to(450.0, 400.0);
        if scroll_by != 0.0 {
            ui.scroll(0.0, -scroll_by);
            ui.advance(16);
        }
        rest(&mut ui);
        let overlay_open = || {
            blinc_layout::overlay_state::overlay_stack()
                .lock()
                .map(|s| s.has_visible_overlays())
                .unwrap_or(false)
        };
        let mut triggers: Vec<_> = ui
            .tree()
            .element_registry()
            .class_to_nodes_index()
            .get("cn-menubar-trigger")
            .map(|set| set.iter().copied().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|n| {
                ui.tree()
                    .layout_tree
                    .get_absolute_bounds(n)
                    .map(|b| (b.x, b.y, b.width, b.height))
            })
            .collect();
        triggers.sort_by(|a, b| a.0.total_cmp(&b.0));
        let start = page(&ui);
        println!(
            "menubar: window {}x{}, page at {start}, trigger at y {:.0} in the content, {} triggers",
            W,
            height(),
            triggers.first().map_or(0.0, |t| t.1),
            triggers.len()
        );
        for round in 0..2 {
            for (i, (x, y, w, h)) in triggers.iter().enumerate() {
                let offset = page(&ui);
                // The window position of the trigger's centre.
                let (px, py) = (x + w / 2.0, y + h / 2.0 + offset);
                ui.move_to(px, py);
                for _ in 0..16 {
                    ui.advance(16);
                    let _ = ui.paint_timed(true);
                }
                let content = ui
                    .find(".cn-menubar-content")
                    .and_then(|n| ui.tree().layout_tree.get_absolute_bounds(n));
                let viewport = blinc_layout::overlay_state::overlay_stack()
                    .lock()
                    .map(|s| s.viewport())
                    .unwrap_or((-1.0, -1.0));
                println!("    overlay stack viewport {viewport:?}");
                if round == 0 && i == 0 && std::env::var("COSTS_CHAIN").is_ok() {
                    if let Some(n) = ui.find(".cn-menubar-content") {
                        for a in std::iter::once(n).chain(ui.tree().layout_tree.ancestors(n)) {
                            let b = ui.tree().layout_tree.get_absolute_bounds(a);
                            let id = ui.tree().element_registry().get_id(a);
                            println!(
                                "      chain: {id:?} {:?}",
                                b.map(|b| (b.x, b.y, b.width, b.height))
                            );
                        }
                    }
                }
                println!(
                    "    round {round} trigger {i} (window y {py:.0}): page {} (was {start}), overlay open {}, content {}",
                    page(&ui),
                    overlay_open(),
                    content.map_or("none".to_string(), |b| format!(
                        "x {:.0} y {:.0} w {:.0} h {:.0}",
                        b.x, b.y, b.width, b.height
                    ))
                );
            }
        }
        ui.leave();
        rest(&mut ui);

        // The pointer along the items inside each open dropdown, then on to the next trigger.
        println!("  moving between the items of the dropdowns:");
        for (i, (x, y, w, h)) in triggers.iter().enumerate().take(3) {
            let offset = page(&ui);
            ui.move_to(x + w / 2.0, y + h / 2.0 + offset);
            for _ in 0..16 {
                ui.advance(16);
                let _ = ui.paint_timed(true);
            }
            let items: Vec<_> = ui
                .tree()
                .element_registry()
                .class_to_nodes_index()
                .get("cn-menubar-item")
                .map(|set| set.iter().copied().collect::<Vec<_>>())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|n| {
                    ui.tree()
                        .layout_tree
                        .get_absolute_bounds(n)
                        .map(|b| (b.x, b.y, b.width, b.height))
                })
                .collect();
            let mut items = items;
            items.sort_by(|a, b| a.1.total_cmp(&b.1));
            println!("    menu {i}: {} items, page {}", items.len(), page(&ui));
            for (j, (ix, iy, iw, ih)) in items.iter().enumerate() {
                // Overlay nodes are laid out in window coordinates.
                ui.move_to(ix + iw / 2.0, iy + ih / 2.0);
                for _ in 0..10 {
                    ui.advance(16);
                    let _ = ui.paint_timed(true);
                }
                println!(
                    "      item {j} at window y {:.0}: page {} (was {start}), hovered item row, overlay open {}",
                    iy + ih / 2.0,
                    page(&ui),
                    overlay_open()
                );
            }
        }
        ui.leave();
        rest(&mut ui);
    }

    // The first accordion, opened and closed three times.
    for round in 1..=3 {
        for what in ["open", "close"] {
            let mut scene = Scene::new(&mut ui, log.clone());
            scene.ui.move_to(450.0, 120.0);
            scene.ui.click(".cn-accordion-trigger");
            for _ in 0..40 {
                scene.step();
            }
            let total: f64 = scene.session.iter().chain(&scene.render).sum();
            scene.report(&format!("accordion {what}, round {round}"));
            println!("    total work over 40 frames: {total:.0} ms");
        }
    }

    // An active scroll from the top.
    {
        let mut scene = Scene::new(&mut ui, log.clone());
        scene.ui.move_to(450.0, 500.0);
        for _ in 0..90 {
            scene.ui.scroll(0.0, -36.0);
            scene.step();
        }
        scene.report("scrolling, 36 px a frame");
        scene.ui.scroll_end();
        rest(scene.ui);
    }

    if std::env::var("COSTS_FIND_DIAG").is_ok() {
        for sel in [
            ".cn-progress",
            ".cn-progress-bar",
            "[data-indeterminate]",
            ".cn-progress[data-indeterminate]",
            ".cn-switch",
            ".cn-slider",
            ".cn-menubar",
        ] {
            println!("find {sel:<36} -> {}", ui.find(sel).is_some());
        }
        for sel in ["@indeterminate-bar", ".cn-switch", ".cn-slider"] {
            let n = find_node(&ui, sel);
            let b = n.and_then(|n| ui.tree().get_absolute_bounds(n));
            println!(
                "node {sel:<22} -> {:?}",
                b.map(|b| (b.x as i32, b.y as i32, b.width as i32, b.height as i32))
            );
        }
        ui.move_to(450.0, 400.0);
        for _ in 0..6 {
            ui.scroll(0.0, -400.0);
            ui.advance(16);
            let _ = ui.paint_timed(true);
            let n = find_node(&ui, "@indeterminate-bar");
            let b = n.and_then(|n| ui.tree().get_absolute_bounds(n));
            println!(
                "after a wheel step: bar at {:?}",
                b.map(|b| (b.x as i32, b.y as i32))
            );
        }
        return;
    }

    if std::env::var("COSTS_ROW_BLOCKERS").is_ok() {
        // The page's scroll container: the scroll ancestor of the menubar.
        let bar = ui.find(".cn-menubar").expect("the menubar");
        let t = ui.tree();
        let page = t.layout_tree.ancestors(bar).into_iter().find(|n| {
            t.scrolled_offsets().iter().any(|e| e.0 == n.to_raw()) || t.is_scroll_container(*n)
        });
        if let Some(page) = page {
            println!("page on rows: {}", t.is_scroll_row_node(page));
            for (reason, n) in t.scroll_row_blockers(page) {
                println!("  {reason:<20} {n}");
            }
        }
        return;
    }

    if std::env::var("COSTS_BAR_STATE").is_ok() {
        // Let the animations run a while, as a window does, then ask the gate.
        for _ in 0..60 {
            ui.advance(16);
            let _ = ui.paint_timed(true);
        }
        {
            let t = ui.tree();
            println!(
                "gate: needs full paint {}, all promotable {}, flip off rows {}",
                t.animation_needs_full_paint(),
                t.css_active_all_composite_promotable(),
                t.has_flip_animations_off_rows()
            );
            let store = t.css_anim_store();
            let g = store.lock().unwrap();
            for (id, a) in g.animations.iter().filter(|(_, a)| a.is_playing) {
                let p = &a.current_properties;
                println!(
                    "  playing animation {:?}: opacity {:?} tx {:?} ty {:?} sx {:?} rot {:?} bg {:?}",
                    id,
                    p.opacity.is_some(),
                    p.translate_x.is_some(),
                    p.translate_y.is_some(),
                    p.scale_x.is_some(),
                    p.rotate.is_some(),
                    p.background_color.is_some()
                );
            }
        }
        let tracks: Vec<_> = ui
            .tree()
            .element_registry()
            .class_to_nodes_index()
            .get("cn-progress")
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default();
        for track in tracks {
            if ui
                .tree()
                .element_registry()
                .attribute(track, "data-indeterminate")
                .is_none()
            {
                continue;
            }
            let t = ui.tree();
            let Some(bar) = t.layout_tree.children(track).into_iter().next() else {
                continue;
            };
            let render = t.get_render_node(bar);
            println!(
                "indeterminate bar: carried {}, bindings {}, css off rows {}, css active {}, y {:?}, opacity {:?}, background solid {}, children {}",
                t.css_carried(bar),
                t.has_motion_bindings(bar),
                t.css_active_off_rows(),
                t.css_has_active(),
                t.get_absolute_bounds(bar).map(|b| b.y as i32),
                render.map(|r| r.props.opacity),
                render.is_some_and(|r| matches!(
                    r.props.background,
                    Some(blinc_core::Brush::Solid(_))
                )),
                t.layout_tree.children(bar).len(),
            );
        }
        return;
    }

    // Whether the CSS animations playing are carried by rows, and what the gate sees.
    if bring_into_view(
        &mut ui,
        ".cn-progress[data-indeterminate] > .cn-progress-bar",
    ) {
        rest(&mut ui);
        let bar = find_node(&ui, "@indeterminate-bar").expect("the bar");
        let t = ui.tree();
        println!(
            "indeterminate bar: carried by a binding {}, css off rows {}, css active {}, has bindings {}",
            t.css_carried(bar),
            t.css_active_off_rows(),
            t.css_has_active(),
            t.has_motion_bindings(bar),
        );
        if std::env::var("COSTS_BAR_ONLY").is_ok() {
            return;
        }
    }

    // The indeterminate bar, brought into view and left to play.
    if bring_into_view(&mut ui, "@indeterminate-bar") {
        rest(&mut ui);
        let mut scene = Scene::new(&mut ui, log.clone());
        for _ in 0..90 {
            scene.step();
        }
        scene.report("indeterminate progress, in view");
    } else {
        println!("indeterminate progress: never painted");
    }

    // A switch toggled.
    if bring_into_view(&mut ui, ".cn-switch") {
        rest(&mut ui);
        let mut scene = Scene::new(&mut ui, log.clone());
        scene.ui.click(".cn-switch");
        for _ in 0..40 {
            scene.step();
        }
        scene.report("switch toggled, 40 frames");
    } else {
        println!("switch: never painted");
    }

    // A slider dragged along its track.
    if bring_into_view(&mut ui, ".cn-slider") {
        rest(&mut ui);
        let (x, y) = ui.centre(".cn-slider");
        let mut scene = Scene::new(&mut ui, log.clone());
        scene.ui.move_to(x - 60.0, y);
        scene.ui.mouse_down();
        for i in 0..60 {
            scene.ui.move_to(x - 60.0 + i as f32 * 2.0, y);
            scene.step();
        }
        scene.ui.mouse_up();
        scene.report("slider dragged, 60 frames");
    } else {
        println!("slider: never painted");
    }
}
