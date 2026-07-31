//! `cn.Input(value = <signal>)` is two-way, and `on_change` fires.
//!
//! A bound field shares the signal: the signal seeds it, and every edit
//! writes back, so the DSL reads what was typed without an explicit
//! `key` and without polling the buffer.

use blinc_core::events::event_types::{POINTER_DOWN, POINTER_UP, TEXT_INPUT};
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::ElementBuilder;
use blinc_layout::event_handler::EventContext;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let s = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(s.handle());
            // Two globals hold a scheduler: this one, and
            // `blinc_layout::render_state`'s, which a real app fills
            // from `RenderState::new`. Widgets that animate read the
            // second, so a test that sets only the first panics with a
            // message naming neither.
            blinc_layout::render_state::set_global_scheduler(s.handle());
            Box::leak(Box::new(s));
        }
        if !blinc_core::BlincContextState::is_initialized() {
            blinc_core::BlincContextState::init(
                blinc_core::reactive::global_graph(),
                Arc::new(std::sync::Mutex::new(
                    blinc_core::context_state::HookState::new(),
                )),
                Arc::new(AtomicBool::new(false)),
            );
        }
    });
}

/// Focus is a process-global (`FOCUSED_TEXT_INPUT`), so two tests
/// typing at once steal it from each other. Serialise them.
fn focus_lock() -> std::sync::MutexGuard<'static, ()> {
    static L: std::sync::Mutex<()> = std::sync::Mutex::new(());
    L.lock().unwrap_or_else(|e| e.into_inner())
}

fn dsl() -> BlincDsl {
    init();
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.*");
    dsl
}

/// Type one character into the first element that takes text input.
///
/// A click first: the widget drops TEXT_INPUT unless it is focused, and
/// focus comes from the pointer.
fn type_char(el: &dyn ElementBuilder, c: char) -> bool {
    if let Some(handlers) = el.event_handlers()
        && handlers.has_handler(TEXT_INPUT)
    {
        let node = blinc_layout::LayoutNodeId::default();
        handlers.dispatch(&EventContext::new(POINTER_DOWN, node).with_local_pos(4.0, 8.0));
        handlers.dispatch(&EventContext::new(POINTER_UP, node).with_local_pos(4.0, 8.0));
        handlers.dispatch(&EventContext::new(TEXT_INPUT, node).with_key_char(c));
        return true;
    }
    for child in el.children_builders() {
        if type_char(child.as_ref(), c) {
            return true;
        }
    }
    false
}

#[test]
fn typing_writes_the_bound_signal() {
    let _guard = focus_lock();
    let dsl = dsl();
    dsl.compile_source(
        r#"
        signal in_name: string
        view { Div { cn.Input(value = in_name, label = "Name") } }
        "#,
        "input_bound.blinc",
    )
    .expect("compile");
    dsl.set_signal_string("in_name", "");

    let widget = dsl.view_widget();
    assert!(type_char(widget.as_ref(), 'z'), "the field must take text");
    assert_eq!(
        dsl.get_signal_string("in_name").as_deref(),
        Some("z"),
        "typing must write the bound signal"
    );
}

/// The signal seeds the field, so a value set before the first build
/// shows up in it.
#[test]
fn the_signal_seeds_the_field() {
    use blinc_cn_dsl::bridge::text_input_data_for_field;
    use blinc_dsl_core::Reactive;

    init();
    let sig = blinc_core::reactive::signal::<String>("seeded".to_string());
    let data = text_input_data_for_field(&Reactive::Signal(sig), "", Default::default());
    assert_eq!(
        data.lock().unwrap().value,
        "seeded",
        "a bound field opens showing the signal's value"
    );

    // A rebuild must not clobber what the user typed since.
    data.lock().unwrap().value = "typed".to_string();
    sig.set("changed".to_string());
    let again = text_input_data_for_field(&Reactive::Signal(sig), "", Default::default());
    assert_eq!(
        again.lock().unwrap().value,
        "typed",
        "seeding is for an empty buffer only"
    );
}

/// An unbound field still works, keyed as before.
#[test]
fn an_unbound_field_keeps_its_key() {
    let _guard = focus_lock();
    let dsl = dsl();
    dsl.compile_source(
        r#"view { Div { cn.Input(key = "plain", placeholder = "type") } }"#,
        "input_unbound.blinc",
    )
    .expect("compile");
    let widget = dsl.view_widget();
    assert!(type_char(widget.as_ref(), 'q'), "the field must take text");
    assert_eq!(
        blinc_cn_dsl::bridge::text_input_data_keyed("plain")
            .lock()
            .unwrap()
            .value,
        "q",
        "an unbound field still stores through its key"
    );
}

/// `on_change` runs the author's closure after the signal is written,
/// so a zero-arg closure can read the new text off the binding.
#[test]
fn on_change_fires_after_the_write() {
    let _guard = focus_lock();
    let dsl = dsl();
    dsl.compile_source(
        r#"
        signal oc_text: string
        signal oc_hits: i32
        signal oc_seen: string
        view {
            Div {
                cn.Input(value = oc_text, on_change = || {
                    oc_hits.set(oc_hits.get() + 1)
                    oc_seen.set(oc_text.get())
                })
            }
        }
        "#,
        "input_on_change.blinc",
    )
    .expect("compile");
    dsl.set_signal_string("oc_text", "");
    dsl.set_signal_i32("oc_hits", 0);
    dsl.set_signal_string("oc_seen", "");

    let widget = dsl.view_widget();
    assert!(type_char(widget.as_ref(), 'k'), "the field must take text");

    assert_eq!(
        dsl.get_signal_i32("oc_hits"),
        Some(1),
        "on_change must fire"
    );
    assert_eq!(
        dsl.get_signal_string("oc_seen").as_deref(),
        Some("k"),
        "the closure must see the value already written"
    );
}

/// `cn.Textarea` has the same contract as `cn.Input`.
#[test]
fn textarea_typing_writes_the_bound_signal() {
    let _guard = focus_lock();
    let dsl = dsl();
    dsl.compile_source(
        r#"
        signal ta_bio: string
        signal ta_hits: i32
        view {
            Div {
                cn.Textarea(value = ta_bio, rows = 3, on_change = || {
                    ta_hits.set(ta_hits.get() + 1)
                })
            }
        }
        "#,
        "textarea_bound.blinc",
    )
    .expect("compile");
    dsl.set_signal_string("ta_bio", "");
    dsl.set_signal_i32("ta_hits", 0);

    let widget = dsl.view_widget();
    assert!(
        type_char(widget.as_ref(), 'y'),
        "the textarea must take text"
    );
    assert_eq!(
        dsl.get_signal_string("ta_bio").as_deref(),
        Some("y"),
        "typing must write the bound signal"
    );
    assert_eq!(
        dsl.get_signal_i32("ta_hits"),
        Some(1),
        "on_change must fire once per edit"
    );
}

/// A `cn.Badge` bound to the same signal tracks it: text has no
/// property writer, so the chip rebuilds through `deps()`.
#[test]
fn a_bound_badge_follows_the_signal() {
    use blinc_layout::renderer::RenderTree;

    let _guard = focus_lock();
    let dsl = dsl();
    dsl.compile_source(
        r#"
        signal chip_text: string
        view { Div { cn.Badge(label = chip_text, variant = "secondary") } }
        "#,
        "badge_bound.blinc",
    )
    .expect("compile");
    dsl.set_signal_string("chip_text", "ab");

    let host = blinc_layout::div::div()
        .w(400.0)
        .h(200.0)
        .child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);
    // Widest LEAF: the containers stretch to the 400px host, so only a
    // childless node reports the text's own measured width.
    let width = |t: &RenderTree| {
        let mut max = 0.0f32;
        let mut stack = vec![t.root().unwrap()];
        while let Some(id) = stack.pop() {
            let kids = t.layout_tree.children(id);
            if kids.is_empty()
                && let Some(l) = t.layout_tree.get_layout(id)
            {
                max = max.max(l.size.width);
            }
            stack.extend(kids);
        }
        max
    };
    let before = width(&tree);

    dsl.set_signal_string("chip_text", "a much longer chip label");
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(400.0, 200.0);
    let after = width(&tree);

    println!("BADGE width {before} -> {after}");
    assert!(
        after > before,
        "a bound chip must re-render with the new text: {before} -> {after}"
    );
}
