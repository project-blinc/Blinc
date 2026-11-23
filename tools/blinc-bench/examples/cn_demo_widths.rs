//! The cn_demo page at narrow widths: which elements stick out of their parent or past the
//! window's right edge.
//!
//! ```text
//! cargo run --release -p blinc-bench --example cn_demo_widths
//! WIDTHS=360,420 WIDTHS_SNAP=/tmp/cn cargo run --release -p blinc-bench --example cn_demo_widths
//! ```
//!
//! - `WIDTHS`: comma-separated window widths in logical pixels (default `360,480,640,768`).
//! - `WIDTHS_SNAP=<prefix>`: also save the page in viewport-high strips, `<prefix>-<w>-<n>.png`.
//! - `WIDTHS_SECTION=<title>`: with `WIDTHS_SNAP`, save only the section with that title, scrolled
//!   to the top of the window, and the two windows below it, as `<prefix>-<w>-<title>-<n>.png`.
//!
//! An element is reported when the part of it that shows (up to the nearest clipping ancestor)
//! passes its parent's right edge by more than a pixel, and the parent does not clip or scroll
//! it. Only the first element on each path is reported, with its classes and the classes of the
//! nearest `cn-` component above it.

#![allow(dead_code)]

#[path = "../../../examples/blinc_app_examples/examples/cn_demo.rs"]
mod cn_demo;

use blinc_layout::RenderTree;
use blinc_layout::renderer::ElementType;
use blinc_layout::tree::LayoutNodeId;
use blinc_recorder::{Headless, MotionPaint};
use std::collections::HashMap;

const H: u32 = 900;

fn widths() -> Vec<u32> {
    std::env::var("WIDTHS")
        .unwrap_or_else(|_| "360,480,640,768".into())
        .split(',')
        .filter_map(|w| w.trim().parse().ok())
        .collect()
}

fn session(width: u32) -> Option<Headless> {
    let mut ui = Headless::with_motion_paint_at(width, H, 1.0, MotionPaint::Vector).ok()?;
    ui.gate_state_styles().gate_animation_apply();
    ui.theme(cn_demo::theme_bundle(), blinc_theme::ColorScheme::Dark);
    ui.mount(cn_demo::build_ui);
    ui.settle();
    // `on_ready` callbacks land on a thread after a wall-clock delay.
    std::thread::sleep(std::time::Duration::from_millis(300));
    ui.settle();
    Some(ui)
}

/// The classes of each node.
fn classes(tree: &RenderTree) -> HashMap<LayoutNodeId, Vec<String>> {
    let mut out: HashMap<LayoutNodeId, Vec<String>> = HashMap::new();
    for (class, nodes) in tree.element_registry().class_to_nodes_index() {
        for node in nodes {
            out.entry(node).or_default().push(class.clone());
        }
    }
    for list in out.values_mut() {
        list.sort();
    }
    out
}

fn describe(
    tree: &RenderTree,
    classes: &HashMap<LayoutNodeId, Vec<String>>,
    node: LayoutNodeId,
) -> String {
    let own = classes.get(&node).map(|c| c.join(".")).unwrap_or_default();
    let id = tree
        .element_registry()
        .get_id(node)
        .map(|id| format!("#{id}"))
        .unwrap_or_default();
    let kind = match tree.get_render_node(node).map(|r| &r.element_type) {
        Some(ElementType::Text(t)) => {
            format!("text {:?}", t.content.chars().take(30).collect::<String>())
        }
        Some(ElementType::Div) => "div".into(),
        Some(other) => format!("{:?}", std::mem::discriminant(other)),
        None => "?".into(),
    };
    let component = tree
        .layout_tree
        .ancestors(node)
        .into_iter()
        .find_map(|a| {
            classes
                .get(&a)
                .filter(|c| c.iter().any(|c| c.starts_with("cn-")))
                .map(|c| c.join("."))
        })
        .unwrap_or_default();
    let section = tree
        .layout_tree
        .ancestors(node)
        .into_iter()
        .find_map(|a| title(tree, a))
        .unwrap_or_default();
    format!("[{section}] {kind} {id} .{own}  in .{component}")
}

/// The title of a demo section: its first child, a large text.
fn title(tree: &RenderTree, node: LayoutNodeId) -> Option<String> {
    let first = *tree.layout_tree.children(node).first()?;
    match tree.get_render_node(first).map(|r| &r.element_type) {
        Some(ElementType::Text(t)) if t.font_size >= 18.0 => Some(t.content.clone()),
        _ => None,
    }
}

fn clips(tree: &RenderTree, node: LayoutNodeId) -> bool {
    tree.is_scroll_container(node)
        || tree
            .get_render_node(node)
            .is_some_and(|r| r.props.clips_content)
}

fn absolute(tree: &RenderTree, node: LayoutNodeId) -> bool {
    tree.layout_tree
        .style(node)
        .is_some_and(|s| format!("{:?}", s.position) == "Absolute")
}

fn report(ui: &Headless, width: u32) {
    let tree = ui.tree();
    let classes = classes(tree);
    let Some(root) = tree.root() else {
        return;
    };
    let mut found = Vec::new();
    // (node, its parent, the right edge of the nearest clipping ancestor, whether an ancestor
    // already overflowed)
    let mut stack = vec![(root, None, width as f32, false)];
    while let Some((node, parent, clip_right, settled)) = stack.pop() {
        let bounds = tree.get_absolute_bounds(node);
        let mut done = settled;
        if !settled && let Some(b) = bounds {
            // What shows of the node stops at the clip above it.
            let right = (b.x + b.width).min(clip_right);
            let past_parent = parent
                .filter(|p| !clips(tree, *p))
                .and_then(|p| tree.get_absolute_bounds(p))
                .is_some_and(|pb| right > pb.x + pb.width + 1.0);
            if past_parent && !absolute(tree, node) && b.width > 0.0 {
                found.push(format!(
                    "  x {:>6.1}..{:>6.1}{} {}",
                    b.x,
                    b.x + b.width,
                    if right >= width as f32 - 1.0 {
                        " to the edge"
                    } else {
                        ""
                    },
                    describe(tree, &classes, node)
                ));
                done = true;
            }
        }
        let clip_right = match bounds {
            Some(b) if clips(tree, node) => clip_right.min(b.x + b.width),
            _ => clip_right,
        };
        for child in tree.layout_tree.children(node).into_iter().rev() {
            stack.push((child, Some(node), clip_right, done));
        }
    }
    println!("width {width}: {} overflowing", found.len());
    for line in found {
        println!("{line}");
    }
}

fn snap(ui: &mut Headless, width: u32, prefix: &str) {
    ui.move_to(4.0, H as f32 / 2.0);
    for n in 0..40 {
        let frame = ui.snapshot().expect("a frame");
        blinc_recorder::ScreenshotExporter::save_png(
            &frame,
            format!("{prefix}-{width}-{n:02}.png"),
        )
        .expect("saved");
        let before = frame.data.clone();
        for _ in 0..((H - 100) / 120) {
            ui.scroll(0.0, -120.0);
            ui.advance(16);
        }
        ui.settle();
        if ui.snapshot().expect("a frame").data == before {
            break;
        }
    }
}

/// Scroll the section titled `title` to the top of the window and save the frame.
fn snap_section(ui: &mut Headless, width: u32, prefix: &str, title_text: &str) {
    let top = |ui: &Headless| -> Option<f32> {
        let tree = ui.tree();
        let mut stack = vec![tree.root()?];
        while let Some(node) = stack.pop() {
            if title(tree, node).as_deref() == Some(title_text) {
                return tree.get_absolute_bounds(node).map(|b| b.y);
            }
            stack.extend(tree.layout_tree.children(node));
        }
        None
    };
    ui.move_to(4.0, H as f32 / 2.0);
    let mut last = None;
    for _ in 0..400 {
        // Absolute bounds include the scroll offsets above the node.
        let Some(on_screen) = top(ui) else {
            eprintln!("no section {title_text:?}");
            return;
        };
        // Near the top, or as near as the end of the page lets it come.
        if (80.0..200.0).contains(&on_screen) || last == Some(on_screen) {
            break;
        }
        last = Some(on_screen);
        if std::env::var_os("WIDTHS_DEBUG").is_some() {
            eprintln!("section at {on_screen}");
        }
        let step = -(on_screen - 140.0).clamp(-600.0, 600.0);
        ui.scroll(0.0, step);
        ui.advance(16);
        ui.settle();
    }
    ui.settle();
    // The section, and two windows further down for a long one.
    for part in 0..3 {
        let frame = ui.snapshot().expect("a frame");
        blinc_recorder::ScreenshotExporter::save_png(
            &frame,
            format!("{prefix}-{width}-{title_text}-{part}.png"),
        )
        .expect("saved");
        ui.scroll(0.0, -(H as f32 - 160.0));
        ui.advance(16);
        ui.settle();
    }
}

fn main() {
    if !Headless::available() {
        eprintln!("no GPU adapter");
        return;
    }
    let prefix = std::env::var("WIDTHS_SNAP").ok();
    for width in widths() {
        let Some(mut ui) = session(width) else {
            eprintln!("no session at {width}");
            continue;
        };
        report(&ui, width);
        if let Some(prefix) = &prefix {
            match std::env::var("WIDTHS_SECTION") {
                Ok(title) => {
                    for title in title.split(',') {
                        snap_section(&mut ui, width, prefix, title.trim());
                    }
                }
                Err(_) => snap(&mut ui, width, prefix),
            }
        }
    }
}
