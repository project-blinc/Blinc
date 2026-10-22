//! Draw Context - Unified Rendering API
//!
//! The `DrawContext` trait provides a unified interface for all drawing operations
//! in the BLINC canvas architecture. It adapts to the current layer type, providing
//! appropriate operations for 2D UI, 2D canvas drawing, and 3D scenes.
//!
//! # Design Philosophy
//!
//! Rather than having separate APIs for different rendering contexts, DrawContext
//! provides a single interface that:
//!
//! - Maintains transform, clip, and opacity stacks
//! - Supports 2D path-based drawing (fill, stroke, text)
//! - Supports 3D scene operations (meshes, lights, cameras)
//! - Enables dimension bridging (billboards, 3D viewports)
//! - Records commands for deferred GPU execution
//!
//! # Example
//!
//! ```ignore
//! fn paint(ctx: &mut dyn DrawContext) {
//!     // Transform stack
//!     ctx.push_transform(Transform::translate(10.0, 20.0));
//!
//!     // Draw a rounded rectangle
//!     ctx.fill_rect(Rect::new(0.0, 0.0, 100.0, 50.0), 8.0.into(), Color::BLUE);
//!
//!     // Draw text
//!     ctx.draw_text("Hello", Point::new(10.0, 30.0), &TextStyle::default());
//!
//!     ctx.pop_transform();
//! }
//! ```

use crate::layer::{
    Affine2D, BillboardFacing, BlendMode, Brush, Camera, ClipShape, Color, CornerRadius,
    CubemapData, Environment, LayerId, Light, Mat4, ParticleSystemData, Point, Rect, Sdf3DViewport,
    Shadow, Size, Vec2,
};

// ─────────────────────────────────────────────────────────────────────────────
// Transform Types
// ─────────────────────────────────────────────────────────────────────────────

/// Unified transform that can represent 2D or 3D transformations
#[derive(Clone, Debug)]
pub enum Transform {
    /// 2D affine transformation
    Affine2D(Affine2D),
    /// 3D matrix transformation
    Mat4(Mat4),
}

impl Transform {
    /// Create a 2D translation
    pub fn translate(x: f32, y: f32) -> Self {
        Transform::Affine2D(Affine2D::translation(x, y))
    }

    /// Create a 2D scale around the origin (0, 0)
    ///
    /// Note: This scales around the top-left corner. For centered scaling,
    /// use `scale_centered()` instead.
    pub fn scale(sx: f32, sy: f32) -> Self {
        Transform::Affine2D(Affine2D::scale(sx, sy))
    }

    /// Create a 2D scale centered around a specific point
    ///
    /// This creates a transform that:
    /// 1. Translates the center point to the origin
    /// 2. Applies the scale
    /// 3. Translates back
    ///
    /// This results in scaling that appears to grow/shrink from the center point.
    pub fn scale_centered(sx: f32, sy: f32, center_x: f32, center_y: f32) -> Self {
        // Combined transform: translate(cx, cy) * scale(sx, sy) * translate(-cx, -cy)
        // This can be computed directly in the affine matrix:
        // tx = cx * (1 - sx)
        // ty = cy * (1 - sy)
        let tx = center_x * (1.0 - sx);
        let ty = center_y * (1.0 - sy);
        Transform::Affine2D(Affine2D {
            elements: [sx, 0.0, 0.0, sy, tx, ty],
        })
    }

    /// Create a 2D rotation around the origin (0, 0)
    ///
    /// Note: This rotates around the top-left corner. For centered rotation,
    /// use `rotate_centered()` instead.
    pub fn rotate(angle: f32) -> Self {
        Transform::Affine2D(Affine2D::rotation(angle))
    }

    /// Create a 2D rotation centered around a specific point
    ///
    /// This creates a transform that:
    /// 1. Translates the center point to the origin
    /// 2. Applies the rotation
    /// 3. Translates back
    pub fn rotate_centered(angle: f32, center_x: f32, center_y: f32) -> Self {
        // Combined transform: translate(cx, cy) * rotate(angle) * translate(-cx, -cy)
        let c = angle.cos();
        let s = angle.sin();
        // tx = cx - cx*cos + cy*sin
        // ty = cy - cx*sin - cy*cos
        let tx = center_x - center_x * c + center_y * s;
        let ty = center_y - center_x * s - center_y * c;
        Transform::Affine2D(Affine2D {
            elements: [c, s, -s, c, tx, ty],
        })
    }

    /// Create a 3D translation
    pub fn translate_3d(x: f32, y: f32, z: f32) -> Self {
        Transform::Mat4(Mat4::translation(x, y, z))
    }

    /// Create a 3D scale
    pub fn scale_3d(x: f32, y: f32, z: f32) -> Self {
        Transform::Mat4(Mat4::scale(x, y, z))
    }

    /// Create identity transform
    pub fn identity() -> Self {
        Transform::Affine2D(Affine2D::IDENTITY)
    }

    /// Check if this is a 2D transform
    pub fn is_2d(&self) -> bool {
        matches!(self, Transform::Affine2D(_))
    }

    /// Check if this is a 3D transform
    pub fn is_3d(&self) -> bool {
        matches!(self, Transform::Mat4(_))
    }
}

impl Default for Transform {
    fn default() -> Self {
        Transform::identity()
    }
}

impl From<Affine2D> for Transform {
    fn from(t: Affine2D) -> Self {
        Transform::Affine2D(t)
    }
}

impl From<Mat4> for Transform {
    fn from(t: Mat4) -> Self {
        Transform::Mat4(t)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Stroke Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Line cap style
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineCap {
    /// Flat cap at the endpoint
    #[default]
    Butt,
    /// Rounded cap extending past the endpoint
    Round,
    /// Square cap extending past the endpoint
    Square,
}

/// Line join style
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineJoin {
    /// Miter join (sharp corner)
    #[default]
    Miter,
    /// Round join
    Round,
    /// Bevel join (flat corner)
    Bevel,
}

/// Stroke style configuration
#[derive(Clone, Debug)]
pub struct Stroke {
    /// Line width
    pub width: f32,
    /// Line cap style
    pub cap: LineCap,
    /// Line join style
    pub join: LineJoin,
    /// Miter limit (for Miter joins)
    pub miter_limit: f32,
    /// Dash pattern (empty for solid line)
    pub dash: Vec<f32>,
    /// Dash offset
    pub dash_offset: f32,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            width: 1.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: 4.0,
            dash: Vec::new(),
            dash_offset: 0.0,
        }
    }
}

impl Stroke {
    /// Create a new stroke with the given width
    pub fn new(width: f32) -> Self {
        Self {
            width,
            ..Default::default()
        }
    }

    /// Set line cap style
    pub fn with_cap(mut self, cap: LineCap) -> Self {
        self.cap = cap;
        self
    }

    /// Set line join style
    pub fn with_join(mut self, join: LineJoin) -> Self {
        self.join = join;
        self
    }

    /// Set dash pattern
    pub fn with_dash(mut self, pattern: Vec<f32>, offset: f32) -> Self {
        self.dash = pattern;
        self.dash_offset = offset;
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Text Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Text alignment
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Text baseline
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextBaseline {
    Top,
    Middle,
    #[default]
    Alphabetic,
    Bottom,
}

/// Font weight
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FontWeight {
    Thin,
    Light,
    #[default]
    Regular,
    Medium,
    Bold,
    Black,
}

/// Text style configuration
#[derive(Clone, Debug)]
pub struct TextStyle {
    /// Font family name
    pub family: String,
    /// Font size in pixels
    pub size: f32,
    /// Font weight
    pub weight: FontWeight,
    /// Text color
    pub color: Color,
    /// Text alignment
    pub align: TextAlign,
    /// Text baseline
    pub baseline: TextBaseline,
    /// Letter spacing adjustment
    pub letter_spacing: f32,
    /// Line height multiplier
    pub line_height: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            family: "system-ui".to_string(),
            size: 14.0,
            weight: FontWeight::Regular,
            color: Color::BLACK,
            align: TextAlign::Left,
            baseline: TextBaseline::Alphabetic,
            letter_spacing: 0.0,
            line_height: 1.2,
        }
    }
}

impl TextStyle {
    /// Create a new text style with font size
    pub fn new(size: f32) -> Self {
        Self {
            size,
            ..Default::default()
        }
    }

    /// Set text color
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set font weight
    pub fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Set font family
    pub fn with_family(mut self, family: impl Into<String>) -> Self {
        self.family = family.into();
        self
    }

    /// Set the horizontal alignment relative to the anchor x.
    pub fn with_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    /// Set the vertical baseline — determines which reference point
    /// the origin `y` coordinate represents (top of text, middle,
    /// or text baseline — matches the HTML5 Canvas convention).
    pub fn with_baseline(mut self, baseline: TextBaseline) -> Self {
        self.baseline = baseline;
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Path Types
// ─────────────────────────────────────────────────────────────────────────────

/// Path command for building vector paths
#[derive(Clone, Debug)]
pub enum PathCommand {
    /// Move to a point
    MoveTo(Point),
    /// Line to a point
    LineTo(Point),
    /// Quadratic Bézier curve
    QuadTo { control: Point, end: Point },
    /// Cubic Bézier curve
    CubicTo {
        control1: Point,
        control2: Point,
        end: Point,
    },
    /// Arc to a point
    ArcTo {
        radii: Vec2,
        rotation: f32,
        large_arc: bool,
        sweep: bool,
        end: Point,
    },
    /// Close the current subpath
    Close,
}

/// A vector path
#[derive(Clone, Debug, Default)]
pub struct Path {
    commands: Vec<PathCommand>,
}

impl Path {
    /// Create a new empty path
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Create a path from a vector of commands
    pub fn from_commands(commands: Vec<PathCommand>) -> Self {
        Self { commands }
    }

    /// Move to a point
    pub fn move_to(mut self, x: f32, y: f32) -> Self {
        self.commands.push(PathCommand::MoveTo(Point::new(x, y)));
        self
    }

    /// Line to a point
    pub fn line_to(mut self, x: f32, y: f32) -> Self {
        self.commands.push(PathCommand::LineTo(Point::new(x, y)));
        self
    }

    /// Quadratic Bézier curve
    pub fn quad_to(mut self, cx: f32, cy: f32, x: f32, y: f32) -> Self {
        self.commands.push(PathCommand::QuadTo {
            control: Point::new(cx, cy),
            end: Point::new(x, y),
        });
        self
    }

    /// Cubic Bézier curve
    pub fn cubic_to(mut self, cx1: f32, cy1: f32, cx2: f32, cy2: f32, x: f32, y: f32) -> Self {
        self.commands.push(PathCommand::CubicTo {
            control1: Point::new(cx1, cy1),
            control2: Point::new(cx2, cy2),
            end: Point::new(x, y),
        });
        self
    }

    /// Close the path
    pub fn close(mut self) -> Self {
        self.commands.push(PathCommand::Close);
        self
    }

    /// SVG Arc to a point
    ///
    /// - `radii`: The x and y radii of the ellipse
    /// - `rotation`: Rotation angle of the ellipse in radians
    /// - `large_arc`: If true, use the larger arc (> 180 degrees)
    /// - `sweep`: If true, draw clockwise; if false, counter-clockwise
    /// - `x`, `y`: End point of the arc
    pub fn arc_to(
        mut self,
        radii: Vec2,
        rotation: f32,
        large_arc: bool,
        sweep: bool,
        x: f32,
        y: f32,
    ) -> Self {
        self.commands.push(PathCommand::ArcTo {
            radii,
            rotation,
            large_arc,
            sweep,
            end: Point::new(x, y),
        });
        self
    }

    /// Create a rectangle path
    pub fn rect(rect: Rect) -> Self {
        Self::new()
            .move_to(rect.x(), rect.y())
            .line_to(rect.x() + rect.width(), rect.y())
            .line_to(rect.x() + rect.width(), rect.y() + rect.height())
            .line_to(rect.x(), rect.y() + rect.height())
            .close()
    }

    /// Create a circle path
    pub fn circle(center: Point, radius: f32) -> Self {
        // Approximate circle with 4 cubic Bézier curves
        let k = 0.552_284_8; // Magic number for cubic Bézier circle approximation
        let r = radius;
        let cx = center.x;
        let cy = center.y;

        Self::new()
            .move_to(cx + r, cy)
            .cubic_to(cx + r, cy + r * k, cx + r * k, cy + r, cx, cy + r)
            .cubic_to(cx - r * k, cy + r, cx - r, cy + r * k, cx - r, cy)
            .cubic_to(cx - r, cy - r * k, cx - r * k, cy - r, cx, cy - r)
            .cubic_to(cx + r * k, cy - r, cx + r, cy - r * k, cx + r, cy)
            .close()
    }

    /// Create a line path
    pub fn line(from: Point, to: Point) -> Self {
        Self::new().move_to(from.x, from.y).line_to(to.x, to.y)
    }

    /// Get the path commands
    pub fn commands(&self) -> &[PathCommand] {
        &self.commands
    }

    /// Check if the path is empty
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Calculate the bounding rectangle of this path
    pub fn bounds(&self) -> Rect {
        if self.commands.is_empty() {
            return Rect::ZERO;
        }

        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;

        for cmd in &self.commands {
            match cmd {
                PathCommand::MoveTo(p) | PathCommand::LineTo(p) => {
                    min_x = min_x.min(p.x);
                    min_y = min_y.min(p.y);
                    max_x = max_x.max(p.x);
                    max_y = max_y.max(p.y);
                }
                PathCommand::QuadTo { control, end } => {
                    min_x = min_x.min(control.x).min(end.x);
                    min_y = min_y.min(control.y).min(end.y);
                    max_x = max_x.max(control.x).max(end.x);
                    max_y = max_y.max(control.y).max(end.y);
                }
                PathCommand::CubicTo {
                    control1,
                    control2,
                    end,
                } => {
                    min_x = min_x.min(control1.x).min(control2.x).min(end.x);
                    min_y = min_y.min(control1.y).min(control2.y).min(end.y);
                    max_x = max_x.max(control1.x).max(control2.x).max(end.x);
                    max_y = max_y.max(control1.y).max(control2.y).max(end.y);
                }
                PathCommand::ArcTo { end, radii, .. } => {
                    // Conservative bounds: include endpoint and radii extent
                    min_x = min_x.min(end.x).min(end.x - radii.x);
                    min_y = min_y.min(end.y).min(end.y - radii.y);
                    max_x = max_x.max(end.x).max(end.x + radii.x);
                    max_y = max_y.max(end.y).max(end.y + radii.y);
                }
                PathCommand::Close => {}
            }
        }

        if min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite() {
            Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
        } else {
            Rect::ZERO
        }
    }

    /// Create a rounded rectangle path
    pub fn rounded_rect(rect: Rect, corner_radius: impl Into<CornerRadius>) -> Self {
        let r = corner_radius.into();
        let x = rect.x();
        let y = rect.y();
        let w = rect.width();
        let h = rect.height();

        // Clamp radii to half the minimum dimension
        let max_r = (w.min(h) / 2.0).max(0.0);
        let tl = r.top_left.min(max_r);
        let tr = r.top_right.min(max_r);
        let br = r.bottom_right.min(max_r);
        let bl = r.bottom_left.min(max_r);

        // Magic number for cubic Bézier circle approximation
        let k = 0.552_284_8;

        let mut path = Self::new().move_to(x + tl, y);

        // Top edge
        path = path.line_to(x + w - tr, y);
        if tr > 0.0 {
            path = path.cubic_to(
                x + w - tr * (1.0 - k),
                y,
                x + w,
                y + tr * (1.0 - k),
                x + w,
                y + tr,
            );
        }

        // Right edge
        path = path.line_to(x + w, y + h - br);
        if br > 0.0 {
            path = path.cubic_to(
                x + w,
                y + h - br * (1.0 - k),
                x + w - br * (1.0 - k),
                y + h,
                x + w - br,
                y + h,
            );
        }

        // Bottom edge
        path = path.line_to(x + bl, y + h);
        if bl > 0.0 {
            path = path.cubic_to(
                x + bl * (1.0 - k),
                y + h,
                x,
                y + h - bl * (1.0 - k),
                x,
                y + h - bl,
            );
        }

        // Left edge
        path = path.line_to(x, y + tl);
        if tl > 0.0 {
            path = path.cubic_to(x, y + tl * (1.0 - k), x + tl * (1.0 - k), y, x + tl, y);
        }

        path.close()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Image Types
// ─────────────────────────────────────────────────────────────────────────────

/// Handle to a loaded image
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageId(pub u64);

/// Image rendering options
#[derive(Clone, Debug, Default)]
pub struct ImageOptions {
    /// Source rectangle within the image (None = entire image)
    pub source_rect: Option<Rect>,
    /// Tint color (white = no tint)
    pub tint: Option<Color>,
    /// Opacity (1.0 = fully opaque)
    pub opacity: f32,
}

impl ImageOptions {
    pub fn new() -> Self {
        Self {
            source_rect: None,
            tint: None,
            opacity: 1.0,
        }
    }

    pub fn with_tint(mut self, color: Color) -> Self {
        self.tint = Some(color);
        self
    }

    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 3D Types
// ─────────────────────────────────────────────────────────────────────────────

/// Handle to a loaded mesh (for cached/registered meshes)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshId(pub u64);

/// Handle to a material (for cached/registered materials)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialId(pub u64);

/// Mesh instance for instanced rendering
#[derive(Clone, Debug)]
pub struct MeshInstance {
    pub transform: Mat4,
    pub material: Option<MaterialId>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Generic Mesh Data — users convert from glTF/OBJ/FBX/custom formats
// ─────────────────────────────────────────────────────────────────────────────

/// A single vertex with position, normal, UV, and color
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    /// Tangent vector for normal mapping (xyz = direction, w = handedness ±1)
    pub tangent: [f32; 4],
    /// Joint indices for skeletal animation (up to 4 influences per vertex)
    pub joints: [u32; 4],
    /// Joint weights for skeletal animation (should sum to 1.0)
    pub weights: [f32; 4],
}

impl Vertex {
    pub fn new(pos: [f32; 3]) -> Self {
        Self {
            position: pos,
            normal: [0.0, 1.0, 0.0],
            uv: [0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0],
            tangent: [1.0, 0.0, 0.0, 1.0],
            joints: [0; 4],
            weights: [1.0, 0.0, 0.0, 0.0],
        }
    }

    pub fn with_normal(mut self, n: [f32; 3]) -> Self {
        self.normal = n;
        self
    }
    pub fn with_uv(mut self, uv: [f32; 2]) -> Self {
        self.uv = uv;
        self
    }
    pub fn with_color(mut self, c: [f32; 4]) -> Self {
        self.color = c;
        self
    }
    pub fn with_tangent(mut self, t: [f32; 4]) -> Self {
        self.tangent = t;
        self
    }
    pub fn with_joints(mut self, joints: [u32; 4], weights: [f32; 4]) -> Self {
        self.joints = joints;
        self.weights = weights;
        self
    }
}

/// Generic mesh data — the interchange format for 3D geometry.
///
/// Users convert from any source format (glTF, OBJ, FBX, procedural)
/// into this struct, then pass it to `DrawContext::draw_mesh_data()`.
///
/// # Example
///
/// ```ignore
/// // Triangle
/// let mesh = MeshData {
///     vertices: vec![
///         Vertex::new([-0.5, -0.5, 0.0]).with_color([1.0, 0.0, 0.0, 1.0]),
///         Vertex::new([ 0.5, -0.5, 0.0]).with_color([0.0, 1.0, 0.0, 1.0]),
///         Vertex::new([ 0.0,  0.5, 0.0]).with_color([0.0, 0.0, 1.0, 1.0]),
///     ],
///     indices: vec![0, 1, 2],
///     material: Material::default(),
/// };
/// ctx.draw_mesh_data(&mesh, Mat4::IDENTITY);
/// ```
#[derive(Clone, Debug)]
pub struct MeshData {
    /// Shared-reference vertex buffer. `Arc<Vec<…>>` (not plain
    /// `Vec<…>`) so animated meshes can clone `MeshData` per frame
    /// for per-draw morph / skin updates without deep-copying the
    /// vertex data each time. A 5 K-vertex mesh is ~400 KB; at
    /// 60 fps × N meshes this add up to significant bandwidth
    /// otherwise. Readers should Deref-chain through to the slice:
    /// `mesh.vertices.len()`, `&mesh.vertices[..]`,
    /// `bytemuck::cast_slice(&mesh.vertices)` all work.
    pub vertices: std::sync::Arc<Vec<Vertex>>,
    /// Shared-reference index buffer. Same rationale as
    /// [`Self::vertices`].
    pub indices: std::sync::Arc<Vec<u32>>,
    pub material: Material,
    /// Optional skinning data for skeletal animation.
    /// When provided, the GPU applies bone transforms to each vertex
    /// based on joint indices and weights.
    pub skin: Option<SkinningData>,
    /// Per-primitive morph targets (blend shapes). Shared-reference
    /// for the same reason as `vertices` — morph deltas on a
    /// 152-target face can be tens of megabytes. Each entry is a
    /// set of per-vertex deltas (position, optionally normal /
    /// tangent) authored against the base `vertices` array. At
    /// runtime the renderer computes
    /// `final_vertex = base_vertex + Σ weights[i] · morph_targets[i]`
    /// using weights provided by the animation pipeline
    /// (`blinc_skeleton::Pose::morph_weights`). Empty `Arc<Vec>` for
    /// meshes without morph data — which is the common case.
    pub morph_targets: std::sync::Arc<Vec<MorphTarget>>,
    /// Per-draw morph weights, one float per entry in `morph_targets`.
    /// The renderer reads this to compute
    /// `final = base + Σ weights[i] · morph_targets[i]` in the vertex
    /// stage. Callers update this each frame from their animation
    /// source (typically `blinc_skeleton::Pose::morph_weights_for_node`).
    /// Plain `Vec` (not Arc) because it's written per draw — the
    /// whole point of Arc-ing the *other* fields is that this one
    /// stays cheap to mutate.
    pub morph_weights: Vec<f32>,
}

/// One morph target (aka blend shape) — per-vertex deltas that layer
/// on top of the base mesh. A mesh can have any number of morph
/// targets; each carries a weight ∈ [0, 1] (or outside that range
/// for over/undershoot), and the final rendered vertex is the base
/// plus a weighted sum of the targets.
///
/// `delta_positions.len()` must equal `MeshData::vertices.len()`;
/// the renderer uses positional identity between the base vertex at
/// index `v` and `delta_positions[v]`. Normal and tangent deltas
/// follow the same convention and are optional — when a target
/// only animates positions (a character's cheek bulging with no
/// meaningful shading change) leaving the normal / tangent slots
/// empty saves memory.
#[derive(Clone, Debug, Default)]
pub struct MorphTarget {
    /// Per-vertex position delta: `final_position[v] += weight *
    /// delta_positions[v]`.
    pub delta_positions: Vec<[f32; 3]>,
    /// Optional per-vertex normal delta. Same length as
    /// `delta_positions` when present.
    pub delta_normals: Option<Vec<[f32; 3]>>,
    /// Optional per-vertex tangent delta (xyz — the handedness `w`
    /// of the base vertex tangent isn't morphed). Same length as
    /// `delta_positions` when present.
    pub delta_tangents: Option<Vec<[f32; 3]>>,
}

/// Per-material UV transform — affine offset + rotation + scale applied
/// to the interpolated texture coordinate before any slot is sampled.
/// Encodes the `KHR_texture_transform` glTF extension.
///
/// Spec form: `uv_out = translate * rotate * scale * uv_in` — the
/// renderer flattens that to a 2×2 matrix + 2-element offset at upload
/// time. Identity transform is the default (all slots sampled with the
/// raw vertex UV).
///
/// **Scope note:** Blinc currently stores one transform per-material,
/// applied uniformly to every slot. The spec allows independent
/// transforms per texture binding; in practice atlas-packed assets
/// apply the same transform across all slots and that's what `Option<
/// TextureTransform>` on [`Material`] reflects. Per-slot transforms
/// can be layered on later without breaking the current API.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureTransform {
    /// UV offset added after scale + rotation.
    pub offset: [f32; 2],
    /// Counter-clockwise rotation in radians, around UV origin.
    pub rotation: f32,
    /// Per-axis scale applied before rotation. `[1.0, 1.0]` is identity.
    pub scale: [f32; 2],
}

impl Default for TextureTransform {
    fn default() -> Self {
        Self {
            offset: [0.0, 0.0],
            rotation: 0.0,
            scale: [1.0, 1.0],
        }
    }
}

/// PBR material for mesh rendering.
///
/// Follows the glTF 2.0 metallic-roughness workflow. Scalar factors
/// (`base_color`, `metallic`, `roughness`, `emissive`) are multiplied
/// by their optional textures when those textures are present; when a
/// texture is `None` the scalar acts alone, which means a caller can
/// leave every texture unset and still get a valid flat-shaded material.
#[derive(Clone, Debug)]
pub struct Material {
    /// Base color (RGBA). Multiplied against `base_color_texture` when
    /// that texture is present.
    pub base_color: [f32; 4],
    /// Metallic factor. Multiplied against the `.b` channel of
    /// `metallic_roughness_texture` when present. glTF convention:
    /// `0.0` = dielectric, `1.0` = pure metal.
    pub metallic: f32,
    /// Roughness factor. Multiplied against the `.g` channel of
    /// `metallic_roughness_texture` when present. `0.0` = mirror,
    /// `1.0` = perfectly diffuse.
    pub roughness: f32,
    /// Emissive color (RGB, linear). Multiplied against
    /// `emissive_texture` when present.
    pub emissive: [f32; 3],
    /// Base color texture (sRGB RGBA pixels, decoded per-fragment).
    /// `None` = use `base_color` alone.
    pub base_color_texture: Option<TextureData>,
    /// Normal map (tangent-space normals encoded as RGB in `[0,1]`,
    /// shader unpacks to `[-1,1]`).
    pub normal_map: Option<TextureData>,
    /// Normal map strength. `0.0` = flat, `1.0` = full effect.
    pub normal_scale: f32,
    /// Metallic/roughness texture. glTF convention: metallic in `.b`,
    /// roughness in `.g`. The shader multiplies these per-texel by the
    /// scalar `metallic` / `roughness` factors above.
    pub metallic_roughness_texture: Option<TextureData>,
    /// Emissive texture (sRGB RGB). Multiplied per-texel by
    /// `emissive`. Used for glowing HUD elements, lights, screens —
    /// anything the mesh emits light from regardless of incident
    /// illumination.
    pub emissive_texture: Option<TextureData>,
    /// Ambient occlusion texture (grayscale). The `.r` channel
    /// attenuates the ambient + indirect diffuse terms to simulate
    /// crevice self-shadowing without running a full ray query.
    pub occlusion_texture: Option<TextureData>,
    /// Occlusion strength. `0.0` = no AO, `1.0` = full AO from the
    /// texture. Matches the glTF `occlusionTexture.strength` semantic.
    pub occlusion_strength: f32,
    /// Displacement / height map (grayscale). Drives parallax
    /// occlusion mapping in the shader.
    pub displacement_map: Option<TextureData>,
    /// Displacement scale in world units.
    pub displacement_scale: f32,
    /// Whether the material is unlit (ignore lighting).
    pub unlit: bool,
    /// Alpha mode.
    pub alpha_mode: AlphaMode,
    /// Per-material cutoff used when `alpha_mode == Mask`. Fragments
    /// with base-color alpha below this threshold are discarded.
    /// glTF default is `0.5`; overridden per asset via the
    /// `alphaCutoff` material property. Ignored for `Opaque` and
    /// `Blend` modes.
    pub alpha_cutoff: f32,
    /// Whether this mesh receives shadows from other meshes.
    pub receives_shadows: bool,
    /// Whether this mesh casts shadows onto other meshes.
    pub casts_shadows: bool,
    /// Optional UV transform (`KHR_texture_transform`). Applied to the
    /// interpolated UV before every texture sample — see
    /// [`TextureTransform`] for semantics. `None` means the identity
    /// transform and costs the shader one extra vec2 multiply with
    /// zero runtime branch.
    pub texture_transform: Option<TextureTransform>,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            base_color: [1.0, 1.0, 1.0, 1.0],
            metallic: 0.0,
            roughness: 0.5,
            emissive: [0.0, 0.0, 0.0],
            base_color_texture: None,
            normal_map: None,
            normal_scale: 1.0,
            metallic_roughness_texture: None,
            emissive_texture: None,
            occlusion_texture: None,
            occlusion_strength: 1.0,
            displacement_map: None,
            displacement_scale: 0.05,
            unlit: false,
            alpha_mode: AlphaMode::Opaque,
            alpha_cutoff: 0.5,
            receives_shadows: true,
            casts_shadows: true,
            texture_transform: None,
        }
    }
}

/// Pixel storage format for a [`TextureData`].
///
/// Block-compressed variants match the wgpu `TexturePixelFormat` family
/// they map to — the GPU sampler handles the on-the-fly
/// decompression, so callers don't need a shader change when
/// switching a texture slot from uncompressed to BC.
///
/// All BC variants pack 4×4 pixel blocks into a fixed byte budget:
///
/// | Variant | Intended use           | Block size | Bits/pixel |
/// |---------|------------------------|-----------:|-----------:|
/// | `Rgba8` | Uncompressed RGBA8     |   —        |    32      |
/// | `Bc1`   | Opaque RGB (diffuse)   |  8 bytes   |     4      |
/// | `Bc3`   | RGBA with alpha        | 16 bytes   |     8      |
/// | `Bc4`   | Single-channel R       |  8 bytes   |     4      |
/// | `Bc5`   | Two-channel RG         | 16 bytes   |     8      |
///
/// Diffuse/emissive slots are sRGB-encoded; normal/MR/occlusion
/// slots are linear. The GPU upload path in `blinc_gpu` picks the
/// right sRGB-vs-linear wgpu format per slot intent — `TextureData`
/// only carries the compression topology.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TexturePixelFormat {
    /// RGBA8 — 32 bits per pixel. Default; matches PNG/JPEG decode
    /// output.
    #[default]
    Rgba8,
    /// BC1 — 4 bpp; one-bit alpha at best. Good for opaque RGB
    /// diffuse and emissive.
    Bc1,
    /// BC3 — 8 bpp; full alpha. Good for RGBA diffuse when alpha
    /// is actually used.
    Bc3,
    /// BC4 — 4 bpp; single channel. Good for occlusion / AO or
    /// any R-only input.
    Bc4,
    /// BC5 — 8 bpp; two channels. Good for tangent-space normal
    /// maps (RG; B reconstructed in shader) and metallic-roughness
    /// (G+B in glTF convention).
    Bc5,
}

impl TexturePixelFormat {
    /// Bits per pixel — used by load-time size sanity checks and
    /// by the memory accounting in debug logs.
    pub const fn bits_per_pixel(self) -> u32 {
        match self {
            TexturePixelFormat::Rgba8 => 32,
            TexturePixelFormat::Bc1 | TexturePixelFormat::Bc4 => 4,
            TexturePixelFormat::Bc3 | TexturePixelFormat::Bc5 => 8,
        }
    }

    /// `true` iff the variant is one of the block-compressed formats
    /// (everything except `Rgba8`).
    pub const fn is_compressed(self) -> bool {
        !matches!(self, TexturePixelFormat::Rgba8)
    }
}

/// Texture data for materials.
///
/// Stores CPU-side pixel bytes (RGBA8 or a BC variant — see
/// [`TexturePixelFormat`]) plus the dimensions. Cloning a
/// `TextureData` (and therefore the `Material` that contains it) is a
/// refcount bump, not a pixel-data copy — multiple materials that
/// reference the same decoded image share one backing buffer.
///
/// The CPU buffer is wrapped in `Mutex<Option<Arc<[u8]>>>` and lives
/// behind an outer `Arc` shared across clones. That lets the GPU
/// renderer drop the CPU copy via [`TextureData::drop_cpu_bytes`]
/// after uploading to VRAM without needing mutable access to every
/// `Material` that holds a clone. Every clone sees the drop, and
/// [`TextureData::cache_key`] keeps returning the same stable
/// identifier so the GPU cache's per-texture entry stays reachable
/// for subsequent frames.
///
/// Readers use [`TextureData::with_bytes`] to borrow `&[u8]` while
/// the CPU copy is still present. After `drop_cpu_bytes()` those
/// calls return `None` — callers that need to re-upload must hold
/// their own copy elsewhere.
#[derive(Clone, Debug)]
pub struct TextureData {
    inner: std::sync::Arc<TextureDataInner>,
    pub width: u32,
    pub height: u32,
    /// Pixel storage format. Defaults to [`TexturePixelFormat::Rgba8`]
    /// for the legacy constructor; [`TextureData::new_compressed`]
    /// sets one of the BC variants.
    pub format: TexturePixelFormat,
}

#[derive(Debug)]
struct TextureDataInner {
    /// CPU-side pixel buffer. `None` once
    /// [`TextureData::drop_cpu_bytes`] has been called — typically
    /// after the first GPU upload. For compressed variants this
    /// holds the already-encoded BC blocks, not raw RGBA.
    rgba: std::sync::Mutex<Option<std::sync::Arc<[u8]>>>,
    /// Identifier used as the key in GPU texture caches. Captured at
    /// construction from the original `Arc<[u8]>` pointer and
    /// preserved even after `rgba` has been dropped.
    cache_key: usize,
}

impl TextureData {
    /// Construct a new `TextureData` from raw RGBA8 bytes. Panics (in
    /// debug builds) if `bytes.len() != width * height * 4`.
    pub fn new(bytes: Vec<u8>, width: u32, height: u32) -> Self {
        debug_assert_eq!(
            bytes.len(),
            (width as usize) * (height as usize) * 4,
            "TextureData::new: byte count doesn't match dimensions",
        );
        let arc_bytes: std::sync::Arc<[u8]> = bytes.into();
        // `Arc::as_ptr(&Arc<[u8]>)` returns a fat `*const [u8]` — cast
        // through the thin `*const u8` element pointer to get a plain
        // address for the cache key.
        let cache_key = std::sync::Arc::as_ptr(&arc_bytes) as *const u8 as usize;
        Self {
            inner: std::sync::Arc::new(TextureDataInner {
                rgba: std::sync::Mutex::new(Some(arc_bytes)),
                cache_key,
            }),
            width,
            height,
            format: TexturePixelFormat::Rgba8,
        }
    }

    /// Construct a new `TextureData` backed by already-encoded
    /// block-compressed pixels (one of the `Bc*` [`TexturePixelFormat`]
    /// variants). Panics in debug builds when `bytes.len()` doesn't
    /// match the format's expected block count for `width × height`.
    ///
    /// Width and height both round up to the nearest multiple of 4
    /// for block coverage; fractional blocks at the edge are the
    /// caller's responsibility to pad.
    pub fn new_compressed(
        bytes: Vec<u8>,
        format: TexturePixelFormat,
        width: u32,
        height: u32,
    ) -> Self {
        debug_assert!(
            format.is_compressed(),
            "TextureData::new_compressed: expected a BC variant, got {format:?}"
        );
        // 4×4 block count. Round up so non-multiple-of-4 dimensions
        // don't under-report the buffer size.
        let block_w = width.div_ceil(4) as usize;
        let block_h = height.div_ceil(4) as usize;
        let block_size = match format {
            TexturePixelFormat::Bc1 | TexturePixelFormat::Bc4 => 8,
            TexturePixelFormat::Bc3 | TexturePixelFormat::Bc5 => 16,
            TexturePixelFormat::Rgba8 => unreachable!(),
        };
        debug_assert_eq!(
            bytes.len(),
            block_w * block_h * block_size,
            "TextureData::new_compressed: byte count doesn't match {format:?} block count"
        );
        let arc_bytes: std::sync::Arc<[u8]> = bytes.into();
        let cache_key = std::sync::Arc::as_ptr(&arc_bytes) as *const u8 as usize;
        Self {
            inner: std::sync::Arc::new(TextureDataInner {
                rgba: std::sync::Mutex::new(Some(arc_bytes)),
                cache_key,
            }),
            width,
            height,
            format,
        }
    }

    /// Stable identifier for the backing CPU buffer, suitable for use
    /// as a GPU cache key. Preserved across `clone()` and across
    /// [`Self::drop_cpu_bytes`].
    pub fn cache_key(&self) -> usize {
        self.inner.cache_key
    }

    /// Borrow the CPU pixel buffer and run `f` with `&[u8]`. Returns
    /// `None` if the buffer has been released via
    /// [`Self::drop_cpu_bytes`]. Blocks only for the duration of
    /// `f` — callers shouldn't do GPU submits inside.
    pub fn with_bytes<R>(&self, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
        let guard = self.inner.rgba.lock().unwrap();
        guard.as_ref().map(|arc| f(arc))
    }

    /// `true` iff the CPU pixel buffer is still retained.
    pub fn has_cpu_bytes(&self) -> bool {
        self.inner.rgba.lock().unwrap().is_some()
    }

    /// Release the CPU pixel buffer. Every clone of this
    /// `TextureData` shares the same inner handle, so one call drops
    /// the buffer for all of them. Intended to be called by the GPU
    /// renderer after [`GpuRenderer`](`crate::draw::DrawContext`)-
    /// shaped code has uploaded the texture to VRAM.
    pub fn drop_cpu_bytes(&self) {
        *self.inner.rgba.lock().unwrap() = None;
    }
}

/// Alpha blending mode
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlphaMode {
    #[default]
    Opaque,
    Blend,
    Mask,
}

// ─────────────────────────────────────────────────────────────────────────────
// Skeletal Animation
// ─────────────────────────────────────────────────────────────────────────────

/// A bone in a skeletal hierarchy.
///
/// Users construct bones from glTF skins, FBX skeletons, etc.
/// The `inverse_bind_matrix` transforms from mesh space to bone-local space.
#[derive(Clone, Debug)]
pub struct Bone {
    /// Human-readable name (e.g., "LeftUpperArm")
    pub name: String,
    /// Index of parent bone, or None for the root
    pub parent: Option<usize>,
    /// Inverse bind matrix — transforms mesh-space positions into this bone's local space
    pub inverse_bind_matrix: [f32; 16],
}

/// Skeletal hierarchy (bind pose).
///
/// The bone list defines the hierarchy and rest pose.
/// Users animate by computing per-frame joint matrices and passing
/// them via `SkinningData`.
#[derive(Clone, Debug)]
pub struct Skeleton {
    pub bones: Vec<Bone>,
}

/// Per-frame skinning data sent to the GPU.
///
/// Each joint matrix is the product of the bone's current world transform
/// and its inverse bind matrix: `joint_matrix[i] = world_transform[i] * inverse_bind[i]`.
///
/// Maximum 256 joints. Vertices reference these via `Vertex::joints` indices.
#[derive(Clone, Debug)]
pub struct SkinningData {
    /// Joint matrices — one per bone, max 256.
    /// Each is a column-major 4x4 matrix.
    pub joint_matrices: Vec<[f32; 16]>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Layer Effects
// ─────────────────────────────────────────────────────────────────────────────

/// Post-processing effect quality levels
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlurQuality {
    /// Single-pass box blur (fastest, lowest quality)
    Low,
    /// Two-pass separable Gaussian (balanced)
    #[default]
    Medium,
    /// Multi-pass Kawase blur (slowest, highest quality)
    High,
}

/// Post-processing effects that can be applied to layers
#[derive(Clone, Debug, PartialEq)]
pub enum LayerEffect {
    /// Gaussian blur effect
    Blur {
        /// Blur radius in pixels
        radius: f32,
        /// Quality level (affects performance and visual quality)
        quality: BlurQuality,
    },
    /// Drop shadow effect (rendered behind the layer)
    DropShadow {
        /// Horizontal offset
        offset_x: f32,
        /// Vertical offset
        offset_y: f32,
        /// Blur radius
        blur: f32,
        /// Spread radius (positive expands, negative contracts)
        spread: f32,
        /// Shadow color
        color: Color,
    },
    /// Outer glow effect
    Glow {
        /// Glow color
        color: Color,
        /// Blur softness (higher = softer edges)
        blur: f32,
        /// Glow range (how far the glow extends from the element)
        range: f32,
        /// Glow opacity (0.0 to 1.0)
        opacity: f32,
    },
    /// Color matrix transformation (4x5 matrix for RGBA + offset)
    ColorMatrix {
        /// 4x5 color transformation matrix stored row-major:
        /// `[R_new]` = `[m0  m1  m2  m3  m4 ]` * `[R]`
        /// `[G_new]` = `[m5  m6  m7  m8  m9 ]` * `[G]`
        /// `[B_new]` = `[m10 m11 m12 m13 m14]` * `[B]`
        /// `[A_new]` = `[m15 m16 m17 m18 m19]` * `[A]`
        ///                                       `[1]`
        matrix: [f32; 20],
    },
    /// Mask image effect (multiplies layer alpha by mask luminance/alpha)
    MaskImage {
        /// Image URL or path
        image_url: String,
        /// Mask sizing mode
        mask_mode: MaskMode,
    },
}

/// How a mask image is interpreted
#[derive(Clone, Debug, PartialEq, Default)]
pub enum MaskMode {
    /// Use the alpha channel of the mask image
    #[default]
    Alpha,
    /// Use the luminance of the mask image as alpha
    Luminance,
}

/// CSS mask-image value
#[derive(Clone, Debug)]
pub enum MaskImage {
    /// URL to an image file
    Url(String),
    /// Gradient mask (reuses the existing Gradient type)
    Gradient(crate::layer::Gradient),
}

impl LayerEffect {
    /// Create a blur effect with default quality
    pub fn blur(radius: f32) -> Self {
        Self::Blur {
            radius,
            quality: BlurQuality::default(),
        }
    }

    /// Create a blur effect with specified quality
    pub fn blur_with_quality(radius: f32, quality: BlurQuality) -> Self {
        Self::Blur { radius, quality }
    }

    /// Create a drop shadow effect
    pub fn drop_shadow(offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        Self::DropShadow {
            offset_x,
            offset_y,
            blur,
            spread: 0.0,
            color,
        }
    }

    /// Create a glow effect
    ///
    /// ## Parameters
    /// - `color`: Glow color
    /// - `blur`: Blur softness (higher = softer edges), typically 4-24
    /// - `range`: How far the glow extends from the element, typically 0-20
    /// - `opacity`: Glow visibility (0.0 to 1.0)
    pub fn glow(color: Color, blur: f32, range: f32, opacity: f32) -> Self {
        Self::Glow {
            color,
            blur,
            range,
            opacity,
        }
    }

    /// Create an identity color matrix (no change)
    pub fn color_matrix_identity() -> Self {
        Self::ColorMatrix {
            matrix: [
                1.0, 0.0, 0.0, 0.0, 0.0, // R
                0.0, 1.0, 0.0, 0.0, 0.0, // G
                0.0, 0.0, 1.0, 0.0, 0.0, // B
                0.0, 0.0, 0.0, 1.0, 0.0, // A
            ],
        }
    }

    /// Create a grayscale color matrix
    pub fn grayscale() -> Self {
        Self::ColorMatrix {
            matrix: [
                0.299, 0.587, 0.114, 0.0, 0.0, // R = 0.299R + 0.587G + 0.114B
                0.299, 0.587, 0.114, 0.0, 0.0, // G = same
                0.299, 0.587, 0.114, 0.0, 0.0, // B = same
                0.0, 0.0, 0.0, 1.0, 0.0, // A = A
            ],
        }
    }

    /// Create a sepia color matrix
    pub fn sepia() -> Self {
        Self::ColorMatrix {
            matrix: [
                0.393, 0.769, 0.189, 0.0, 0.0, // R
                0.349, 0.686, 0.168, 0.0, 0.0, // G
                0.272, 0.534, 0.131, 0.0, 0.0, // B
                0.0, 0.0, 0.0, 1.0, 0.0, // A
            ],
        }
    }

    /// Create a brightness adjustment matrix
    pub fn brightness(factor: f32) -> Self {
        Self::ColorMatrix {
            matrix: [
                factor, 0.0, 0.0, 0.0, 0.0, // R
                0.0, factor, 0.0, 0.0, 0.0, // G
                0.0, 0.0, factor, 0.0, 0.0, // B
                0.0, 0.0, 0.0, 1.0, 0.0, // A
            ],
        }
    }

    /// Create a contrast adjustment matrix
    pub fn contrast(factor: f32) -> Self {
        let offset = 0.5 * (1.0 - factor);
        Self::ColorMatrix {
            matrix: [
                factor, 0.0, 0.0, 0.0, offset, // R
                0.0, factor, 0.0, 0.0, offset, // G
                0.0, 0.0, factor, 0.0, offset, // B
                0.0, 0.0, 0.0, 1.0, 0.0, // A
            ],
        }
    }

    /// Create a saturation adjustment matrix
    pub fn saturation(factor: f32) -> Self {
        let inv = 1.0 - factor;
        let r = 0.299 * inv;
        let g = 0.587 * inv;
        let b = 0.114 * inv;
        Self::ColorMatrix {
            matrix: [
                r + factor,
                g,
                b,
                0.0,
                0.0, // R
                r,
                g + factor,
                b,
                0.0,
                0.0, // G
                r,
                g,
                b + factor,
                0.0,
                0.0, // B
                0.0,
                0.0,
                0.0,
                1.0,
                0.0, // A
            ],
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Layer Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// 3D perspective transform parameters for layer-based compositing.
/// When a container has CSS rotate-x/rotate-y, its entire subtree (including text)
/// is rendered flat to a layer texture, then the layer is composited with perspective
/// distortion applied to the blit quad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform3DParams {
    pub sin_rx: f32,
    pub cos_rx: f32,
    pub sin_ry: f32,
    pub cos_ry: f32,
    /// Perspective distance in physical pixels (already DPI-scaled)
    pub perspective_d: f32,
}

/// Configuration for offscreen layers
#[derive(Clone, Debug, Default)]
pub struct LayerConfig {
    /// Layer ID (optional)
    pub id: Option<LayerId>,
    /// Layer position in viewport coordinates (for proper compositing)
    pub position: Option<crate::Point>,
    /// Layer size (None = inherit from parent)
    pub size: Option<Size>,
    /// Blend mode with parent
    pub blend_mode: BlendMode,
    /// Opacity
    pub opacity: f32,
    /// Enable depth buffer
    pub depth: bool,
    /// Post-processing effects to apply when layer is composited
    pub effects: Vec<LayerEffect>,
    /// 3D perspective transform for layer compositing (rotate-x/rotate-y on containers)
    pub transform_3d: Option<Transform3DParams>,
}

impl LayerConfig {
    /// Create a new layer config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the layer ID
    pub fn id(mut self, id: LayerId) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the layer size
    pub fn size(mut self, size: Size) -> Self {
        self.size = Some(size);
        self
    }

    /// Set the layer position in viewport coordinates
    pub fn position(mut self, position: crate::Point) -> Self {
        self.position = Some(position);
        self
    }

    /// Set the blend mode
    pub fn blend_mode(mut self, mode: BlendMode) -> Self {
        self.blend_mode = mode;
        self
    }

    /// Set the opacity
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// Enable depth buffer
    pub fn with_depth(mut self) -> Self {
        self.depth = true;
        self
    }

    /// Add a post-processing effect
    pub fn effect(mut self, effect: LayerEffect) -> Self {
        self.effects.push(effect);
        self
    }

    /// Add a blur effect
    pub fn blur(self, radius: f32) -> Self {
        self.effect(LayerEffect::blur(radius))
    }

    /// Add a drop shadow effect
    pub fn drop_shadow(self, offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        self.effect(LayerEffect::drop_shadow(offset_x, offset_y, blur, color))
    }

    /// Add a glow effect
    ///
    /// ## Parameters
    /// - `color`: Glow color
    /// - `blur`: Blur softness (higher = softer edges), typically 4-24
    /// - `range`: How far the glow extends from the element, typically 0-20
    /// - `opacity`: Glow visibility (0.0 to 1.0)
    pub fn glow(self, color: Color, blur: f32, range: f32, opacity: f32) -> Self {
        self.effect(LayerEffect::glow(color, blur, range, opacity))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// SDF Builder
// ─────────────────────────────────────────────────────────────────────────────

/// Shape ID returned by SDF operations
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ShapeId(pub u32);

/// Builder for SDF (Signed Distance Field) shapes
///
/// This provides an optimized path for rendering UI primitives using GPU SDF shaders.
/// Operations here are batched and rendered very efficiently.
pub trait SdfBuilder {
    // ─────────────────────────────────────────────────────────────────────────
    // Primitives
    // ─────────────────────────────────────────────────────────────────────────

    /// Create a rectangle with optional corner radius
    fn rect(&mut self, rect: Rect, corner_radius: CornerRadius) -> ShapeId;

    /// Create a circle
    fn circle(&mut self, center: Point, radius: f32) -> ShapeId;

    /// Create an ellipse
    fn ellipse(&mut self, center: Point, radii: Vec2) -> ShapeId;

    /// Create a line segment
    fn line(&mut self, from: Point, to: Point, width: f32) -> ShapeId;

    /// Create an arc
    fn arc(&mut self, center: Point, radius: f32, start: f32, end: f32, width: f32) -> ShapeId;

    /// Create a quadratic Bézier curve (has closed-form SDF)
    fn quad_bezier(&mut self, p0: Point, p1: Point, p2: Point, width: f32) -> ShapeId;

    // ─────────────────────────────────────────────────────────────────────────
    // Boolean Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Union of two shapes
    fn union(&mut self, a: ShapeId, b: ShapeId) -> ShapeId;

    /// Subtract b from a
    fn subtract(&mut self, a: ShapeId, b: ShapeId) -> ShapeId;

    /// Intersect two shapes
    fn intersect(&mut self, a: ShapeId, b: ShapeId) -> ShapeId;

    /// Smooth union with blend radius
    fn smooth_union(&mut self, a: ShapeId, b: ShapeId, radius: f32) -> ShapeId;

    /// Smooth subtract with blend radius
    fn smooth_subtract(&mut self, a: ShapeId, b: ShapeId, radius: f32) -> ShapeId;

    /// Smooth intersect with blend radius
    fn smooth_intersect(&mut self, a: ShapeId, b: ShapeId, radius: f32) -> ShapeId;

    // ─────────────────────────────────────────────────────────────────────────
    // Modifiers
    // ─────────────────────────────────────────────────────────────────────────

    /// Round the corners of a shape
    fn round(&mut self, shape: ShapeId, radius: f32) -> ShapeId;

    /// Create an outline of a shape
    fn outline(&mut self, shape: ShapeId, width: f32) -> ShapeId;

    /// Offset a shape (positive = expand, negative = shrink)
    fn offset(&mut self, shape: ShapeId, distance: f32) -> ShapeId;

    // ─────────────────────────────────────────────────────────────────────────
    // Rendering
    // ─────────────────────────────────────────────────────────────────────────

    /// Fill a shape with a brush
    fn fill(&mut self, shape: ShapeId, brush: Brush);

    /// Stroke a shape
    fn stroke(&mut self, shape: ShapeId, stroke: &Stroke, brush: Brush);

    /// Add a shadow to a shape
    fn shadow(&mut self, shape: ShapeId, shadow: Shadow);
}

// ─────────────────────────────────────────────────────────────────────────────
// Draw Context Trait
// ─────────────────────────────────────────────────────────────────────────────

/// Unified drawing context that adapts to the current layer type
///
/// This is the primary interface for all drawing operations in BLINC. It provides:
///
/// - Transform, clip, and opacity stacks
/// - 2D drawing operations (fill, stroke, text, images)
/// - SDF primitive operations (optimized for UI)
/// - 3D scene operations (meshes, lights, cameras)
/// - Dimension bridging (billboards, 3D viewports)
/// - Layer management
pub trait DrawContext {
    // ─────────────────────────────────────────────────────────────────────────
    // Transform Stack
    // ─────────────────────────────────────────────────────────────────────────

    /// Push a transform onto the stack
    fn push_transform(&mut self, transform: Transform);

    /// Pop the top transform from the stack
    fn pop_transform(&mut self);

    /// Get the current combined transform
    fn current_transform(&self) -> Transform;

    /// Snapshot the current transform-stack depth. Pair with
    /// [`Self::restore_transform_stack`] to bracket a paint section
    /// against any inner code path that might leak a push (early
    /// return, conditional pop with a mutated gate, panic-recover).
    ///
    /// Default returns `0` — non-GPU contexts that don't keep an
    /// explicit stack can no-op this pair.
    fn transform_stack_depth(&self) -> usize {
        0
    }

    /// Truncate the transform stack back to the snapshot depth from
    /// [`Self::transform_stack_depth`]. Default no-op for contexts
    /// that don't keep an explicit stack.
    fn restore_transform_stack(&mut self, _depth: usize) {}

    // ─────────────────────────────────────────────────────────────────────────
    // State Stack
    // ─────────────────────────────────────────────────────────────────────────

    /// Push a clip shape onto the stack
    fn push_clip(&mut self, shape: ClipShape);

    /// Pop the top clip from the stack
    fn pop_clip(&mut self);

    /// Push an opacity value (multiplied with parent)
    fn push_opacity(&mut self, opacity: f32);

    /// Pop the top opacity from the stack
    fn pop_opacity(&mut self);

    /// Push a blend mode
    fn push_blend_mode(&mut self, mode: BlendMode);

    /// Pop the top blend mode from the stack
    fn pop_blend_mode(&mut self);

    /// Set whether we're rendering to the foreground layer (after glass)
    ///
    /// When true, primitives should be rendered on top of glass elements.
    /// Default is false (background layer). This is used by the three-pass
    /// rendering system to separate background and foreground primitives.
    fn set_foreground_layer(&mut self, _is_foreground: bool) {
        // Default implementation does nothing (for contexts that don't support layering)
    }

    /// Set the current z-layer for rendering
    ///
    /// Z-layers are used to interleave primitive and text rendering for proper
    /// Stack z-ordering. Each Stack child increments the z-layer, ensuring that
    /// all content (primitives + text) within that child renders together.
    fn set_z_layer(&mut self, _layer: u32) {
        // Default implementation does nothing
    }

    /// Get the current z-layer
    fn z_layer(&self) -> u32 {
        0
    }

    /// Number of background-batch primitives the context has emitted
    /// so far this frame. Read by the paint walker to bracket the
    /// primitive range a node (and its descendants) contributes, so
    /// the compositor-path fast Phase 4 knows which primitives to
    /// patch when a motion binding's value changes. Default `0` for
    /// contexts that don't track primitives (mock test contexts).
    fn bg_primitive_count(&self) -> usize {
        0
    }

    /// Whether background counts and layer indices currently refer to the
    /// separately dispatched motion batch instead of the static batch.
    fn bg_batch_is_dynamic(&self) -> bool {
        false
    }

    /// Whether background counts and layer indices refer to a per-node scratch
    /// batch whose content is baked into a cached composite texture.
    fn bg_batch_is_composite(&self) -> bool {
        false
    }

    /// Whether a descendant may leave the current batch for a separately
    /// composited texture. An open ancestor group can require one shared owner.
    fn can_promote_composite_layer(&self) -> bool {
        true
    }

    /// Screen-space bounds of the active composite scratch, including paths,
    /// nested groups and effect margins. Defaults to primitive-range bounds.
    fn composite_layer_bounds(&self) -> Option<[f32; 4]> {
        None
    }

    /// Number of layer commands the context has recorded so far this
    /// frame. Read by the walker right after `push_layer` so the
    /// Phase 4 CSS-anim recording can capture the just-pushed
    /// layer's index. The fast-path patcher uses that index to
    /// update `LayerConfig.opacity` in place when a CSS opacity
    /// animation took the layered (non-flattened) path. Default `0`
    /// for contexts that don't track layer commands (mock tests).
    fn bg_layer_command_count(&self) -> usize {
        0
    }

    /// Notify the context that the walker is entering a motion-bound
    /// subtree. Subsequent primitive / path / glass emissions route
    /// to a separate "dynamic batch" instead of the main batch, so
    /// the compositor v2 fast path can dispatch them per-frame as an
    /// overlay rather than baking them into the static cache. Pair
    /// with [`Self::pop_motion_subtree`]. Default no-op for contexts
    /// without this distinction (mock tests, future backends).
    fn push_motion_subtree(&mut self) {}

    /// Pair with [`Self::push_motion_subtree`]. Default no-op.
    fn pop_motion_subtree(&mut self) {}

    /// Notify the context that the walker is entering an
    /// `is_overlay_root` subtree (portal / popover / dropdown /
    /// dialog / toast). The renderer uses the depth this tracks to
    /// bucket a canvas's `CanvasPaintRecord` into the overlay-
    /// canvas pool instead of the global canvas-overlay pool, so
    /// the canvas dispatches AFTER its surrounding overlay's
    /// dyn-batch SDF pass — keeping caret blinks visible above the
    /// popover bg/border their host input paints. Default no-op for
    /// contexts without this distinction.
    fn push_overlay_subtree(&mut self) {}

    /// Pair with [`Self::push_overlay_subtree`]. Default no-op.
    fn pop_overlay_subtree(&mut self) {}

    /// True when the walker is currently inside one or more
    /// `is_overlay_root` subtrees. Default false.
    fn in_overlay_subtree(&self) -> bool {
        false
    }

    /// Walker entered a composite-promotable CSS-animated subtree.
    /// Subsequent primitive / path / glass emissions route into a
    /// per-node scratch batch keyed by `node_id` (rather than the
    /// bg batch). At end of paint, the compositor rasterizes each
    /// scratch batch into its own `LayerTexture` and the per-frame
    /// composite blits the texture with the active animation
    /// transform applied — no walker re-entry, no per-frame
    /// rasterization. Pair with [`Self::pop_composite_layer`].
    /// Default no-op for contexts without this distinction.
    fn push_composite_layer(&mut self, _node_id: u64) {}

    /// Pair with [`Self::push_composite_layer`]. Default no-op.
    fn pop_composite_layer(&mut self) {}

    /// Union AABB (`[x, y, w, h]` in screen pixels, post-DPI) of every
    /// background-batch primitive in `start..end`. Used by the
    /// compositor v2 damage-rect path: it captures the on-screen
    /// rectangle a motion-bound subtree occupied at last paint so the
    /// fast path can union it with the new AABB and re-render just
    /// the damaged region of the static cache.
    ///
    /// Returns `None` if `start >= end`, the range is out of bounds,
    /// or the context doesn't track primitives (mock test contexts).
    fn bg_primitive_aabb(&self, _start: usize, _end: usize) -> Option<[f32; 4]> {
        None
    }

    /// Snapshot the intersected AABB of all currently-pushed clips,
    /// in *screen coordinates* (i.e. each `push_clip` entry already
    /// transformed by the affine that was on the stack at the time
    /// of push).
    ///
    /// Returns `None` when the stack is empty (no active clip).
    /// Returns `Some([x, y, w, h])` otherwise — the intersection
    /// rectangle, or zero-size if the rects don't overlap.
    ///
    /// Used by the layer compositor: when the walker reaches a
    /// `Canvas` node, the ancestor clip stack (scroll viewports,
    /// overflow:hidden containers, etc.) is active. The compositor's
    /// overlay pass needs to apply the same clip so canvas content
    /// scrolled out of its parent stays hidden. Default returns
    /// `None` for contexts without a clip stack (mock tests).
    fn current_clip_aabb(&self) -> Option<[f32; 4]> {
        None
    }

    /// Snapshot the current ancestor clip as
    /// `(aabb_xywh, corner_radius_tl_tr_br_bl)`. Used by P4.3's
    /// motion-subtree bake to capture the parent's rounded-rect clip
    /// alongside the AABB so the per-frame blit can use the blit
    /// shader's rounded-rect scissor — without it, a progress
    /// indicator inside an `overflow_clip`-rounded track gets its
    /// left rounded corner squared off by the AABB-only scissor.
    /// Radius is `[0; 4]` when the ancestor chain has no rounded-
    /// rect clip. Default returns `None` for contexts without a
    /// clip stack (mock tests).
    fn current_clip_rounded(&self) -> Option<([f32; 4], [f32; 4])> {
        None
    }

    /// Snapshot the current composed affine transform as
    /// `[a, b, c, d, tx, ty]`.
    ///
    /// The compositor's split-paint hook uses this when the walker
    /// reaches a `Canvas` node — it captures the affine at paint
    /// time so a later fast-path frame can replay the canvas's
    /// `render_fn` with the same transform state without re-walking
    /// the rest of the tree. Default identity; real implementations
    /// in `blinc_gpu` override with the top of their transform
    /// stack.
    fn current_affine_elements(&self) -> [f32; 6] {
        [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
    }

    // ─────────────────────────────────────────────────────────────────────────
    // 3D Transform (per-element, transient)
    // ─────────────────────────────────────────────────────────────────────────

    /// Set 3D rotation and perspective for the current element
    fn set_3d_transform(&mut self, _rx_rad: f32, _ry_rad: f32, _perspective_d: f32) {}

    /// Set 3D shape parameters for the current element
    fn set_3d_shape(&mut self, _shape_type: f32, _depth: f32, _ambient: f32, _specular: f32) {}

    /// Set 3D light parameters for the current element
    fn set_3d_light(&mut self, _direction: [f32; 3], _intensity: f32) {}

    /// Set translate-z offset for 3D elements (positive = toward viewer)
    fn set_3d_translate_z(&mut self, _z: f32) {}

    /// Set group shape descriptors for compound 3D rendering
    /// Each shape is 16 floats: [offset(4), params(4), half_ext(4), color(4)]
    fn set_3d_group_raw(&mut self, _shapes: &[[f32; 16]]) {}

    /// Reset 3D transient state to defaults
    fn clear_3d(&mut self) {}

    /// Set CSS filter parameters for the current element
    #[allow(clippy::too_many_arguments)]
    fn set_css_filter(
        &mut self,
        _grayscale: f32,
        _invert: f32,
        _sepia: f32,
        _hue_rotate_deg: f32,
        _brightness: f32,
        _contrast: f32,
        _saturate: f32,
    ) {
    }

    /// Reset CSS filter state to identity
    fn clear_css_filter(&mut self) {}

    /// Set mask gradient parameters for the current element
    /// params: linear=(x1,y1,x2,y2), radial=(cx,cy,r,0) in pixel coords
    /// info: [mask_type, start_alpha, end_alpha, 0] where mask_type: 0=none, 1=linear, 2=radial
    fn set_mask_gradient(&mut self, _params: [f32; 4], _info: [f32; 4]) {}

    /// Clear mask gradient state
    fn clear_mask_gradient(&mut self) {}

    /// Set corner shape (superellipse n parameter) for the current element.
    /// Values: [top_left, top_right, bottom_right, bottom_left].
    /// n=1.0 = round (default), n=0.0 = bevel, n=2.0 = squircle, n=-1.0 = scoop.
    fn set_corner_shape(&mut self, _shape: [f32; 4]) {}

    /// Clear corner shape to default (round, n=1.0)
    fn clear_corner_shape(&mut self) {}

    /// Set overflow fade distances for the next push_clip.
    /// Values: [top, right, bottom, left] in CSS pixels.
    fn set_overflow_fade(&mut self, _fade: [f32; 4]) {}

    /// Clear pending overflow fade
    fn clear_overflow_fade(&mut self) {}

    /// Set the fill rule the next `push_clip` of a polygon or supported path uses.
    /// It applies to that clip only; the default is [`crate::FillRule::NonZero`].
    fn set_clip_fill_rule(&mut self, _rule: crate::FillRule) {}

    // ─────────────────────────────────────────────────────────────────────────
    // 2D Drawing Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Fill a path with a brush
    fn fill_path(&mut self, path: &Path, brush: Brush);

    /// Stroke a path
    fn stroke_path(&mut self, path: &Path, stroke: &Stroke, brush: Brush);

    /// Fill a rectangle (convenience method)
    fn fill_rect(&mut self, rect: Rect, corner_radius: CornerRadius, brush: Brush);

    /// Fill a notched rect using the SDF pipeline.
    ///
    /// A notch is a rounded rect with:
    /// - Optional per-corner concave curves (indicated by the `corner_types`
    ///   slot: 0.0 = sharp/convex, 1.0 = concave).
    /// - Optional top and/or bottom edge modifier — scoop (1), bulge (2),
    ///   v-cut (3), or v-peak (4) — described by the `top_mod` / `bottom_mod`
    ///   tuples as `(type, width, height_or_depth, corner_radius)`. Pass
    ///   `(0, 0, 0, 0)` for no modifier on a given edge.
    ///
    /// All geometry is evaluated per-pixel in the SDF fragment shader so
    /// notches inherit the main pipeline's antialiasing, clip/layer
    /// composition, transform stack, and border/shadow support — no CPU
    /// path tessellation and no dependency on the separate tessellated-
    /// path pipeline (which had portability issues on some WebGPU
    /// implementations).
    ///
    /// The default implementation falls back to `fill_rect` so non-GPU
    /// renderers (hit-testing, headless unit tests, etc.) don't have to
    /// reimplement the full SDF composition.
    ///
    /// # Parameters
    /// - `rect` — outer bounds in layout coordinates.
    /// - `corner_radii` — per-corner radii (top-left, top-right,
    ///   bottom-right, bottom-left). Magnitude only; sign is ignored.
    /// - `corner_types` — per-corner type flags in the same order
    ///   (0.0 = sharp or convex, 1.0 = concave).
    /// - `top_mod` — top edge modifier `(type, width, height, corner_radius)`.
    /// - `bottom_mod` — bottom edge modifier, same layout as `top_mod`.
    /// - `border` — optional `(width, color)` to stroke the notch with.
    /// - `shadow` — optional drop shadow.
    /// - `brush` — fill brush.
    #[allow(clippy::too_many_arguments)]
    fn fill_notch(
        &mut self,
        rect: Rect,
        corner_radii: [f32; 4],
        corner_types: [f32; 4],
        top_mod: [f32; 4],
        bottom_mod: [f32; 4],
        border: Option<(f32, Color)>,
        shadow: Option<Shadow>,
        brush: Brush,
    ) {
        // Default: drop every notch feature and draw a plain rounded rect.
        // The sign-less max() keeps concave radii from bleeding a rect
        // larger than the caller's bounds on fallback backends.
        let _ = (corner_types, top_mod, bottom_mod);
        let cr = CornerRadius {
            top_left: corner_radii[0].max(0.0),
            top_right: corner_radii[1].max(0.0),
            bottom_right: corner_radii[2].max(0.0),
            bottom_left: corner_radii[3].max(0.0),
        };
        if let Some(sh) = shadow {
            self.draw_shadow(rect, cr, sh);
        }
        self.fill_rect(rect, cr, brush);
        if let Some((width, color)) = border {
            if width > 0.0 {
                self.stroke_rect(rect, cr, &Stroke::new(width), Brush::Solid(color));
            }
        }
    }

    /// Fill a rectangle with per-side borders (all same color)
    /// Border format: [top, right, bottom, left]
    /// Default implementation draws fill then strokes with max border width
    fn fill_rect_with_per_side_border(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        brush: Brush,
        border_widths: [f32; 4],
        border_color: Color,
    ) {
        // Default: draw fill then stroke (suboptimal but works)
        self.fill_rect(rect, corner_radius, brush);
        let max_border = border_widths.iter().cloned().fold(0.0f32, |a, b| a.max(b));
        if max_border > 0.0 {
            let stroke = Stroke::new(max_border);
            self.stroke_rect(rect, corner_radius, &stroke, Brush::Solid(border_color));
        }
    }

    /// Stroke a rectangle (convenience method)
    fn stroke_rect(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        stroke: &Stroke,
        brush: Brush,
    );

    /// Fill a circle (convenience method)
    fn fill_circle(&mut self, center: Point, radius: f32, brush: Brush);

    /// Stroke a circle (convenience method)
    fn stroke_circle(&mut self, center: Point, radius: f32, stroke: &Stroke, brush: Brush);

    /// Draw text at a position
    fn draw_text(&mut self, text: &str, origin: Point, style: &TextStyle);

    /// Draw an image
    fn draw_image(&mut self, image: ImageId, rect: Rect, options: &ImageOptions);

    /// Draw raw RGBA pixel data directly to the target.
    ///
    /// Uploads the pixel data as a GPU texture and renders it to the
    /// destination rect. Use for video frames, camera preview, or
    /// any dynamically-generated image data.
    ///
    /// # Arguments
    /// * `data` — RGBA pixel data (4 bytes per pixel)
    /// * `width` — Image width in pixels
    /// * `height` — Image height in pixels
    /// * `dest` — Destination rectangle in layout coordinates
    fn draw_rgba_pixels(&mut self, _data: &[u8], _width: u32, _height: u32, _dest: Rect) {
        // Default no-op — GPU implementations override
    }

    /// Draw a drop shadow (renders outside the shape)
    fn draw_shadow(&mut self, rect: Rect, corner_radius: CornerRadius, shadow: Shadow);

    /// Draw an inner shadow (renders inside the shape, like CSS inset box-shadow)
    fn draw_inner_shadow(&mut self, rect: Rect, corner_radius: CornerRadius, shadow: Shadow);

    /// Draw a circle drop shadow with radially symmetric blur
    fn draw_circle_shadow(&mut self, center: Point, radius: f32, shadow: Shadow);

    /// Draw a circle inner shadow (renders inside the circle)
    fn draw_circle_inner_shadow(&mut self, center: Point, radius: f32, shadow: Shadow);

    /// Build SDF shapes using the optimized SDF pipeline
    ///
    /// This is the most efficient way to render UI primitives:
    /// ```ignore
    /// ctx.sdf_build(|sdf| {
    ///     let rect = sdf.rect(bounds, 8.0.into());
    ///     sdf.shadow(rect, Shadow::new(0.0, 4.0, 10.0, Color::BLACK.with_alpha(0.2)));
    ///     sdf.fill(rect, Color::WHITE.into());
    /// });
    /// ```
    fn sdf_build(&mut self, f: &mut dyn FnMut(&mut dyn SdfBuilder));

    // ─────────────────────────────────────────────────────────────────────────
    // 3D Drawing Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Set the camera for 3D rendering
    fn set_camera(&mut self, camera: &Camera);

    /// Set the logical bounds of the 3D viewport region. Called by
    /// `SceneKit3D::element` before `draw_mesh_data` so the paint
    /// context can compute the physical-pixel viewport rect from the
    /// current transform stack + these bounds, clipping the mesh to
    /// the canvas element rather than the full frame.
    fn set_3d_viewport_bounds(&mut self, _width: f32, _height: f32) {}

    /// Draw a mesh with a material (using cached mesh/material handles)
    fn draw_mesh(&mut self, mesh: MeshId, material: MaterialId, transform: Mat4);

    /// Draw instanced meshes (using cached mesh handle)
    fn draw_mesh_instanced(&mut self, mesh: MeshId, instances: &[MeshInstance]);

    /// Draw mesh data directly (no registration needed).
    ///
    /// Users convert from any format (glTF, OBJ, FBX, procedural) into
    /// `MeshData`, then pass it here. The GPU implementation handles
    /// vertex/index buffer upload and rendering.
    ///
    /// ```ignore
    /// let mesh = Arc::new(MeshData {
    ///     vertices: vec![Vertex::new([-0.5, -0.5, 0.0]), ...],
    ///     indices: vec![0, 1, 2],
    ///     material: Material::default(),
    /// });
    /// ctx.draw_mesh_data(mesh.clone(), Mat4::IDENTITY);
    /// ```
    fn draw_mesh_data(&mut self, _mesh: std::sync::Arc<MeshData>, _transform: Mat4) {
        // Default no-op — GPU implementations override
    }

    /// Add a light to the scene
    fn add_light(&mut self, light: Light);

    /// Set the environment (skybox, IBL)
    fn set_environment(&mut self, env: &Environment);

    /// Provide a pre-generated cubemap for IBL reflections.
    ///
    /// The GPU implementation uploads the face/mip data to the environment
    /// cubemap texture. Scenes that don't call this get a neutral gray
    /// fallback.
    fn set_environment_cubemap(&mut self, _data: std::sync::Arc<CubemapData>) {}

    // ─────────────────────────────────────────────────────────────────────────
    // Dimension Bridging
    // ─────────────────────────────────────────────────────────────────────────

    /// Embed 2D content in the current 3D context as a billboard
    fn billboard_draw(
        &mut self,
        size: Size,
        transform: Mat4,
        facing: BillboardFacing,
        f: &mut dyn FnMut(&mut dyn DrawContext),
    );

    /// Embed a 3D viewport in the current 2D context
    fn viewport_3d_draw(
        &mut self,
        rect: Rect,
        camera: &Camera,
        f: &mut dyn FnMut(&mut dyn DrawContext),
    );

    /// Draw an SDF 3D viewport using GPU raymarching
    ///
    /// This renders a procedural 3D scene defined by signed distance functions.
    /// The shader WGSL code should contain a `map_scene(p: vec3<f32>) -> f32` function
    /// that defines the SDF scene, and a `get_material(p: vec3<f32>) -> SdfMaterial`
    /// function for materials.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_3d::sdf::{SdfScene, SdfCodegen};
    /// use blinc_core::{DrawContext, Sdf3DViewport, Rect};
    ///
    /// // Build an SDF scene
    /// let scene = SdfScene::new()
    ///     .sphere(1.0)
    ///     .translate(0.0, 1.0, 0.0);
    ///
    /// // Generate shader and create viewport
    /// let mut viewport = Sdf3DViewport::default();
    /// viewport.shader_wgsl = SdfCodegen::generate_full_shader(&scene);
    ///
    /// // Render the viewport
    /// ctx.draw_sdf_viewport(Rect::new(0.0, 0.0, 800.0, 600.0), &viewport);
    /// ```
    fn draw_sdf_viewport(&mut self, _rect: Rect, _viewport: &Sdf3DViewport) {
        // Default implementation does nothing
        // GPU implementations override this to add SDF viewports to the render batch
    }

    /// Draw GPU-accelerated particles
    ///
    /// This renders a particle system using GPU compute and instanced rendering.
    /// The particle simulation and rendering happens entirely on the GPU for
    /// maximum performance.
    ///
    /// # Arguments
    ///
    /// * `rect` - The viewport rectangle to render particles in
    /// * `particle_data` - The particle system configuration and state
    ///
    /// # Example
    ///
    /// ```ignore
    /// use blinc_core::{DrawContext, ParticleSystemData, Rect};
    ///
    /// // Create particle system data
    /// let particles = ParticleSystemData {
    ///     emitter_position: Vec3::new(0.0, 0.0, 0.0),
    ///     emission_rate: 100.0,
    ///     ..Default::default()
    /// };
    ///
    /// // Render the particles
    /// ctx.draw_particles(Rect::new(0.0, 0.0, 800.0, 600.0), &particles);
    /// ```
    fn draw_particles(&mut self, _rect: Rect, _particle_data: &ParticleSystemData) {
        // Default implementation does nothing
        // GPU implementations override this to render particles
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Layer Management
    // ─────────────────────────────────────────────────────────────────────────

    /// Begin an offscreen layer
    fn push_layer(&mut self, config: LayerConfig);

    /// End the current offscreen layer
    fn pop_layer(&mut self);

    /// Sample from a named layer's output
    fn sample_layer(&mut self, id: LayerId, source_rect: Rect, dest_rect: Rect);

    // ─────────────────────────────────────────────────────────────────────────
    // Custom GPU passes
    // ─────────────────────────────────────────────────────────────────────────

    /// Schedule a user-defined GPU pass to run inline with this paint
    /// session.
    ///
    /// `viewport` is the rect (in local coordinates) the pass should
    /// clip its output to. Pass `Some(bounds.rect())` from inside a
    /// `canvas(|ctx, bounds| …)` closure to clip to the canvas's layout
    /// region, or `None` to inherit whatever clip is already on the
    /// stack (the GPU backend captures the current clip-stack AABB
    /// when `None`). Falling back further when no clip is pushed runs
    /// the pass against the full frame target.
    ///
    /// The default impl is a no-op so non-GPU contexts (mock test
    /// contexts, the recording context) silently skip the pass. The
    /// GPU-backed paint context overrides this to queue the pass for
    /// dispatch during composite.
    ///
    /// `GpuPassHook` is an opaque marker from `blinc_core` to keep this
    /// trait free of `wgpu` references. The concrete bridge type lives
    /// in `blinc_gpu` (`GpuPass`); construct one via
    /// `blinc_gpu::GpuPass::new(my_custom_pass)`. Takes `&dyn` (not
    /// `&mut dyn`) because canvas closures are `Fn`, not `FnMut`; the
    /// concrete `GpuPass` uses interior mutability so the user can hold
    /// it through a captured-by-move binding without `RefCell` /
    /// `Mutex` of their own.
    ///
    /// See the "Custom GPU passes" chapter of the book for the full
    /// pattern.
    fn run_gpu_pass(&mut self, _pass: &dyn GpuPassHook, _viewport: Option<Rect>) {}

    // ─────────────────────────────────────────────────────────────────────────
    // State Queries
    // ─────────────────────────────────────────────────────────────────────────

    /// Get the current viewport size
    fn viewport_size(&self) -> Size;

    /// Check if we're in a 3D context
    fn is_3d_context(&self) -> bool;

    /// Get the current opacity
    fn current_opacity(&self) -> f32;

    /// Get the current blend mode
    fn current_blend_mode(&self) -> BlendMode;
}

/// Opaque hook for user-defined GPU work scheduled through
/// [`DrawContext::run_gpu_pass`].
///
/// `blinc_core` doesn't know what's inside — only the concrete GPU paint
/// context (`blinc_gpu::GpuPaintContext`) does. The canonical bridge type
/// is `blinc_gpu::GpuPass`, which wraps any `CustomRenderPass` and
/// implements this trait.
///
/// Implementations should return `self` from `as_any`; the GPU layer
/// downcasts to its canonical wrapper to retrieve the underlying pass.
pub trait GpuPassHook: 'static {
    /// Bridge to `Any` for downcast inside the GPU backend.
    fn as_any(&self) -> &dyn core::any::Any;
}

/// Extension trait for DrawContext that provides ergonomic generic methods
///
/// These methods are implemented on concrete types and provide convenient
/// APIs using `impl Into<Brush>` for colors and brushes.
pub trait DrawContextExt: DrawContext {
    /// Fill a path with a color or brush
    fn fill<B: Into<Brush>>(&mut self, path: &Path, brush: B) {
        self.fill_path(path, brush.into());
    }

    /// Stroke a path with a color or brush
    fn stroke<B: Into<Brush>>(&mut self, path: &Path, stroke: &Stroke, brush: B) {
        self.stroke_path(path, stroke, brush.into());
    }

    /// Fill a rectangle with a color or brush
    fn fill_rounded_rect<B: Into<Brush>>(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        brush: B,
    ) {
        self.fill_rect(rect, corner_radius, brush.into());
    }

    /// Build SDF shapes with a closure (convenience wrapper)
    fn sdf<F: FnMut(&mut dyn SdfBuilder)>(&mut self, mut f: F) {
        self.sdf_build(&mut f);
    }

    /// Embed 2D content as a billboard (convenience wrapper)
    fn billboard<F: FnMut(&mut dyn DrawContext)>(
        &mut self,
        size: Size,
        transform: Mat4,
        facing: BillboardFacing,
        mut f: F,
    ) {
        self.billboard_draw(size, transform, facing, &mut f);
    }

    /// Embed a 3D viewport (convenience wrapper)
    fn viewport_3d<F: FnMut(&mut dyn DrawContext)>(
        &mut self,
        rect: Rect,
        camera: &Camera,
        mut f: F,
    ) {
        self.viewport_3d_draw(rect, camera, &mut f);
    }
}

// Blanket implementation for all DrawContext implementers
impl<T: DrawContext + ?Sized> DrawContextExt for T {}

// ─────────────────────────────────────────────────────────────────────────────
// Recording Draw Context
// ─────────────────────────────────────────────────────────────────────────────

/// A draw command that can be recorded and replayed
#[derive(Clone, Debug)]
pub enum DrawCommand {
    // State
    PushTransform(Transform),
    PopTransform,
    PushClip(ClipShape),
    PopClip,
    PushOpacity(f32),
    PopOpacity,
    PushBlendMode(BlendMode),
    PopBlendMode,

    // 2D Drawing
    FillPath {
        path: Path,
        brush: Brush,
    },
    StrokePath {
        path: Path,
        stroke: Stroke,
        brush: Brush,
    },
    FillRect {
        rect: Rect,
        corner_radius: CornerRadius,
        brush: Brush,
    },
    StrokeRect {
        rect: Rect,
        corner_radius: CornerRadius,
        stroke: Stroke,
        brush: Brush,
    },
    FillCircle {
        center: Point,
        radius: f32,
        brush: Brush,
    },
    StrokeCircle {
        center: Point,
        radius: f32,
        stroke: Stroke,
        brush: Brush,
    },
    DrawText {
        text: String,
        origin: Point,
        style: TextStyle,
    },
    DrawImage {
        image: ImageId,
        rect: Rect,
        options: ImageOptions,
    },
    DrawShadow {
        rect: Rect,
        corner_radius: CornerRadius,
        shadow: Shadow,
    },
    DrawInnerShadow {
        rect: Rect,
        corner_radius: CornerRadius,
        shadow: Shadow,
    },
    DrawCircleShadow {
        center: Point,
        radius: f32,
        shadow: Shadow,
    },
    DrawCircleInnerShadow {
        center: Point,
        radius: f32,
        shadow: Shadow,
    },

    // 3D
    SetCamera(Camera),
    DrawMesh {
        mesh: MeshId,
        material: MaterialId,
        transform: Mat4,
    },
    DrawMeshInstanced {
        mesh: MeshId,
        instances: Vec<MeshInstance>,
    },
    AddLight(Light),
    SetEnvironment(Environment),

    // Layer
    PushLayer(LayerConfig),
    PopLayer,
    SampleLayer {
        id: LayerId,
        source_rect: Rect,
        dest_rect: Rect,
    },
}

/// A draw context that records commands for later execution
#[derive(Debug, Default)]
pub struct RecordingContext {
    commands: Vec<DrawCommand>,
    transform_stack: Vec<Transform>,
    opacity_stack: Vec<f32>,
    blend_mode_stack: Vec<BlendMode>,
    viewport: Size,
    is_3d: bool,
}

impl RecordingContext {
    /// Create a new recording context
    pub fn new(viewport: Size) -> Self {
        Self {
            commands: Vec::new(),
            transform_stack: vec![Transform::identity()],
            opacity_stack: vec![1.0],
            blend_mode_stack: vec![BlendMode::Normal],
            viewport,
            is_3d: false,
        }
    }

    /// Get the recorded commands
    pub fn commands(&self) -> &[DrawCommand] {
        &self.commands
    }

    /// Take the recorded commands
    pub fn take_commands(&mut self) -> Vec<DrawCommand> {
        std::mem::take(&mut self.commands)
    }

    /// Clear all recorded commands
    pub fn clear(&mut self) {
        self.commands.clear();
        self.transform_stack = vec![Transform::identity()];
        self.opacity_stack = vec![1.0];
        self.blend_mode_stack = vec![BlendMode::Normal];
    }
}

impl DrawContext for RecordingContext {
    fn push_transform(&mut self, transform: Transform) {
        self.commands
            .push(DrawCommand::PushTransform(transform.clone()));
        self.transform_stack.push(transform);
    }

    fn pop_transform(&mut self) {
        self.commands.push(DrawCommand::PopTransform);
        if self.transform_stack.len() > 1 {
            self.transform_stack.pop();
        }
    }

    fn current_transform(&self) -> Transform {
        self.transform_stack.last().cloned().unwrap_or_default()
    }

    fn push_clip(&mut self, shape: ClipShape) {
        self.commands.push(DrawCommand::PushClip(shape));
    }

    fn pop_clip(&mut self) {
        self.commands.push(DrawCommand::PopClip);
    }

    fn push_opacity(&mut self, opacity: f32) {
        self.commands.push(DrawCommand::PushOpacity(opacity));
        let current = *self.opacity_stack.last().unwrap_or(&1.0);
        self.opacity_stack.push(current * opacity);
    }

    fn pop_opacity(&mut self) {
        self.commands.push(DrawCommand::PopOpacity);
        if self.opacity_stack.len() > 1 {
            self.opacity_stack.pop();
        }
    }

    fn push_blend_mode(&mut self, mode: BlendMode) {
        self.commands.push(DrawCommand::PushBlendMode(mode));
        self.blend_mode_stack.push(mode);
    }

    fn pop_blend_mode(&mut self) {
        self.commands.push(DrawCommand::PopBlendMode);
        if self.blend_mode_stack.len() > 1 {
            self.blend_mode_stack.pop();
        }
    }

    fn fill_path(&mut self, path: &Path, brush: Brush) {
        self.commands.push(DrawCommand::FillPath {
            path: path.clone(),
            brush,
        });
    }

    fn stroke_path(&mut self, path: &Path, stroke: &Stroke, brush: Brush) {
        self.commands.push(DrawCommand::StrokePath {
            path: path.clone(),
            stroke: stroke.clone(),
            brush,
        });
    }

    fn fill_rect(&mut self, rect: Rect, corner_radius: CornerRadius, brush: Brush) {
        self.commands.push(DrawCommand::FillRect {
            rect,
            corner_radius,
            brush,
        });
    }

    fn stroke_rect(
        &mut self,
        rect: Rect,
        corner_radius: CornerRadius,
        stroke: &Stroke,
        brush: Brush,
    ) {
        self.commands.push(DrawCommand::StrokeRect {
            rect,
            corner_radius,
            stroke: stroke.clone(),
            brush,
        });
    }

    fn fill_circle(&mut self, center: Point, radius: f32, brush: Brush) {
        self.commands.push(DrawCommand::FillCircle {
            center,
            radius,
            brush,
        });
    }

    fn stroke_circle(&mut self, center: Point, radius: f32, stroke: &Stroke, brush: Brush) {
        self.commands.push(DrawCommand::StrokeCircle {
            center,
            radius,
            stroke: stroke.clone(),
            brush,
        });
    }

    fn draw_text(&mut self, text: &str, origin: Point, style: &TextStyle) {
        self.commands.push(DrawCommand::DrawText {
            text: text.to_string(),
            origin,
            style: style.clone(),
        });
    }

    fn draw_image(&mut self, image: ImageId, rect: Rect, options: &ImageOptions) {
        self.commands.push(DrawCommand::DrawImage {
            image,
            rect,
            options: options.clone(),
        });
    }

    fn draw_shadow(&mut self, rect: Rect, corner_radius: CornerRadius, shadow: Shadow) {
        self.commands.push(DrawCommand::DrawShadow {
            rect,
            corner_radius,
            shadow,
        });
    }

    fn draw_inner_shadow(&mut self, rect: Rect, corner_radius: CornerRadius, shadow: Shadow) {
        self.commands.push(DrawCommand::DrawInnerShadow {
            rect,
            corner_radius,
            shadow,
        });
    }

    fn draw_circle_shadow(&mut self, center: Point, radius: f32, shadow: Shadow) {
        self.commands.push(DrawCommand::DrawCircleShadow {
            center,
            radius,
            shadow,
        });
    }

    fn draw_circle_inner_shadow(&mut self, center: Point, radius: f32, shadow: Shadow) {
        self.commands.push(DrawCommand::DrawCircleInnerShadow {
            center,
            radius,
            shadow,
        });
    }

    fn sdf_build(&mut self, f: &mut dyn FnMut(&mut dyn SdfBuilder)) {
        let mut builder = RecordingSdfBuilder::new();
        f(&mut builder);

        // Process shadows first (they render behind fills)
        for (shape_id, shadow) in &builder.shadows {
            if let Some(shape) = builder.shapes.get(shape_id.0 as usize) {
                match shape {
                    SdfShape::Rect {
                        rect,
                        corner_radius,
                    } => {
                        self.draw_shadow(*rect, *corner_radius, *shadow);
                    }
                    SdfShape::Circle { center, radius } => {
                        // Use proper circle shadow for radially symmetric blur
                        self.draw_circle_shadow(*center, *radius, *shadow);
                    }
                    SdfShape::Ellipse { center, radii } => {
                        let rect =
                            Rect::from_center(*center, Size::new(radii.x * 2.0, radii.y * 2.0));
                        // Use smaller radius for corner approximation
                        self.draw_shadow(rect, radii.x.min(radii.y).into(), *shadow);
                    }
                    _ => {
                        // Complex shapes: use bounding box approximation
                    }
                }
            }
        }

        // Process fills
        for (shape_id, brush) in builder.fills {
            if let Some(shape) = builder.shapes.get(shape_id.0 as usize) {
                match shape {
                    SdfShape::Rect {
                        rect,
                        corner_radius,
                    } => {
                        self.fill_rect(*rect, *corner_radius, brush);
                    }
                    SdfShape::Circle { center, radius } => {
                        self.fill_circle(*center, *radius, brush);
                    }
                    SdfShape::Ellipse { center, radii } => {
                        // Ellipse as a path (approximated with bezier curves)
                        let path = Path::circle(*center, radii.x); // Simplified: use as circle
                        self.fill_path(&path, brush);
                    }
                    SdfShape::Line { from, to, width } => {
                        // Line as a stroked path
                        let path = Path::line(*from, *to);
                        self.stroke_path(&path, &Stroke::new(*width), brush);
                    }
                    _ => {
                        // Complex SDF shapes need GPU-side evaluation
                    }
                }
            }
        }

        // Process strokes
        for (shape_id, stroke, brush) in builder.strokes {
            if let Some(shape) = builder.shapes.get(shape_id.0 as usize) {
                match shape {
                    SdfShape::Rect {
                        rect,
                        corner_radius,
                    } => {
                        self.stroke_rect(*rect, *corner_radius, &stroke, brush);
                    }
                    SdfShape::Circle { center, radius } => {
                        self.stroke_circle(*center, *radius, &stroke, brush);
                    }
                    SdfShape::Ellipse { center, radii } => {
                        let path = Path::circle(*center, radii.x); // Simplified
                        self.stroke_path(&path, &stroke, brush);
                    }
                    SdfShape::Line { from, to, .. } => {
                        let path = Path::line(*from, *to);
                        self.stroke_path(&path, &stroke, brush);
                    }
                    _ => {
                        // Complex SDF shapes need GPU-side evaluation
                    }
                }
            }
        }
    }

    fn set_camera(&mut self, camera: &Camera) {
        self.commands.push(DrawCommand::SetCamera(camera.clone()));
        self.is_3d = true;
    }

    fn draw_mesh(&mut self, mesh: MeshId, material: MaterialId, transform: Mat4) {
        self.commands.push(DrawCommand::DrawMesh {
            mesh,
            material,
            transform,
        });
    }

    fn draw_mesh_instanced(&mut self, mesh: MeshId, instances: &[MeshInstance]) {
        self.commands.push(DrawCommand::DrawMeshInstanced {
            mesh,
            instances: instances.to_vec(),
        });
    }

    fn add_light(&mut self, light: Light) {
        self.commands.push(DrawCommand::AddLight(light));
    }

    fn set_environment(&mut self, env: &Environment) {
        self.commands.push(DrawCommand::SetEnvironment(env.clone()));
    }

    fn billboard_draw(
        &mut self,
        _size: Size,
        _transform: Mat4,
        _facing: BillboardFacing,
        f: &mut dyn FnMut(&mut dyn DrawContext),
    ) {
        // Create a sub-context for the billboard content
        let mut sub_ctx = RecordingContext::new(self.viewport);
        f(&mut sub_ctx);
        // In a real implementation, this would record the billboard as a nested layer
        self.commands.extend(sub_ctx.commands);
    }

    fn viewport_3d_draw(
        &mut self,
        _rect: Rect,
        camera: &Camera,
        f: &mut dyn FnMut(&mut dyn DrawContext),
    ) {
        // Set up 3D context
        let was_3d = self.is_3d;
        self.set_camera(camera);

        // Execute 3D drawing
        f(self);

        // Restore 2D context
        self.is_3d = was_3d;
    }

    fn push_layer(&mut self, config: LayerConfig) {
        self.commands.push(DrawCommand::PushLayer(config));
    }

    fn pop_layer(&mut self) {
        self.commands.push(DrawCommand::PopLayer);
    }

    fn sample_layer(&mut self, id: LayerId, source_rect: Rect, dest_rect: Rect) {
        self.commands.push(DrawCommand::SampleLayer {
            id,
            source_rect,
            dest_rect,
        });
    }

    fn viewport_size(&self) -> Size {
        self.viewport
    }

    fn is_3d_context(&self) -> bool {
        self.is_3d
    }

    fn current_opacity(&self) -> f32 {
        *self.opacity_stack.last().unwrap_or(&1.0)
    }

    fn current_blend_mode(&self) -> BlendMode {
        self.blend_mode_stack
            .last()
            .copied()
            .unwrap_or(BlendMode::Normal)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Recording SDF Builder
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum SdfShape {
    Rect {
        rect: Rect,
        corner_radius: CornerRadius,
    },
    Circle {
        center: Point,
        radius: f32,
    },
    Ellipse {
        center: Point,
        radii: Vec2,
    },
    Line {
        from: Point,
        to: Point,
        width: f32,
    },
    Arc {
        center: Point,
        radius: f32,
        start: f32,
        end: f32,
        width: f32,
    },
    QuadBezier {
        p0: Point,
        p1: Point,
        p2: Point,
        width: f32,
    },
    Union {
        a: ShapeId,
        b: ShapeId,
    },
    Subtract {
        a: ShapeId,
        b: ShapeId,
    },
    Intersect {
        a: ShapeId,
        b: ShapeId,
    },
    SmoothUnion {
        a: ShapeId,
        b: ShapeId,
        radius: f32,
    },
    SmoothSubtract {
        a: ShapeId,
        b: ShapeId,
        radius: f32,
    },
    SmoothIntersect {
        a: ShapeId,
        b: ShapeId,
        radius: f32,
    },
    Round {
        shape: ShapeId,
        radius: f32,
    },
    Outline {
        shape: ShapeId,
        width: f32,
    },
    Offset {
        shape: ShapeId,
        distance: f32,
    },
}

struct RecordingSdfBuilder {
    shapes: Vec<SdfShape>,
    fills: Vec<(ShapeId, Brush)>,
    strokes: Vec<(ShapeId, Stroke, Brush)>,
    shadows: Vec<(ShapeId, Shadow)>,
}

impl RecordingSdfBuilder {
    fn new() -> Self {
        Self {
            shapes: Vec::new(),
            fills: Vec::new(),
            strokes: Vec::new(),
            shadows: Vec::new(),
        }
    }

    fn add_shape(&mut self, shape: SdfShape) -> ShapeId {
        let id = ShapeId(self.shapes.len() as u32);
        self.shapes.push(shape);
        id
    }
}

impl SdfBuilder for RecordingSdfBuilder {
    fn rect(&mut self, rect: Rect, corner_radius: CornerRadius) -> ShapeId {
        self.add_shape(SdfShape::Rect {
            rect,
            corner_radius,
        })
    }

    fn circle(&mut self, center: Point, radius: f32) -> ShapeId {
        self.add_shape(SdfShape::Circle { center, radius })
    }

    fn ellipse(&mut self, center: Point, radii: Vec2) -> ShapeId {
        self.add_shape(SdfShape::Ellipse { center, radii })
    }

    fn line(&mut self, from: Point, to: Point, width: f32) -> ShapeId {
        self.add_shape(SdfShape::Line { from, to, width })
    }

    fn arc(&mut self, center: Point, radius: f32, start: f32, end: f32, width: f32) -> ShapeId {
        self.add_shape(SdfShape::Arc {
            center,
            radius,
            start,
            end,
            width,
        })
    }

    fn quad_bezier(&mut self, p0: Point, p1: Point, p2: Point, width: f32) -> ShapeId {
        self.add_shape(SdfShape::QuadBezier { p0, p1, p2, width })
    }

    fn union(&mut self, a: ShapeId, b: ShapeId) -> ShapeId {
        self.add_shape(SdfShape::Union { a, b })
    }

    fn subtract(&mut self, a: ShapeId, b: ShapeId) -> ShapeId {
        self.add_shape(SdfShape::Subtract { a, b })
    }

    fn intersect(&mut self, a: ShapeId, b: ShapeId) -> ShapeId {
        self.add_shape(SdfShape::Intersect { a, b })
    }

    fn smooth_union(&mut self, a: ShapeId, b: ShapeId, radius: f32) -> ShapeId {
        self.add_shape(SdfShape::SmoothUnion { a, b, radius })
    }

    fn smooth_subtract(&mut self, a: ShapeId, b: ShapeId, radius: f32) -> ShapeId {
        self.add_shape(SdfShape::SmoothSubtract { a, b, radius })
    }

    fn smooth_intersect(&mut self, a: ShapeId, b: ShapeId, radius: f32) -> ShapeId {
        self.add_shape(SdfShape::SmoothIntersect { a, b, radius })
    }

    fn round(&mut self, shape: ShapeId, radius: f32) -> ShapeId {
        self.add_shape(SdfShape::Round { shape, radius })
    }

    fn outline(&mut self, shape: ShapeId, width: f32) -> ShapeId {
        self.add_shape(SdfShape::Outline { shape, width })
    }

    fn offset(&mut self, shape: ShapeId, distance: f32) -> ShapeId {
        self.add_shape(SdfShape::Offset { shape, distance })
    }

    fn fill(&mut self, shape: ShapeId, brush: Brush) {
        self.fills.push((shape, brush));
    }

    fn stroke(&mut self, shape: ShapeId, stroke: &Stroke, brush: Brush) {
        self.strokes.push((shape, stroke.clone(), brush));
    }

    fn shadow(&mut self, shape: ShapeId, shadow: Shadow) {
        self.shadows.push((shape, shadow));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recording_context() {
        let mut ctx = RecordingContext::new(Size::new(800.0, 600.0));

        ctx.push_transform(Transform::translate(10.0, 20.0));
        ctx.fill_rect(
            Rect::new(0.0, 0.0, 100.0, 50.0),
            8.0.into(),
            Color::BLUE.into(),
        );
        ctx.draw_text("Hello", Point::new(10.0, 30.0), &TextStyle::default());
        ctx.pop_transform();

        assert_eq!(ctx.commands().len(), 4);
    }

    #[test]
    fn test_path_builder() {
        let path = Path::new()
            .move_to(0.0, 0.0)
            .line_to(100.0, 0.0)
            .line_to(100.0, 100.0)
            .line_to(0.0, 100.0)
            .close();

        assert_eq!(path.commands().len(), 5);
    }

    #[test]
    fn test_path_shortcuts() {
        let rect = Path::rect(Rect::new(0.0, 0.0, 100.0, 50.0));
        assert_eq!(rect.commands().len(), 5); // move + 3 lines + close

        let circle = Path::circle(Point::new(50.0, 50.0), 25.0);
        assert!(!circle.is_empty());
    }

    #[test]
    fn test_transform_stack() {
        let mut ctx = RecordingContext::new(Size::new(800.0, 600.0));

        assert!(ctx.current_transform().is_2d());

        ctx.push_transform(Transform::translate(10.0, 20.0));
        ctx.push_transform(Transform::scale(2.0, 2.0));

        ctx.pop_transform();
        ctx.pop_transform();

        // Should not panic when popping past the root
        ctx.pop_transform();
    }

    #[test]
    fn test_opacity_stack() {
        let mut ctx = RecordingContext::new(Size::new(800.0, 600.0));

        assert_eq!(ctx.current_opacity(), 1.0);

        ctx.push_opacity(0.5);
        assert_eq!(ctx.current_opacity(), 0.5);

        ctx.push_opacity(0.5);
        assert_eq!(ctx.current_opacity(), 0.25); // 0.5 * 0.5

        ctx.pop_opacity();
        assert_eq!(ctx.current_opacity(), 0.5);
    }

    #[test]
    fn test_sdf_builder() {
        let mut ctx = RecordingContext::new(Size::new(800.0, 600.0));

        ctx.sdf(|sdf| {
            let rect = sdf.rect(Rect::new(0.0, 0.0, 100.0, 50.0), 8.0.into());
            sdf.fill(rect, Color::BLUE.into());

            let circle = sdf.circle(Point::new(50.0, 50.0), 25.0);
            sdf.fill(circle, Color::RED.into());
        });

        // Should have recorded the fills as rect/circle commands
        assert!(!ctx.commands().is_empty());
    }

    #[test]
    fn test_stroke_configuration() {
        let stroke = Stroke::new(2.0)
            .with_cap(LineCap::Round)
            .with_join(LineJoin::Bevel)
            .with_dash(vec![5.0, 3.0], 0.0);

        assert_eq!(stroke.width, 2.0);
        assert_eq!(stroke.cap, LineCap::Round);
        assert_eq!(stroke.join, LineJoin::Bevel);
        assert_eq!(stroke.dash.len(), 2);
    }

    #[test]
    fn test_text_style() {
        let style = TextStyle::new(16.0)
            .with_color(Color::WHITE)
            .with_weight(FontWeight::Bold)
            .with_family("Arial");

        assert_eq!(style.size, 16.0);
        assert_eq!(style.weight, FontWeight::Bold);
        assert_eq!(style.family, "Arial");
    }

    #[test]
    fn test_draw_context_ext() {
        let mut ctx = RecordingContext::new(Size::new(800.0, 600.0));

        // Test the extension trait methods
        let path = Path::rect(Rect::new(0.0, 0.0, 100.0, 50.0));
        ctx.fill(&path, Color::BLUE);
        ctx.fill_rounded_rect(Rect::new(10.0, 10.0, 80.0, 30.0), 4.0.into(), Color::RED);

        assert_eq!(ctx.commands().len(), 2);
    }
}
