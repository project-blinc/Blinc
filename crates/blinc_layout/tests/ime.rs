use blinc_layout::{div::div, ime::ImeEdit, renderer::RenderTree, widgets::text_input::*};
use std::sync::{Arc, Mutex, atomic::AtomicBool};

static LOCK: Mutex<()> = Mutex::new(());
fn input_with_value(value: &str) -> SharedTextInputData {
    Arc::new(Mutex::new(TextInputData::with_value(value)))
}
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
fn composition_replaces_candidates_and_commits_one_callback_and_undo() {
    let _guard = init();
    let data = input_with_value("A🦊");
    let changes = Arc::new(Mutex::new(Vec::new()));
    let changed = changes.clone();
    let widget =
        text_input(&data).on_change(move |value| changed.lock().unwrap().push(value.to_owned()));
    let mut tree = RenderTree::from_element(&div().w(300.0).h(80.0).child(widget));
    tree.compute_layout(300.0, 80.0);
    focus_text_input(&data);
    edit(&tree, "A🦊ni", [5, 5], Some([3, 5]));
    edit(&tree, "A🦊nihao", [8, 8], Some([3, 8]));
    assert_eq!(data.lock().unwrap().value, "A🦊nihao");
    assert!(changes.lock().unwrap().is_empty());
    edit(&tree, "A🦊你好", [5, 5], None);
    assert_eq!(*changes.lock().unwrap(), ["A🦊你好"]);
    let mut d = data.lock().unwrap();
    assert!(d.undo());
    assert_eq!(d.value, "A🦊");
    assert!(!d.undo());
    assert!(d.redo());
    assert_eq!(d.value, "A🦊你好");
}

#[test]
fn cancelled_preedit_and_selection_updates_do_not_emit_changes() {
    let _guard = init();
    let data = input_with_value("word");
    let changes = Arc::new(Mutex::new(Vec::new()));
    let changed = changes.clone();
    let mut tree =
        RenderTree::from_element(&div().w(300.0).h(80.0).child(
            text_input(&data).on_change(move |v| changed.lock().unwrap().push(v.to_owned())),
        ));
    tree.compute_layout(300.0, 80.0);
    focus_text_input(&data);
    edit(&tree, "word", [0, 4], None);
    edit(&tree, "ni", [2, 2], Some([0, 2]));
    edit(&tree, "word", [0, 4], None);
    assert!(changes.lock().unwrap().is_empty());
    assert!(!data.lock().unwrap().undo());
}

#[test]
fn invalid_commit_rolls_back_atomically_and_acknowledges_the_native_edit() {
    let _guard = init();
    let data = input_with_value("42");
    let widget = text_input(&data).input_type(InputType::Integer);
    let mut tree = RenderTree::from_element(&div().w(300.0).h(80.0).child(widget));
    tree.compute_layout(300.0, 80.0);
    focus_text_input(&data);
    edit(&tree, "42ni", [4, 4], Some([2, 4]));
    edit(&tree, "42你", [3, 3], None);
    let snapshot = tree.ime_snapshot().unwrap();
    assert_eq!(snapshot.content.edit.text, "42");
    assert_eq!(snapshot.content.edit.sequence, 2);
    assert_eq!(snapshot.content.edit.composing, None);
    assert!(!data.lock().unwrap().undo());
}

#[test]
fn stale_sequences_focus_sessions_and_removed_nodes_are_rejected() {
    let _guard = init();
    let data = text_input_data();
    let mut tree = RenderTree::from_element(&div().w(300.0).h(80.0).child(text_input(&data)));
    tree.compute_layout(300.0, 80.0);
    focus_text_input(&data);
    let snapshot = tree.ime_snapshot().unwrap();
    let stale = ImeEdit {
        sequence: 1,
        text: "first".into(),
        selection: [5, 5],
        ..snapshot.content.edit
    };
    assert!(tree.apply_ime_edit(snapshot.id, &stale));
    assert!(!tree.apply_ime_edit(snapshot.id, &stale));
    blur_all_text_inputs();
    focus_text_input(&data);
    assert!(!tree.apply_ime_edit(
        snapshot.id,
        &ImeEdit {
            sequence: 2,
            ..stale.clone()
        }
    ));
    tree.update_if_changed(&div().w(300.0).h(80.0));
    assert!(!tree.apply_ime_edit(
        snapshot.id,
        &ImeEdit {
            sequence: 3,
            ..stale
        }
    ));
}

#[test]
fn textarea_composition_keeps_unicode_selection_and_trailing_newlines() {
    let _guard = init();
    use blinc_layout::widgets::text_area::*;
    let data = text_area_state();
    let changes = Arc::new(Mutex::new(Vec::new()));
    let changed = changes.clone();
    let mut tree =
        RenderTree::from_element(&div().w(300.0).h(200.0).child(
            text_area(&data).on_change(move |s| changed.lock().unwrap().push(s.to_owned())),
        ));
    tree.compute_layout(300.0, 200.0);
    focus_text_area(&data);
    edit(&tree, "line\nni", [7, 7], Some([5, 7]));
    assert!(changes.lock().unwrap().is_empty());
    edit(&tree, "line\n你🦊\n", [9, 9], None);
    assert_eq!(data.lock().unwrap().value(), "line\n你🦊\n");
    assert_eq!(*changes.lock().unwrap(), ["line\n你🦊\n"]);
    assert!(tree.ime_snapshot().unwrap().content.multiline);
}

#[test]
fn code_candidates_form_one_undo_group() {
    let _guard = init();
    use blinc_layout::widgets::code::*;
    let data = code_editor_state("let name = \"\";");
    let mut tree = RenderTree::from_element(&div().w(400.0).h(200.0).child(code_editor(&data)));
    tree.compute_layout(400.0, 200.0);
    data.lock().unwrap().focused = true;
    edit(&tree, "let name = \"ni\";", [14, 14], Some([12, 14]));
    edit(&tree, "let name = \"你\";", [13, 13], None);
    let mut d = data.lock().unwrap();
    assert_eq!(d.value(), "let name = \"你\";");
    assert!(d.undo());
    assert_eq!(d.value(), "let name = \"\";");
    assert!(!d.undo());
    d.focused = false;
}

#[test]
fn rich_text_candidates_preserve_the_surrounding_document() {
    let _guard = init();
    use blinc_layout::widgets::rich_text_editor::*;
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

#[test]
fn blur_commits_once_and_allows_a_reentrant_change_callback() {
    let _guard = init();
    let data = text_input_data();
    let other = text_input_data();
    let calls = Arc::new(Mutex::new(0));
    let calls_for_change = calls.clone();
    let other_for_change = other.clone();
    let widget = text_input(&data).on_change(move |_| {
        *calls_for_change.lock().unwrap() += 1;
        focus_text_input(&other_for_change);
    });
    let mut tree = RenderTree::from_element(
        &div()
            .w(300.0)
            .h(100.0)
            .child(widget)
            .child(text_input(&other)),
    );
    tree.compute_layout(300.0, 100.0);
    focus_text_input(&data);
    edit(&tree, "你好", [2, 2], Some([0, 2]));
    blur_text_input(&data);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(tree.ime_snapshot().unwrap().content.edit.text, "");
    blur_all_text_inputs();
    assert_eq!(*calls.lock().unwrap(), 1);
}

#[test]
fn external_replacement_cancels_the_old_native_session() {
    let _guard = init();
    let data = input_with_value("old");
    let mut tree = RenderTree::from_element(&div().w(300.0).h(80.0).child(text_input(&data)));
    tree.compute_layout(300.0, 80.0);
    focus_text_input(&data);
    edit(&tree, "oldni", [5, 5], Some([3, 5]));
    let old = tree.ime_snapshot().unwrap();
    data.lock().unwrap().value = "new".into();
    refresh_text_input(&data);
    let new = tree.ime_snapshot().unwrap();
    assert_eq!(new.content.edit.text, "new");
    assert_eq!(new.content.edit.composing, None);
    assert_ne!(old.content.edit.session, new.content.edit.session);
    assert!(!tree.apply_ime_edit(
        old.id,
        &ImeEdit {
            text: "old你".into(),
            sequence: 2,
            composing: None,
            ..old.content.edit
        }
    ));
    assert_eq!(data.lock().unwrap().value, "new");
    assert!(!data.lock().unwrap().undo());
}

#[test]
fn password_constraints_and_surrogate_boundaries_are_preserved() {
    let _guard = init();
    let data = input_with_value("🦊");
    let mut tree = RenderTree::from_element(
        &div()
            .w(300.0)
            .h(80.0)
            .child(text_input(&data).masked(true).max_length(2)),
    );
    tree.compute_layout(300.0, 80.0);
    focus_text_input(&data);
    assert!(tree.ime_snapshot().unwrap().content.secure);
    edit(&tree, "🦊", [1, 1], None);
    assert_eq!(tree.ime_snapshot().unwrap().content.edit.selection, [2, 2]);
    edit(&tree, "🦊ni", [4, 4], Some([2, 4]));
    edit(&tree, "🦊你好", [4, 4], None);
    assert_eq!(data.lock().unwrap().value, "🦊");
    assert!(!data.lock().unwrap().undo());
    edit(&tree, "🦊你", [3, 3], None);
    assert_eq!(data.lock().unwrap().value, "🦊你");
}

#[test]
fn disabling_a_field_cancels_its_provisional_edit() {
    let _guard = init();
    let input = input_with_value("before");
    let mut tree = RenderTree::from_element(&div().w(300.0).h(80.0).child(text_input(&input)));
    tree.compute_layout(300.0, 80.0);
    focus_text_input(&input);
    edit(&tree, "beforeni", [8, 8], Some([6, 8]));
    let _disabled = text_input(&input).disabled(true);
    assert!(tree.ime_snapshot().is_none());
    assert_eq!(input.lock().unwrap().value, "before");
    assert!(!input.lock().unwrap().undo());

    use blinc_layout::widgets::text_area::*;
    let area = text_area_state();
    let mut tree = RenderTree::from_element(&div().w(300.0).h(160.0).child(text_area(&area)));
    tree.compute_layout(300.0, 160.0);
    focus_text_area(&area);
    edit(&tree, "ni", [2, 2], Some([0, 2]));
    let _disabled = text_area(&area).disabled(true);
    assert!(tree.ime_snapshot().is_none());
    assert_eq!(area.lock().unwrap().value(), "");
    assert_eq!(take_keyboard_state_change(), Some(false));
}

#[test]
fn clicking_within_the_field_commits_before_moving_the_caret() {
    let _guard = init();
    let data = input_with_value("seed");
    let changes = Arc::new(Mutex::new(Vec::new()));
    let changed = changes.clone();
    let mut tree = RenderTree::from_element(
        &text_input(&data).on_change(move |value| changed.lock().unwrap().push(value.to_owned())),
    );
    tree.compute_layout(300.0, 80.0);
    focus_text_input(&data);
    edit(&tree, "seedni", [6, 6], Some([4, 6]));
    let old = tree.ime_snapshot().unwrap();
    tree.dispatch_event(
        tree.root().unwrap(),
        blinc_core::events::event_types::POINTER_DOWN,
        1.0,
        1.0,
    );
    assert_eq!(*changes.lock().unwrap(), ["seedni"]);
    assert_eq!(tree.ime_snapshot().unwrap().content.edit.composing, None);
    assert!(!tree.apply_ime_edit(
        old.id,
        &ImeEdit {
            sequence: 2,
            text: "seed你".into(),
            composing: None,
            ..old.content.edit
        }
    ));
    edit(&tree, "Xseedni", [1, 1], None);
    assert_eq!(*changes.lock().unwrap(), ["seedni", "Xseedni"]);
    let mut d = data.lock().unwrap();
    assert!(d.undo());
    assert_eq!(d.value, "seedni");
    assert!(d.undo());
    assert_eq!(d.value, "seed");
}

#[test]
fn rich_text_link_drafts_have_their_own_native_session() {
    let _guard = init();
    use blinc_layout::widgets::rich_text_editor::*;
    let data = rich_text_state(RichDocument::from_blocks(vec![Block::paragraph(
        "Link",
        blinc_core::Color::WHITE,
    )]));
    let mut tree =
        RenderTree::from_element(&rich_text_editor(&data, RichTextTheme::default(), 400.0));
    tree.compute_layout(400.0, 300.0);
    data.lock().unwrap().set_focus(true);
    let document = tree.ime_snapshot().unwrap();
    data.lock().unwrap().picker = PickerState::Link {
        draft: "https://".into(),
    };
    let draft = tree.ime_snapshot().unwrap();
    assert!(!draft.content.multiline);
    assert_ne!(draft.content.edit.session, document.content.edit.session);
    edit(&tree, "https://ni", [10, 10], Some([8, 10]));
    edit(&tree, "https://你", [9, 9], None);
    assert_eq!(data.lock().unwrap().document.plain_text(), "Link");
    tree.broadcast_key_event(
        blinc_core::events::event_types::KEY_DOWN,
        13,
        false,
        false,
        false,
        false,
    );
    assert!(matches!(data.lock().unwrap().picker, PickerState::None));
    assert_eq!(tree.ime_snapshot().unwrap().content.edit.text, "Link");
    assert_ne!(
        tree.ime_snapshot().unwrap().content.edit.session,
        draft.content.edit.session
    );
    data.lock().unwrap().set_focus(false);
}

#[test]
fn modifier_and_control_notifications_do_not_commit_composition() {
    let _guard = init();
    let data = text_input_data();
    let calls = Arc::new(Mutex::new(0));
    let changed = calls.clone();
    let mut tree = RenderTree::from_element(
        &text_input(&data).on_change(move |_| *changed.lock().unwrap() += 1),
    );
    tree.compute_layout(300.0, 80.0);
    focus_text_input(&data);
    edit(&tree, "ni", [2, 2], Some([0, 2]));
    tree.broadcast_key_event(
        blinc_core::events::event_types::KEY_DOWN,
        16,
        false,
        false,
        false,
        false,
    );
    tree.broadcast_text_input_event('\0', false, false, false, false);
    assert_eq!(*calls.lock().unwrap(), 0);
    assert_eq!(
        tree.ime_snapshot().unwrap().content.edit.composing,
        Some([0, 2])
    );
    edit(&tree, "你", [1, 1], None);
    assert_eq!(*calls.lock().unwrap(), 1);
}
