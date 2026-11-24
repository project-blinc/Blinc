//! The rich text editor's input-method edits: candidates replace text within its document.

use blinc_cn::components::rich_text_editor::*;
use blinc_layout::{div::div, ime::ImeEdit, renderer::RenderTree, widgets::text_input::*};
use std::sync::{Arc, Mutex, atomic::AtomicBool};

static LOCK: Mutex<()> = Mutex::new(());
fn init() -> std::sync::MutexGuard<'static, ()> {
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        blinc_theme::ThemeState::init(
            blinc_theme::HybridTheme::bundle(),
            blinc_theme::ColorScheme::Dark,
        );
        blinc_core::BlincContextState::init(
            blinc_core::reactive::global_graph(),
            Arc::new(Mutex::new(blinc_core::context_state::HookState::new())),
            Arc::new(AtomicBool::new(false)),
        );
    });
    blur_all_text_inputs();
    guard
}

fn edit(tree: &RenderTree, text: &str, selection: [usize; 2], composing: Option<[usize; 2]>) {
    let snapshot = tree.ime_snapshot().unwrap();
    assert!(tree.apply_ime_edit(
        snapshot.id,
        &ImeEdit {
            session: snapshot.content.edit.session,
            sequence: snapshot.content.edit.sequence + 1,
            text: text.into(),
            selection,
            composing,
        }
    ));
}

#[test]
fn rich_text_candidates_preserve_the_surrounding_document() {
    let _guard = init();
    let document = RichDocument::from_blocks(vec![
        Block::paragraph("Hello 🦊", blinc_core::Color::WHITE),
        Block::paragraph("Next", blinc_core::Color::WHITE),
    ]);
    let data = rich_text_state(document);
    let widget = rich_text_editor(&data, RichTextTheme::default(), 400.0);
    let mut tree = RenderTree::from_element(&div().w(400.0).h(300.0).child(widget));
    tree.compute_layout(400.0, 300.0);
    data.lock().unwrap().set_focus(true);
    edit(&tree, "Hello 🦊ni\nNext", [10, 10], Some([8, 10]));
    edit(&tree, "Hello 🦊你\nNext", [9, 9], None);
    let mut d = data.lock().unwrap();
    assert_eq!(d.document.blocks.len(), 2);
    assert_eq!(d.document.plain_text(), "Hello 🦊你\nNext");
    assert!(d.undo());
    assert_eq!(d.document.plain_text(), "Hello 🦊\nNext");
    assert!(!d.undo());
    d.set_focus(false);
}
