#![cfg(feature = "headless")]
//! Dialogs and sheets enter and leave by CSS rules: the backdrop fades, the
//! dialog stays until its exit has played, a sheet slides by its own size,
//! and a rule of the app's own overrides the timing.

use blinc_cn::cn;
use blinc_core::Color;
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::overlay_state::overlay_stack;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

const W: u32 = 480;
const H: u32 = 360;
const PAGE: Color = Color {
    r: 0.2,
    g: 0.25,
    b: 0.3,
    a: 1.0,
};

fn session(extra_css: &str) -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(W, H).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(format!("{}\n{extra_css}", blinc_cn::cn_styles::CN_STYLES)),
        ColorScheme::Dark,
    );
    ui.mount(|_| div().w(W as f32).h(H as f32).bg(PAGE));
    ui.leave().settle();
    Some(ui)
}

fn all(ui: &Headless, class: &str) -> Vec<LayoutNodeId> {
    let tree = ui.tree();
    let registry = tree.element_registry();
    let mut found = Vec::new();
    let mut stack = vec![tree.root().unwrap()];
    while let Some(node) = stack.pop() {
        if registry.has_class(node, class) {
            found.push(node);
        }
        stack.extend(tree.layout_tree.children(node));
    }
    found
}

/// The red channel at a page corner the backdrop covers: lower is darker.
fn dimness(ui: &mut Headless) -> u8 {
    ui.snapshot().unwrap().get_pixel(10, 10).unwrap()[0]
}

fn motion(ui: &Headless, class: &str) -> Option<String> {
    let node = *all(ui, class).first()?;
    ui.tree().element_registry().attribute(node, "data-motion")
}

#[test]
fn a_dialog_fades_in_and_stays_until_its_exit_has_played() {
    let Some(mut ui) = session("") else { return };
    let page = dimness(&mut ui);
    let handle = cn::dialog().title("Delete file?").show();
    ui.advance(16);
    assert_eq!(motion(&ui, "cn-dialog-motion").as_deref(), Some("enter"));
    let entering = dimness(&mut ui);
    ui.settle();
    let open = dimness(&mut ui);
    assert!(
        open < entering && entering < page,
        "the backdrop fades in: page {page}, entering {entering}, open {open}"
    );

    handle.close();
    ui.advance(40);
    assert_eq!(
        motion(&ui, "cn-dialog-motion").as_deref(),
        Some("exit"),
        "still there, leaving"
    );
    let leaving = dimness(&mut ui);
    assert!(
        leaving > open && leaving < page,
        "the backdrop fades out: {leaving}"
    );
    ui.settle();
    assert!(
        all(&ui, "cn-dialog-motion").is_empty(),
        "gone once its exit played"
    );
    assert!(overlay_stack().lock().unwrap().is_empty());
    assert_eq!(dimness(&mut ui), page);
}

/// A sheet slides in from beyond its edge by its own width: early on, its
/// panel covers only the window's last columns.
#[test]
fn a_sheet_slides_by_its_own_width() {
    let Some(mut ui) = session("") else { return };
    let handle = cn::sheet().title("Settings").show();
    ui.advance(16);
    let early = ui.snapshot().unwrap().get_pixel(W - 150, 200).unwrap();
    ui.settle();
    let open = ui.snapshot().unwrap().get_pixel(W - 150, 200).unwrap();
    // 150px in from the edge: the page early on, the panel once open.
    let apart: u32 = (0..3).map(|c| early[c].abs_diff(open[c]) as u32).sum();
    assert!(
        apart > 15,
        "the panel had not arrived early: early {early:?}, open {open:?}"
    );
    handle.close();
    ui.settle();
    assert!(overlay_stack().lock().unwrap().is_empty());
}

/// A rule of the app's own replaces the dialog's enter: a slower fade is
/// still under way when the default would have finished.
#[test]
fn an_app_rule_overrides_the_enter() {
    let Some(mut ui) = session(
        r#".cn-dialog-motion[data-motion="enter"] { animation: cn-fade-in 2000ms linear; }"#,
    ) else {
        return;
    };
    let handle = cn::dialog().title("Slow").show();
    ui.advance(400);
    let wrapper = all(&ui, "cn-dialog-motion")[0];
    let id = ui
        .tree()
        .element_registry()
        .get_id(wrapper)
        .expect("the wrapper's id");
    assert!(
        blinc_layout::css_animation_playing(&id),
        "the app's 2s fade is still playing at 400ms"
    );
    handle.close();
    ui.settle();
}

/// A toast slides in from beyond its side, and stays while it slides back
/// out after it is dismissed.
#[test]
fn a_toast_slides_in_and_out() {
    let Some(mut ui) = session("") else { return };
    // Bottom right, 16px in: its middle column sits 196px from the right.
    let (x, y) = (W - 196, H - 40);
    let page = ui.snapshot().unwrap().get_pixel(x, y).unwrap();
    let handle = cn::toast("Saved")
        .description("Your changes are saved.")
        .show();
    ui.advance(16);
    let early = ui.snapshot().unwrap().get_pixel(x, y).unwrap();
    ui.settle();
    let shown = ui.snapshot().unwrap().get_pixel(x, y).unwrap();
    assert_eq!(early, page, "early on, the toast is still beyond the edge");
    assert_ne!(shown, page, "it has slid in");
    assert!(all(&ui, "cn-toast-motion-right").len() == 1);

    handle.dismiss();
    ui.advance(30);
    assert_eq!(
        motion(&ui, "cn-toast-motion-right").as_deref(),
        Some("exit")
    );
    ui.settle();
    assert!(
        all(&ui, "cn-toast-motion-right").is_empty(),
        "gone once it slid out"
    );
    assert!(
        blinc_layout::overlay_state::toast_tray()
            .lock()
            .unwrap()
            .is_empty()
    );
}
