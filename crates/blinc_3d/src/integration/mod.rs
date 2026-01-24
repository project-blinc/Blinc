//! Blinc framework integration
//!
//! Provides integration with Blinc's animation, FSM, and canvas systems.

mod animation;
mod canvas;
mod color;
mod fsm;

pub use animation::{AnimatedQuat, AnimatedTransform, AnimatedVec3};
pub use canvas::{
    render_scene, render_sdf_scene, render_sdf_scene_with_config, CanvasBounds, CanvasBoundsExt,
    RenderConfig, SdfRenderConfig,
};
pub use color::AnimatedColor;
pub use fsm::{
    create_game_fsm, game_events, game_states, GameEvent, GameState, GameStateMachine,
};
