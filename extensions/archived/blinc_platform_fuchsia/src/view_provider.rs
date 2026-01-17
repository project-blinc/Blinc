//! ViewProvider service implementation
//!
//! Implements `fuchsia.ui.app.ViewProvider` FIDL protocol to receive View creation
//! requests from the Fuchsia system.
//!
//! # Architecture
//!
//! When a Fuchsia component is launched as a GUI app, the system:
//! 1. Connects to our ViewProvider capability
//! 2. Calls CreateView2 with tokens for View creation
//! 3. We create our View in Flatland and start rendering
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Fuchsia System                            │
//! │  ┌─────────────────────────────────────────────────────────┐│
//! │  │                  Session Manager                        ││
//! │  │  - Launches components                                  ││
//! │  │  - Requests ViewProvider.CreateView2                    ││
//! │  └─────────────────────────────────────────────────────────┘│
//! └──────────────────────────┬──────────────────────────────────┘
//!                            │ CreateView2(tokens)
//!                            ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Blinc App                                 │
//! │  ┌─────────────────────────────────────────────────────────┐│
//! │  │                  ViewProvider                           ││
//! │  │  - Receives ViewCreationToken                           ││
//! │  │  - Creates View in Flatland                             ││
//! │  │  - Watches ParentViewportWatcher for size               ││
//! │  └─────────────────────────────────────────────────────────┘│
//! └─────────────────────────────────────────────────────────────┘
//! ```

use std::sync::Arc;

#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use fidl::endpoints::{ClientEnd, ServerEnd};
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use fidl_fuchsia_ui_app::{ViewProviderRequest, ViewProviderRequestStream};
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use fidl_fuchsia_ui_composition::{
    LayoutInfo as FidlLayoutInfo, ParentViewportWatcherProxy,
};
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use fidl_fuchsia_ui_views::{
    ViewCreationToken as FidlViewCreationToken, ViewIdentityOnCreation, ViewRef as FidlViewRef,
    ViewRefFocusedProxy,
};
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use futures::TryStreamExt;

use crate::flatland::{FlatlandError, FlatlandSession};
use crate::scenic::ViewProperties;

/// View creation token from the system
///
/// On Fuchsia, this wraps `fuchsia.ui.views.ViewCreationToken`.
#[derive(Debug)]
pub struct ViewCreationToken {
    /// The raw token value (zx::Channel on Fuchsia)
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub value: u64,
    /// The actual FIDL token on Fuchsia
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub inner: Option<FidlViewCreationToken>,
}

impl ViewCreationToken {
    /// Create a new view creation token (non-Fuchsia placeholder)
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn new(value: u64) -> Self {
        Self { value }
    }

    /// Create from FIDL token (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn from_fidl(token: FidlViewCreationToken) -> Self {
        Self { inner: Some(token) }
    }

    /// Create an invalid token (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn new_invalid() -> Self {
        Self { inner: None }
    }

    /// Check if the token is valid
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn is_valid(&self) -> bool {
        self.value != 0
    }

    /// Check if the token is valid (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn is_valid(&self) -> bool {
        self.inner.is_some()
    }

    /// Take the inner FIDL token (Fuchsia only)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn take_fidl(&mut self) -> Option<FidlViewCreationToken> {
        self.inner.take()
    }
}

/// ViewRef for identifying our view
///
/// On Fuchsia, this wraps `fuchsia.ui.views.ViewRef`.
#[derive(Debug)]
pub struct ViewRef {
    /// The raw reference value (non-Fuchsia)
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub value: u64,
    /// The actual FIDL ViewRef on Fuchsia
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub inner: Option<FidlViewRef>,
}

impl Clone for ViewRef {
    fn clone(&self) -> Self {
        #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
        {
            Self { value: self.value }
        }
        #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
        {
            // ViewRef contains a handle that can't be cloned directly
            // We create an empty clone - the original retains ownership
            Self { inner: None }
        }
    }
}

impl ViewRef {
    /// Create a new view ref (non-Fuchsia)
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn new(value: u64) -> Self {
        Self { value }
    }

    /// Create from FIDL ViewRef (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn from_fidl(view_ref: FidlViewRef) -> Self {
        Self { inner: Some(view_ref) }
    }

    /// Check if this ViewRef is valid
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn is_valid(&self) -> bool {
        self.inner.is_some()
    }

    /// Check if this ViewRef is valid (non-Fuchsia)
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn is_valid(&self) -> bool {
        self.value != 0
    }
}

/// Arguments for CreateView2
#[derive(Debug)]
pub struct CreateView2Args {
    /// Token to create the view
    pub view_creation_token: ViewCreationToken,
    /// Our view's identity
    pub view_ref: Option<ViewRef>,
    /// View identity for accessibility
    pub view_identity: Option<ViewIdentity>,
}

/// View identity for accessibility
#[derive(Debug)]
pub struct ViewIdentity {
    /// View ref for this identity
    pub view_ref: ViewRef,
}

/// Events from the view system
#[derive(Debug, Clone)]
pub enum ViewEvent {
    /// View was created
    Created {
        /// Initial view properties
        properties: ViewProperties,
    },
    /// Layout changed (size, insets, scale)
    LayoutChanged {
        /// New logical width
        width: f32,
        /// New logical height
        height: f32,
        /// Device pixel ratio
        scale_factor: f64,
        /// Insets (top, right, bottom, left)
        insets: (f32, f32, f32, f32),
    },
    /// Focus state changed
    FocusChanged(bool),
    /// View was destroyed
    Destroyed,
}

/// Errors from ViewProvider
#[derive(Debug, Clone)]
pub enum ViewProviderError {
    /// Already have a view
    ViewAlreadyExists,
    /// Invalid token
    InvalidToken,
    /// Flatland error
    FlatlandError(String),
    /// Channel closed
    ChannelClosed,
}

impl std::fmt::Display for ViewProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ViewAlreadyExists => write!(f, "View already exists"),
            Self::InvalidToken => write!(f, "Invalid view creation token"),
            Self::FlatlandError(msg) => write!(f, "Flatland error: {}", msg),
            Self::ChannelClosed => write!(f, "Channel closed"),
        }
    }
}

impl std::error::Error for ViewProviderError {}

impl From<FlatlandError> for ViewProviderError {
    fn from(e: FlatlandError) -> Self {
        ViewProviderError::FlatlandError(e.to_string())
    }
}

/// ViewProvider service state
///
/// Manages the View lifecycle and forwards events to the app.
pub struct ViewProvider {
    /// Flatland session for rendering
    flatland: Option<Arc<std::sync::RwLock<FlatlandSession>>>,
    /// Current view ref
    view_ref: Option<ViewRef>,
    /// Whether we have an active view
    has_view: bool,
    /// Current view properties
    current_properties: Option<ViewProperties>,
}

impl ViewProvider {
    /// Create a new ViewProvider
    pub fn new() -> Self {
        Self {
            flatland: None,
            view_ref: None,
            has_view: false,
            current_properties: None,
        }
    }

    /// Create with an existing Flatland session
    pub fn with_flatland(flatland: Arc<std::sync::RwLock<FlatlandSession>>) -> Self {
        Self {
            flatland: Some(flatland),
            view_ref: None,
            has_view: false,
            current_properties: None,
        }
    }

    /// Handle CreateView2 request
    ///
    /// Called when the system wants us to create our View.
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub async fn create_view2(
        &mut self,
        mut args: CreateView2Args,
    ) -> Result<ViewEvent, ViewProviderError> {
        use fidl::endpoints::create_proxy;
        use fidl_fuchsia_ui_composition::ParentViewportWatcherMarker;

        if self.has_view {
            return Err(ViewProviderError::ViewAlreadyExists);
        }

        if !args.view_creation_token.is_valid() {
            return Err(ViewProviderError::InvalidToken);
        }

        // Get the FIDL token
        let view_creation_token = args.view_creation_token.take_fidl()
            .ok_or(ViewProviderError::InvalidToken)?;

        // Store view ref
        self.view_ref = args.view_ref;

        // Create ParentViewportWatcher endpoints
        let (parent_viewport_watcher, parent_viewport_watcher_request) =
            create_proxy::<ParentViewportWatcherMarker>()
                .map_err(|e| ViewProviderError::FlatlandError(format!("Failed to create ParentViewportWatcher: {:?}", e)))?;

        // Create View in Flatland
        if let Some(flatland) = &self.flatland {
            let mut session = flatland.write().unwrap();

            // Build ViewBoundProtocols - we want the ParentViewportWatcher
            let view_bound_protocols = fidl_fuchsia_ui_composition::ViewBoundProtocols {
                view_ref_focused: None, // Will be set up separately
                ..Default::default()
            };

            // Create the view with Flatland
            session.create_view_with_token(
                view_creation_token,
                parent_viewport_watcher_request,
                view_bound_protocols,
            )?;

            // Store the parent viewport watcher proxy for layout updates
            session.set_parent_viewport_watcher(parent_viewport_watcher);
        }

        self.has_view = true;

        // Return initial properties (will be updated by ParentViewportWatcher)
        let properties = ViewProperties::default();
        self.current_properties = Some(properties.clone());

        tracing::info!("View created successfully");

        Ok(ViewEvent::Created { properties })
    }

    /// Handle CreateView2 (sync placeholder for non-Fuchsia)
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn create_view2_sync(
        &mut self,
        args: CreateView2Args,
    ) -> Result<ViewEvent, ViewProviderError> {
        if self.has_view {
            return Err(ViewProviderError::ViewAlreadyExists);
        }

        if !args.view_creation_token.is_valid() {
            return Err(ViewProviderError::InvalidToken);
        }

        self.view_ref = args.view_ref;
        self.has_view = true;

        let properties = ViewProperties::default();
        self.current_properties = Some(properties.clone());

        Ok(ViewEvent::Created { properties })
    }

    /// Update view properties from ParentViewportWatcher
    pub fn update_properties(&mut self, properties: ViewProperties) -> ViewEvent {
        self.current_properties = Some(properties.clone());

        ViewEvent::LayoutChanged {
            width: properties.width,
            height: properties.height,
            scale_factor: properties.scale_factor,
            insets: properties.insets,
        }
    }

    /// Handle focus change
    pub fn set_focus(&mut self, focused: bool) -> ViewEvent {
        if let Some(ref mut props) = self.current_properties {
            props.focused = focused;
        }
        ViewEvent::FocusChanged(focused)
    }

    /// Destroy the view
    pub fn destroy(&mut self) -> ViewEvent {
        self.has_view = false;
        self.view_ref = None;
        self.current_properties = None;
        ViewEvent::Destroyed
    }

    /// Check if we have an active view
    pub fn has_view(&self) -> bool {
        self.has_view
    }

    /// Get current view properties
    pub fn properties(&self) -> Option<&ViewProperties> {
        self.current_properties.as_ref()
    }

    /// Get the view ref
    pub fn view_ref(&self) -> Option<&ViewRef> {
        self.view_ref.as_ref()
    }
}

impl Default for ViewProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Serve the ViewProvider FIDL protocol
///
/// This function runs the FIDL service loop for ViewProvider.
/// Receives CreateView2 requests and forwards events to the app.
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
pub async fn serve_view_provider(
    mut stream: ViewProviderRequestStream,
    mut event_sender: futures::channel::mpsc::Sender<ViewEvent>,
    provider: Arc<std::sync::RwLock<ViewProvider>>,
) -> Result<(), ViewProviderError> {
    use futures::SinkExt;

    tracing::info!("ViewProvider service started");

    while let Some(request) = stream.try_next().await.map_err(|_| ViewProviderError::ChannelClosed)? {
        match request {
            ViewProviderRequest::CreateView2 { args, control_handle: _ } => {
                tracing::info!("Received CreateView2 request");

                // Extract view creation token
                let view_creation_token = match args.view_creation_token {
                    Some(token) => ViewCreationToken::from_fidl(token),
                    None => {
                        tracing::error!("CreateView2 missing view_creation_token");
                        continue;
                    }
                };

                // Extract view identity (contains ViewRef)
                let view_ref = args.view_identity.and_then(|identity| {
                    identity.view_ref.map(ViewRef::from_fidl)
                });

                let create_args = CreateView2Args {
                    view_creation_token,
                    view_ref,
                    view_identity: None,
                };

                let mut provider = provider.write().unwrap();
                match provider.create_view2(create_args).await {
                    Ok(event) => {
                        if event_sender.send(event).await.is_err() {
                            tracing::warn!("Event receiver dropped");
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::error!("CreateView2 failed: {}", e);
                    }
                }
            }
            // Deprecated CreateView - log and ignore
            ViewProviderRequest::CreateView { .. } => {
                tracing::warn!("Received deprecated CreateView request, ignoring");
            }
            ViewProviderRequest::CreateViewWithViewRef { .. } => {
                tracing::warn!("Received deprecated CreateViewWithViewRef request, ignoring");
            }
        }
    }

    tracing::info!("ViewProvider service ended");
    Ok(())
}

/// Watch the parent viewport for layout changes
///
/// This monitors the ParentViewportWatcher for:
/// - Size changes
/// - Scale factor changes
/// - Inset changes
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
pub async fn watch_parent_viewport(
    watcher: ParentViewportWatcherProxy,
    mut event_sender: futures::channel::mpsc::Sender<ViewEvent>,
    provider: Arc<std::sync::RwLock<ViewProvider>>,
) {
    use futures::SinkExt;

    tracing::info!("ParentViewportWatcher started");

    loop {
        match watcher.get_layout().await {
            Ok(layout_info) => {
                // Convert FIDL LayoutInfo to our LayoutInfo
                let layout = convert_layout_info(&layout_info);

                // Get current properties and update
                let event = {
                    let mut provider = provider.write().unwrap();
                    let current = provider.properties().cloned().unwrap_or_default();
                    let new_props = layout.to_view_properties(&current);
                    provider.update_properties(new_props)
                };

                if event_sender.send(event).await.is_err() {
                    tracing::warn!("Event receiver dropped, stopping ParentViewportWatcher");
                    break;
                }
            }
            Err(e) => {
                tracing::error!("ParentViewportWatcher error: {:?}", e);
                break;
            }
        }
    }

    tracing::info!("ParentViewportWatcher ended");
}

/// Convert FIDL LayoutInfo to our LayoutInfo
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
fn convert_layout_info(fidl_layout: &FidlLayoutInfo) -> LayoutInfo {
    let logical_size = fidl_layout.logical_size.map(|size| {
        (size.width as f32, size.height as f32)
    });

    let device_pixel_ratio = fidl_layout.device_pixel_ratio.map(|ratio| {
        ratio.x as f32 // Assume uniform scaling
    });

    let inset = fidl_layout.inset.as_ref().map(|inset| {
        ViewInset {
            top: inset.top,
            right: inset.right,
            bottom: inset.bottom,
            left: inset.left,
        }
    });

    LayoutInfo {
        logical_size,
        device_pixel_ratio,
        inset,
    }
}

// ============================================================================
// ParentViewportWatcher Integration
// ============================================================================

/// Layout information from fuchsia.ui.composition.ParentViewportWatcher
///
/// This corresponds to `fuchsia.ui.composition.LayoutInfo`.
#[derive(Clone, Debug, Default)]
pub struct LayoutInfo {
    /// Logical size of the view in DIP (device-independent pixels)
    pub logical_size: Option<(f32, f32)>,
    /// Device pixel ratio
    pub device_pixel_ratio: Option<f32>,
    /// Insets from edges (for safe areas, notches, etc.)
    pub inset: Option<ViewInset>,
}

/// Insets for view safe areas
#[derive(Clone, Copy, Debug, Default)]
pub struct ViewInset {
    /// Inset from top edge
    pub top: i32,
    /// Inset from right edge
    pub right: i32,
    /// Inset from bottom edge
    pub bottom: i32,
    /// Inset from left edge
    pub left: i32,
}

impl LayoutInfo {
    /// Convert to ViewProperties
    pub fn to_view_properties(&self, current: &ViewProperties) -> ViewProperties {
        let (width, height) = self.logical_size.unwrap_or((current.width, current.height));
        let scale_factor = self.device_pixel_ratio.map(|r| r as f64).unwrap_or(current.scale_factor);

        let mut props = ViewProperties {
            width,
            height,
            scale_factor,
            focused: current.focused,
            ..Default::default()
        };

        if let Some(inset) = &self.inset {
            props.set_insets((
                inset.top as f32,
                inset.right as f32,
                inset.bottom as f32,
                inset.left as f32,
            ));
        }

        props
    }
}

/// ParentViewportWatcher state manager
///
/// Tracks layout updates from the Fuchsia compositor.
pub struct ParentViewportWatcher {
    /// Current layout info
    current_layout: LayoutInfo,
    /// Whether we've received initial layout
    has_layout: bool,
}

impl ParentViewportWatcher {
    /// Create a new viewport watcher
    pub fn new() -> Self {
        Self {
            current_layout: LayoutInfo::default(),
            has_layout: false,
        }
    }

    /// Update layout from FIDL response
    ///
    /// Returns true if layout changed significantly.
    pub fn update(&mut self, layout: LayoutInfo) -> bool {
        let changed = self.current_layout.logical_size != layout.logical_size
            || self.current_layout.device_pixel_ratio != layout.device_pixel_ratio;

        self.current_layout = layout;
        self.has_layout = true;

        changed
    }

    /// Get current layout info
    pub fn layout(&self) -> &LayoutInfo {
        &self.current_layout
    }

    /// Check if we've received initial layout
    pub fn has_layout(&self) -> bool {
        self.has_layout
    }

    /// Get logical size or default
    pub fn logical_size(&self) -> (f32, f32) {
        self.current_layout.logical_size.unwrap_or((1920.0, 1080.0))
    }

    /// Get device pixel ratio or default
    pub fn device_pixel_ratio(&self) -> f32 {
        self.current_layout.device_pixel_ratio.unwrap_or(1.0)
    }
}

impl Default for ParentViewportWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Watch for focus changes
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
pub async fn watch_focus(
    view_ref_focused: ViewRefFocusedProxy,
    mut event_sender: futures::channel::mpsc::Sender<ViewEvent>,
    provider: Arc<std::sync::RwLock<ViewProvider>>,
) {
    use futures::SinkExt;

    tracing::info!("Focus watcher started");

    loop {
        match view_ref_focused.watch().await {
            Ok(state) => {
                let focused = state.focused.unwrap_or(false);

                let event = {
                    let mut provider = provider.write().unwrap();
                    provider.set_focus(focused)
                };

                tracing::debug!("Focus changed: {}", focused);

                if event_sender.send(event).await.is_err() {
                    tracing::warn!("Event receiver dropped, stopping focus watcher");
                    break;
                }
            }
            Err(e) => {
                tracing::error!("ViewRefFocused error: {:?}", e);
                break;
            }
        }
    }

    tracing::info!("Focus watcher ended");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_view_provider_new() {
        let provider = ViewProvider::new();
        assert!(!provider.has_view());
        assert!(provider.properties().is_none());
    }

    #[test]
    fn test_create_view_sync() {
        let mut provider = ViewProvider::new();

        let args = CreateView2Args {
            view_creation_token: ViewCreationToken::new(123),
            view_ref: Some(ViewRef::new(456)),
            view_identity: None,
        };

        let result = provider.create_view2_sync(args);
        assert!(result.is_ok());
        assert!(provider.has_view());

        // Can't create another view
        let args2 = CreateView2Args {
            view_creation_token: ViewCreationToken::new(789),
            view_ref: None,
            view_identity: None,
        };
        let result2 = provider.create_view2_sync(args2);
        assert!(matches!(result2, Err(ViewProviderError::ViewAlreadyExists)));
    }

    #[test]
    fn test_invalid_token() {
        let mut provider = ViewProvider::new();

        let args = CreateView2Args {
            view_creation_token: ViewCreationToken::new(0), // Invalid
            view_ref: None,
            view_identity: None,
        };

        let result = provider.create_view2_sync(args);
        assert!(matches!(result, Err(ViewProviderError::InvalidToken)));
    }

    #[test]
    fn test_update_properties() {
        let mut provider = ViewProvider::new();

        // Create view first
        let args = CreateView2Args {
            view_creation_token: ViewCreationToken::new(123),
            view_ref: None,
            view_identity: None,
        };
        provider.create_view2_sync(args).unwrap();

        // Update properties
        let props = ViewProperties {
            width: 800.0,
            height: 600.0,
            scale_factor: 2.0,
            ..Default::default()
        };

        let event = provider.update_properties(props);
        match event {
            ViewEvent::LayoutChanged { width, height, scale_factor, .. } => {
                assert_eq!(width, 800.0);
                assert_eq!(height, 600.0);
                assert_eq!(scale_factor, 2.0);
            }
            _ => panic!("Expected LayoutChanged event"),
        }
    }

    #[test]
    fn test_focus_change() {
        let mut provider = ViewProvider::new();

        let args = CreateView2Args {
            view_creation_token: ViewCreationToken::new(123),
            view_ref: None,
            view_identity: None,
        };
        provider.create_view2_sync(args).unwrap();

        let event = provider.set_focus(true);
        assert!(matches!(event, ViewEvent::FocusChanged(true)));

        let event = provider.set_focus(false);
        assert!(matches!(event, ViewEvent::FocusChanged(false)));
    }

    #[test]
    fn test_destroy_view() {
        let mut provider = ViewProvider::new();

        let args = CreateView2Args {
            view_creation_token: ViewCreationToken::new(123),
            view_ref: None,
            view_identity: None,
        };
        provider.create_view2_sync(args).unwrap();
        assert!(provider.has_view());

        let event = provider.destroy();
        assert!(matches!(event, ViewEvent::Destroyed));
        assert!(!provider.has_view());
    }
}
