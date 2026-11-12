//! Layer commands recorded during painting for the ordered renderer.

use super::DynamicImage;

/// Commands for layer operations during rendering
///
/// These commands are recorded during painting and executed by the renderer
/// to manage offscreen render targets and layer composition.
#[derive(Clone, Debug)]
pub enum LayerCommand {
    /// Glass sampled from the frame backdrop within its foreground visual group.
    Glass(Box<crate::paint::GroupGlass>),
    /// Consecutive mesh draws rendered as one scene within their native group.
    Meshes(Box<crate::paint::GroupMeshes>),
    /// A canvas callback executed at this paint position within its native group.
    GpuPass(Box<crate::paint::GroupGpuPass>),
    /// Collected image resolved to its retained texture before dispatch.
    DeferredImage { id: u64, opacity: f32 },
    /// Collected SVG resolved to an ordered raster image before dispatch.
    DeferredSvg { id: u64, opacity: f32 },
    /// Collected text resolved to native glyph primitives before dispatch.
    DeferredText { id: u64, opacity: f32 },
    /// Push a new layer - begins rendering to an offscreen target
    Push {
        /// Layer configuration
        config: blinc_core::LayerConfig,
    },
    /// Pop the current layer - composite it back to the parent
    Pop,
    /// Sample a completed named layer in the same ordered paint pass.
    /// Missing names and source rectangles outside the output draw nothing.
    Sample {
        /// ID of the layer to sample from
        id: blinc_core::LayerId,
        /// Source rectangle in the layer's texture (in pixels)
        source: blinc_core::Rect,
        /// Destination rectangle in the current target (in pixels)
        dest: blinc_core::Rect,
        /// Captured destination transform, opacity and clips; None uses viewport geometry.
        paint: Option<Box<DynamicImage>>,
    },
}

/// A recorded layer command with its primitive index
#[derive(Clone, Debug)]
pub struct LayerCommandEntry {
    /// The primitive index when this command was recorded
    pub primitive_index: usize,
    /// The foreground primitive index when this command was recorded.
    /// Needed so an effect layer composed of foreground primitives
    /// can be ranged the same way background primitives are.
    pub foreground_primitive_index: usize,
    /// The path-vertex index when this command was recorded. Each
    /// tessellated `fill_path` / `stroke_path` appends to
    /// `PathBatch.vertices`; ranging by this index lets the effect
    /// pipeline pull the exact path chunk that belongs to the
    /// current layer into its offscreen texture. Without it, path
    /// content (all of Lottie's shapes) bypassed the offscreen
    /// composite and rendered directly onto the final framebuffer
    /// — so layer-level opacity on a Lottie layer was visually
    /// indistinguishable from per-fill alpha multiplication.
    pub path_vertex_index: usize,
    /// The path-index count (triangle-list index buffer position)
    /// when this command was recorded. Paired with
    /// `path_vertex_index` so the renderer can pull both the vertex
    /// range and the index range for an effect layer without
    /// scanning the index buffer to find which triangles reference
    /// a given vertex window.
    pub path_index_count: usize,
    /// Foreground path-vertex index.
    pub foreground_path_vertex_index: usize,
    /// Foreground path-index count.
    pub foreground_path_index_count: usize,
    /// The layer command
    pub command: LayerCommand,
}
