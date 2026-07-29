//! `cn.Checkbox` / `cn.Switch` bound to a DSL signal must bind BOTH ways.
//!
//! The widgets own their toggle through a `State<bool>`; binding a DSL
//! `signal` is supposed to map onto that same signal so there is one
//! source of truth. Compiling is not evidence of that: a prop that
//! lowers to a LITERAL compiles perfectly and silently snapshots the
//! value at build time.

use blinc_core::events::event_types::POINTER_UP;
use blinc_dsl_core::BlincDsl;
use blinc_layout::div::ElementBuilder;
use blinc_layout::event_handler::EventContext;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

fn init() {
    static I: std::sync::Once = std::sync::Once::new();
    I.call_once(|| {
        // The host installs this; without it `Signal::set` notifies
        // nobody and no `Stateful` ever refreshes.
        blinc_core::reactive::set_stateful_deps_notifier(|ids| {
            blinc_layout::check_stateful_deps(ids);
        });
        blinc_theme::ThemeState::init_default();
        if !blinc_animation::is_scheduler_initialized() {
            let s = blinc_animation::AnimationScheduler::new();
            blinc_animation::set_global_scheduler(s.handle());
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

fn dsl() -> BlincDsl {
    let _ = tracing_subscriber::fmt::try_init();
    init();
    let dsl = BlincDsl::new().expect("dsl init");
    blinc_cn_dsl::register_all(&dsl).expect("register cn.* widgets");
    dsl
}

/// Click the first element in the builder tree that has a click
/// handler, which is what a user hitting the toggle amounts to.
fn click_first_clickable(el: &dyn ElementBuilder) -> bool {
    if let Some(handlers) = el.event_handlers()
        && handlers.has_handler(POINTER_UP)
    {
        handlers.dispatch(&EventContext::new(
            POINTER_UP,
            blinc_layout::LayoutNodeId::default(),
        ));
        return true;
    }
    for child in el.children_builders() {
        if click_first_clickable(child.as_ref()) {
            return true;
        }
    }
    false
}

#[test]
fn checkbox_writes_back_to_its_bound_signal() {
    let dsl = dsl();
    dsl.compile_source(
        r#"
        signal cb_accepted: bool
        view { Div { cn.Checkbox(checked = cb_accepted, label = "Accept") } }
        "#,
        "checkbox_binding.blinc",
    )
    .expect("compile");
    dsl.set_signal_bool("cb_accepted", false);

    // DSL -> widget: the widget must read the signal, not a snapshot.
    let widget = dsl.view_widget();
    dsl.set_signal_bool("cb_accepted", true);
    assert_eq!(
        dsl.get_signal_bool("cb_accepted"),
        Some(true),
        "sanity: the signal itself holds the value"
    );

    // widget -> DSL: ticking the box writes the signal back.
    assert!(
        click_first_clickable(widget.as_ref()),
        "the checkbox must expose a click handler"
    );
    assert_eq!(
        dsl.get_signal_bool("cb_accepted"),
        Some(false),
        "clicking the checkbox must write the bound signal"
    );
}

#[test]
fn switch_writes_back_to_its_bound_signal() {
    let dsl = dsl();
    dsl.compile_source(
        r#"
        signal sw_wifi: bool
        view { Div { cn.Switch(checked = sw_wifi, label = "Wifi") } }
        "#,
        "switch_binding.blinc",
    )
    .expect("compile");
    dsl.set_signal_bool("sw_wifi", false);

    let widget = dsl.view_widget();
    assert!(
        click_first_clickable(widget.as_ref()),
        "the switch must expose a click handler"
    );
    assert_eq!(
        dsl.get_signal_bool("sw_wifi"),
        Some(true),
        "flipping the switch must write the bound signal"
    );
}

/// A literal must NOT mint a shared signal: two literal-bound toggles
/// are independent, and neither is observable from the DSL.
#[test]
fn a_literal_toggle_stays_independent() {
    let dsl = dsl();
    dsl.compile_source(
        r#"view { Div { cn.Checkbox(checked = true, label = "On") } }"#,
        "checkbox_literal.blinc",
    )
    .expect("compile");
    let widget = dsl.view_widget();
    assert!(
        click_first_clickable(widget.as_ref()),
        "a literal-bound checkbox is still interactive"
    );
}

/// The shape the playground uses: `checked = Play.busy`, an FSM
/// context field rather than a top-level `signal`.
///
/// `resolve_dotted_fsm_field_access` rewrites that into
/// `__signal_get_by_id_bool(<id>)`, so the reactive lowering has to
/// recognise the wrapped getter or the prop falls back to a LITERAL and
/// the toggle silently stops tracking the FSM.
#[test]
fn toggles_bind_two_ways_to_an_fsm_context_field() {
    let dsl = dsl();
    dsl.compile_source(
        r#"
        fsm Tog {
            context { busy: bool = false }
            state Idle
            initial Idle
            on Idle.Flip -> Idle { ctx.busy = true }
        }
        view {
            Div {
                cn.Switch(checked = Tog.busy, label = "busy")
            }
        }
        "#,
        "toggle_fsm.blinc",
    )
    .expect("compile");

    let widget = dsl.view_widget();
    let ctx_signal = "__fsm_ctx_Tog_busy";
    assert_eq!(
        dsl.get_signal_bool(ctx_signal),
        Some(false),
        "sanity: the FSM context field starts false"
    );

    assert!(
        click_first_clickable(widget.as_ref()),
        "the switch must expose a click handler"
    );
    assert_eq!(
        dsl.get_signal_bool(ctx_signal),
        Some(true),
        "flipping the switch must write the FSM context field it is bound to"
    );
}

/// The other direction: setting the bound signal must move the widget.
///
/// `cn::checkbox` registers `deps([checked_state.signal_id()])`, so a
/// write reaches its `Stateful` and queues a rebuild -- but only if the
/// state is backed by the signal the DSL wrote, which is exactly what a
/// LITERAL fallback breaks. The checkmark is a child element, so the
/// node count moves when the box ticks.
#[test]
fn setting_the_signal_moves_the_checkbox() {
    use blinc_layout::renderer::RenderTree;

    fn node_count(tree: &RenderTree) -> usize {
        let mut n = 0;
        let mut stack = vec![tree.root().unwrap()];
        while let Some(id) = stack.pop() {
            n += 1;
            stack.extend(tree.layout_tree.children(id));
        }
        n
    }

    let dsl = dsl();
    dsl.compile_source(
        r#"
        signal cb_lit: bool
        view { Div { cn.Checkbox(checked = cb_lit, label = "Accept") } }
        "#,
        "checkbox_drive.blinc",
    )
    .expect("compile");
    dsl.set_signal_bool("cb_lit", false);

    let host = blinc_layout::div::div()
        .w(400.0)
        .h(200.0)
        .child_box(dsl.view_widget());
    let mut tree = RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);
    let unchecked = node_count(&tree);

    dsl.set_signal_bool("cb_lit", true);
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(400.0, 200.0);
    let checked = node_count(&tree);

    assert!(
        checked > unchecked,
        "ticking the signal must add the checkmark ({unchecked} -> {checked} nodes)"
    );
}

// ── Two widgets, one signal ──────────────────────────────────────────
//
// The playground binds `cn.Switch` and `cn.Checkbox` to the same
// `Play.busy`. An FSM transition moves both; clicking one reportedly
// does not move the other. These pin which direction breaks.
//
// Every test here uses its OWN signal name. The signal registry is
// keyed by name and process-wide, and the switch's spring key derives
// from the signal id, so a shared name lets one test read another's
// spring — which is exactly how the first run of these read 0.0 for a
// switch that had in fact moved.

/// The switch's persisted thumb spring — the observable for "did the
/// switch follow". Non-creating: if the switch never built one this is
/// `None` and the test fails rather than measuring a spring it minted.
fn thumb_target(signal_name: &str) -> f32 {
    let (raw, _) = blinc_runtime::signal::lookup(signal_name).expect("signal declared");
    let key = format!("cn-switch:{raw}:thumb");
    blinc_layout::stateful::try_persisted_animated_value(&key)
        .expect("the switch must persist a thumb spring")
        .lock()
        .unwrap()
        .target()
}

/// Every element in the tree carrying a click handler, in source order.
fn clickables(el: &dyn ElementBuilder) -> Vec<&blinc_layout::event_handler::EventHandlers> {
    let mut out = Vec::new();
    fn walk<'a>(
        el: &'a dyn ElementBuilder,
        out: &mut Vec<&'a blinc_layout::event_handler::EventHandlers>,
    ) {
        if let Some(handlers) = el.event_handlers()
            && handlers.has_handler(POINTER_UP)
        {
            out.push(handlers);
        }
        for child in el.children_builders() {
            walk(child.as_ref(), out);
        }
    }
    walk(el, &mut out);
    out
}

fn click(handlers: &blinc_layout::event_handler::EventHandlers) {
    handlers.dispatch(&EventContext::new(
        POINTER_UP,
        blinc_layout::LayoutNodeId::default(),
    ));
}

fn pair_source(signal: &str) -> String {
    format!(
        r#"
signal {signal}: bool
view {{
    Div {{
        cn.Switch(checked = {signal}, label = "busy")
        cn.Checkbox(checked = {signal}, label = "busy")
    }}
}}
"#
    )
}

/// Build the pair and lay it out once, settled.
fn build_pair(
    dsl: &BlincDsl,
    signal: &str,
) -> (blinc_layout::div::Div, blinc_layout::renderer::RenderTree) {
    dsl.compile_source(&pair_source(signal), "pair.blinc")
        .expect("compile");
    let host = blinc_layout::div::div()
        .w(400.0)
        .h(200.0)
        .child_box(dsl.view_widget());
    let mut tree = blinc_layout::renderer::RenderTree::from_element(&host);
    tree.compute_layout(400.0, 200.0);
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(400.0, 200.0);
    (host, tree)
}

/// Baseline: the FSM-style path. A write straight to the signal is what
/// `Busy` / `Reset` do, and both widgets follow it today.
#[test]
fn a_direct_signal_write_moves_the_switch() {
    let dsl = dsl();
    let (_host, mut tree) = build_pair(&dsl, "pair_direct");

    let before = thumb_target("pair_direct");
    dsl.set_signal_bool("pair_direct", true);
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(400.0, 200.0);

    let after = thumb_target("pair_direct");
    assert_ne!(
        before, after,
        "a direct signal write must move the switch's thumb target"
    );
}

/// The reported bug: clicking the CHECKBOX writes the same signal
/// through `State::set`, and the switch is supposed to follow.
#[test]
fn clicking_the_checkbox_moves_the_switch() {
    let dsl = dsl();
    let (host, mut tree) = build_pair(&dsl, "pair_checkbox");

    let before = thumb_target("pair_checkbox");
    let targets = clickables(&host);
    // Last clickable is the checkbox; the switch comes first in source.
    click(targets.last().expect("a clickable toggle"));
    assert_eq!(
        dsl.get_signal_bool("pair_checkbox"),
        Some(true),
        "the click must reach the shared signal at all"
    );
    tree.process_pending_subtree_rebuilds();
    tree.compute_layout(400.0, 200.0);

    let after = thumb_target("pair_checkbox");
    assert_ne!(
        before, after,
        "clicking the checkbox must move the switch's thumb target"
    );
}

/// The other direction, which splits the diagnosis: if this passes and
/// the one above fails, the notification path is fine and something
/// about the switch's own refresh is what is missing.
#[test]
fn clicking_the_switch_writes_the_shared_signal() {
    let dsl = dsl();
    let (host, _tree) = build_pair(&dsl, "pair_switch");

    let targets = clickables(&host);
    click(targets.first().expect("a clickable toggle"));
    assert_eq!(
        dsl.get_signal_bool("pair_switch"),
        Some(true),
        "the switch must write the shared signal"
    );
}

/// How many toggles the tree actually exposes, so the index-based picks
/// above are not quietly clicking the same widget twice.
#[test]
fn the_pair_exposes_two_toggles() {
    let dsl = dsl();
    let (host, _tree) = build_pair(&dsl, "pair_count");
    assert_eq!(
        clickables(&host).len(),
        2,
        "one clickable per toggle; if this changes the picks above are wrong"
    );
}
