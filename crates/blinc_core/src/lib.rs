//! Blinc Core Runtime
//!
//! This crate provides the foundational primitives for the Blinc UI framework:
//!
//! - **Reactive Signals**: Fine-grained reactivity without VDOM overhead
//! - **State Machines**: Harel statecharts for widget interaction states
//! - **Event Dispatch**: Unified event handling across platforms
//! - **Layer Model**: Unified visual content representation (2D, 3D, composition)
//! - **Draw Context**: Unified rendering API for 2D/3D content
//!
//! # Example
//!
//! ```rust
//! use blinc_core::reactive::ReactiveGraph;
//!
//! let mut graph = ReactiveGraph::new();
//!
//! // Create a signal
//! let count = graph.create_signal(0i32);
//!
//! // Create a derived value
//! let doubled = graph.create_derived(move |g| {
//!     g.get(count).unwrap_or(0) * 2
//! });
//!
//! // Create an effect
//! let _effect = graph.create_effect(move |g| {
//!     println!("Count is now: {:?}", g.get(count));
//! });
//!
//! // Update the signal
//! graph.set(count, 5);
//! assert_eq!(graph.get_derived(doubled), Some(10));
//! ```

pub mod context;
pub mod context_state;
pub mod draw;
pub mod events;
pub mod fsm;
pub mod layer;
pub mod reactive;
pub mod runtime;
pub mod store;
pub mod value;

pub use draw::{
    BlurQuality, DrawCommand, DrawContext, DrawContextExt, FontWeight, ImageId, ImageOptions,
    LayerConfig, LayerEffect, LineCap, LineJoin, MaterialId, MeshId, MeshInstance, Path,
    PathCommand, RecordingContext, SdfBuilder, ShapeId, Stroke, TextAlign, TextBaseline, TextStyle,
    Transform,
};
pub use events::{Event, EventData, EventDispatcher, EventType, KeyCode, Modifiers};
pub use fsm::{FsmId, FsmRuntime, StateId, StateMachine, Transition};
pub use layer::{
    Affine2D, BillboardFacing, BlendMode, BlurStyle, Brush, CachePolicy, Camera, CameraProjection,
    Canvas2DCommand, Canvas2DCommands, ClipShape, Color, CornerRadius, Environment, GlassStyle,
    Gradient, GradientSpace, GradientSpread, GradientStop, ImageBrush, ImageFit, ImagePosition,
    Layer, LayerId, LayerIdGenerator, LayerProperties, Light, Mat4, Point, PointerEvents,
    PostEffect, Rect, Scene3DCommand, Scene3DCommands, SceneGraph, Shadow, Size, TextureFormat,
    UiNode, Vec2, Vec3,
};
pub use reactive::{
    Derived, DerivedId, DirtyFlag, Effect, EffectId, ReactiveGraph, SharedReactiveGraph, Signal,
    SignalId, State, StatefulDepsCallback,
};
pub use runtime::BlincReactiveRuntime;
pub use value::{
    AnimationAccess, BoxedValue, DynFloat, DynValue, ReactiveAccess, SpringValue, Static, Value,
    ValueContext,
};

// Re-export context types at crate level for convenience
pub use context::{BlincContext, BlincContextExt};
pub use context_state::{
    query, query_motion, request_rebuild, use_signal_keyed, use_state_keyed, AnyElementRegistry,
    BlincContextState, Bounds, BoundsCallback, FocusCallback, HookState, MotionAnimationState,
    MotionStateCallback, QueryCallback, RecordedEventAny, RecorderEventCallback,
    RecorderSnapshotCallback, ScrollCallback, SharedHookState, StateKey, TreeSnapshotAny,
};

// Re-export store types
pub use store::{
    clear_all_stores, create_store, create_store_with, get_store_state, kv_delete, kv_get, kv_set,
    remove_store, set_store_state, update_store_state, KVStore, Store, SubscriptionHandle,
};
