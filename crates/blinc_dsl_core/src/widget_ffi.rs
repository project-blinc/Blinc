use super::*;
use crate::host::blinc_string_decode;
use std::collections::BTreeMap;

// =====================================================================
// Widget primitives — value-returning externs for `Div` / `Text`
// =====================================================================
//
// Each registered widget primitive has a matching extern here that boxes a
// `blinc_layout` value and returns the raw pointer as `i64`. Consumers
// reclaim via `Box::from_raw`. Handles are leak-on-construct until owned.

/// Tagged box carrying a concrete `blinc_layout` widget across the JIT boundary.
/// `Custom` carries arbitrary `ElementBuilder` implementations registered via
/// [`BlincDsl::register_extern_widget`].
pub enum WidgetBox {
    Text(Box<blinc_layout::text::Text>),
    Div(Box<blinc_layout::div::Div>),
    Custom(Box<dyn blinc_layout::div::ElementBuilder>),
}

impl WidgetBox {
    /// Coerce into a `Box<dyn ElementBuilder>` regardless of variant.
    pub fn into_element_builder(self) -> Box<dyn blinc_layout::div::ElementBuilder> {
        match self {
            WidgetBox::Text(t) => t,
            WidgetBox::Div(d) => d,
            WidgetBox::Custom(c) => c,
        }
    }
}

// =====================================================================
// Call-site instance IDs — span-derived stable identities
// =====================================================================
//
// Each `__component_call__` lowering site emits a `__push_call_id__` /
// `__pop_call_id__` bracket around the actual call. The pushed ID is a
// stable hash of `(source_filename, call_site_byte_offset)`, computed
// at lowering time. Widget FFI reads the top of this stack via
// `current_call_id()` and uses it to key per-instance state — so two
// `button("Click")` invocations at distinct source positions hold
// distinct FSMs, even if their labels match.
//
// The stack is per-thread (DSL JIT is single-threaded today) — we can
// switch to a Mutex if multi-thread invocation becomes a thing.

std::thread_local! {
    static CALL_ID_STACK: std::cell::RefCell<Vec<u64>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Push an instance-ID onto the current thread's call-site stack.
/// Emitted by `lower_component_calls` immediately before every
/// rewritten call expression.
pub extern "C" fn blinc_dsl_push_call_id(id: u64) {
    CALL_ID_STACK.with(|s| s.borrow_mut().push(id));
}

/// Pop the most recently pushed instance-ID. Emitted by
/// `lower_component_calls` immediately after every rewritten call
/// expression returns.
pub extern "C" fn blinc_dsl_pop_call_id() {
    CALL_ID_STACK.with(|s| {
        let _ = s.borrow_mut().pop();
    });
}

/// Pop the top-of-stack instance-ID AND pass-through the supplied
/// widget-handle value. Used by `lower_component_calls` as the
/// trailing expression of the push/call/pop bracket so the block's
/// value is the original call's return value, but the pop runs
/// AFTER the call's argument evaluation (which is when widget FFI
/// reads the call_id).
///
/// Calling-convention quirk: the value passes through unchanged —
/// this is purely a sequencing primitive.
pub extern "C" fn blinc_dsl_pop_call_id_and_return(value: i64) -> i64 {
    CALL_ID_STACK.with(|s| {
        let _ = s.borrow_mut().pop();
    });
    value
}

/// Read the top of the call-id stack, or `0` if the stack is empty
/// (i.e. we're outside any lowered component call — e.g. inside a
/// top-level `view {}` block, or pre-lowering test harness).
///
/// Exposed as an extern so JIT-compiled code can query it.
pub extern "C" fn blinc_dsl_current_call_id() -> u64 {
    CALL_ID_STACK.with(|s| s.borrow().last().copied().unwrap_or(0))
}

/// Rust-side convenience used by `dsl_state_key` and other state
/// allocators that need to key widgets to their nearest enclosing
/// call site. Currently always returns `0` because the wrap-injection
/// lowering pass that calls `__push_call_id__` is deferred — see the
/// comment in `lower_component_calls`. Kept here so the follow-up
/// commit can switch `dsl_state_key` over without touching the FFI
/// surface.
#[allow(dead_code)]
pub fn current_call_id() -> u64 {
    CALL_ID_STACK.with(|s| s.borrow().last().copied().unwrap_or(0))
}

/// Per-call overlay of visual styling props. `Some` fields
/// override the wrapped widget's `render_props()`; `None` fields
/// leave it alone.
///
/// Reactive props come in two shapes, both taking precedence over
/// the literal:
///
/// - `*_signal_id_raw` is set when the prop was bound to a bare
///   signal identifier. `apply_to` reads the current value via
///   `Signal::<T>::from_id`.
/// - `*_computed_id_raw` is set when the prop was bound to a
///   `computed { … } : T` expression. `apply_to` reads the current
///   value via `Computed::<T>::from_id().try_get()`, so any signal
///   the computed body reads stays a live dependency without
///   re-running the view.
///
/// At most one of the two reactive slots is set per prop on any
/// given call — the lowering pass picks the shape from the call-site
/// expression.
#[derive(Debug, Default, Clone)]
pub struct RenderPropsOverlay {
    pub background: Option<blinc_core::layer::Brush>,
    pub opacity: Option<f32>,
    /// Uniform corner radius (px). Maps to a four-corner-equal `CornerRadius`.
    pub corner_radius: Option<f32>,
    pub border_width: Option<f32>,
    pub border_color: Option<blinc_core::layer::Color>,
    /// `SignalId.to_raw()` for an f64 DSL signal bound to opacity.
    /// When `Some`, `apply_to` reads the current value via the
    /// process-global reactive graph (clamped to f32 since
    /// `RenderProps::opacity` is f32).
    pub opacity_signal_id_raw: Option<u64>,
    /// f64 signal id bound to corner_radius (px).
    pub corner_radius_signal_id_raw: Option<u64>,
    /// f64 signal id bound to border_width (px).
    pub border_width_signal_id_raw: Option<u64>,
    /// i32 signal id bound to bg. The signal value is read as a packed
    /// color hex (`Color::from_hex(value as u32)`), matching the
    /// literal-path encoding for `bg = 0xRRGGBB`.
    pub bg_signal_id_raw: Option<u64>,
    /// i32 signal id bound to border_color. Same hex-packed convention
    /// as [`Self::bg_signal_id_raw`].
    pub border_color_signal_id_raw: Option<u64>,
    /// `DerivedId.to_raw()` for an f64 `computed { … } : f64` bound
    /// to opacity. `apply_to` rehydrates a `Computed<f64>` from the
    /// id and reads its current value.
    pub opacity_computed_id_raw: Option<u64>,
    /// f64 computed id bound to corner_radius (px).
    pub corner_radius_computed_id_raw: Option<u64>,
    /// f64 computed id bound to border_width (px).
    pub border_width_computed_id_raw: Option<u64>,
    /// i32 computed id bound to bg. Same hex-packed value convention
    /// as the signal slot.
    pub bg_computed_id_raw: Option<u64>,
    /// i32 computed id bound to border_color.
    pub border_color_computed_id_raw: Option<u64>,
}

impl RenderPropsOverlay {
    /// Register every signal- or computed-bound prop on this overlay
    /// with the global `PropertyBindingRegistry`. Called by
    /// `Styled::build` once the wrapped widget has resolved a real
    /// `LayoutNodeId`. The registry's notifier fires on every
    /// `Signal::set` (and on derived re-evaluation), walking
    /// subscribers and queueing partial prop updates that trigger
    /// `request_redraw`. Without this, the overlay's `apply_to`
    /// reads the live signal value at paint time but nothing
    /// triggers the next paint when the signal mutates.
    ///
    /// Each registered write closure mirrors the matching branch of
    /// `apply_to`, so a binding fire and a fresh `apply_to` walk
    /// produce identical `RenderProps`.
    pub fn register_bindings(&self, node_id: blinc_layout::LayoutNodeId) {
        use blinc_core::reactive::{Computed, DerivedId, Signal, SignalId};
        use blinc_layout::binding::{BoundValue, with_registry};
        use blinc_layout::property::PropertyId;
        use std::sync::Arc;

        // ── Signal-bound slots ─────────────────────────────────────
        if let Some(id_raw) = self.opacity_signal_id_raw {
            let sid = SignalId::from_raw(id_raw);
            with_registry(|reg| {
                reg.register(
                    sid,
                    node_id,
                    PropertyId::Opacity,
                    Arc::new(move || Signal::<f64>::from_id(sid).try_get().map(BoundValue::new)),
                    Arc::new(|props: &mut blinc_layout::RenderProps, val: &BoundValue| {
                        if let Some(v) = val.downcast_ref::<f64>() {
                            props.opacity = *v as f32;
                        }
                    }),
                );
            });
        }
        if let Some(id_raw) = self.corner_radius_signal_id_raw {
            let sid = SignalId::from_raw(id_raw);
            with_registry(|reg| {
                reg.register(
                    sid,
                    node_id,
                    PropertyId::CornerRadius,
                    Arc::new(move || Signal::<f64>::from_id(sid).try_get().map(BoundValue::new)),
                    Arc::new(|props: &mut blinc_layout::RenderProps, val: &BoundValue| {
                        if let Some(v) = val.downcast_ref::<f64>() {
                            let r = *v as f32;
                            props.border_radius = blinc_core::layer::CornerRadius::new(r, r, r, r);
                            props.border_radius_explicit = true;
                        }
                    }),
                );
            });
        }
        if let Some(id_raw) = self.border_width_signal_id_raw {
            let sid = SignalId::from_raw(id_raw);
            with_registry(|reg| {
                reg.register(
                    sid,
                    node_id,
                    PropertyId::BorderWidth,
                    Arc::new(move || Signal::<f64>::from_id(sid).try_get().map(BoundValue::new)),
                    Arc::new(|props: &mut blinc_layout::RenderProps, val: &BoundValue| {
                        if let Some(v) = val.downcast_ref::<f64>() {
                            props.border_width = *v as f32;
                        }
                    }),
                );
            });
        }
        if let Some(id_raw) = self.bg_signal_id_raw {
            let sid = SignalId::from_raw(id_raw);
            with_registry(|reg| {
                reg.register(
                    sid,
                    node_id,
                    PropertyId::Background,
                    Arc::new(move || Signal::<i32>::from_id(sid).try_get().map(BoundValue::new)),
                    Arc::new(|props: &mut blinc_layout::RenderProps, val: &BoundValue| {
                        if let Some(v) = val.downcast_ref::<i32>() {
                            props.background = Some(blinc_core::layer::Brush::Solid(
                                blinc_core::layer::Color::from_hex(*v as u32),
                            ));
                        }
                    }),
                );
            });
        }
        if let Some(id_raw) = self.border_color_signal_id_raw {
            let sid = SignalId::from_raw(id_raw);
            with_registry(|reg| {
                reg.register(
                    sid,
                    node_id,
                    PropertyId::BorderColor,
                    Arc::new(move || Signal::<i32>::from_id(sid).try_get().map(BoundValue::new)),
                    Arc::new(|props: &mut blinc_layout::RenderProps, val: &BoundValue| {
                        if let Some(v) = val.downcast_ref::<i32>() {
                            props.border_color =
                                Some(blinc_core::layer::Color::from_hex(*v as u32));
                        }
                    }),
                );
            });
        }

        // ── Computed-bound slots ───────────────────────────────────
        // Same shape, but the read closure goes through
        // `Computed::<T>::from_id(...).try_get()` which re-evaluates
        // through the reactive graph (and fires on every dependency
        // signal's mutation).
        if let Some(id_raw) = self.opacity_computed_id_raw {
            let did = DerivedId::from_raw(id_raw);
            with_registry(|reg| {
                reg.register_derived(
                    did,
                    node_id,
                    PropertyId::Opacity,
                    Arc::new(move || Computed::<f64>::from_id(did).try_get().map(BoundValue::new)),
                    Arc::new(|props: &mut blinc_layout::RenderProps, val: &BoundValue| {
                        if let Some(v) = val.downcast_ref::<f64>() {
                            props.opacity = *v as f32;
                        }
                    }),
                );
            });
        }
        if let Some(id_raw) = self.corner_radius_computed_id_raw {
            let did = DerivedId::from_raw(id_raw);
            with_registry(|reg| {
                reg.register_derived(
                    did,
                    node_id,
                    PropertyId::CornerRadius,
                    Arc::new(move || Computed::<f64>::from_id(did).try_get().map(BoundValue::new)),
                    Arc::new(|props: &mut blinc_layout::RenderProps, val: &BoundValue| {
                        if let Some(v) = val.downcast_ref::<f64>() {
                            let r = *v as f32;
                            props.border_radius = blinc_core::layer::CornerRadius::new(r, r, r, r);
                            props.border_radius_explicit = true;
                        }
                    }),
                );
            });
        }
        if let Some(id_raw) = self.border_width_computed_id_raw {
            let did = DerivedId::from_raw(id_raw);
            with_registry(|reg| {
                reg.register_derived(
                    did,
                    node_id,
                    PropertyId::BorderWidth,
                    Arc::new(move || Computed::<f64>::from_id(did).try_get().map(BoundValue::new)),
                    Arc::new(|props: &mut blinc_layout::RenderProps, val: &BoundValue| {
                        if let Some(v) = val.downcast_ref::<f64>() {
                            props.border_width = *v as f32;
                        }
                    }),
                );
            });
        }
        if let Some(id_raw) = self.bg_computed_id_raw {
            let did = DerivedId::from_raw(id_raw);
            with_registry(|reg| {
                reg.register_derived(
                    did,
                    node_id,
                    PropertyId::Background,
                    Arc::new(move || Computed::<i32>::from_id(did).try_get().map(BoundValue::new)),
                    Arc::new(|props: &mut blinc_layout::RenderProps, val: &BoundValue| {
                        if let Some(v) = val.downcast_ref::<i32>() {
                            props.background = Some(blinc_core::layer::Brush::Solid(
                                blinc_core::layer::Color::from_hex(*v as u32),
                            ));
                        }
                    }),
                );
            });
        }
        if let Some(id_raw) = self.border_color_computed_id_raw {
            let did = DerivedId::from_raw(id_raw);
            with_registry(|reg| {
                reg.register_derived(
                    did,
                    node_id,
                    PropertyId::BorderColor,
                    Arc::new(move || Computed::<i32>::from_id(did).try_get().map(BoundValue::new)),
                    Arc::new(|props: &mut blinc_layout::RenderProps, val: &BoundValue| {
                        if let Some(v) = val.downcast_ref::<i32>() {
                            props.border_color =
                                Some(blinc_core::layer::Color::from_hex(*v as u32));
                        }
                    }),
                );
            });
        }
    }

    pub fn apply_to(&self, base: &mut blinc_layout::RenderProps) {
        // Priority per prop: computed > signal > literal. The lowering
        // pass picks exactly one shape per arg so we never see both a
        // signal slot and a computed slot set together; the else-ifs
        // are defensive.
        //
        // `Computed::try_get` reads through the same reactive graph
        // signals do, so any signals the computed body touches stay
        // tracked dependencies without us inspecting them here.

        // bg: i32 hex.
        if let Some(id_raw) = self.bg_computed_id_raw {
            let derived = blinc_core::reactive::Computed::<i32>::from_id(
                blinc_core::reactive::DerivedId::from_raw(id_raw),
            );
            if let Some(v) = derived.try_get() {
                base.background = Some(blinc_core::layer::Brush::Solid(
                    blinc_core::layer::Color::from_hex(v as u32),
                ));
            }
        } else if let Some(id_raw) = self.bg_signal_id_raw {
            let signal = blinc_core::reactive::Signal::<i32>::from_id(
                blinc_core::reactive::SignalId::from_raw(id_raw),
            );
            if let Some(v) = signal.try_get() {
                base.background = Some(blinc_core::layer::Brush::Solid(
                    blinc_core::layer::Color::from_hex(v as u32),
                ));
            }
        } else if let Some(bg) = self.background.clone() {
            base.background = Some(bg);
        }

        // opacity: f64 → f32.
        if let Some(id_raw) = self.opacity_computed_id_raw {
            let derived = blinc_core::reactive::Computed::<f64>::from_id(
                blinc_core::reactive::DerivedId::from_raw(id_raw),
            );
            if let Some(v) = derived.try_get() {
                base.opacity = v as f32;
            }
        } else if let Some(id_raw) = self.opacity_signal_id_raw {
            let signal = blinc_core::reactive::Signal::<f64>::from_id(
                blinc_core::reactive::SignalId::from_raw(id_raw),
            );
            if let Some(v) = signal.try_get() {
                base.opacity = v as f32;
            }
        } else if let Some(o) = self.opacity {
            base.opacity = o;
        }

        // corner_radius: f64 → uniform CornerRadius.
        if let Some(id_raw) = self.corner_radius_computed_id_raw {
            let derived = blinc_core::reactive::Computed::<f64>::from_id(
                blinc_core::reactive::DerivedId::from_raw(id_raw),
            );
            if let Some(v) = derived.try_get() {
                let r = v as f32;
                base.border_radius = blinc_core::layer::CornerRadius::new(r, r, r, r);
                base.border_radius_explicit = true;
            }
        } else if let Some(id_raw) = self.corner_radius_signal_id_raw {
            let signal = blinc_core::reactive::Signal::<f64>::from_id(
                blinc_core::reactive::SignalId::from_raw(id_raw),
            );
            if let Some(v) = signal.try_get() {
                let r = v as f32;
                base.border_radius = blinc_core::layer::CornerRadius::new(r, r, r, r);
                base.border_radius_explicit = true;
            }
        } else if let Some(r) = self.corner_radius {
            base.border_radius = blinc_core::layer::CornerRadius::new(r, r, r, r);
            base.border_radius_explicit = true;
        }

        // border_width: f64 → f32.
        if let Some(id_raw) = self.border_width_computed_id_raw {
            let derived = blinc_core::reactive::Computed::<f64>::from_id(
                blinc_core::reactive::DerivedId::from_raw(id_raw),
            );
            if let Some(v) = derived.try_get() {
                base.border_width = v as f32;
            }
        } else if let Some(id_raw) = self.border_width_signal_id_raw {
            let signal = blinc_core::reactive::Signal::<f64>::from_id(
                blinc_core::reactive::SignalId::from_raw(id_raw),
            );
            if let Some(v) = signal.try_get() {
                base.border_width = v as f32;
            }
        } else if let Some(w) = self.border_width {
            base.border_width = w;
        }

        // border_color: i32 hex.
        if let Some(id_raw) = self.border_color_computed_id_raw {
            let derived = blinc_core::reactive::Computed::<i32>::from_id(
                blinc_core::reactive::DerivedId::from_raw(id_raw),
            );
            if let Some(v) = derived.try_get() {
                base.border_color = Some(blinc_core::layer::Color::from_hex(v as u32));
            }
        } else if let Some(id_raw) = self.border_color_signal_id_raw {
            let signal = blinc_core::reactive::Signal::<i32>::from_id(
                blinc_core::reactive::SignalId::from_raw(id_raw),
            );
            if let Some(v) = signal.try_get() {
                base.border_color = Some(blinc_core::layer::Color::from_hex(v as u32));
            }
        } else if let Some(c) = self.border_color {
            base.border_color = Some(c);
        }
    }
}

/// Runtime-owned representation of a DSL `struct` literal after it has been
/// marshalled across the JIT boundary as an opaque `i64` handle.
#[derive(Debug, Clone, Default)]
pub struct BlincStructValue {
    fields: BTreeMap<String, BlincStructFieldValue>,
}

impl BlincStructValue {
    pub fn insert(&mut self, name: impl Into<String>, value: BlincStructFieldValue) {
        self.fields.insert(name.into(), value);
    }

    pub fn get(&self, name: &str) -> Option<&BlincStructFieldValue> {
        self.fields.get(name)
    }

    pub fn get_string(&self, name: &str) -> Option<&str> {
        match self.get(name) {
            Some(BlincStructFieldValue::String(value)) => Some(value),
            _ => None,
        }
    }

    pub fn get_i32(&self, name: &str) -> Option<i32> {
        match self.get(name) {
            Some(BlincStructFieldValue::I32(value)) => Some(*value),
            Some(BlincStructFieldValue::Bool(value)) => Some(i32::from(*value)),
            Some(BlincStructFieldValue::I64(value)) => (*value).try_into().ok(),
            _ => None,
        }
    }

    pub fn get_i64(&self, name: &str) -> Option<i64> {
        match self.get(name) {
            Some(BlincStructFieldValue::I32(value)) => Some(i64::from(*value)),
            Some(BlincStructFieldValue::Bool(value)) => Some(i64::from(*value)),
            Some(BlincStructFieldValue::I64(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn get_f64(&self, name: &str) -> Option<f64> {
        match self.get(name) {
            Some(BlincStructFieldValue::F64(value)) => Some(*value),
            _ => None,
        }
    }

    pub fn get_bool(&self, name: &str) -> Option<bool> {
        match self.get(name) {
            Some(BlincStructFieldValue::Bool(value)) => Some(*value),
            Some(BlincStructFieldValue::I32(value)) => Some(*value != 0),
            Some(BlincStructFieldValue::I64(value)) => Some(*value != 0),
            _ => None,
        }
    }

    pub fn get_struct(&self, name: &str) -> Option<&BlincStructValue> {
        match self.get(name) {
            Some(BlincStructFieldValue::Struct(value)) => Some(value),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum BlincStructFieldValue {
    Bool(bool),
    I32(i32),
    I64(i64),
    F64(f64),
    String(String),
    Struct(BlincStructValue),
}

/// Wraps a widget with a per-call styling overlay. Build /
/// children delegate to the inner widget; `render_props()`
/// merges the overlay.
pub struct Styled<W: blinc_layout::div::ElementBuilder> {
    inner: W,
    overlay: RenderPropsOverlay,
}

impl<W: blinc_layout::div::ElementBuilder> Styled<W> {
    pub fn new(widget: W, overlay: RenderPropsOverlay) -> Self {
        Self {
            inner: widget,
            overlay,
        }
    }
    pub fn inner(&self) -> &W {
        &self.inner
    }
    pub fn overlay(&self) -> &RenderPropsOverlay {
        &self.overlay
    }
}

impl<W: blinc_layout::div::ElementBuilder> blinc_layout::div::ElementBuilder for Styled<W> {
    fn build(&self, tree: &mut blinc_layout::LayoutTree) -> blinc_layout::LayoutNodeId {
        let node_id = self.inner.build(tree);
        // Register every signal/computed prop on the overlay with the
        // global PropertyBindingRegistry now that we have a node_id.
        // This is what makes `Div(opacity = sig)` actually react to
        // `sig.set(...)` in a live app: signal fire → registry walks
        // subscribers → queues a partial prop update → renderer
        // requests a redraw → next paint reads the live value through
        // the same `apply_to` path the static-only render uses.
        //
        // The unit tests for the overlay path (`dsl_*_signal_binding_*`)
        // work without this because they call `builder.render_props()`
        // manually, sidestepping the redraw chain.
        self.overlay.register_bindings(node_id);
        node_id
    }
    fn render_props(&self) -> blinc_layout::RenderProps {
        let mut base = self.inner.render_props();
        self.overlay.apply_to(&mut base);
        base
    }
    fn children_builders(&self) -> &[Box<dyn blinc_layout::div::ElementBuilder>] {
        self.inner.children_builders()
    }
    // Forward identity so CSS class/id/type-name selectors match through the wrapper.
    fn element_classes(&self) -> &[std::sync::Arc<str>] {
        self.inner.element_classes()
    }
    fn element_id(&self) -> Option<&str> {
        self.inner.element_id()
    }
    fn element_type_id(&self) -> blinc_layout::div::ElementTypeId {
        self.inner.element_type_id()
    }
    // MUST forward — every DSL widget is wrapped here, so a container
    // reading typed structure from its children (`cn.Accordion` looking
    // for `cn.AccordionItem`) would otherwise only ever see `Styled`.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        self.inner.as_any()
    }
    // MUST forward — without this, on_click/on_hover on the inner Div never fire.
    fn event_handlers(&self) -> Option<&blinc_layout::event_handler::EventHandlers> {
        self.inner.event_handlers()
    }
    fn scroll_physics(&self) -> Option<blinc_layout::scroll::SharedScrollPhysics> {
        self.inner.scroll_physics()
    }
    fn visual_animation_config(
        &self,
    ) -> Option<blinc_layout::visual_animation::VisualAnimationConfig> {
        self.inner.visual_animation_config()
    }
}

/// Internals for the [`extern_widget`] attribute macro's code generation. Not stable API.
#[doc(hidden)]
pub mod __extern_widget_internals {
    pub use crate::{
        BlincDsl, BlincDslError, BlincDslResult, BlincStructFieldValue, BlincStructValue,
        ExternWidget, ExternWidgetSpec, RenderPropsOverlay, Styled, WidgetBox,
    };
    pub use blinc_runtime::component::PropDef;
    pub use blinc_runtime::{
        REACTIVE_TAG_COMPUTED, REACTIVE_TAG_LITERAL, REACTIVE_TAG_SIGNAL, Reactive,
    };
    pub use zyntax_typed_ast::InternedString;
    pub use zyntax_typed_ast::type_registry::{PrimitiveType, Type};

    /// Reclaim a `RenderPropsOverlay` from `__new_style_overlay__` for macro thunks.
    ///
    /// # Safety
    ///
    /// `ptr` must come from `__new_style_overlay__`.
    pub unsafe fn decode_overlay(ptr: i64) -> crate::RenderPropsOverlay {
        unsafe { crate::materialize_overlay(ptr) }
    }

    /// Wrap an `ElementBuilder` in `WidgetBox::Custom` and leak as a `WidgetHandle`.
    pub fn into_handle(widget: Box<dyn blinc_layout::div::ElementBuilder>) -> i64 {
        Box::into_raw(Box::new(WidgetBox::Custom(widget))) as i64
    }

    /// A Zyntax list, as the compiler lays it out.
    ///
    /// An array literal lowers to `List<T> { data, len, capacity }`
    /// (see `prelude.zynml`), so a collection prop crosses as one
    /// pointer to this struct. Nothing marshals per element.
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct ZynList {
        /// Pointer to the element buffer.
        pub data: i64,
        pub len: i64,
        pub capacity: i64,
    }

    /// Read a list header. A null pointer reads as empty, which is what
    /// an omitted collection prop arrives as.
    ///
    /// # Safety
    ///
    /// `ptr` must be a pointer to a Zyntax `List<T>`.
    pub unsafe fn decode_list(ptr: i64) -> ZynList {
        if ptr == 0 {
            return ZynList {
                data: 0,
                len: 0,
                capacity: 0,
            };
        }
        unsafe { *(ptr as *const ZynList) }
    }

    /// Copy `len` elements of type `T` out of a list's buffer.
    ///
    /// The stride is `size_of::<T>()`, which has to match Zyntax's
    /// `hir_ty_size` for the element type the compiler inferred: 1 for
    /// `bool`, 4 for `i32`, 8 for `i64` / `f64` / any pointer. Reading
    /// a packed `bool` buffer at an 8-byte stride yields garbage, so
    /// the caller picks `T` to match, and `#[extern_widget]` derives it
    /// from the declared `Vec<T>`.
    ///
    /// Copies rather than borrows: the buffer is `Alloca`'d in the JIT
    /// caller's frame and dies when the call returns.
    ///
    /// # Safety
    ///
    /// `T` must be the element type the compiler used, and `list` must
    /// describe a live buffer.
    pub unsafe fn read_list_elements<T: Copy>(list: ZynList) -> Vec<T> {
        if list.data == 0 || list.len <= 0 {
            return Vec::new();
        }
        let base = list.data as *const T;
        (0..list.len as usize)
            .map(|i| unsafe { std::ptr::read_unaligned(base.add(i)) })
            .collect()
    }

    /// Copy a list of DSL strings out.
    ///
    /// Two indirections: the element is a pointer (stride 8, since
    /// `string` converts to `Ptr(U8)`), and the pointed-to value is a
    /// length-prefixed buffer rather than a C string.
    ///
    /// # Safety
    ///
    /// `list` must describe a live buffer of DSL string pointers.
    pub unsafe fn read_string_list(list: ZynList) -> Vec<String> {
        unsafe { read_list_elements::<i64>(list) }
            .into_iter()
            .map(|p| unsafe { decode_string(p as *const i32) })
            .collect()
    }

    /// Decode a Zyntax-FFI string argument (length-prefixed
    /// UTF-8 buffer) to an owned `String`. Empty for null.
    ///
    /// # Safety
    ///
    /// `ptr` must come from Zyntax JIT's String FFI lowering.
    pub unsafe fn decode_string(ptr: *const i32) -> String {
        if ptr.is_null() {
            return String::new();
        }
        // SAFETY: forwarded from caller per fn-level doc.
        let s = unsafe { crate::host::blinc_string_decode(ptr) };
        s.to_string()
    }

    /// Decode a `__new_child_list__` pointer into a `Vec<Box<dyn ElementBuilder>>`.
    /// Null/zero pointer means no children.
    ///
    /// # Safety
    ///
    /// `ptr` must be the `i64`-encoded payload of a list minted by `__new_child_list__`.
    pub unsafe fn decode_children(ptr: i64) -> Vec<Box<dyn blinc_layout::div::ElementBuilder>> {
        if ptr == 0 {
            return Vec::new();
        }
        // SAFETY: see fn-level doc.
        let handles: Vec<i64> = *unsafe { Box::from_raw(ptr as *mut Vec<i64>) };
        handles
            .into_iter()
            .filter_map(|h| {
                unsafe { super::materialize_widget(h) }.map(|w| w.into_element_builder())
            })
            .collect()
    }

    /// Decode a DSL struct-value pointer. Null/zero yields an empty struct.
    ///
    /// # Safety
    ///
    /// `ptr` must be the `i64`-encoded payload minted by `__new_struct_value__`.
    pub unsafe fn decode_struct(ptr: i64) -> crate::BlincStructValue {
        if ptr == 0 {
            return crate::BlincStructValue::default();
        }
        *unsafe { Box::from_raw(ptr as *mut crate::BlincStructValue) }
    }
}

/// Rust types callable from Blinc DSL source. Generated by `#[extern_widget]`,
/// or implemented manually. Register via `dsl.register_extern_widget::<T>()`.
pub trait ExternWidget {
    /// User-facing identifier — what `.blinc` source types to call the widget.
    const DSL_NAME: &'static str;

    /// Build the full registration spec.
    fn extern_widget_spec() -> ExternWidgetSpec;
}

/// Description of a Rust-side widget exposed to the DSL. Passed to
/// [`BlincDsl::register_extern_widget`]. `view_symbol` convention is
/// `$Blinc$<Name>$view`. `extern_ptr` must be `extern "C" fn(...)` matching
/// `param_types`, returning a `WidgetHandle` (`0` = null/build-failed).
pub struct ExternWidgetSpec {
    /// User-facing DSL name (e.g. `"Button"`).
    pub name: String,
    /// JIT-linker-visible symbol. Convention: `$Blinc$<Name>$view`.
    pub view_symbol: String,
    /// Substrate metadata about the widget's props.
    pub props: Vec<blinc_runtime::component::PropDef>,
    /// FFI parameter types in declaration order; must match `extern_ptr` exactly.
    pub param_types: Vec<Type>,
    /// FFI return type (typically `i64` for widget handle).
    pub return_type: Type,
    /// `extern "C" fn(...)` cast to `*const u8`.
    pub extern_ptr: *const u8,
}

/// Opaque widget handle across the JIT boundary. `Box<WidgetBox>` raw as `i64`,
/// with `0` as the null sentinel.
type WidgetHandle = i64;

/// Take ownership of a `WidgetHandle` returned by a view fn. `None` for `0`.
///
/// # Safety
///
/// `handle` must be from one of this crate's `$Blinc$<X>$view` externs (or `0`).
/// Each non-zero handle may be materialised exactly once.
pub unsafe fn materialize_widget(handle: WidgetHandle) -> Option<Box<WidgetBox>> {
    if handle == 0 {
        return None;
    }
    // SAFETY: see fn-level doc.
    Some(unsafe { Box::from_raw(handle as *mut WidgetBox) })
}

fn materialize_children(children: WidgetHandle) -> Vec<Box<dyn blinc_layout::div::ElementBuilder>> {
    if children == 0 {
        return Vec::new();
    }
    let list: Box<Vec<WidgetHandle>> = unsafe { Box::from_raw(children as *mut Vec<WidgetHandle>) };
    let mut out = Vec::with_capacity(list.len());
    for handle in *list {
        if let Some(child_box) = unsafe { materialize_widget(handle) } {
            out.push(child_box.into_element_builder());
        }
    }
    out
}

fn decode_string_arg(ptr: *const i32) -> String {
    if ptr.is_null() {
        String::new()
    } else {
        // SAFETY: all string params use Zyntax's length-prefixed UTF-8 ABI.
        unsafe { blinc_string_decode(ptr) }.to_string()
    }
}

fn decoded_class_names(class_str: *const i32) -> Vec<String> {
    decode_string_arg(class_str)
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

fn ensure_theme_state() {
    let _ = blinc_theme::ThemeState::try_get().unwrap_or_else(|| {
        blinc_theme::ThemeState::init_default();
        blinc_theme::ThemeState::get()
    });
}

fn ensure_context_state() {
    if blinc_core::BlincContextState::try_get().is_some() {
        return;
    }

    // Use the process-global reactive graph so bare `signal()` calls
    // from DSL widgets interop with the rest of the reactive surface.
    let reactive = blinc_core::reactive::global_graph();
    let hooks = std::sync::Arc::new(std::sync::Mutex::new(blinc_core::HookState::new()));
    let dirty = blinc_core::reactive::global_dirty_flag();
    blinc_core::BlincContextState::init(reactive, hooks, dirty);
}

/// Build a per-call-site state key for DSL widgets. The `call_id` is
/// the span-derived `u64` injected by `crate::passes::inject_call_site_keys`
/// at the lowering stage — every substrate-primitive widget call carries
/// it as the leading positional arg, so two `Button("Play")` invocations
/// at distinct source positions get distinct keys even though their
/// labels match.
///
/// The format `blinc-dsl:<kind>:<call_id_hex>` is opaque but stable;
/// `dsl_state_key` consumers don't parse it.
fn dsl_state_key(kind: &str, call_id: u64) -> String {
    let mut key = String::with_capacity(kind.len() + 32);
    key.push_str("blinc-dsl:");
    key.push_str(kind);
    key.push(':');
    use std::fmt::Write as _;
    let _ = write!(&mut key, "{call_id:016x}");
    key
}

pub(crate) extern "C" fn blinc_new_struct_value() -> i64 {
    Box::into_raw(Box::new(BlincStructValue::default())) as i64
}

fn with_struct_value(ptr: i64, f: impl FnOnce(&mut BlincStructValue)) {
    if ptr == 0 {
        return;
    }
    // SAFETY: `ptr` is owned by the DSL-side struct marshalling block until
    // the final widget thunk consumes it.
    let value = unsafe { &mut *(ptr as *mut BlincStructValue) };
    f(value);
}

pub(crate) extern "C" fn blinc_set_struct_i32(ptr: i64, name_ptr: *const i32, value: i32) {
    let name = decode_string_arg(name_ptr);
    with_struct_value(ptr, |s| {
        s.insert(name, BlincStructFieldValue::I32(value));
    });
}

pub(crate) extern "C" fn blinc_set_struct_bool(ptr: i64, name_ptr: *const i32, value: i32) {
    let name = decode_string_arg(name_ptr);
    with_struct_value(ptr, |s| {
        s.insert(name, BlincStructFieldValue::Bool(value != 0));
    });
}

pub(crate) extern "C" fn blinc_set_struct_i64(ptr: i64, name_ptr: *const i32, value: i64) {
    let name = decode_string_arg(name_ptr);
    with_struct_value(ptr, |s| {
        s.insert(name, BlincStructFieldValue::I64(value));
    });
}

pub(crate) extern "C" fn blinc_set_struct_f64(ptr: i64, name_ptr: *const i32, value: f64) {
    let name = decode_string_arg(name_ptr);
    with_struct_value(ptr, |s| {
        s.insert(name, BlincStructFieldValue::F64(value));
    });
}

pub(crate) extern "C" fn blinc_set_struct_string(
    ptr: i64,
    name_ptr: *const i32,
    value_ptr: *const i32,
) {
    let name = decode_string_arg(name_ptr);
    let value = decode_string_arg(value_ptr);
    with_struct_value(ptr, |s| {
        s.insert(name, BlincStructFieldValue::String(value));
    });
}

pub(crate) extern "C" fn blinc_set_struct_handle(ptr: i64, name_ptr: *const i32, value_ptr: i64) {
    let name = decode_string_arg(name_ptr);
    if value_ptr == 0 {
        with_struct_value(ptr, |s| {
            s.insert(
                name,
                BlincStructFieldValue::Struct(BlincStructValue::default()),
            );
        });
        return;
    }
    // SAFETY: nested struct handles are consumed when inserted into the parent.
    let nested = *unsafe { Box::from_raw(value_ptr as *mut BlincStructValue) };
    with_struct_value(ptr, |s| {
        s.insert(name, BlincStructFieldValue::Struct(nested));
    });
}

fn leak_custom(widget: impl blinc_layout::div::ElementBuilder + 'static) -> WidgetHandle {
    Box::into_raw(Box::new(WidgetBox::Custom(Box::new(widget)))) as WidgetHandle
}

fn finish_text_widget(
    mut widget: blinc_layout::text::Text,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    for name in decoded_class_names(class_str) {
        widget = widget.class(name);
    }
    if style == 0 {
        Box::into_raw(Box::new(WidgetBox::Text(Box::new(widget)))) as WidgetHandle
    } else {
        let overlay = unsafe { materialize_overlay(style) };
        leak_custom(Styled::new(widget, overlay))
    }
}

fn finish_div_widget(
    mut widget: blinc_layout::div::Div,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    for name in decoded_class_names(class_str) {
        widget = widget.class(name);
    }
    let overlay = unsafe { materialize_overlay(style) };
    leak_custom(Styled::new(widget, overlay))
}

fn finish_custom_widget(
    widget: impl blinc_layout::div::ElementBuilder + 'static,
    style: i64,
) -> WidgetHandle {
    let overlay = unsafe { materialize_overlay(style) };
    leak_custom(Styled::new(widget, overlay))
}

macro_rules! typography_view {
    ($fn_name:ident, $builder:path) => {
        pub(crate) extern "C" fn $fn_name(
            _call_id: u64,
            content_ptr: *const i32,
            style: i64,
            class_str: *const i32,
        ) -> WidgetHandle {
            let content = decode_string_arg(content_ptr);
            finish_text_widget($builder(content), style, class_str)
        }
    };
}

typography_view!(blinc_h1_view, blinc_layout::typography::h1);
typography_view!(blinc_h2_view, blinc_layout::typography::h2);
typography_view!(blinc_h3_view, blinc_layout::typography::h3);
typography_view!(blinc_h4_view, blinc_layout::typography::h4);
typography_view!(blinc_h5_view, blinc_layout::typography::h5);
typography_view!(blinc_h6_view, blinc_layout::typography::h6);
typography_view!(blinc_p_view, blinc_layout::typography::p);
typography_view!(blinc_span_view, blinc_layout::typography::span);
typography_view!(blinc_small_view, blinc_layout::typography::small);
typography_view!(blinc_label_view, blinc_layout::typography::label);
typography_view!(blinc_muted_view, blinc_layout::typography::muted);
typography_view!(blinc_strong_view, blinc_layout::typography::strong);
typography_view!(blinc_b_view, blinc_layout::typography::b);
typography_view!(blinc_caption_view, blinc_layout::typography::caption);
typography_view!(
    blinc_inline_code_view,
    blinc_layout::typography::inline_code
);

/// `$Blinc$Text$view(content, style, class) -> WidgetHandle`
///
/// # Safety
///
/// `content_ptr` must point at a Zyntax length-prefixed UTF-8 buffer.
pub(crate) extern "C" fn blinc_text_view(
    _call_id: u64,
    content_ptr: *const i32,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    if content_ptr.is_null() {
        tracing::warn!("$Blinc$Text$view called with null content pointer");
        return 0;
    }
    // SAFETY: see fn-level doc.
    let content = unsafe { blinc_string_decode(content_ptr) };
    finish_text_widget(blinc_layout::text::Text::new(content), style, class_str)
}

pub(crate) extern "C" fn blinc_hr_view(
    _call_id: u64,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    finish_div_widget(blinc_layout::widgets::hr(), style, class_str)
}

pub(crate) extern "C" fn blinc_blockquote_view(
    _call_id: u64,
    children: WidgetHandle,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    let mut widget = blinc_layout::widgets::blockquote();
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    for name in decoded_class_names(class_str) {
        widget = widget.class(&name);
    }
    finish_custom_widget(widget, style)
}

pub(crate) extern "C" fn blinc_link_view(
    _call_id: u64,
    label_ptr: *const i32,
    url_ptr: *const i32,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    let label = decode_string_arg(label_ptr);
    let url = decode_string_arg(url_ptr);
    let mut widget = blinc_layout::widgets::link(label, url);
    for name in decoded_class_names(class_str) {
        widget = widget.class(&name);
    }
    finish_custom_widget(widget, style)
}

pub(crate) extern "C" fn blinc_ul_view(
    _call_id: u64,
    children: WidgetHandle,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    let mut widget = blinc_layout::widgets::ul();
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    for name in decoded_class_names(class_str) {
        widget = widget.class(&name);
    }
    finish_custom_widget(widget, style)
}

pub(crate) extern "C" fn blinc_ol_view(
    _call_id: u64,
    children: WidgetHandle,
    start: i32,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    let mut widget = blinc_layout::widgets::ol_start(start.max(1) as usize);
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    for name in decoded_class_names(class_str) {
        widget = widget.class(&name);
    }
    finish_custom_widget(widget, style)
}

pub(crate) extern "C" fn blinc_li_view(
    _call_id: u64,
    children: WidgetHandle,
    style: i64,
) -> WidgetHandle {
    let mut widget = blinc_layout::widgets::li();
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    finish_custom_widget(widget, style)
}

pub(crate) extern "C" fn blinc_task_item_view(
    _call_id: u64,
    children: WidgetHandle,
    checked: i32,
    style: i64,
) -> WidgetHandle {
    let mut widget = blinc_layout::widgets::task_item(checked != 0);
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    finish_custom_widget(widget, style)
}

pub(crate) extern "C" fn blinc_table_view(
    _call_id: u64,
    children: WidgetHandle,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    let mut widget = blinc_layout::widgets::table();
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    finish_div_widget(widget, style, class_str)
}

pub(crate) extern "C" fn blinc_thead_view(
    _call_id: u64,
    children: WidgetHandle,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    let mut widget = blinc_layout::widgets::thead();
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    finish_div_widget(widget, style, class_str)
}

pub(crate) extern "C" fn blinc_tbody_view(
    _call_id: u64,
    children: WidgetHandle,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    let mut widget = blinc_layout::widgets::tbody();
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    finish_div_widget(widget, style, class_str)
}

pub(crate) extern "C" fn blinc_tfoot_view(
    _call_id: u64,
    children: WidgetHandle,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    let mut widget = blinc_layout::widgets::tfoot();
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    finish_div_widget(widget, style, class_str)
}

pub(crate) extern "C" fn blinc_tr_view(
    _call_id: u64,
    children: WidgetHandle,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    let mut widget = blinc_layout::widgets::tr();
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    finish_div_widget(widget, style, class_str)
}

pub(crate) extern "C" fn blinc_th_view(
    _call_id: u64,
    content_ptr: *const i32,
    style: i64,
) -> WidgetHandle {
    let content = decode_string_arg(content_ptr);
    finish_custom_widget(blinc_layout::widgets::th(content), style)
}

pub(crate) extern "C" fn blinc_td_view(
    _call_id: u64,
    content_ptr: *const i32,
    style: i64,
) -> WidgetHandle {
    let content = decode_string_arg(content_ptr);
    finish_custom_widget(blinc_layout::widgets::td(content), style)
}

pub(crate) extern "C" fn blinc_cell_view(
    _call_id: u64,
    children: WidgetHandle,
    style: i64,
) -> WidgetHandle {
    let mut widget = blinc_layout::widgets::cell();
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    finish_custom_widget(widget, style)
}

pub(crate) extern "C" fn blinc_button_view(
    call_id: u64,
    label_ptr: *const i32,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    ensure_theme_state();
    ensure_context_state();
    let label = decode_string_arg(label_ptr);
    // Key by call_id (span-derived) so dup-labelled buttons at distinct
    // call sites hold distinct FSM state.
    let key = dsl_state_key("button", call_id);
    let state = blinc_layout::use_fsm_keyed::<_, blinc_layout::stateful::ButtonState>(
        &key,
        blinc_layout::stateful::ButtonState::Idle,
    );
    let mut widget = blinc_layout::widgets::button(state, label);
    for name in decoded_class_names(class_str) {
        widget = widget.class(&name);
    }
    finish_custom_widget(widget, style)
}

pub(crate) extern "C" fn blinc_checkbox_view(
    call_id: u64,
    label_ptr: *const i32,
    checked: i32,
    style: i64,
) -> WidgetHandle {
    ensure_theme_state();
    ensure_context_state();
    let label = decode_string_arg(label_ptr);
    // Same call_id keying as Button — see comment there.
    let key = dsl_state_key("checkbox", call_id);
    let state = blinc_core::use_state_keyed(&key, || checked != 0);
    let widget = blinc_layout::widgets::checkbox_labeled(&state, label);
    finish_custom_widget(widget, style)
}

pub(crate) extern "C" fn blinc_text_input_view(
    _call_id: u64,
    placeholder_ptr: *const i32,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    ensure_theme_state();
    let placeholder = decode_string_arg(placeholder_ptr);
    let state = blinc_layout::widgets::text_input_state_with_placeholder(placeholder.clone());
    let mut widget = blinc_layout::widgets::text_input(&state).placeholder(placeholder);
    for name in decoded_class_names(class_str) {
        widget = widget.class(&name);
    }
    finish_custom_widget(widget, style)
}

pub(crate) extern "C" fn blinc_text_area_view(
    _call_id: u64,
    placeholder_ptr: *const i32,
    rows: i32,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    ensure_theme_state();
    let placeholder = decode_string_arg(placeholder_ptr);
    let state = blinc_layout::widgets::text_area_state_with_placeholder(placeholder.clone());
    let mut widget = blinc_layout::widgets::text_area(&state)
        .placeholder(placeholder)
        .rows(rows.max(1) as usize);
    for name in decoded_class_names(class_str) {
        widget = widget.class(&name);
    }
    finish_custom_widget(widget, style)
}

pub(crate) extern "C" fn blinc_code_view(
    _call_id: u64,
    content_ptr: *const i32,
    line_numbers: i32,
    style: i64,
) -> WidgetHandle {
    ensure_theme_state();
    let content = decode_string_arg(content_ptr);
    finish_custom_widget(
        blinc_layout::widgets::code(content).line_numbers(line_numbers != 0),
        style,
    )
}

pub(crate) extern "C" fn blinc_pre_view(
    _call_id: u64,
    content_ptr: *const i32,
    style: i64,
) -> WidgetHandle {
    ensure_theme_state();
    let content = decode_string_arg(content_ptr);
    finish_custom_widget(blinc_layout::widgets::pre(content), style)
}

/// `$Blinc$Div$view(children, style, class_str, on_click) -> WidgetHandle`.
/// Consumes the child-list and each child handle exactly once.
pub(crate) extern "C" fn blinc_div_view(
    _call_id: u64,
    children: WidgetHandle,
    style: i64,
    class_str: *const i32,
    on_click_closure: i64,
    overflow_scroll: i32,
) -> WidgetHandle {
    let mut widget = blinc_layout::div::Div::new();
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    // SAFETY: `class_str` is `*const i32` per registered sig.
    for name in decoded_class_names(class_str) {
        widget = widget.class(name);
    }
    // `on_click` closure is a raw `extern "C" fn()` pointer minted by Zyntax's
    // `CreateClosure` → `func_addr`. Signal writes inside route through
    // `__signal_set_i32` → reactive `State::set` → stateful refresh.
    if on_click_closure != 0 {
        type ClosureFn = extern "C" fn();
        let func: ClosureFn = unsafe { std::mem::transmute(on_click_closure) };
        widget = widget.cursor_pointer().on_click(move |_ctx| {
            func();
        });
    }
    if overflow_scroll != 0 {
        widget = widget.overflow_scroll();
    }
    let overlay = unsafe { materialize_overlay(style) };
    leak_custom(Styled::new(widget, overlay))
}

/// `$Blinc$Stack$view(children, style) -> WidgetHandle`.
pub(crate) extern "C" fn blinc_stack_view(
    _call_id: u64,
    children: WidgetHandle,
    style: i64,
) -> WidgetHandle {
    let mut widget = blinc_layout::stack::Stack::new();
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    let overlay = unsafe { materialize_overlay(style) };
    leak_custom(Styled::new(widget, overlay))
}

/// `$Blinc$Image$view(source, style) -> WidgetHandle`.
pub(crate) extern "C" fn blinc_image_view(
    _call_id: u64,
    source_ptr: *const i32,
    style: i64,
) -> WidgetHandle {
    if source_ptr.is_null() {
        tracing::warn!("$Blinc$Image$view called with null source pointer");
        return 0;
    }
    let source = decode_string_arg(source_ptr);
    let widget = blinc_layout::image::Image::new(source);
    let overlay = unsafe { materialize_overlay(style) };
    leak_custom(Styled::new(widget, overlay))
}

/// `$Blinc$Svg$view(source, style, class) -> WidgetHandle`.
pub(crate) extern "C" fn blinc_svg_view(
    _call_id: u64,
    source_ptr: *const i32,
    style: i64,
    class_str: *const i32,
) -> WidgetHandle {
    if source_ptr.is_null() {
        tracing::warn!("$Blinc$Svg$view called with null source pointer");
        return 0;
    }
    let source = decode_string_arg(source_ptr);
    let mut widget = blinc_layout::svg::Svg::new(source);
    for name in decoded_class_names(class_str) {
        widget = widget.class(name);
    }
    let overlay = unsafe { materialize_overlay(style) };
    leak_custom(Styled::new(widget, overlay))
}

/// `$Blinc$Canvas$view(style) -> WidgetHandle`.
pub(crate) extern "C" fn blinc_canvas_view(_call_id: u64, style: i64) -> WidgetHandle {
    let widget = blinc_layout::canvas::Canvas::new();
    let overlay = unsafe { materialize_overlay(style) };
    leak_custom(Styled::new(widget, overlay))
}

/// `$Blinc$RichText$view(markup, style) -> WidgetHandle`.
pub(crate) extern "C" fn blinc_rich_text_view(
    _call_id: u64,
    markup_ptr: *const i32,
    style: i64,
) -> WidgetHandle {
    if markup_ptr.is_null() {
        tracing::warn!("$Blinc$RichText$view called with null markup pointer");
        return 0;
    }
    let markup = decode_string_arg(markup_ptr);
    let widget = blinc_layout::rich_text::RichText::new(markup);
    let overlay = unsafe { materialize_overlay(style) };
    leak_custom(Styled::new(widget, overlay))
}

/// `$Blinc$Motion$view(children, style) -> WidgetHandle`.
pub(crate) extern "C" fn blinc_motion_view(
    _call_id: u64,
    children: WidgetHandle,
    style: i64,
) -> WidgetHandle {
    let mut widget = blinc_layout::motion::motion();
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    let overlay = unsafe { materialize_overlay(style) };
    leak_custom(Styled::new(widget, overlay))
}

/// `$Blinc$Notch$view(children, style) -> WidgetHandle`.
pub(crate) extern "C" fn blinc_notch_view(
    _call_id: u64,
    children: WidgetHandle,
    style: i64,
) -> WidgetHandle {
    let mut widget = blinc_layout::notch::Notch::new();
    for child in materialize_children(children) {
        widget = widget.child_box(child);
    }
    let overlay = unsafe { materialize_overlay(style) };
    leak_custom(Styled::new(widget, overlay))
}

/// `__new_child_list__() -> i64` — mints a fresh `Vec<WidgetHandle>` for a container.
pub(crate) extern "C" fn blinc_new_child_list() -> i64 {
    Box::into_raw(Box::new(Vec::<WidgetHandle>::new())) as i64
}

/// `__push_child__(list, child)` — appends to a list minted by `__new_child_list__`.
///
/// # Safety
///
/// `list` must come from `__new_child_list__` and remain live (reclaimed by the container).
pub(crate) fn push_child_handle(list: i64, child: WidgetHandle) {
    blinc_push_child(list, child);
}

pub(crate) extern "C" fn blinc_push_child(list: i64, child: WidgetHandle) {
    if list == 0 {
        return;
    }
    // SAFETY: keep alloc live for the container extern to reclaim.
    let vec: &mut Vec<WidgetHandle> = unsafe { &mut *(list as *mut Vec<WidgetHandle>) };
    vec.push(child);
}

// Overlay-builder externs for inline visual props (bg, opacity, …). Consumed by
// the container/widget extern, which wraps the widget in `Styled<W>`.

pub(crate) extern "C" fn blinc_new_style_overlay() -> i64 {
    Box::into_raw(Box::new(RenderPropsOverlay::default())) as i64
}

pub(crate) extern "C" fn blinc_set_overlay_bg(ptr: i64, color: i64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.background = Some(blinc_core::layer::Brush::Solid(
        blinc_core::layer::Color::from_hex(color as u32),
    ));
}

pub(crate) extern "C" fn blinc_set_overlay_opacity(ptr: i64, val: f64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.opacity = Some(val as f32);
}

/// Reactive-binding variant of `blinc_set_overlay_opacity`. Records
/// the `SignalId.to_raw()` on the overlay so `apply_to` reads the
/// current signal value each render. The DSL grammar emits this
/// extern when the `opacity = …` named-arg's value is a Variable
/// referring to a DSL-declared f64 signal (see
/// `passes::lower_styling_args_to_overlays`).
///
/// `id_raw` arrives as i64 — same wire convention as the signal-by-id
/// getters/setters (Cranelift's value-map doesn't handle
/// `HirConstant::U64`).
pub(crate) extern "C" fn blinc_set_overlay_opacity_signal(ptr: i64, id_raw: i64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.opacity_signal_id_raw = Some(id_raw as u64);
}

pub(crate) extern "C" fn blinc_set_overlay_corner_radius(ptr: i64, val: f64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.corner_radius = Some(val as f32);
}

pub(crate) extern "C" fn blinc_set_overlay_border_width(ptr: i64, val: f64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.border_width = Some(val as f32);
}

pub(crate) extern "C" fn blinc_set_overlay_border_color(ptr: i64, color: i64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.border_color = Some(blinc_core::layer::Color::from_hex(color as u32));
}

/// Reactive-binding variants of the four remaining overlay setters.
/// Each records a `SignalId.to_raw()` on the overlay so `apply_to`
/// reads the live value from the reactive graph each render. Float
/// props bind to `f64` signals; color props bind to `i32` signals
/// carrying a packed hex value (same convention as
/// `__set_overlay_bg__` / `__set_overlay_border_color__`).
///
/// `id_raw` arrives as i64 — same wire convention as the signal-by-id
/// getters/setters (Cranelift's value-map doesn't handle
/// `HirConstant::U64`).
pub(crate) extern "C" fn blinc_set_overlay_bg_signal(ptr: i64, id_raw: i64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.bg_signal_id_raw = Some(id_raw as u64);
}

pub(crate) extern "C" fn blinc_set_overlay_corner_radius_signal(ptr: i64, id_raw: i64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.corner_radius_signal_id_raw = Some(id_raw as u64);
}

pub(crate) extern "C" fn blinc_set_overlay_border_width_signal(ptr: i64, id_raw: i64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.border_width_signal_id_raw = Some(id_raw as u64);
}

pub(crate) extern "C" fn blinc_set_overlay_border_color_signal(ptr: i64, id_raw: i64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.border_color_signal_id_raw = Some(id_raw as u64);
}

/// Computed-binding variants. Each records a `DerivedId.to_raw()` on
/// the overlay so `apply_to` rehydrates a `Computed<T>` and reads the
/// live value each render. The DSL lowering emits these when the
/// styling arg's value is a `__blinc_computed_<T>__(closure)` call
/// (the desugaring of `computed { … } : T`).
///
/// `id_raw` arrives as i64 — same wire convention as the signal
/// variants (Cranelift's value-map doesn't handle `HirConstant::U64`).
pub(crate) extern "C" fn blinc_set_overlay_opacity_computed(ptr: i64, id_raw: i64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.opacity_computed_id_raw = Some(id_raw as u64);
}

pub(crate) extern "C" fn blinc_set_overlay_bg_computed(ptr: i64, id_raw: i64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.bg_computed_id_raw = Some(id_raw as u64);
}

pub(crate) extern "C" fn blinc_set_overlay_corner_radius_computed(ptr: i64, id_raw: i64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.corner_radius_computed_id_raw = Some(id_raw as u64);
}

pub(crate) extern "C" fn blinc_set_overlay_border_width_computed(ptr: i64, id_raw: i64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.border_width_computed_id_raw = Some(id_raw as u64);
}

pub(crate) extern "C" fn blinc_set_overlay_border_color_computed(ptr: i64, id_raw: i64) {
    if ptr == 0 {
        return;
    }
    let overlay: &mut RenderPropsOverlay = unsafe { &mut *(ptr as *mut RenderPropsOverlay) };
    overlay.border_color_computed_id_raw = Some(id_raw as u64);
}

/// Reclaim a `Box<RenderPropsOverlay>` from `__new_style_overlay__`. Default for null.
///
/// # Safety
///
/// `ptr` must come from `__new_style_overlay__`.
pub unsafe fn materialize_overlay(ptr: i64) -> RenderPropsOverlay {
    if ptr == 0 {
        return RenderPropsOverlay::default();
    }
    *unsafe { Box::from_raw(ptr as *mut RenderPropsOverlay) }
}
