//! Runtime side of `with @fsm([…]) { … }` — see
//! [`crate::passes::lower_with_blocks`] for the lowering.
//!
//! The pass records each region's dependencies as written; compilation
//! then resolves them against the declared signals and files the result
//! here. When the JIT'd view reaches `__blinc_with__`, [`mount`] wraps
//! the region's already-built widget in a `Stateful` bound to those
//! dependencies. A later write re-renders only that region's view.

use std::sync::Mutex;

/// A region with its dependencies resolved to signal names.
#[derive(Debug, Clone)]
pub(crate) struct MountedRegion {
    /// `__blinc_with_<id>` — what `render_component` is called with.
    pub name: String,
    /// Signals whose writes re-render the region.
    pub signal_names: Vec<String>,
    /// FSM the region binds its shared state to, if any. First listed
    /// wins: a `Stateful` exposes a single `SharedState`.
    pub fsm: Option<String>,
}

/// Keyed by region id, which the pass mints process-wide, so a hot
/// reload's regions never collide with the previous compile's.
static REGIONS: Mutex<Vec<(i64, MountedRegion)>> = Mutex::new(Vec::new());

/// File a resolved region. Replaces any entry under the same id.
pub(crate) fn register(id: i64, region: MountedRegion) {
    let mut regions = REGIONS.lock().unwrap_or_else(|e| e.into_inner());
    match regions.iter_mut().find(|(existing, _)| *existing == id) {
        Some((_, slot)) => *slot = region,
        None => regions.push((id, region)),
    }
}

fn lookup(id: i64) -> Option<MountedRegion> {
    REGIONS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(existing, _)| *existing == id)
        .map(|(_, region)| region.clone())
}

/// Mount `child` — the region's freshly built widget — under a
/// `Stateful` bound to the region's dependencies. Returns the new
/// widget handle.
///
/// # Safety
///
/// `child` must be a handle from a `$Blinc$<X>$view` extern (or `0`),
/// not yet materialised.
pub(crate) unsafe fn mount(id: i64, child: i64) -> i64 {
    use blinc_core::reactive::SignalId;
    use blinc_runtime::fsm::FsmStateId;

    // Close the scope the call site opened. Whatever the body read
    // while rendering is in here — exact, rather than inferred from the
    // source. Empty means either nothing was read or reads do not yet
    // perform, so fall back to the registered set rather than mounting
    // a region that subscribes to nothing.
    let observed = crate::read_scope::exit(id).unwrap_or_default();

    let Some(region) = lookup(id) else {
        // No entry: render the body, just not reactively. Losing the
        // subscription is a bug; losing the content would be a blank
        // hole in the UI.
        tracing::warn!(
            region = id,
            "`with` region not registered — rendering without a Stateful"
        );
        return child;
    };

    let signal_ids: Vec<SignalId> = if observed.is_empty() {
        region
            .signal_names
            .iter()
            .filter_map(|name| blinc_runtime::signal::lookup(name))
            .map(|(raw, _ty)| SignalId::from_raw(raw))
            .collect()
    } else {
        observed
            .iter()
            .map(|raw| SignalId::from_raw(*raw))
            .collect()
    };

    let mut builder = blinc_layout::stateful::stateful::<FsmStateId>();
    if let Some(fsm) = region.fsm.as_deref()
        && let Some(shared) = blinc_runtime::fsm::default_state(fsm)
    {
        builder = builder.with_shared_state(shared);
    }
    builder = builder.deps(signal_ids);

    // The first callback invocation happens while `on_state` builds, so
    // it must NOT re-enter the JIT: the view that produced `child` is
    // still executing. Serve the pre-built widget instead, and re-render
    // only from the second call on, which fires from a layout rebuild
    // with no view in flight.
    let pending = Mutex::new(child);
    let name = region.name.clone();
    let stateful = builder.on_state(move |_ctx| {
        let handle = {
            let mut slot = pending.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::replace(&mut *slot, 0)
        };
        if handle != 0 {
            // SAFETY: the handle came from the region's own view call
            // and this is the only place it is materialised.
            if let Some(widget) = unsafe { crate::widget_ffi::materialize_widget(handle) } {
                return blinc_layout::div::div().child_box(widget.into_element_builder());
            }
        }
        render_region(&name)
    });

    Box::into_raw(Box::new(crate::widget_ffi::WidgetBox::Custom(Box::new(
        stateful,
    )))) as i64
}

/// Re-run a region's view through the process-wide renderer.
fn render_region(name: &str) -> blinc_layout::div::Div {
    use zyntax_embed::ZyntaxValue;

    // Same phrasing as the whole-program container's line, so a host
    // counting re-renders sees a scoped one the same way.
    tracing::debug!(region = name, "with region: on_state re-render");
    let Some(renderer) = blinc_runtime::view::global_renderer() else {
        tracing::warn!(
            region = name,
            "no renderer installed — `with` region left empty"
        );
        return blinc_layout::div::div();
    };
    let value = match blinc_runtime::view::render_component(&renderer, name) {
        Ok(value) => value,
        Err(err) => {
            tracing::warn!(region = name, %err, "`with` region failed to re-render");
            return blinc_layout::div::div();
        }
    };
    let ZyntaxValue::Int(handle) = value else {
        return blinc_layout::div::div();
    };
    // SAFETY: the handle is what the region's view just returned.
    match unsafe { crate::widget_ffi::materialize_widget(handle) } {
        Some(widget) => blinc_layout::div::div().child_box(widget.into_element_builder()),
        None => blinc_layout::div::div(),
    }
}
