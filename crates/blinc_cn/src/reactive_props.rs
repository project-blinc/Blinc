//! Helpers for threading [`Reactive<T>`] values through cn widget
//! builders.
//!
//! cn widgets fall into two shapes, and the shape decides the
//! mechanism:
//!
//! * **Single visual property** (a colour, a radius, an opacity) —
//!   forward the `Reactive<T>` straight into the matching
//!   [`blinc_layout::div::Div`] setter, which registers a property
//!   binding. The signal patches `RenderProps` in place; no rebuild,
//!   no `compute_layout`. `cn::progress` takes this path.
//! * **Structural property** (`disabled`, `variant`, `checked`) — the
//!   value gates branches that pick different colours, borders,
//!   shadows, or FSM start states, so it can't be a single property
//!   write. Read the current value at build time and register the
//!   source signal via the widget's `Stateful::deps`, which rebuilds
//!   the subtree on change.
//!
//! [`current`] and [`dep_signal`] serve the second path: one reads the
//! build-time value, the other yields the [`SignalId`] to subscribe to.

use blinc_core::reactive::SignalId;
use blinc_layout::binding::Reactive;

/// Read the value a [`Reactive<T>`] holds right now.
///
/// Used by builders that branch on the value at build time. For
/// `Bound`/`Computed` this is the current signal read, which pairs with
/// a [`dep_signal`] subscription so the branch re-evaluates on change.
pub fn current<T>(r: &Reactive<T>) -> T
where
    T: Clone + Send + Default + 'static,
{
    match r {
        Reactive::Const(v) => v.clone(),
        Reactive::Bound(state) => state.try_get().unwrap_or_default(),
        Reactive::Computed(computed) => computed.try_get().unwrap_or_default(),
    }
}

/// The [`SignalId`] a structural prop should subscribe to, if any.
///
/// `Const` never changes, so it yields `None`. `Computed` is keyed by a
/// `DerivedId` rather than a `SignalId` and so can't drive `deps()`
/// directly; bind the underlying state instead, or use a property
/// binding where the shape allows one.
pub fn dep_signal<T: Clone + Send + 'static>(r: &Reactive<T>) -> Option<SignalId> {
    match r {
        Reactive::Bound(state) => Some(state.signal_id()),
        Reactive::Const(_) | Reactive::Computed(_) => None,
    }
}

/// Clone a [`Reactive<T>`] — the enum itself isn't `Clone` because it
/// carries type-erased handles, but every variant's payload is.
pub fn clone_reactive<T: Clone>(r: &Reactive<T>) -> Reactive<T> {
    match r {
        Reactive::Const(v) => Reactive::Const(v.clone()),
        Reactive::Bound(state) => Reactive::Bound(state.clone()),
        Reactive::Computed(computed) => Reactive::Computed(computed.clone()),
    }
}

/// An element that follows a reactive source.
///
/// The general form behind [`reactive_text`]: a `Const` builds once
/// with no wrapper at all, and a bound source gets a `Stateful`
/// subscribed to the signal that re-runs `build` on every change. Use
/// it for content a property write can't express -- text, an image
/// source, anything that picks a different element.
///
/// `build` runs on every rebuild and MUST set everything the content
/// needs. It runs after the stylesheet pass has walked the tree, so it
/// cannot rely on inheriting from an ancestor's class; content that did
/// would render unstyled until some later event triggered another pass.
/// See `gotcha_stateful_content_loses_css_inheritance`.
pub fn reactive_node<F>(
    src: &Reactive<String>,
    build: F,
) -> Box<dyn blinc_layout::div::ElementBuilder>
where
    F: Fn(String) -> Box<dyn blinc_layout::div::ElementBuilder> + Send + Sync + 'static,
{
    use blinc_layout::stateful::{ButtonState, stateful};

    let Some(sig) = dep_signal(src) else {
        return build(current(src));
    };
    let src = clone_reactive(src);
    // The Stateful sits inside a plain hugging Div.
    //
    // A Stateful's own container does not take the parent's
    // `items_center` the way an ordinary child does, so on its own it
    // lands at the top-left of a taller box -- visible as a chip label
    // pinned to the top of its pill instead of centred. Wrapped, the
    // parent aligns an ordinary Div and the content follows it.
    //
    // The inner Div hugs too: the callback has to return a container,
    // and a stretching one would put the content back in the corner.
    Box::new(
        blinc_layout::div::div()
            .w_fit()
            .h_fit()
            // `h_fit()` also sets `align_self: Start`, which pins the
            // wrapper to the top of a taller parent -- a chip label
            // against the top of its pill rather than centred in it.
            // Content in a box is centred in every caller here, so say
            // so.
            .align_self_center()
            .child(
                stateful::<ButtonState>()
                    .deps([sig])
                    .on_state(move |_ctx| {
                        blinc_layout::div::div()
                            .w_fit()
                            .h_fit()
                            .child_box(build(current(&src)))
                    })
                    .w_fit()
                    .h_fit(),
            ),
    )
}

/// A text node that follows a reactive source.
///
/// Text content has no property writer, so a changed string needs a new
/// text node: bound sources get a `Stateful` subscribed to the signal,
/// and a `Const` is built once with no wrapper at all.
///
/// `style` runs on every rebuild and MUST set everything the text
/// needs -- size, colour, weight. Content built inside a stateful
/// callback is created after the stylesheet pass has walked the tree,
/// so it cannot rely on inheriting anything from an ancestor's class;
/// it would render unstyled until some later event triggered another
/// pass. See `gotcha_stateful_content_loses_css_inheritance`.
pub fn reactive_text<F>(
    src: &Reactive<String>,
    style: F,
) -> Box<dyn blinc_layout::div::ElementBuilder>
where
    F: Fn(blinc_layout::text::Text) -> blinc_layout::text::Text + Send + Sync + 'static,
{
    use blinc_layout::text::text;
    reactive_node(src, move |s| Box::new(style(text(s))))
}

// =====================================================================
// Binding animated values to a bound bool
// =====================================================================

/// Signals already bound, so N widgets sharing one signal register one
/// effect rather than N. Keyed by raw `SignalId` plus a per-widget
/// discriminator, since two DIFFERENT widget kinds on the same signal
/// each need their own targets.
static BOUND: std::sync::Mutex<Option<std::collections::HashSet<(u64, u64)>>> =
    std::sync::Mutex::new(None);

/// One animated value and where it should sit when the bound bool is
/// true or false.
pub struct BoolTarget {
    pub anim: blinc_layout::motion::SharedAnimatedValue,
    pub on: f32,
    pub off: f32,
}

impl BoolTarget {
    /// The common case: 0 when false, `on` when true.
    pub fn to(anim: &blinc_layout::motion::SharedAnimatedValue, on: f32) -> Self {
        Self {
            anim: anim.clone(),
            on,
            off: 0.0,
        }
    }
}

/// Drive animated values from a bound `bool`, so a widget follows a
/// signal written anywhere rather than only its own toggle.
///
/// A widget that samples its state once at construction animates only
/// when something calls its own `set_*` method. Bound to a signal that
/// a switch, a button or an FSM action writes, it changes the signal
/// and stays put — which looks like the binding is ignored. Every
/// animated widget taking a `State<bool>` needs this, so it lives here
/// rather than being written per widget.
///
/// `kind` distinguishes widget types sharing a signal: a switch and a
/// collapsible bound to the same bool each want their own targets, but
/// two switches want one registration between them. Pass a constant per
/// widget kind.
///
/// The read inside the effect is what subscribes it; the graph tracks
/// reads, so it re-fires on every write to that signal.
pub fn bind_bool_targets(
    kind: u64,
    state: &blinc_core::reactive::State<bool>,
    targets: Vec<BoolTarget>,
) {
    let signal = state.signal();
    {
        let mut guard = BOUND.lock().unwrap_or_else(|e| e.into_inner());
        let seen = guard.get_or_insert_with(std::collections::HashSet::new);
        if !seen.insert((signal.id().to_raw(), kind)) {
            return;
        }
    }

    blinc_core::reactive::effect(move |graph| {
        // Read through the PASSED graph, not `State::get`. The first
        // run happens inside `create_effect` with the global graph
        // mutex already held, so the global path deadlocks on a
        // non-reentrant lock. The argument exists for this and tracks
        // the read the same way.
        let on = graph.get(signal).unwrap_or(false);
        for t in &targets {
            t.anim
                .lock()
                .unwrap()
                .set_target(if on { t.on } else { t.off });
        }
    });
}

/// Widget-kind discriminators for [`bind_bool_targets`].
pub mod bool_binding {
    pub const SWITCH: u64 = 1;
    pub const COLLAPSIBLE: u64 = 2;
}

/// Drive an animated value from a bound number, the numeric counterpart
/// of [`bind_bool_targets`].
///
/// Generic over the number's type so a widget can be bound to whatever
/// precision the caller keeps.
///
/// `place` maps the value to where the animation should sit — for a
/// slider, a value to a thumb offset in pixels. It is captured once, so
/// the geometry it closes over is the geometry of the first build; a
/// widget whose size changes has to re-seed the animation itself.
///
/// Registered once per `(signal, kind)`, which is why the animation it
/// drives has to outlive a rebuild. Pair it with
/// `persisted_animated_value`, or later builds animate a value nothing
/// paints.
pub fn bind_number_target<T, F>(
    kind: u64,
    signal: blinc_core::reactive::Signal<T>,
    anim: &blinc_layout::motion::SharedAnimatedValue,
    place: F,
) where
    T: Clone + Send + 'static,
    F: Fn(T) -> f32 + Send + Sync + 'static,
{
    {
        let mut guard = BOUND.lock().unwrap_or_else(|e| e.into_inner());
        let seen = guard.get_or_insert_with(std::collections::HashSet::new);
        if !seen.insert((signal.id().to_raw(), kind)) {
            return;
        }
    }

    let anim = anim.clone();
    blinc_core::reactive::effect(move |graph| {
        // Through the PASSED graph — see `bind_bool_targets`.
        let Some(v) = graph.get(signal) else { return };
        anim.lock().unwrap().set_target(place(v));
    });
}

/// Widget-kind discriminators for [`bind_number_target`]. Numbered apart
/// from `bool_binding` because both share one registration set.
pub mod num_binding {
    pub const SLIDER: u64 = 101;
}

/// A number a widget reads and writes, in whichever precision the
/// caller keeps it.
///
/// Widgets work in `f32`, because geometry and springs do. The state
/// behind one need not: `number_input` keeps an `f64`, and a slider
/// bound to the same number has to *be* that number rather than a copy
/// narrowed on the way in. Accepting either precision means neither
/// side mirrors the other, so there is one value with one identity
/// instead of two that can drift.
#[derive(Clone)]
pub enum NumberValue {
    F32(blinc_core::reactive::State<f32>),
    F64(blinc_core::reactive::State<f64>),
}

impl NumberValue {
    /// The current number, narrowed to what a widget works in.
    pub fn get(&self) -> f32 {
        match self {
            Self::F32(s) => s.get(),
            Self::F64(s) => s.get() as f32,
        }
    }

    /// Write it back in the caller's precision.
    pub fn set(&self, value: f32) {
        match self {
            Self::F32(s) => s.set(value),
            Self::F64(s) => s.set(value as f64),
        }
    }

    /// What to subscribe to. One id either way, so a `deps` list or a
    /// persisted animation key built from it is stable.
    pub fn signal_id(&self) -> blinc_core::reactive::SignalId {
        match self {
            Self::F32(s) => s.signal_id(),
            Self::F64(s) => s.signal_id(),
        }
    }

    /// Follow this number with an animation, whatever its precision.
    /// See [`bind_number_target`] for what `kind` and `place` mean.
    pub fn bind_target<F>(
        &self,
        kind: u64,
        anim: &blinc_layout::motion::SharedAnimatedValue,
        place: F,
    ) where
        F: Fn(f32) -> f32 + Send + Sync + 'static,
    {
        match self {
            Self::F32(s) => bind_number_target(kind, s.signal(), anim, place),
            Self::F64(s) => {
                bind_number_target(kind, s.signal(), anim, move |v: f64| place(v as f32))
            }
        }
    }
}

impl From<&blinc_core::reactive::State<f32>> for NumberValue {
    fn from(s: &blinc_core::reactive::State<f32>) -> Self {
        Self::F32(s.clone())
    }
}

impl From<blinc_core::reactive::State<f32>> for NumberValue {
    fn from(s: blinc_core::reactive::State<f32>) -> Self {
        Self::F32(s)
    }
}

impl From<&blinc_core::reactive::State<f64>> for NumberValue {
    fn from(s: &blinc_core::reactive::State<f64>) -> Self {
        Self::F64(s.clone())
    }
}

impl From<blinc_core::reactive::State<f64>> for NumberValue {
    fn from(s: blinc_core::reactive::State<f64>) -> Self {
        Self::F64(s)
    }
}
