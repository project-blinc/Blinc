#![cfg(feature = "headless")]

use blinc_cn::prelude::*;
use blinc_layout::{
    accessibility::{AccessibilityAction, AccessibilityLive, AccessibilityNode, AccessibilityRole},
    div::div,
};
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn session() -> Headless {
    let mut ui = Headless::new(640, 480).expect("offscreen GPU");
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    ui.mount(|_| div().size(640.0, 480.0)).settle();
    ui
}

#[track_caller]
fn semantic(ui: &Headless, role: AccessibilityRole) -> AccessibilityNode {
    let nodes = ui.tree().accessibility_nodes();
    if let Some(node) = nodes.iter().find(|n| n.role == role) {
        return node.clone();
    }
    panic!(
        "missing {role:?}; exposed: {:?}",
        nodes.iter().map(|n| (n.role, &n.label)).collect::<Vec<_>>()
    )
}

fn capture(ui: &mut Headless, name: &str) {
    if let Some(out) = std::env::var_os("BLINC_A11Y_ARTIFACTS") {
        let out = std::path::PathBuf::from(out);
        std::fs::create_dir_all(&out).unwrap();
        blinc_recorder::ScreenshotExporter::save_png(
            &ui.snapshot().unwrap(),
            out.join(format!("{name}.png")),
        )
        .unwrap();
    }
}

#[test]
fn dialog_keeps_controls_and_reactive_names_and_routes_native_actions() {
    let mut ui = session();
    let title = blinc_core::reactive::signal(String::from("Publish changes?"));
    let count = Arc::new(AtomicUsize::new(0));
    let hit = count.clone();
    dialog()
        .title(title)
        .description("Your workspace is ready to share.")
        .confirm_text("Publish")
        .on_confirm(move || {
            hit.fetch_add(1, Ordering::SeqCst);
        })
        .show();
    ui.settle();
    let panel = semantic(&ui, AccessibilityRole::Dialog);
    assert_eq!(panel.label, "Publish changes?");
    assert_eq!(panel.hint, "Your workspace is ready to share.");
    assert!(panel.modal);
    let publish = ui
        .tree()
        .accessibility_nodes()
        .into_iter()
        .find(|n| n.label == "Publish")
        .unwrap();
    assert_eq!(publish.parent, Some(panel.id));
    assert!(ui.accessibility_action(publish.id, AccessibilityAction::Focus));
    ui.settle();
    assert!(
        ui.tree()
            .accessibility_nodes()
            .iter()
            .any(|n| n.id == publish.id && n.focused)
    );
    title.set("Share workspace?".into());
    ui.settle();
    assert_eq!(
        semantic(&ui, AccessibilityRole::Dialog).label,
        "Share workspace?"
    );
    capture(&mut ui, "dialog");
    assert!(ui.accessibility_action(publish.id, AccessibilityAction::Activate));
    ui.settle();
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert!(!ui.accessibility_action(publish.id, AccessibilityAction::Activate));
}

#[test]
fn alert_dialog_registers_its_distinct_role() {
    let mut ui = session();
    alert_dialog()
        .title("Session expired")
        .description("Sign in again to continue.")
        .show();
    ui.settle();
    let alert = semantic(&ui, AccessibilityRole::AlertDialog);
    assert_eq!(alert.label, "Session expired");
    assert!(
        ui.tree()
            .accessibility_nodes()
            .iter()
            .any(|n| n.role == AccessibilityRole::Button)
    );
    capture(&mut ui, "alert-dialog");
}

#[test]
fn toast_registers_polite_live_semantics_and_keeps_its_dismiss_action() {
    let mut ui = session();
    let description = blinc_core::reactive::signal(String::from("Your draft is backed up."));
    toast("File saved")
        .persistent()
        .description(description)
        .show_close(true)
        .show();
    ui.settle();
    capture(&mut ui, "toast-initial");
    let status = semantic(&ui, AccessibilityRole::Status);
    assert!(status.label.contains("File saved") && status.label.contains("backed up"));
    assert_eq!(status.live.priority, AccessibilityLive::Polite);
    assert!(status.live.atomic);
    description.set("All changes are synced.".into());
    ui.settle();
    capture(&mut ui, "toast-updated");
    assert!(
        semantic(&ui, AccessibilityRole::Status)
            .label
            .contains("All changes are synced")
    );
    capture(&mut ui, "toast");
    let close = ui
        .tree()
        .accessibility_nodes()
        .into_iter()
        .find(|n| n.label == "Dismiss notification")
        .unwrap();
    assert!(ui.accessibility_action(close.id, AccessibilityAction::Activate));
    ui.settle();
    assert!(
        !ui.tree()
            .accessibility_nodes()
            .iter()
            .any(|n| n.role == AccessibilityRole::Status)
    );
}

#[test]
fn error_toast_registers_an_assertive_live_alert() {
    let mut ui = session();
    toast_error("Upload failed")
        .description("Try again when you are connected.")
        .show();
    ui.settle();
    let alert = semantic(&ui, AccessibilityRole::Alert);
    assert_eq!(alert.live.priority, AccessibilityLive::Assertive);
    assert!(alert.live.atomic);
}

#[test]
fn tooltip_and_popover_register_their_own_semantics() {
    let mut ui = session();
    ui.mount(|_| {
        div().p(24.0).child(
            tooltip(|| div().child(button("Help")))
                .text("Share this workspace")
                .open_delay_ms(0)
                .auto_hide_ms(60000),
        )
    })
    .settle();
    ui.hover(".cn-button").advance(400).settle();
    capture(&mut ui, "tooltip");
    assert_eq!(
        semantic(&ui, AccessibilityRole::Tooltip).label,
        "Share this workspace"
    );
    ui.leave().advance(400).settle();
    ui.mount(|_| {
        div().p(24.0).child(
            popover(|_| div().child(button("Settings")))
                .content(|| div().w(220.0).child(button("Manage workspace"))),
        )
    })
    .settle();
    ui.click(".cn-button").settle();
    let panel = semantic(&ui, AccessibilityRole::Dialog);
    assert_eq!(panel.label, "Settings");
    assert!(!panel.modal);
    assert!(
        ui.tree()
            .accessibility_nodes()
            .iter()
            .any(|n| n.label == "Manage workspace")
    );
    capture(&mut ui, "popover");
}
