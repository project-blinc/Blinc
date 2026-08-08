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
    /// Resolved at REGISTRATION, not carried as names.
    ///
    /// `mount` runs at render time, when the module a name belonged to
    /// is long gone — every attempt to answer "which module is this
    /// name in?" there resolved under whichever module compiled last.
    /// Registration happens during that module's own compile, so the
    /// question has an answer exactly once.
    pub signal_ids: Vec<u64>,
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
            .signal_ids
            .iter()
            .map(|raw| SignalId::from_raw(*raw))
            .collect()
    } else {
        observed
            .iter()
            .map(|raw| SignalId::from_raw(*raw))
            .collect()
    };

    record_mounted_deps(id, &signal_ids);

    // Logged because a region's `Stateful` is created at ONE source
    // location, so every region shares `InstanceKey`'s per-location
    // call counter. That counter only resets at a frame boundary. If
    // this logs repeatedly, or the key changes between logs for the
    // same region, each render is getting fresh key-derived storage and
    // the old entries are never reclaimed — which is what a CPU figure
    // that grows without input looks like.
    let mut builder = blinc_layout::stateful::stateful::<FsmStateId>();
    {
        static MOUNTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = MOUNTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        tracing::debug!(
            region = %region.name,
            mount = n,
            deps = signal_ids.len(),
            "with region: mounting a Stateful"
        );
    }
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
    let stateful = builder.on_state(move |ctx| {
        let handle = {
            let mut slot = pending.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::replace(&mut *slot, 0)
        };
        if handle != 0 {
            // SAFETY: the handle came from the region's own view call
            // and this is the only place it is materialised.
            if let Some(widget) = unsafe { crate::widget_ffi::materialize_widget(handle) } {
                // A region holds view content, so it stacks. The
                // default row stretches what it holds to the row's own
                // height, which inside a bounded parent pinned a tall
                // page to that height and collapsed its trailing rows
                // to zero rather than letting them overflow.
                return blinc_layout::div::div()
                    .flex_col()
                    .child_box(widget.into_element_builder());
            }
        }

        // Every later render re-observes. A body that branches reads
        // different signals on different renders, so the deps have to
        // follow it: what mattered last time may not this time. The
        // set is replaced, not extended — `set_deps` skips the registry
        // write when nothing moved, which is the common case.
        crate::read_scope::enter(id);
        let rendered = render_region(&name);
        if let Some(observed) = crate::read_scope::exit(id) {
            ctx.set_deps(
                observed
                    .iter()
                    .map(|raw| SignalId::from_raw(*raw))
                    .collect(),
            );
        }
        rendered
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

// =====================================================================
// Test observation
// =====================================================================

/// What `mount` last subscribed each region to.
///
/// The dep set is the whole point of a read scope, and it is otherwise
/// invisible: a region with the wrong subscriptions renders correctly
/// and only misbehaves later, on a write. Asserting on rendered output
/// instead would pass while measuring nothing.
static MOUNTED_DEPS: std::sync::Mutex<Vec<(i64, Vec<u64>)>> = std::sync::Mutex::new(Vec::new());

fn record_mounted_deps(id: i64, ids: &[blinc_core::reactive::SignalId]) {
    if let Ok(mut log) = MOUNTED_DEPS.lock() {
        log.push((id, ids.iter().map(|s| s.to_raw()).collect()));
    }
}

/// Log the whole-program `@stateful`'s dep set under the reserved id, so
/// a test can assert on it the same way it does a region's.
#[doc(hidden)]
pub fn __record_program_deps(ids: &[blinc_core::reactive::SignalId]) {
    record_mounted_deps(i64::MIN, ids);
}

/// Every mount so far, as `(region_id, signal_ids)`.
///
/// Not just the last: tests run in parallel in one process and this log
/// is global, so a positional accessor hands one test another's mount.
/// A caller selects the entry that mentions a signal it owns.
#[doc(hidden)]
pub fn __mounted_deps() -> Vec<(i64, Vec<u64>)> {
    MOUNTED_DEPS
        .lock()
        .map(|log| log.clone())
        .unwrap_or_default()
}

/// The deps of the LATEST mount that subscribed to `signal_raw`.
///
/// Latest, not first: a dep set narrows as renders observe what is
/// actually read, so the first entry is the widest guess and the last
/// is what the thing is subscribed to now.
#[doc(hidden)]
pub fn __deps_mentioning(signal_raw: u64) -> Option<Vec<u64>> {
    __mounted_deps()
        .into_iter()
        .rev()
        .find(|(_, ids)| ids.contains(&signal_raw))
        .map(|(_, ids)| ids)
}

/// Forget what has been mounted so far.
#[doc(hidden)]
pub fn __clear_mounted_deps() {
    if let Ok(mut log) = MOUNTED_DEPS.lock() {
        log.clear();
    }
}
