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
    Environment, LayerId, Light, Mat4, ParticleSystemData, Point, Rect, Sdf3DViewport, Shadow,
    Size, Vec2,
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
        let k = 0.5522847498; // Magic number for cubic Bézier circle approximation
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
        let k = 0.5522847498;

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

/// Handle to a loaded mesh
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MeshId(pub u64);

/// Handle to a material
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialId(pub u64);

/// Mesh instance for instanced rendering
#[derive(Clone, Debug)]
pub struct MeshInstance {
    pub transform: Mat4,
    pub material: Option<MaterialId>,
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

    // ─────────────────────────────────────────────────────────────────────────
    // 2D Drawing Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Fill a path with a brush
    fn fill_path(&mut self, path: &Path, brush: Brush);

    /// Stroke a path
    fn stroke_path(&mut self, path: &Path, stroke: &Stroke, brush: Brush);

    /// Fill a rectangle (convenience method)
    fn fill_rect(&mut self, rect: Rect, corner_radius: CornerRadius, brush: Brush);

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

    /// Draw a mesh with a material
    fn draw_mesh(&mut self, mesh: MeshId, material: MaterialId, transform: Mat4);

    /// Draw instanced meshes
    fn draw_mesh_instanced(&mut self, mesh: MeshId, instances: &[MeshInstance]);

    /// Add a light to the scene
    fn add_light(&mut self, light: Light);

    /// Set the environment (skybox, IBL)
    fn set_environment(&mut self, env: &Environment);

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
                        self.draw_shadow(*rect, *corner_radius, shadow.clone());
                    }
                    SdfShape::Circle { center, radius } => {
                        // Use proper circle shadow for radially symmetric blur
                        self.draw_circle_shadow(*center, *radius, shadow.clone());
                    }
                    SdfShape::Ellipse { center, radii } => {
                        let rect =
                            Rect::from_center(*center, Size::new(radii.x * 2.0, radii.y * 2.0));
                        // Use smaller radius for corner approximation
                        self.draw_shadow(rect, radii.x.min(radii.y).into(), shadow.clone());
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
