//! Scenic compositor integration
//!
//! Provides types and helpers for integrating with Fuchsia's Scenic
//! graphics compositor via FIDL.
//!
//! # Architecture
//!
//! Scenic is Fuchsia's graphics composition system. Blinc integrates via:
//!
//! - **ViewProvider**: Protocol to provide views to the system
//! - **Flatland**: Modern 2D composition API (preferred over Scene Graph)
//! - **View/ViewRef**: Unique identifiers for the application's view
//!
//! # References
//!
//! - [Scenic Overview](https://fuchsia.dev/fuchsia-src/concepts/graphics/scenic)
//! - [Flatland](https://fuchsia.dev/fuchsia-src/development/graphics/flatland)

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// View lifecycle state
///
/// Represents the current state of the Scenic view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewState {
    /// View is being created, not yet attached
    Creating,
    /// View is attached and visible
    Attached,
    /// View is detached (app backgrounded)
    Detached,
    /// View is being destroyed
    Destroying,
}

impl Default for ViewState {
    fn default() -> Self {
        Self::Creating
    }
}

/// View focus state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusState {
    /// View does not have focus
    Unfocused,
    /// View has focus
    Focused,
}

impl Default for FocusState {
    fn default() -> Self {
        Self::Unfocused
    }
}

/// Scenic view properties received from the parent
///
/// Contains the layout bounds and other properties assigned by the parent.
#[derive(Clone, Debug)]
pub struct ViewProperties {
    /// Logical width of the view in DIP
    pub width: f32,
    /// Logical height of the view in DIP
    pub height: f32,
    /// Device pixel ratio (scale factor)
    pub scale_factor: f64,
    /// Whether the view is focused
    pub focused: bool,
    /// Insets as (top, right, bottom, left)
    pub insets: (f32, f32, f32, f32),
    /// Inset from the left edge (for safe areas)
    pub inset_left: f32,
    /// Inset from the top edge (for safe areas)
    pub inset_top: f32,
    /// Inset from the right edge (for safe areas)
    pub inset_right: f32,
    /// Inset from the bottom edge (for safe areas)
    pub inset_bottom: f32,
}

impl Default for ViewProperties {
    fn default() -> Self {
        Self {
            width: 1920.0,
            height: 1080.0,
            scale_factor: 1.0,
            focused: false,
            insets: (0.0, 0.0, 0.0, 0.0),
            inset_left: 0.0,
            inset_top: 0.0,
            inset_right: 0.0,
            inset_bottom: 0.0,
        }
    }
}

impl ViewProperties {
    /// Create view properties with given dimensions
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            ..Default::default()
        }
    }

    /// Create view properties with dimensions and scale factor
    pub fn with_scale(width: f32, height: f32, scale_factor: f64) -> Self {
        Self {
            width,
            height,
            scale_factor,
            ..Default::default()
        }
    }

    /// Get physical width (logical * scale_factor)
    pub fn physical_width(&self) -> u32 {
        (self.width * self.scale_factor as f32) as u32
    }

    /// Get physical height (logical * scale_factor)
    pub fn physical_height(&self) -> u32 {
        (self.height * self.scale_factor as f32) as u32
    }

    /// Get the usable (safe) width
    pub fn safe_width(&self) -> f32 {
        self.width - self.inset_left - self.inset_right
    }

    /// Get the usable (safe) height
    pub fn safe_height(&self) -> f32 {
        self.height - self.inset_top - self.inset_bottom
    }

    /// Set insets from tuple
    pub fn set_insets(&mut self, insets: (f32, f32, f32, f32)) {
        self.insets = insets;
        self.inset_top = insets.0;
        self.inset_right = insets.1;
        self.inset_bottom = insets.2;
        self.inset_left = insets.3;
    }
}

/// Display information from fuchsia.ui.display
#[derive(Clone, Debug)]
pub struct DisplayInfo {
    /// Physical width in pixels
    pub physical_width: u32,
    /// Physical height in pixels
    pub physical_height: u32,
    /// Display scale factor (DPI ratio)
    pub scale_factor: f64,
    /// Refresh rate in Hz
    pub refresh_rate_hz: f32,
}

impl Default for DisplayInfo {
    fn default() -> Self {
        Self {
            physical_width: 1920,
            physical_height: 1080,
            scale_factor: 1.0,
            refresh_rate_hz: 60.0,
        }
    }
}

impl DisplayInfo {
    /// Get logical width (physical / scale_factor)
    pub fn logical_width(&self) -> f32 {
        self.physical_width as f32 / self.scale_factor as f32
    }

    /// Get logical height (physical / scale_factor)
    pub fn logical_height(&self) -> f32 {
        self.physical_height as f32 / self.scale_factor as f32
    }

    /// Get frame duration in seconds
    pub fn frame_duration_secs(&self) -> f32 {
        1.0 / self.refresh_rate_hz
    }
}

/// Scenic view handle wrapper
///
/// This is a placeholder for the actual View/ViewRef from Scenic.
/// On Fuchsia, this would wrap the FIDL ViewRef and ViewToken.
#[derive(Clone)]
pub struct ScenicView {
    /// View state
    state: Arc<AtomicU32>,
    /// Focus state
    focus: Arc<AtomicBool>,
    /// View properties
    properties: Arc<std::sync::RwLock<ViewProperties>>,
}

impl ScenicView {
    /// Create a new Scenic view wrapper
    pub fn new() -> Self {
        Self {
            state: Arc::new(AtomicU32::new(ViewState::Creating as u32)),
            focus: Arc::new(AtomicBool::new(false)),
            properties: Arc::new(std::sync::RwLock::new(ViewProperties::default())),
        }
    }

    /// Get the current view state
    pub fn state(&self) -> ViewState {
        match self.state.load(Ordering::SeqCst) {
            0 => ViewState::Creating,
            1 => ViewState::Attached,
            2 => ViewState::Detached,
            3 => ViewState::Destroying,
            _ => ViewState::Creating,
        }
    }

    /// Set the view state
    pub fn set_state(&self, state: ViewState) {
        self.state.store(state as u32, Ordering::SeqCst);
    }

    /// Check if the view is focused
    pub fn is_focused(&self) -> bool {
        self.focus.load(Ordering::SeqCst)
    }

    /// Set the focus state
    pub fn set_focused(&self, focused: bool) {
        self.focus.store(focused, Ordering::SeqCst);
    }

    /// Get the current view properties
    pub fn properties(&self) -> ViewProperties {
        self.properties.read().unwrap().clone()
    }

    /// Update the view properties
    pub fn update_properties(&self, props: ViewProperties) {
        *self.properties.write().unwrap() = props;
    }

    /// Get the view dimensions
    pub fn size(&self) -> (f32, f32) {
        let props = self.properties.read().unwrap();
        (props.width, props.height)
    }
}

impl Default for ScenicView {
    fn default() -> Self {
        Self::new()
    }
}

/// Frame scheduling information from Scenic
#[derive(Clone, Debug, Default)]
pub struct FrameInfo {
    /// Presentation time in nanoseconds
    pub presentation_time_ns: u64,
    /// Latch point - deadline for submitting frame
    pub latch_point_ns: u64,
    /// Whether we should present this frame
    pub should_present: bool,
}

impl FrameInfo {
    /// Create frame info with current time
    pub fn now() -> Self {
        Self {
            presentation_time_ns: 0, // Would use zx::Time::get_monotonic() on Fuchsia
            latch_point_ns: 0,
            should_present: true,
        }
    }

    /// Time until presentation in milliseconds
    pub fn time_until_present_ms(&self) -> f64 {
        // Placeholder - would calculate from actual timestamps
        16.67
    }
}

/// Placeholder for ViewProvider service implementation
///
/// On Fuchsia, this would implement the fuchsia.ui.app.ViewProvider protocol.
pub struct ViewProviderService {
    view: ScenicView,
}

impl ViewProviderService {
    /// Create a new ViewProvider service
    pub fn new() -> Self {
        Self {
            view: ScenicView::new(),
        }
    }

    /// Get a reference to the view
    pub fn view(&self) -> &ScenicView {
        &self.view
    }

    /// Called when the view is created
    #[cfg(target_os = "fuchsia")]
    pub fn on_create_view(&self) {
        self.view.set_state(ViewState::Attached);
        tracing::info!("Scenic view created");
    }

    /// Called when the view is destroyed
    #[cfg(target_os = "fuchsia")]
    pub fn on_destroy_view(&self) {
        self.view.set_state(ViewState::Destroying);
        tracing::info!("Scenic view destroyed");
    }
}

impl Default for ViewProviderService {
    fn default() -> Self {
        Self::new()
    }
}

/// Component manifest helpers
pub mod manifest {
    //! Helper utilities for Fuchsia component manifests (.cml)
    //!
    //! # Example Component Manifest
    //!
    //! ```json5
    //! // my_app.cml
    //! {
    //!     include: [
    //!         "syslog/client.shard.cml",
    //!     ],
    //!     program: {
    //!         runner: "elf",
    //!         binary: "bin/my_app",
    //!     },
    //!     capabilities: [
    //!         { protocol: "fuchsia.ui.app.ViewProvider" },
    //!     ],
    //!     use: [
    //!         { protocol: "fuchsia.ui.composition.Flatland" },
    //!         { protocol: "fuchsia.ui.input3.Keyboard" },
    //!         { protocol: "fuchsia.ui.pointer.TouchSource" },
    //!         { protocol: "fuchsia.ui.pointer.MouseSource" },
    //!         { protocol: "fuchsia.vulkan.loader.Loader" },
    //!     ],
    //!     expose: [
    //!         {
    //!             protocol: "fuchsia.ui.app.ViewProvider",
    //!             from: "self",
    //!         },
    //!     ],
    //! }
    //! ```

    /// Required capabilities for a Blinc app on Fuchsia
    pub const REQUIRED_CAPABILITIES: &[&str] = &[
        "fuchsia.ui.composition.Flatland",
        "fuchsia.ui.input3.Keyboard",
        "fuchsia.ui.pointer.TouchSource",
        "fuchsia.vulkan.loader.Loader",
    ];

    /// Optional capabilities that enhance functionality
    pub const OPTIONAL_CAPABILITIES: &[&str] = &[
        "fuchsia.ui.pointer.MouseSource",
        "fuchsia.media.AudioRenderer",
        "fuchsia.accessibility.semantics.SemanticsManager",
    ];

    /// Generate a basic component manifest for a Blinc app
    pub fn generate_manifest(binary_name: &str) -> String {
        ManifestBuilder::new(binary_name).build()
    }

    /// Builder for Fuchsia component manifests
    pub struct ManifestBuilder {
        /// Binary name
        binary_name: String,
        /// Include touch input
        touch: bool,
        /// Include mouse input
        mouse: bool,
        /// Include keyboard input
        keyboard: bool,
        /// Include audio
        audio: bool,
        /// Include accessibility
        accessibility: bool,
        /// Additional protocols to use
        extra_protocols: Vec<String>,
    }

    impl ManifestBuilder {
        /// Create a new manifest builder
        pub fn new(binary_name: &str) -> Self {
            Self {
                binary_name: binary_name.to_string(),
                touch: true,
                mouse: true,
                keyboard: true,
                audio: false,
                accessibility: false,
                extra_protocols: vec![],
            }
        }

        /// Include touch input (default: true)
        pub fn with_touch(mut self, enabled: bool) -> Self {
            self.touch = enabled;
            self
        }

        /// Include mouse input (default: true)
        pub fn with_mouse(mut self, enabled: bool) -> Self {
            self.mouse = enabled;
            self
        }

        /// Include keyboard input (default: true)
        pub fn with_keyboard(mut self, enabled: bool) -> Self {
            self.keyboard = enabled;
            self
        }

        /// Include audio playback
        pub fn with_audio(mut self, enabled: bool) -> Self {
            self.audio = enabled;
            self
        }

        /// Include accessibility support
        pub fn with_accessibility(mut self, enabled: bool) -> Self {
            self.accessibility = enabled;
            self
        }

        /// Add a custom protocol
        pub fn with_protocol(mut self, protocol: &str) -> Self {
            self.extra_protocols.push(protocol.to_string());
            self
        }

        /// Build the manifest string
        pub fn build(&self) -> String {
            let mut use_protocols = vec![
                "fuchsia.ui.composition.Flatland".to_string(),
                "fuchsia.ui.composition.Allocator".to_string(),
                "fuchsia.vulkan.loader.Loader".to_string(),
                "fuchsia.sysmem2.Allocator".to_string(),
            ];

            if self.touch {
                use_protocols.push("fuchsia.ui.pointer.TouchSource".to_string());
            }
            if self.mouse {
                use_protocols.push("fuchsia.ui.pointer.MouseSource".to_string());
            }
            if self.keyboard {
                use_protocols.push("fuchsia.ui.input3.Keyboard".to_string());
            }
            if self.audio {
                use_protocols.push("fuchsia.media.AudioRenderer".to_string());
            }
            if self.accessibility {
                use_protocols.push("fuchsia.accessibility.semantics.SemanticsManager".to_string());
            }

            use_protocols.extend(self.extra_protocols.iter().cloned());

            let use_lines: String = use_protocols
                .iter()
                .map(|p| format!("        {{ protocol: \"{}\" }},", p))
                .collect::<Vec<_>>()
                .join("\n");

            format!(
                r#"{{
    include: [
        "syslog/client.shard.cml",
    ],
    program: {{
        runner: "elf",
        binary: "bin/{}",
    }},
    capabilities: [
        {{ protocol: "fuchsia.ui.app.ViewProvider" }},
    ],
    use: [
{}
    ],
    expose: [
        {{
            protocol: "fuchsia.ui.app.ViewProvider",
            from: "self",
        }},
    ],
}}"#,
                self.binary_name, use_lines
            )
        }
    }

    impl Default for ManifestBuilder {
        fn default() -> Self {
            Self::new("blinc_app")
        }
    }

    /// Validate a component manifest
    ///
    /// Checks that required capabilities are present.
    pub fn validate_manifest(manifest: &str) -> Result<(), &'static str> {
        // Check for ViewProvider capability
        if !manifest.contains("fuchsia.ui.app.ViewProvider") {
            return Err("Missing required ViewProvider capability");
        }

        // Check for Flatland
        if !manifest.contains("fuchsia.ui.composition.Flatland") {
            return Err("Missing required Flatland protocol");
        }

        // Check for Vulkan
        if !manifest.contains("fuchsia.vulkan.loader.Loader") {
            return Err("Missing required Vulkan loader protocol");
        }

        Ok(())
    }
}
