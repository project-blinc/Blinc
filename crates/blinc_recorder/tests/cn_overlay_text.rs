#![cfg(feature = "headless")]
//! Text in an open overlay follows the signal it was given: a dialog's title,
//! description and buttons, a sheet's and a drawer's title, a toast's
//! description. The text node stays; only its content changes.

use blinc_core::reactive::{Signal, signal};
use blinc_layout::LayoutNodeId;
use blinc_layout::div::div;
use blinc_layout::renderer::ElementType;
use blinc_recorder::Headless;
use blinc_theme::{ColorScheme, HybridTheme};

fn session() -> Option<Headless> {
    if !Headless::available() {
        eprintln!("no GPU adapter: skipped");
        return None;
    }
    let mut ui = Headless::new(800, 600).ok()?;
    ui.theme(
        HybridTheme::bundle().with_css(blinc_cn::cn_styles::CN_STYLES),
        ColorScheme::Dark,
    );
    ui.mount(|_| div().w(800.0).h(600.0));
    ui.leave().settle();
    Some(ui)
}

/// The node showing `content`, if any.
fn text_node(ui: &Headless, content: &str) -> Option<LayoutNodeId> {
    let tree = ui.tree();
    let mut stack = vec![tree.root()?];
    while let Some(id) = stack.pop() {
        if let Some(node) = tree.get_render_node(id)
            && let ElementType::Text(t) = &node.element_type
            && t.content == content
        {
            return Some(id);
        }
        stack.extend(tree.layout_tree.children(id));
    }
    None
}

/// Moves `text` from `from` to `to` and checks the same node shows it.
fn follows(ui: &mut Headless, text: Signal<String>, from: &str, to: &str) {
    let node = text_node(ui, from).unwrap_or_else(|| panic!("no {from:?} shown"));
    text.set(to.to_string());
    ui.settle();
    assert_eq!(
        text_node(ui, to),
        Some(node),
        "{from:?} did not become {to:?} in place"
    );
}

#[test]
fn a_dialog_follows_its_signals() {
    let Some(mut ui) = session() else { return };
    let (title, desc) = (
        signal("Delete file?".to_string()),
        signal("Gone".to_string()),
    );
    let (confirm, cancel) = (signal("Delete".to_string()), signal("Keep".to_string()));
    let handle = blinc_cn::dialog()
        .title(title)
        .description(desc)
        .confirm_text(confirm)
        .cancel_text(cancel)
        .show();
    ui.settle();
    follows(&mut ui, title, "Delete file?", "Delete 3 files?");
    follows(&mut ui, desc, "Gone", "Gone for good");
    follows(&mut ui, confirm, "Delete", "Delete all");
    follows(&mut ui, cancel, "Keep", "Keep them");
    handle.close();
    ui.settle();
}

#[test]
fn sheet_and_drawer_titles_follow_their_signals() {
    let Some(mut ui) = session() else { return };
    let (title, desc) = (signal("Settings".to_string()), signal("Yours".to_string()));
    let sheet = blinc_cn::sheet().title(title).description(desc).show();
    ui.settle();
    follows(&mut ui, title, "Settings", "Preferences");
    follows(&mut ui, desc, "Yours", "Shared");
    sheet.close();
    ui.settle();

    let title = signal("Menu".to_string());
    let drawer = blinc_cn::drawer().title(title).show();
    ui.settle();
    follows(&mut ui, title, "Menu", "Navigation");
    drawer.close();
    ui.settle();
}

#[test]
fn a_toast_description_follows_its_signal() {
    let Some(mut ui) = session() else { return };
    let desc = signal("Uploading 1 of 3".to_string());
    let toast = blinc_cn::toast("Upload").description(desc).show();
    ui.settle();
    follows(&mut ui, desc, "Uploading 1 of 3", "Uploading 2 of 3");
    toast.dismiss();
    ui.settle();
}
