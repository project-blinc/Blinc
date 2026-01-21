//! Game state machine integration
//!
//! Provides FSM patterns for game state management using Blinc's FSM system.

use blinc_core::{FsmRuntime, StateId, Transition};

/// Game state identifiers
pub mod game_states {
    use blinc_core::StateId;

    /// Loading state
    pub const LOADING: StateId = 0;
    /// Main menu state
    pub const MAIN_MENU: StateId = 1;
    /// Playing state
    pub const PLAYING: StateId = 2;
    /// Paused state
    pub const PAUSED: StateId = 3;
    /// Game over state
    pub const GAME_OVER: StateId = 4;
    /// Settings state
    pub const SETTINGS: StateId = 5;
    /// Cutscene state
    pub const CUTSCENE: StateId = 6;
}

/// Game event identifiers
pub mod game_events {
    /// Loading complete event
    pub const LOAD_COMPLETE: u32 = 0;
    /// Start game event
    pub const START_GAME: u32 = 1;
    /// Pause event
    pub const PAUSE: u32 = 2;
    /// Resume event
    pub const RESUME: u32 = 3;
    /// Player died event
    pub const PLAYER_DIED: u32 = 4;
    /// Return to menu event
    pub const RETURN_TO_MENU: u32 = 5;
    /// Open settings event
    pub const OPEN_SETTINGS: u32 = 6;
    /// Close settings event
    pub const CLOSE_SETTINGS: u32 = 7;
    /// Start cutscene event
    pub const START_CUTSCENE: u32 = 8;
    /// End cutscene event
    pub const END_CUTSCENE: u32 = 9;
    /// Restart event
    pub const RESTART: u32 = 10;
}

/// Game state enum for pattern matching
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameState {
    /// Loading assets and initializing
    Loading,
    /// Main menu screen
    MainMenu,
    /// Active gameplay
    Playing,
    /// Game paused
    Paused,
    /// Game over screen
    GameOver,
    /// Settings menu
    Settings,
    /// Cutscene playing
    Cutscene,
}

impl GameState {
    /// Convert from StateId
    pub fn from_state_id(id: StateId) -> Option<Self> {
        match id {
            game_states::LOADING => Some(GameState::Loading),
            game_states::MAIN_MENU => Some(GameState::MainMenu),
            game_states::PLAYING => Some(GameState::Playing),
            game_states::PAUSED => Some(GameState::Paused),
            game_states::GAME_OVER => Some(GameState::GameOver),
            game_states::SETTINGS => Some(GameState::Settings),
            game_states::CUTSCENE => Some(GameState::Cutscene),
            _ => None,
        }
    }

    /// Convert to StateId
    pub fn to_state_id(self) -> StateId {
        match self {
            GameState::Loading => game_states::LOADING,
            GameState::MainMenu => game_states::MAIN_MENU,
            GameState::Playing => game_states::PLAYING,
            GameState::Paused => game_states::PAUSED,
            GameState::GameOver => game_states::GAME_OVER,
            GameState::Settings => game_states::SETTINGS,
            GameState::Cutscene => game_states::CUTSCENE,
        }
    }

    /// Check if game logic should run
    pub fn is_gameplay_active(self) -> bool {
        matches!(self, GameState::Playing)
    }

    /// Check if UI should be visible
    pub fn is_ui_visible(self) -> bool {
        matches!(
            self,
            GameState::MainMenu
                | GameState::Paused
                | GameState::GameOver
                | GameState::Settings
        )
    }

    /// Check if rendering should happen
    pub fn should_render(self) -> bool {
        !matches!(self, GameState::Loading)
    }
}

/// Game event enum
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameEvent {
    /// Loading complete
    LoadComplete,
    /// Start the game
    StartGame,
    /// Pause the game
    Pause,
    /// Resume from pause
    Resume,
    /// Player died
    PlayerDied,
    /// Return to main menu
    ReturnToMenu,
    /// Open settings
    OpenSettings,
    /// Close settings
    CloseSettings,
    /// Start cutscene
    StartCutscene,
    /// End cutscene
    EndCutscene,
    /// Restart game
    Restart,
}

impl GameEvent {
    /// Convert to event ID
    pub fn to_event_id(self) -> u32 {
        match self {
            GameEvent::LoadComplete => game_events::LOAD_COMPLETE,
            GameEvent::StartGame => game_events::START_GAME,
            GameEvent::Pause => game_events::PAUSE,
            GameEvent::Resume => game_events::RESUME,
            GameEvent::PlayerDied => game_events::PLAYER_DIED,
            GameEvent::ReturnToMenu => game_events::RETURN_TO_MENU,
            GameEvent::OpenSettings => game_events::OPEN_SETTINGS,
            GameEvent::CloseSettings => game_events::CLOSE_SETTINGS,
            GameEvent::StartCutscene => game_events::START_CUTSCENE,
            GameEvent::EndCutscene => game_events::END_CUTSCENE,
            GameEvent::Restart => game_events::RESTART,
        }
    }
}

/// Game state machine wrapper
pub struct GameStateMachine {
    /// FSM instance ID
    pub fsm_id: blinc_core::FsmId,
    /// Previous state (for transitions)
    previous_state: Option<GameState>,
}

impl GameStateMachine {
    /// Create a new game state machine
    fn new(fsm_id: blinc_core::FsmId) -> Self {
        Self {
            fsm_id,
            previous_state: None,
        }
    }

    /// Get current state
    pub fn current_state(&self, runtime: &FsmRuntime) -> Option<GameState> {
        runtime
            .current_state(self.fsm_id)
            .and_then(GameState::from_state_id)
    }

    /// Send an event to the FSM
    pub fn send(&mut self, runtime: &mut FsmRuntime, event: GameEvent) {
        // Store previous state
        self.previous_state = self.current_state(runtime);
        // Send event
        runtime.send(self.fsm_id, event.to_event_id());
    }

    /// Check if state changed
    pub fn state_changed(&self, runtime: &FsmRuntime) -> bool {
        self.previous_state != self.current_state(runtime)
    }

    /// Get previous state (useful during transitions)
    pub fn previous_state(&self) -> Option<GameState> {
        self.previous_state
    }

    /// Quick check methods
    pub fn is_loading(&self, runtime: &FsmRuntime) -> bool {
        self.current_state(runtime) == Some(GameState::Loading)
    }

    pub fn is_playing(&self, runtime: &FsmRuntime) -> bool {
        self.current_state(runtime) == Some(GameState::Playing)
    }

    pub fn is_paused(&self, runtime: &FsmRuntime) -> bool {
        self.current_state(runtime) == Some(GameState::Paused)
    }

    pub fn is_game_over(&self, runtime: &FsmRuntime) -> bool {
        self.current_state(runtime) == Some(GameState::GameOver)
    }

    pub fn is_in_menu(&self, runtime: &FsmRuntime) -> bool {
        matches!(
            self.current_state(runtime),
            Some(GameState::MainMenu) | Some(GameState::Settings)
        )
    }
}

/// Create the standard game FSM
///
/// State machine structure:
/// ```text
/// LOADING --[LOAD_COMPLETE]--> MAIN_MENU
/// MAIN_MENU --[START_GAME]--> PLAYING
/// MAIN_MENU --[OPEN_SETTINGS]--> SETTINGS
/// SETTINGS --[CLOSE_SETTINGS]--> MAIN_MENU
/// PLAYING --[PAUSE]--> PAUSED
/// PLAYING --[PLAYER_DIED]--> GAME_OVER
/// PLAYING --[START_CUTSCENE]--> CUTSCENE
/// PAUSED --[RESUME]--> PLAYING
/// PAUSED --[RETURN_TO_MENU]--> MAIN_MENU
/// GAME_OVER --[RESTART]--> PLAYING
/// GAME_OVER --[RETURN_TO_MENU]--> MAIN_MENU
/// CUTSCENE --[END_CUTSCENE]--> PLAYING
/// ```
pub fn create_game_fsm(runtime: &mut FsmRuntime) -> GameStateMachine {
    // Define transitions
    let transitions = vec![
        // From Loading
        Transition::new(
            game_states::LOADING,
            game_events::LOAD_COMPLETE,
            game_states::MAIN_MENU,
        ),
        // From Main Menu
        Transition::new(
            game_states::MAIN_MENU,
            game_events::START_GAME,
            game_states::PLAYING,
        ),
        Transition::new(
            game_states::MAIN_MENU,
            game_events::OPEN_SETTINGS,
            game_states::SETTINGS,
        ),
        // From Settings
        Transition::new(
            game_states::SETTINGS,
            game_events::CLOSE_SETTINGS,
            game_states::MAIN_MENU,
        ),
        // From Playing
        Transition::new(
            game_states::PLAYING,
            game_events::PAUSE,
            game_states::PAUSED,
        ),
        Transition::new(
            game_states::PLAYING,
            game_events::PLAYER_DIED,
            game_states::GAME_OVER,
        ),
        Transition::new(
            game_states::PLAYING,
            game_events::START_CUTSCENE,
            game_states::CUTSCENE,
        ),
        // From Paused
        Transition::new(
            game_states::PAUSED,
            game_events::RESUME,
            game_states::PLAYING,
        ),
        Transition::new(
            game_states::PAUSED,
            game_events::RETURN_TO_MENU,
            game_states::MAIN_MENU,
        ),
        // From Game Over
        Transition::new(
            game_states::GAME_OVER,
            game_events::RESTART,
            game_states::PLAYING,
        ),
        Transition::new(
            game_states::GAME_OVER,
            game_events::RETURN_TO_MENU,
            game_states::MAIN_MENU,
        ),
        // From Cutscene
        Transition::new(
            game_states::CUTSCENE,
            game_events::END_CUTSCENE,
            game_states::PLAYING,
        ),
    ];

    // Create FSM
    let fsm_id = runtime.create_simple(game_states::LOADING, transitions);

    GameStateMachine::new(fsm_id)
}

/// Level state machine for managing level progression
pub mod level_states {
    use blinc_core::StateId;

    pub const INTRO: StateId = 100;
    pub const ACTIVE: StateId = 101;
    pub const BOSS_FIGHT: StateId = 102;
    pub const VICTORY: StateId = 103;
    pub const FAILED: StateId = 104;
}

/// Level events
pub mod level_events {
    pub const INTRO_COMPLETE: u32 = 100;
    pub const BOSS_TRIGGERED: u32 = 101;
    pub const BOSS_DEFEATED: u32 = 102;
    pub const LEVEL_COMPLETE: u32 = 103;
    pub const LEVEL_FAILED: u32 = 104;
    pub const RESTART_LEVEL: u32 = 105;
}
