//! Flatland compositor integration
//!
//! Provides types and helpers for integrating with Fuchsia's Flatland
//! 2D compositor API via FIDL.
//!
//! # Architecture
//!
//! Flatland is Fuchsia's modern 2D graphics compositor. Key concepts:
//!
//! - **Session**: Connection to Flatland for submitting scene graphs
//! - **TransformId**: Identifies nodes in the scene graph
//! - **ContentId**: Identifies visual content (images, solid colors)
//! - **ImagePipe**: GPU-rendered content via Vulkan
//!
//! # Usage
//!
//! ```ignore
//! // On Fuchsia, this would use actual FIDL proxies
//! let session = FlatlandSession::new()?;
//!
//! // Create root transform
//! let root = session.create_transform();
//! session.set_root_transform(root);
//!
//! // Create image content
//! let image = session.create_image_from_buffer_collection(...);
//! session.set_content(root, image);
//!
//! // Present changes
//! session.present().await?;
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use std::sync::Arc;

#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use fidl::endpoints::ServerEnd;
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use fidl_fuchsia_math as fmath;
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use fidl_fuchsia_ui_composition::{
    self as fcomp, AllocatorMarker, AllocatorProxy, ColorRgba, FlatlandMarker, FlatlandProxy,
    HitRegion as FidlHitRegion, ImageProperties as FidlImageProperties, Orientation,
    ParentViewportWatcherMarker, ParentViewportWatcherProxy, PresentArgs as FidlPresentArgs,
    TransformId as FidlTransformId, ContentId as FidlContentId, ViewBoundProtocols,
};
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use fidl_fuchsia_ui_views::ViewCreationToken as FidlViewCreationToken;
#[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
use fuchsia_component::client::connect_to_protocol;

/// Transform ID for Flatland scene graph nodes
///
/// On Fuchsia, this maps to fuchsia.ui.composition.TransformId.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TransformId(pub u64);

impl TransformId {
    /// The invalid/null transform ID
    pub const INVALID: Self = TransformId(0);

    /// Check if this is a valid transform ID
    pub fn is_valid(&self) -> bool {
        self.0 != 0
    }
}

/// Content ID for Flatland visual content
///
/// On Fuchsia, this maps to fuchsia.ui.composition.ContentId.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContentId(pub u64);

impl ContentId {
    /// The invalid/null content ID
    pub const INVALID: Self = ContentId(0);

    /// Check if this is a valid content ID
    pub fn is_valid(&self) -> bool {
        self.0 != 0
    }
}

/// Image properties for Flatland images
#[derive(Clone, Debug)]
pub struct ImageProperties {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
}

/// Solid color fill
#[derive(Clone, Debug)]
pub struct SolidColor {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
    pub alpha: f32,
}

impl SolidColor {
    pub fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            red: r,
            green: g,
            blue: b,
            alpha: a,
        }
    }
}

/// 2D transformation for scene graph nodes
#[derive(Clone, Debug, Default)]
pub struct Transform2D {
    /// Translation X
    pub tx: f32,
    /// Translation Y
    pub ty: f32,
    /// Scale X
    pub sx: f32,
    /// Scale Y
    pub sy: f32,
}

impl Transform2D {
    /// Identity transform
    pub fn identity() -> Self {
        Self {
            tx: 0.0,
            ty: 0.0,
            sx: 1.0,
            sy: 1.0,
        }
    }

    /// Create translation transform
    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            tx: x,
            ty: y,
            sx: 1.0,
            sy: 1.0,
        }
    }

    /// Create scale transform
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            tx: 0.0,
            ty: 0.0,
            sx,
            sy,
        }
    }
}

/// Hit region for input handling
#[derive(Clone, Debug)]
pub struct HitRegion {
    /// Width of hit region
    pub width: f32,
    /// Height of hit region
    pub height: f32,
}

impl HitRegion {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// Present arguments for frame submission
#[derive(Clone, Debug, Default)]
pub struct PresentArgs {
    /// Requested presentation time (0 = ASAP)
    pub requested_presentation_time: i64,
    /// Acquire fences (GPU completion signals)
    pub acquire_fences: Vec<()>, // Would be Vec<zx::Event> on Fuchsia
    /// Release fences (returned when content can be reused)
    pub release_fences: Vec<()>, // Would be Vec<zx::Event> on Fuchsia
    /// Whether to skip frame if late
    pub unsquashable: bool,
}

/// Frame information returned after present
#[derive(Clone, Debug, Default)]
pub struct FramePresentedInfo {
    /// Actual presentation time
    pub actual_presentation_time: i64,
    /// Number of presents pending
    pub num_presents_allowed: u32,
}

/// Flatland session for scene graph management
///
/// On Fuchsia, this wraps the fuchsia.ui.composition.Flatland proxy.
pub struct FlatlandSession {
    /// Next transform ID to allocate
    next_transform_id: AtomicU64,
    /// Next content ID to allocate
    next_content_id: AtomicU64,
    /// Root transform
    root_transform: Option<TransformId>,
    /// Flatland FIDL proxy (Fuchsia only)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    proxy: Option<FlatlandProxy>,
    /// ParentViewportWatcher for layout updates (Fuchsia only)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    parent_viewport_watcher: Option<ParentViewportWatcherProxy>,
}

impl FlatlandSession {
    /// Create a new Flatland session
    ///
    /// On Fuchsia, this connects to the Flatland service.
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn new() -> Result<Self, FlatlandError> {
        Ok(Self {
            next_transform_id: AtomicU64::new(1),
            next_content_id: AtomicU64::new(1),
            root_transform: None,
        })
    }

    /// Create a new Flatland session (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn new() -> Result<Self, FlatlandError> {
        let proxy = connect_to_protocol::<FlatlandMarker>()
            .map_err(|e| FlatlandError::ConnectionFailed(format!("{:?}", e)))?;

        tracing::info!("Connected to Flatland service");

        Ok(Self {
            next_transform_id: AtomicU64::new(1),
            next_content_id: AtomicU64::new(1),
            root_transform: None,
            proxy: Some(proxy),
            parent_viewport_watcher: None,
        })
    }

    /// Get the Flatland proxy (Fuchsia only)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn proxy(&self) -> Option<&FlatlandProxy> {
        self.proxy.as_ref()
    }

    /// Create a view with the given token (Fuchsia only)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn create_view_with_token(
        &mut self,
        token: FidlViewCreationToken,
        parent_viewport_watcher: ServerEnd<ParentViewportWatcherMarker>,
        view_bound_protocols: ViewBoundProtocols,
    ) -> Result<(), FlatlandError> {
        let proxy = self.proxy.as_ref()
            .ok_or_else(|| FlatlandError::ConnectionFailed("No proxy".to_string()))?;

        proxy.create_view2(
            token,
            view_bound_protocols,
            parent_viewport_watcher,
        ).map_err(|e| FlatlandError::SessionError(format!("CreateView2 failed: {:?}", e)))?;

        tracing::info!("View created in Flatland");
        Ok(())
    }

    /// Set the parent viewport watcher proxy (Fuchsia only)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn set_parent_viewport_watcher(&mut self, watcher: ParentViewportWatcherProxy) {
        self.parent_viewport_watcher = Some(watcher);
    }

    /// Get the parent viewport watcher proxy (Fuchsia only)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn parent_viewport_watcher(&self) -> Option<&ParentViewportWatcherProxy> {
        self.parent_viewport_watcher.as_ref()
    }

    /// Create a new transform node
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn create_transform(&self) -> TransformId {
        let id = self.next_transform_id.fetch_add(1, Ordering::SeqCst);
        TransformId(id)
    }

    /// Create a new transform node (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn create_transform(&self) -> TransformId {
        let id = self.next_transform_id.fetch_add(1, Ordering::SeqCst);
        let transform_id = FidlTransformId { value: id };

        if let Some(proxy) = &self.proxy {
            if let Err(e) = proxy.create_transform(&transform_id) {
                tracing::error!("Failed to create transform: {:?}", e);
            }
        }

        TransformId(id)
    }

    /// Release a transform node
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn release_transform(&self, _id: TransformId) {}

    /// Release a transform node (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn release_transform(&self, id: TransformId) {
        if let Some(proxy) = &self.proxy {
            let transform_id = FidlTransformId { value: id.0 };
            if let Err(e) = proxy.release_transform(&transform_id) {
                tracing::error!("Failed to release transform: {:?}", e);
            }
        }
    }

    /// Set the root transform for this session
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn set_root_transform(&mut self, transform: TransformId) {
        self.root_transform = Some(transform);
    }

    /// Set the root transform for this session (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn set_root_transform(&mut self, transform: TransformId) {
        self.root_transform = Some(transform);

        if let Some(proxy) = &self.proxy {
            let transform_id = FidlTransformId { value: transform.0 };
            if let Err(e) = proxy.set_root_transform(&transform_id) {
                tracing::error!("Failed to set root transform: {:?}", e);
            }
        }
    }

    /// Add a child transform to a parent
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn add_child(&self, _parent: TransformId, _child: TransformId) {}

    /// Add a child transform to a parent (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn add_child(&self, parent: TransformId, child: TransformId) {
        if let Some(proxy) = &self.proxy {
            let parent_id = FidlTransformId { value: parent.0 };
            let child_id = FidlTransformId { value: child.0 };
            if let Err(e) = proxy.add_child(&parent_id, &child_id) {
                tracing::error!("Failed to add child: {:?}", e);
            }
        }
    }

    /// Remove a child transform from a parent
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn remove_child(&self, _parent: TransformId, _child: TransformId) {}

    /// Remove a child transform from a parent (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn remove_child(&self, parent: TransformId, child: TransformId) {
        if let Some(proxy) = &self.proxy {
            let parent_id = FidlTransformId { value: parent.0 };
            let child_id = FidlTransformId { value: child.0 };
            if let Err(e) = proxy.remove_child(&parent_id, &child_id) {
                tracing::error!("Failed to remove child: {:?}", e);
            }
        }
    }

    /// Set the translation of a transform
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn set_translation(&self, _transform: TransformId, _x: f32, _y: f32) {}

    /// Set the translation of a transform (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn set_translation(&self, transform: TransformId, x: f32, y: f32) {
        if let Some(proxy) = &self.proxy {
            let transform_id = FidlTransformId { value: transform.0 };
            let translation = fmath::Vec_ { x: x as i32, y: y as i32 };
            if let Err(e) = proxy.set_translation(&transform_id, &translation) {
                tracing::error!("Failed to set translation: {:?}", e);
            }
        }
    }

    /// Set the scale of a transform
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn set_scale(&self, _transform: TransformId, _sx: f32, _sy: f32) {}

    /// Set the scale of a transform (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn set_scale(&self, transform: TransformId, sx: f32, sy: f32) {
        if let Some(proxy) = &self.proxy {
            let transform_id = FidlTransformId { value: transform.0 };
            let scale = fmath::VecF { x: sx, y: sy };
            if let Err(e) = proxy.set_scale(&transform_id, &scale) {
                tracing::error!("Failed to set scale: {:?}", e);
            }
        }
    }

    /// Set the orientation (rotation) of a transform
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn set_orientation(&self, _transform: TransformId, _degrees: f32) {}

    /// Set the orientation (rotation) of a transform (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn set_orientation(&self, transform: TransformId, degrees: f32) {
        if let Some(proxy) = &self.proxy {
            let transform_id = FidlTransformId { value: transform.0 };
            // Flatland supports 0, 90, 180, 270 degree rotations
            let orientation = match (degrees as i32) % 360 {
                0 => Orientation::Ccw0Degrees,
                90 | -270 => Orientation::Ccw90Degrees,
                180 | -180 => Orientation::Ccw180Degrees,
                270 | -90 => Orientation::Ccw270Degrees,
                _ => Orientation::Ccw0Degrees,
            };
            if let Err(e) = proxy.set_orientation(&transform_id, orientation) {
                tracing::error!("Failed to set orientation: {:?}", e);
            }
        }
    }

    /// Set the clip bounds for a transform
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn set_clip_boundary(&self, _transform: TransformId, _rect: Option<(f32, f32, f32, f32)>) {}

    /// Set the clip bounds for a transform (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn set_clip_boundary(&self, transform: TransformId, rect: Option<(f32, f32, f32, f32)>) {
        if let Some(proxy) = &self.proxy {
            let transform_id = FidlTransformId { value: transform.0 };
            let clip_rect = rect.map(|(x, y, w, h)| fmath::Rect {
                x: x as i32,
                y: y as i32,
                width: w as i32,
                height: h as i32,
            });
            if let Err(e) = proxy.set_clip_boundary(&transform_id, clip_rect.as_ref()) {
                tracing::error!("Failed to set clip boundary: {:?}", e);
            }
        }
    }

    /// Create a new content ID for images
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn create_image(&self, _props: ImageProperties) -> ContentId {
        let id = self.next_content_id.fetch_add(1, Ordering::SeqCst);
        ContentId(id)
    }

    /// Create content from a buffer collection
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn create_image_from_buffer_collection(
        &self,
        _import_token: (),
        _buffer_index: u32,
        _props: ImageProperties,
    ) -> ContentId {
        let id = self.next_content_id.fetch_add(1, Ordering::SeqCst);
        ContentId(id)
    }

    /// Create content from a buffer collection (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn create_image_from_buffer_collection(
        &self,
        import_token: fcomp::BufferCollectionImportToken,
        buffer_index: u32,
        props: ImageProperties,
    ) -> Result<ContentId, FlatlandError> {
        let id = self.next_content_id.fetch_add(1, Ordering::SeqCst);
        let content_id = FidlContentId { value: id };

        if let Some(proxy) = &self.proxy {
            let image_props = FidlImageProperties {
                size: Some(fmath::SizeU {
                    width: props.width,
                    height: props.height,
                }),
                ..Default::default()
            };

            proxy.create_image(
                &content_id,
                import_token,
                buffer_index,
                &image_props,
            ).map_err(|e| FlatlandError::AllocationFailed(format!("{:?}", e)))?;
        }

        Ok(ContentId(id))
    }

    /// Create a solid color fill
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn create_filled_rect(&self, _color: SolidColor) -> ContentId {
        let id = self.next_content_id.fetch_add(1, Ordering::SeqCst);
        ContentId(id)
    }

    /// Create a solid color fill (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn create_filled_rect(&self, color: SolidColor) -> ContentId {
        let id = self.next_content_id.fetch_add(1, Ordering::SeqCst);
        let content_id = FidlContentId { value: id };

        if let Some(proxy) = &self.proxy {
            if let Err(e) = proxy.create_filled_rect(&content_id) {
                tracing::error!("Failed to create filled rect: {:?}", e);
            }
            // Set the solid fill color
            let rgba = ColorRgba {
                red: color.red,
                green: color.green,
                blue: color.blue,
                alpha: color.alpha,
            };
            if let Err(e) = proxy.set_solid_fill(&content_id, &rgba, &fmath::SizeU { width: 1, height: 1 }) {
                tracing::error!("Failed to set solid fill: {:?}", e);
            }
        }

        ContentId(id)
    }

    /// Set the content of a transform
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn set_content(&self, _transform: TransformId, _content: ContentId) {}

    /// Set the content of a transform (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn set_content(&self, transform: TransformId, content: ContentId) {
        if let Some(proxy) = &self.proxy {
            let transform_id = FidlTransformId { value: transform.0 };
            let content_id = FidlContentId { value: content.0 };
            if let Err(e) = proxy.set_content(&transform_id, &content_id) {
                tracing::error!("Failed to set content: {:?}", e);
            }
        }
    }

    /// Clear the content of a transform
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn clear_content(&self, _transform: TransformId) {}

    /// Clear the content of a transform (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn clear_content(&self, transform: TransformId) {
        if let Some(proxy) = &self.proxy {
            let transform_id = FidlTransformId { value: transform.0 };
            let content_id = FidlContentId { value: 0 }; // Invalid ID clears content
            if let Err(e) = proxy.set_content(&transform_id, &content_id) {
                tracing::error!("Failed to clear content: {:?}", e);
            }
        }
    }

    /// Release content
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn release_image(&self, _content: ContentId) {}

    /// Release content (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn release_image(&self, content: ContentId) {
        if let Some(proxy) = &self.proxy {
            let content_id = FidlContentId { value: content.0 };
            if let Err(e) = proxy.release_image(&content_id) {
                tracing::error!("Failed to release image: {:?}", e);
            }
        }
    }

    /// Release filled rect
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn release_filled_rect(&self, _content: ContentId) {}

    /// Release filled rect (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn release_filled_rect(&self, content: ContentId) {
        if let Some(proxy) = &self.proxy {
            let content_id = FidlContentId { value: content.0 };
            if let Err(e) = proxy.release_filled_rect(&content_id) {
                tracing::error!("Failed to release filled rect: {:?}", e);
            }
        }
    }

    /// Set the image destination size
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn set_image_destination_size(&self, _content: ContentId, _width: u32, _height: u32) {}

    /// Set the image destination size (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn set_image_destination_size(&self, content: ContentId, width: u32, height: u32) {
        if let Some(proxy) = &self.proxy {
            let content_id = FidlContentId { value: content.0 };
            let size = fmath::SizeU { width, height };
            if let Err(e) = proxy.set_image_destination_size(&content_id, &size) {
                tracing::error!("Failed to set image destination size: {:?}", e);
            }
        }
    }

    /// Set hit regions for input handling
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn set_hit_regions(&self, _transform: TransformId, _regions: Vec<HitRegion>) {}

    /// Set hit regions for input handling (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn set_hit_regions(&self, transform: TransformId, regions: Vec<HitRegion>) {
        if let Some(proxy) = &self.proxy {
            let transform_id = FidlTransformId { value: transform.0 };
            let fidl_regions: Vec<FidlHitRegion> = regions.iter().map(|r| {
                FidlHitRegion {
                    region: fmath::RectF {
                        x: 0.0,
                        y: 0.0,
                        width: r.width,
                        height: r.height,
                    },
                    hit_test: fcomp::HitTestInteraction::Default,
                }
            }).collect();
            if let Err(e) = proxy.set_hit_regions(&transform_id, &fidl_regions) {
                tracing::error!("Failed to set hit regions: {:?}", e);
            }
        }
    }

    /// Set infinite hit region (catches all input)
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn set_infinite_hit_region(&self, _transform: TransformId) {}

    /// Set infinite hit region (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn set_infinite_hit_region(&self, transform: TransformId) {
        if let Some(proxy) = &self.proxy {
            let transform_id = FidlTransformId { value: transform.0 };
            if let Err(e) = proxy.set_infinite_hit_region(&transform_id, fcomp::HitTestInteraction::Default) {
                tracing::error!("Failed to set infinite hit region: {:?}", e);
            }
        }
    }

    /// Present pending changes
    ///
    /// Returns frame info when the frame is presented.
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub async fn present(&self, args: PresentArgs) -> Result<FramePresentedInfo, FlatlandError> {
        let proxy = self.proxy.as_ref()
            .ok_or_else(|| FlatlandError::ConnectionFailed("No proxy".to_string()))?;

        let present_args = FidlPresentArgs {
            requested_presentation_time: Some(args.requested_presentation_time),
            unsquashable: Some(args.unsquashable),
            ..Default::default()
        };

        proxy.present(present_args)
            .map_err(|e| FlatlandError::PresentFailed(format!("{:?}", e)))?;

        // Wait for OnFramePresented event
        // In practice, you'd use an event stream here
        Ok(FramePresentedInfo::default())
    }

    /// Synchronous present (for placeholder implementation)
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn present_sync(&self, _args: PresentArgs) -> Result<FramePresentedInfo, FlatlandError> {
        Ok(FramePresentedInfo::default())
    }

    /// Get the root transform
    pub fn root_transform(&self) -> Option<TransformId> {
        self.root_transform
    }

    /// Clear the entire scene graph
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn clear(&mut self) {
        self.root_transform = None;
    }

    /// Clear the entire scene graph (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn clear(&mut self) {
        if let Some(proxy) = &self.proxy {
            if let Err(e) = proxy.clear() {
                tracing::error!("Failed to clear Flatland scene: {:?}", e);
            }
        }
        self.root_transform = None;
    }
}

impl Default for FlatlandSession {
    fn default() -> Self {
        Self::new().expect("Failed to create Flatland session")
    }
}

/// Errors from Flatland operations
#[derive(Debug, Clone)]
pub enum FlatlandError {
    /// Connection to Flatland failed
    ConnectionFailed(String),
    /// Session error from Flatland
    SessionError(String),
    /// Invalid transform ID
    InvalidTransform(TransformId),
    /// Invalid content ID
    InvalidContent(ContentId),
    /// Resource allocation failed
    AllocationFailed(String),
    /// Present failed
    PresentFailed(String),
}

impl std::fmt::Display for FlatlandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed(msg) => write!(f, "Flatland connection failed: {}", msg),
            Self::SessionError(msg) => write!(f, "Flatland session error: {}", msg),
            Self::InvalidTransform(id) => write!(f, "Invalid transform ID: {:?}", id),
            Self::InvalidContent(id) => write!(f, "Invalid content ID: {:?}", id),
            Self::AllocationFailed(msg) => write!(f, "Flatland allocation failed: {}", msg),
            Self::PresentFailed(msg) => write!(f, "Flatland present failed: {}", msg),
        }
    }
}

impl std::error::Error for FlatlandError {}

/// Buffer collection for GPU image sharing
///
/// On Fuchsia, this would use sysmem2 for allocating GPU-accessible buffers.
pub struct BufferCollection {
    /// Number of buffers in the collection
    pub buffer_count: u32,
    /// Width of each buffer
    pub width: u32,
    /// Height of each buffer
    pub height: u32,
    /// Pixel format
    pub format: BufferFormat,
}

impl BufferCollection {
    /// Create a new buffer collection
    pub fn new(buffer_count: u32, width: u32, height: u32, format: BufferFormat) -> Self {
        Self {
            buffer_count,
            width,
            height,
            format,
        }
    }

    /// Allocate the buffers
    ///
    /// On Fuchsia, this would use sysmem2 for allocation.
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub async fn allocate(&mut self) -> Result<(), FlatlandError> {
        // TODO: Connect to fuchsia.sysmem2.Allocator
        // Allocate BufferCollection with Vulkan-compatible constraints
        Ok(())
    }
}

/// Pixel format for buffer collections
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferFormat {
    /// 8-bit RGBA (linear)
    R8G8B8A8,
    /// 8-bit BGRA (linear)
    B8G8R8A8,
    /// 8-bit RGBA (sRGB)
    R8G8B8A8Srgb,
    /// 8-bit BGRA (sRGB)
    B8G8R8A8Srgb,
}

impl BufferFormat {
    /// Get the bytes per pixel
    pub fn bytes_per_pixel(&self) -> u32 {
        match self {
            Self::R8G8B8A8 | Self::B8G8R8A8 | Self::R8G8B8A8Srgb | Self::B8G8R8A8Srgb => 4,
        }
    }
}

/// Flatland allocator for managing resources
///
/// On Fuchsia, this uses fuchsia.ui.composition.Allocator
pub struct FlatlandAllocator {
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    _private: (),
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    proxy: Option<AllocatorProxy>,
}

impl FlatlandAllocator {
    /// Create a new allocator (non-Fuchsia)
    #[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
    pub fn new() -> Result<Self, FlatlandError> {
        Ok(Self { _private: () })
    }

    /// Create a new allocator (Fuchsia)
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub fn new() -> Result<Self, FlatlandError> {
        let proxy = connect_to_protocol::<AllocatorMarker>()
            .map_err(|e| FlatlandError::ConnectionFailed(format!("Allocator: {:?}", e)))?;

        tracing::info!("Connected to Flatland Allocator service");

        Ok(Self { proxy: Some(proxy) })
    }

    /// Register a buffer collection with Flatland
    ///
    /// Returns an import token that can be used to create images.
    #[cfg(all(target_os = "fuchsia", feature = "fuchsia-sdk"))]
    pub async fn register_buffer_collection(
        &self,
        export_token: fcomp::BufferCollectionExportToken,
    ) -> Result<fcomp::BufferCollectionImportToken, FlatlandError> {
        use fidl_fuchsia_ui_composition::RegisterBufferCollectionArgs;

        let proxy = self.proxy.as_ref()
            .ok_or_else(|| FlatlandError::ConnectionFailed("No allocator proxy".to_string()))?;

        // Create import/export token pair
        let (import_token, server_end) = fidl::endpoints::create_endpoints::<fcomp::BufferCollectionImportTokenMarker>();

        let args = RegisterBufferCollectionArgs {
            export_token: Some(export_token),
            buffer_collection_token: None,
            usage: Some(fcomp::RegisterBufferCollectionUsage::Default),
            ..Default::default()
        };

        proxy.register_buffer_collection(args)
            .await
            .map_err(|e| FlatlandError::AllocationFailed(format!("{:?}", e)))?
            .map_err(|e| FlatlandError::AllocationFailed(format!("{:?}", e)))?;

        Ok(fcomp::BufferCollectionImportToken {
            value: fidl::EventPair::from(server_end.into_channel()),
        })
    }
}

impl Default for FlatlandAllocator {
    fn default() -> Self {
        Self::new().expect("Failed to create Flatland allocator")
    }
}

/// Token for importing a buffer collection into Flatland (non-Fuchsia placeholder)
#[cfg(not(all(target_os = "fuchsia", feature = "fuchsia-sdk")))]
pub struct BufferCollectionImportToken {
    _private: (),
}
