//! Layer Model for BLINC Canvas Architecture
//!
//! All visual content is represented as composable layers rendered to a unified canvas.
//! This module provides the core types for representing layers, scene graphs, and
//! dimension bridging between 2D UI, 2D canvas drawing, and 3D scenes.
//!
//! # Layer Types
//!
//! - **Ui**: 2D primitives rendered with SDF shaders
//! - **Canvas2D**: Vector drawing with paths and brushes
//! - **Scene3D**: 3D scene with meshes, materials, and lighting
//! - **Composition**: Stack, Transform, Clip, Opacity layers
//! - **Bridging**: Billboard (2D in 3D), Viewport3D (3D in 2D)

use std::collections::HashMap;

// ─────────────────────────────────────────────────────────────────────────────
// Core Geometry Types
// ─────────────────────────────────────────────────────────────────────────────

/// 2D point
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// 2D size
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Size = Size {
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    /// Convert to a Rect at the origin (0, 0)
    pub const fn to_rect(self) -> Rect {
        Rect {
            origin: Point::ZERO,
            size: self,
        }
    }
}

impl From<Size> for Rect {
    /// Convert Size to Rect at origin (0, 0)
    fn from(size: Size) -> Self {
        Rect {
            origin: Point::ZERO,
            size,
        }
    }
}

/// 2D rectangle
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const ZERO: Rect = Rect {
        origin: Point::ZERO,
        size: Size::ZERO,
    };

    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    pub fn from_origin_size(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    pub fn x(&self) -> f32 {
        self.origin.x
    }

    pub fn y(&self) -> f32 {
        self.origin.y
    }

    pub fn width(&self) -> f32 {
        self.size.width
    }

    pub fn height(&self) -> f32 {
        self.size.height
    }

    pub fn center(&self) -> Point {
        Point::new(
            self.origin.x + self.size.width / 2.0,
            self.origin.y + self.size.height / 2.0,
        )
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.origin.x
            && point.x <= self.origin.x + self.size.width
            && point.y >= self.origin.y
            && point.y <= self.origin.y + self.size.height
    }

    /// Get the size of this rect
    pub fn size(&self) -> Size {
        self.size
    }

    /// Offset the rect by a delta
    pub fn offset(&self, dx: f32, dy: f32) -> Self {
        Rect {
            origin: Point::new(self.origin.x + dx, self.origin.y + dy),
            size: self.size,
        }
    }

    /// Inset the rect by a delta (shrink from all sides)
    pub fn inset(&self, dx: f32, dy: f32) -> Self {
        Rect {
            origin: Point::new(self.origin.x + dx, self.origin.y + dy),
            size: Size::new(
                (self.size.width - 2.0 * dx).max(0.0),
                (self.size.height - 2.0 * dy).max(0.0),
            ),
        }
    }

    /// Create a rect from center point and size
    pub fn from_center(center: Point, size: Size) -> Self {
        Rect {
            origin: Point::new(center.x - size.width / 2.0, center.y - size.height / 2.0),
            size,
        }
    }

    /// Create a rect from two corner points
    pub fn from_points(p1: Point, p2: Point) -> Self {
        let min_x = p1.x.min(p2.x);
        let min_y = p1.y.min(p2.y);
        let max_x = p1.x.max(p2.x);
        let max_y = p1.y.max(p2.y);
        Rect {
            origin: Point::new(min_x, min_y),
            size: Size::new(max_x - min_x, max_y - min_y),
        }
    }

    /// Get the union of two rects (smallest rect containing both)
    pub fn union(&self, other: &Rect) -> Self {
        let min_x = self.origin.x.min(other.origin.x);
        let min_y = self.origin.y.min(other.origin.y);
        let max_x = (self.origin.x + self.size.width).max(other.origin.x + other.size.width);
        let max_y = (self.origin.y + self.size.height).max(other.origin.y + other.size.height);
        Rect {
            origin: Point::new(min_x, min_y),
            size: Size::new(max_x - min_x, max_y - min_y),
        }
    }

    /// Expand rect to include a point
    pub fn expand_to_include(&self, point: Point) -> Self {
        let min_x = self.origin.x.min(point.x);
        let min_y = self.origin.y.min(point.y);
        let max_x = (self.origin.x + self.size.width).max(point.x);
        let max_y = (self.origin.y + self.size.height).max(point.y);
        Rect {
            origin: Point::new(min_x, min_y),
            size: Size::new(max_x - min_x, max_y - min_y),
        }
    }

    /// Check if this rect intersects with another
    ///
    /// Returns true if the two rects overlap at any point.
    pub fn intersects(&self, other: &Rect) -> bool {
        let self_right = self.origin.x + self.size.width;
        let self_bottom = self.origin.y + self.size.height;
        let other_right = other.origin.x + other.size.width;
        let other_bottom = other.origin.y + other.size.height;

        self.origin.x < other_right
            && self_right > other.origin.x
            && self.origin.y < other_bottom
            && self_bottom > other.origin.y
    }

    /// Get the intersection of two rects (if they overlap)
    ///
    /// Returns None if the rects don't overlap.
    pub fn intersection(&self, other: &Rect) -> Option<Self> {
        if !self.intersects(other) {
            return None;
        }

        let x = self.origin.x.max(other.origin.x);
        let y = self.origin.y.max(other.origin.y);
        let right = (self.origin.x + self.size.width).min(other.origin.x + other.size.width);
        let bottom = (self.origin.y + self.size.height).min(other.origin.y + other.size.height);

        Some(Rect {
            origin: Point::new(x, y),
            size: Size::new(right - x, bottom - y),
        })
    }
}

/// 2D vector
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };
    pub const ONE: Vec2 = Vec2 { x: 1.0, y: 1.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn normalize(&self) -> Self {
        let len = self.length();
        if len > 0.0 {
            Self::new(self.x / len, self.y / len)
        } else {
            Self::ZERO
        }
    }
}

/// 3D vector
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    pub const ONE: Vec3 = Vec3 {
        x: 1.0,
        y: 1.0,
        z: 1.0,
    };
    pub const UP: Vec3 = Vec3 {
        x: 0.0,
        y: 1.0,
        z: 0.0,
    };
    pub const FORWARD: Vec3 = Vec3 {
        x: 0.0,
        y: 0.0,
        z: -1.0,
    };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn normalize(&self) -> Self {
        let len = self.length();
        if len > 0.0 {
            Self::new(self.x / len, self.y / len, self.z / len)
        } else {
            Self::ZERO
        }
    }

    pub fn dot(&self, other: Vec3) -> f32 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }

    pub fn cross(&self, other: Vec3) -> Vec3 {
        Vec3::new(
            self.y * other.z - self.z * other.y,
            self.z * other.x - self.x * other.z,
            self.x * other.y - self.y * other.x,
        )
    }
}

/// 4x4 transformation matrix (column-major)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub cols: [[f32; 4]; 4],
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Mat4 = Mat4 {
        cols: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
    };

    pub fn translation(x: f32, y: f32, z: f32) -> Self {
        Self {
            cols: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [x, y, z, 1.0],
            ],
        }
    }

    pub fn scale(x: f32, y: f32, z: f32) -> Self {
        Self {
            cols: [
                [x, 0.0, 0.0, 0.0],
                [0.0, y, 0.0, 0.0],
                [0.0, 0.0, z, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    pub fn rotation_y(angle: f32) -> Self {
        let c = angle.cos();
        let s = angle.sin();
        Self {
            cols: [
                [c, 0.0, -s, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [s, 0.0, c, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    /// Multiply two matrices
    pub fn mul(&self, other: &Mat4) -> Mat4 {
        let mut result = [[0.0f32; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                for k in 0..4 {
                    result[i][j] += self.cols[k][j] * other.cols[i][k];
                }
            }
        }
        Mat4 { cols: result }
    }
}

/// 2D affine transformation
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine2D {
    /// Matrix elements [a, b, c, d, tx, ty]
    /// | a  c  tx |
    /// | b  d  ty |
    /// | 0  0   1 |
    pub elements: [f32; 6],
}

impl Default for Affine2D {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine2D {
    pub const IDENTITY: Affine2D = Affine2D {
        elements: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };

    pub fn translation(x: f32, y: f32) -> Self {
        Self {
            elements: [1.0, 0.0, 0.0, 1.0, x, y],
        }
    }

    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            elements: [sx, 0.0, 0.0, sy, 0.0, 0.0],
        }
    }

    pub fn rotation(angle: f32) -> Self {
        let c = angle.cos();
        let s = angle.sin();
        Self {
            elements: [c, s, -s, c, 0.0, 0.0],
        }
    }

    pub fn transform_point(&self, point: Point) -> Point {
        let [a, b, c, d, tx, ty] = self.elements;
        Point::new(
            a * point.x + c * point.y + tx,
            b * point.x + d * point.y + ty,
        )
    }

    /// Concatenate this transform with another (self * other)
    /// The resulting transform first applies `other`, then `self`.
    pub fn then(&self, other: &Affine2D) -> Affine2D {
        let [a1, b1, c1, d1, tx1, ty1] = self.elements;
        let [a2, b2, c2, d2, tx2, ty2] = other.elements;

        // Matrix multiplication for 2D affine transforms:
        // [a1 c1 tx1]   [a2 c2 tx2]
        // [b1 d1 ty1] * [b2 d2 ty2]
        // [0  0  1  ]   [0  0  1  ]
        Affine2D {
            elements: [
                a1 * a2 + c1 * b2,         // a
                b1 * a2 + d1 * b2,         // b
                a1 * c2 + c1 * d2,         // c
                b1 * c2 + d1 * d2,         // d
                a1 * tx2 + c1 * ty2 + tx1, // tx
                b1 * tx2 + d1 * ty2 + ty1, // ty
            ],
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Color and Visual Types
// ─────────────────────────────────────────────────────────────────────────────

/// RGBA color (linear space)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Color = Color::rgb(1.0, 1.0, 1.0);
    pub const BLACK: Color = Color::rgb(0.0, 0.0, 0.0);
    pub const RED: Color = Color::rgb(1.0, 0.0, 0.0);
    pub const GREEN: Color = Color::rgb(0.0, 1.0, 0.0);
    pub const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
    pub const YELLOW: Color = Color::rgb(1.0, 1.0, 0.0);
    pub const CYAN: Color = Color::rgb(0.0, 1.0, 1.0);
    pub const MAGENTA: Color = Color::rgb(1.0, 0.0, 1.0);
    pub const PURPLE: Color = Color::rgb(0.5, 0.0, 0.5);
    pub const ORANGE: Color = Color::rgb(1.0, 0.5, 0.0);
    pub const GRAY: Color = Color::rgb(0.5, 0.5, 0.5);
    pub const TRANSPARENT: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn from_hex(hex: u32) -> Self {
        let r = ((hex >> 16) & 0xFF) as f32 / 255.0;
        let g = ((hex >> 8) & 0xFF) as f32 / 255.0;
        let b = (hex & 0xFF) as f32 / 255.0;
        Self::rgb(r, g, b)
    }

    pub fn with_alpha(mut self, alpha: f32) -> Self {
        self.a = alpha;
        self
    }

    pub fn to_array(&self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Linear interpolation between two colors
    pub fn lerp(a: &Color, b: &Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        Color {
            r: a.r + (b.r - a.r) * t,
            g: a.g + (b.g - a.g) * t,
            b: a.b + (b.b - a.b) * t,
            a: a.a + (b.a - a.a) * t,
        }
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

/// Gradient stop
#[derive(Clone, Copy, Debug)]
pub struct GradientStop {
    /// Position along the gradient (0.0 to 1.0)
    pub offset: f32,
    /// Color at this stop
    pub color: Color,
}

impl GradientStop {
    /// Create a new gradient stop
    pub fn new(offset: f32, color: Color) -> Self {
        Self {
            offset: offset.clamp(0.0, 1.0),
            color,
        }
    }
}

/// Gradient coordinate space
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GradientSpace {
    /// Coordinates are in user/world space (absolute pixels)
    #[default]
    UserSpace,
    /// Coordinates are relative to the bounding box (0.0-1.0)
    ObjectBoundingBox,
}

/// Gradient spread method for areas outside the gradient
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GradientSpread {
    /// Pad with the end colors
    #[default]
    Pad,
    /// Reflect the gradient
    Reflect,
    /// Repeat the gradient
    Repeat,
}

/// Gradient type
#[derive(Clone, Debug)]
pub enum Gradient {
    /// Linear gradient between two points
    Linear {
        /// Start point
        start: Point,
        /// End point
        end: Point,
        /// Color stops (should be sorted by offset)
        stops: Vec<GradientStop>,
        /// Coordinate space interpretation
        space: GradientSpace,
        /// Spread method
        spread: GradientSpread,
    },
    /// Radial gradient from center outward
    Radial {
        /// Center point
        center: Point,
        /// Radius
        radius: f32,
        /// Optional focal point (if None, same as center)
        focal: Option<Point>,
        /// Color stops (should be sorted by offset)
        stops: Vec<GradientStop>,
        /// Coordinate space interpretation
        space: GradientSpace,
        /// Spread method
        spread: GradientSpread,
    },
    /// Conic/angular gradient around a center point
    Conic {
        /// Center point
        center: Point,
        /// Start angle in radians
        start_angle: f32,
        /// Color stops (should be sorted by offset)
        stops: Vec<GradientStop>,
        /// Coordinate space interpretation
        space: GradientSpace,
    },
}

impl Gradient {
    /// Create a simple linear gradient with two colors
    pub fn linear(start: Point, end: Point, from: Color, to: Color) -> Self {
        Gradient::Linear {
            start,
            end,
            stops: vec![GradientStop::new(0.0, from), GradientStop::new(1.0, to)],
            space: GradientSpace::UserSpace,
            spread: GradientSpread::Pad,
        }
    }

    /// Create a linear gradient with multiple stops
    pub fn linear_with_stops(start: Point, end: Point, stops: Vec<GradientStop>) -> Self {
        Gradient::Linear {
            start,
            end,
            stops,
            space: GradientSpace::UserSpace,
            spread: GradientSpread::Pad,
        }
    }

    /// Create a simple radial gradient with two colors
    pub fn radial(center: Point, radius: f32, from: Color, to: Color) -> Self {
        Gradient::Radial {
            center,
            radius,
            focal: None,
            stops: vec![GradientStop::new(0.0, from), GradientStop::new(1.0, to)],
            space: GradientSpace::UserSpace,
            spread: GradientSpread::Pad,
        }
    }

    /// Create a radial gradient with multiple stops
    pub fn radial_with_stops(center: Point, radius: f32, stops: Vec<GradientStop>) -> Self {
        Gradient::Radial {
            center,
            radius,
            focal: None,
            stops,
            space: GradientSpace::UserSpace,
            spread: GradientSpread::Pad,
        }
    }

    /// Create a conic gradient with two colors
    pub fn conic(center: Point, from: Color, to: Color) -> Self {
        Gradient::Conic {
            center,
            start_angle: 0.0,
            stops: vec![GradientStop::new(0.0, from), GradientStop::new(1.0, to)],
            space: GradientSpace::UserSpace,
        }
    }

    /// Get the gradient stops
    pub fn stops(&self) -> &[GradientStop] {
        match self {
            Gradient::Linear { stops, .. } => stops,
            Gradient::Radial { stops, .. } => stops,
            Gradient::Conic { stops, .. } => stops,
        }
    }

    /// Get the first color in the gradient (or BLACK if no stops)
    pub fn first_color(&self) -> Color {
        self.stops()
            .first()
            .map(|s| s.color)
            .unwrap_or(Color::BLACK)
    }

    /// Get the last color in the gradient (or BLACK if no stops)
    pub fn last_color(&self) -> Color {
        self.stops().last().map(|s| s.color).unwrap_or(Color::BLACK)
    }
}

/// Image fill mode for background images
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ImageFit {
    /// Scale image to fill container, cropping if necessary (CSS: cover)
    #[default]
    Cover,
    /// Scale image to fit within container (CSS: contain)
    Contain,
    /// Stretch image to fill container exactly (CSS: fill)
    Fill,
    /// Tile the image to fill the container
    Tile,
}

/// Image alignment within container
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImagePosition {
    /// Horizontal position (0.0 = left, 0.5 = center, 1.0 = right)
    pub x: f32,
    /// Vertical position (0.0 = top, 0.5 = center, 1.0 = bottom)
    pub y: f32,
}

impl ImagePosition {
    pub const CENTER: Self = Self { x: 0.5, y: 0.5 };
    pub const TOP_LEFT: Self = Self { x: 0.0, y: 0.0 };
    pub const TOP_CENTER: Self = Self { x: 0.5, y: 0.0 };
    pub const TOP_RIGHT: Self = Self { x: 1.0, y: 0.0 };
    pub const CENTER_LEFT: Self = Self { x: 0.0, y: 0.5 };
    pub const CENTER_RIGHT: Self = Self { x: 1.0, y: 0.5 };
    pub const BOTTOM_LEFT: Self = Self { x: 0.0, y: 1.0 };
    pub const BOTTOM_CENTER: Self = Self { x: 0.5, y: 1.0 };
    pub const BOTTOM_RIGHT: Self = Self { x: 1.0, y: 1.0 };

    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// Image brush for background fills
#[derive(Clone, Debug)]
pub struct ImageBrush {
    /// Path to the image (relative to assets root or absolute)
    pub source: String,
    /// How to fit the image in the container
    pub fit: ImageFit,
    /// Position of the image within the container
    pub position: ImagePosition,
    /// Opacity (0.0 = transparent, 1.0 = opaque)
    pub opacity: f32,
    /// Tint color (multiplied with image)
    pub tint: Color,
}

impl ImageBrush {
    /// Create a new image brush with default settings
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            fit: ImageFit::Cover,
            position: ImagePosition::CENTER,
            opacity: 1.0,
            tint: Color::WHITE,
        }
    }

    /// Set the fit mode
    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    /// Set cover fit (scales to fill, may crop)
    pub fn cover(self) -> Self {
        self.fit(ImageFit::Cover)
    }

    /// Set contain fit (scales to fit, may letterbox)
    pub fn contain(self) -> Self {
        self.fit(ImageFit::Contain)
    }

    /// Set fill fit (stretches to fill exactly)
    pub fn fill(self) -> Self {
        self.fit(ImageFit::Fill)
    }

    /// Set tile fit (repeats the image)
    pub fn tile(self) -> Self {
        self.fit(ImageFit::Tile)
    }

    /// Set the position
    pub fn position(mut self, position: ImagePosition) -> Self {
        self.position = position;
        self
    }

    /// Center the image
    pub fn center(self) -> Self {
        self.position(ImagePosition::CENTER)
    }

    /// Set opacity
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    /// Set tint color
    pub fn tint(mut self, color: Color) -> Self {
        self.tint = color;
        self
    }
}

/// Brush for filling shapes
#[derive(Clone, Debug)]
pub enum Brush {
    Solid(Color),
    Gradient(Gradient),
    /// Glass/frosted blur effect - blurs content behind the shape
    Glass(GlassStyle),
    /// Pure backdrop blur effect - blurs content behind without glass styling
    Blur(BlurStyle),
    /// Image fill for backgrounds
    Image(ImageBrush),
}

impl From<Color> for Brush {
    fn from(color: Color) -> Self {
        Brush::Solid(color)
    }
}

impl From<GlassStyle> for Brush {
    fn from(style: GlassStyle) -> Self {
        Brush::Glass(style)
    }
}

impl From<ImageBrush> for Brush {
    fn from(brush: ImageBrush) -> Self {
        Brush::Image(brush)
    }
}

impl From<BlurStyle> for Brush {
    fn from(style: BlurStyle) -> Self {
        Brush::Blur(style)
    }
}

/// Blend mode for layer composition
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
}

/// Corner radii for rounded rectangles
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CornerRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl CornerRadius {
    pub const ZERO: CornerRadius = CornerRadius {
        top_left: 0.0,
        top_right: 0.0,
        bottom_right: 0.0,
        bottom_left: 0.0,
    };

    /// Create a corner radius with different values for each corner.
    /// Order: top_left, top_right, bottom_right, bottom_left (clockwise from top-left)
    pub fn new(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> Self {
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    pub fn uniform(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    pub fn to_array(&self) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }

    /// Check if all corner radii are the same
    pub fn is_uniform(&self) -> bool {
        self.top_left == self.top_right
            && self.top_right == self.bottom_right
            && self.bottom_right == self.bottom_left
    }
}

impl From<f32> for CornerRadius {
    fn from(radius: f32) -> Self {
        Self::uniform(radius)
    }
}

/// Shadow configuration
#[derive(Clone, Copy, Debug, Default)]
pub struct Shadow {
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Color,
}

impl Shadow {
    pub fn new(offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        Self {
            offset_x,
            offset_y,
            blur,
            spread: 0.0,
            color,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Glass Style
// ─────────────────────────────────────────────────────────────────────────────

/// Glass/frosted glass effect configuration
///
/// Creates a backdrop blur effect similar to macOS vibrancy or iOS blur.
/// Used with `DrawContext::fill_glass()` to render glass panels.
#[derive(Clone, Copy, Debug)]
pub struct GlassStyle {
    /// Blur intensity (0-50, default 20)
    pub blur: f32,
    /// Tint color applied over the blur
    pub tint: Color,
    /// Color saturation (1.0 = normal, 0.0 = grayscale)
    pub saturation: f32,
    /// Brightness multiplier (1.0 = normal)
    pub brightness: f32,
    /// Noise/grain amount for frosted texture (0.0-0.1)
    pub noise: f32,
    /// Border highlight thickness
    pub border_thickness: f32,
    /// Optional drop shadow
    pub shadow: Option<Shadow>,
    /// Use simple frosted glass mode (no liquid glass effects)
    ///
    /// When true, renders pure frosted glass without edge refraction,
    /// light reflections, or bevel effects. More performant and suitable
    /// for subtle UI backgrounds.
    pub simple: bool,
}

impl Default for GlassStyle {
    fn default() -> Self {
        Self {
            blur: 20.0,
            tint: Color::rgba(1.0, 1.0, 1.0, 0.1),
            saturation: 1.0,
            brightness: 1.0,
            noise: 0.0,
            border_thickness: 0.8,
            shadow: None,
            simple: false,
        }
    }
}

impl GlassStyle {
    /// Create a new glass style with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set blur intensity
    pub fn blur(mut self, blur: f32) -> Self {
        self.blur = blur;
        self
    }

    /// Set tint color
    pub fn tint(mut self, color: Color) -> Self {
        self.tint = color;
        self
    }

    /// Set saturation
    pub fn saturation(mut self, saturation: f32) -> Self {
        self.saturation = saturation;
        self
    }

    /// Set brightness
    pub fn brightness(mut self, brightness: f32) -> Self {
        self.brightness = brightness;
        self
    }

    /// Set noise amount
    pub fn noise(mut self, noise: f32) -> Self {
        self.noise = noise;
        self
    }

    /// Set border thickness
    pub fn border(mut self, thickness: f32) -> Self {
        self.border_thickness = thickness;
        self
    }

    /// Set drop shadow
    pub fn shadow(mut self, shadow: Shadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    // Presets

    /// Ultra-thin glass (subtle blur)
    pub fn ultra_thin() -> Self {
        Self::new().blur(10.0)
    }

    /// Thin glass
    pub fn thin() -> Self {
        Self::new().blur(15.0)
    }

    /// Regular glass (default)
    pub fn regular() -> Self {
        Self::new()
    }

    /// Thick glass (heavy blur)
    pub fn thick() -> Self {
        Self::new().blur(30.0)
    }

    /// Frosted glass with grain texture
    pub fn frosted() -> Self {
        Self::new().noise(0.03)
    }

    /// Simple frosted glass - pure blur without liquid glass effects
    ///
    /// Creates a clean backdrop blur effect without edge refraction,
    /// light reflections, or bevel effects. More performant and ideal
    /// for subtle UI backgrounds where you want pure frosted glass.
    pub fn simple() -> Self {
        Self {
            blur: 15.0,
            tint: Color::rgba(1.0, 1.0, 1.0, 0.15),
            saturation: 1.1,
            brightness: 1.0,
            noise: 0.0,
            border_thickness: 0.0,
            shadow: None,
            simple: true,
        }
    }

    /// Enable/disable simple mode (no liquid glass effects)
    pub fn with_simple(mut self, simple: bool) -> Self {
        self.simple = simple;
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Blur Style
// ─────────────────────────────────────────────────────────────────────────────

/// Pure backdrop blur effect configuration
///
/// Unlike `GlassStyle`, this provides just blur without tinting, noise, or other
/// glass-specific effects. Use this when you want a simple blur effect on the
/// content behind an element.
#[derive(Clone, Copy, Debug)]
pub struct BlurStyle {
    /// Blur radius in pixels (0-50, default 10)
    pub radius: f32,
    /// Blur quality setting
    pub quality: crate::draw::BlurQuality,
    /// Optional tint color (applied after blur)
    pub tint: Option<Color>,
    /// Opacity of the blur effect (0.0-1.0, default 1.0)
    pub opacity: f32,
}

impl Default for BlurStyle {
    fn default() -> Self {
        Self {
            radius: 10.0,
            quality: crate::draw::BlurQuality::Medium,
            tint: None,
            opacity: 1.0,
        }
    }
}

impl BlurStyle {
    /// Create a new blur style with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create blur with specified radius
    pub fn with_radius(radius: f32) -> Self {
        Self {
            radius,
            ..Default::default()
        }
    }

    /// Set blur radius
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Set blur quality
    pub fn quality(mut self, quality: crate::draw::BlurQuality) -> Self {
        self.quality = quality;
        self
    }

    /// Set optional tint color
    pub fn tint(mut self, color: Color) -> Self {
        self.tint = Some(color);
        self
    }

    /// Set opacity
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity;
        self
    }

    // Presets

    /// Light blur (subtle, 5px)
    pub fn light() -> Self {
        Self::with_radius(5.0)
    }

    /// Medium blur (default, 10px)
    pub fn medium() -> Self {
        Self::with_radius(10.0)
    }

    /// Heavy blur (20px)
    pub fn heavy() -> Self {
        Self::with_radius(20.0)
    }

    /// Maximum blur (50px)
    pub fn max() -> Self {
        Self::with_radius(50.0)
    }

    /// High quality blur (Kawase multi-pass)
    pub fn high_quality(mut self) -> Self {
        self.quality = crate::draw::BlurQuality::High;
        self
    }

    /// Low quality blur (fast box blur)
    pub fn low_quality(mut self) -> Self {
        self.quality = crate::draw::BlurQuality::Low;
        self
    }
}

impl From<f32> for BlurStyle {
    fn from(radius: f32) -> Self {
        Self::with_radius(radius)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Layer Identifiers
// ─────────────────────────────────────────────────────────────────────────────

/// Unique identifier for a layer
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayerId(pub u64);

impl LayerId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

/// Generator for unique layer IDs
#[derive(Debug, Default)]
pub struct LayerIdGenerator {
    next: u64,
}

impl LayerIdGenerator {
    pub fn new() -> Self {
        Self { next: 1 }
    }

    pub fn next(&mut self) -> LayerId {
        let id = LayerId(self.next);
        self.next += 1;
        id
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Layer Properties
// ─────────────────────────────────────────────────────────────────────────────

/// Pointer event behavior
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PointerEvents {
    /// Normal hit testing
    #[default]
    Auto,
    /// Transparent to input
    None,
    /// Receive events but don't block
    PassThrough,
}

/// Billboard facing mode for 2D content in 3D space
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BillboardFacing {
    /// Always faces camera
    #[default]
    Camera,
    /// Faces camera but stays upright
    CameraY,
    /// Uses transform rotation
    Fixed,
}

/// Layer cache policy
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CachePolicy {
    /// Always re-render
    #[default]
    None,
    /// Cache until content changes
    Content,
    /// Cache with explicit invalidation
    Manual,
}

/// Post-processing effect
#[derive(Clone, Debug)]
pub enum PostEffect {
    Blur { radius: f32 },
    Saturation { factor: f32 },
    Brightness { factor: f32 },
    Contrast { factor: f32 },
    GlassBlur { radius: f32, tint: Color },
}

/// Texture format for offscreen layers
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextureFormat {
    #[default]
    Bgra8Unorm,
    Rgba8Unorm,
    Rgba16Float,
    Rgba32Float,
}

/// Properties common to all layers
#[derive(Clone, Debug, Default)]
pub struct LayerProperties {
    /// Unique identifier for referencing
    pub id: Option<LayerId>,

    /// Visibility (skips render entirely when false)
    pub visible: bool,

    /// Pointer event behavior
    pub pointer_events: PointerEvents,

    /// Render order hint (within same Z-level)
    pub order: i32,

    /// Optional name for debugging
    pub name: Option<String>,
}

impl LayerProperties {
    pub fn new() -> Self {
        Self {
            visible: true,
            ..Default::default()
        }
    }

    pub fn with_id(mut self, id: LayerId) -> Self {
        self.id = Some(id);
        self
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn hidden(mut self) -> Self {
        self.visible = false;
        self
    }

    pub fn with_order(mut self, order: i32) -> Self {
        self.order = order;
        self
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Clip Shape
// ─────────────────────────────────────────────────────────────────────────────

/// Shape used for clipping
#[derive(Clone, Debug)]
pub enum ClipShape {
    /// Axis-aligned rectangle clip
    Rect(Rect),
    /// Rounded rectangle clip
    RoundedRect {
        rect: Rect,
        corner_radius: CornerRadius,
    },
    /// Circular clip
    Circle { center: Point, radius: f32 },
    /// Elliptical clip
    Ellipse { center: Point, radii: Vec2 },
    /// Arbitrary path clip (requires tessellation or stencil buffer)
    Path(crate::draw::Path),
}

impl ClipShape {
    /// Create a rectangular clip
    pub fn rect(rect: Rect) -> Self {
        ClipShape::Rect(rect)
    }

    /// Create a rounded rectangle clip
    pub fn rounded_rect(rect: Rect, corner_radius: impl Into<CornerRadius>) -> Self {
        ClipShape::RoundedRect {
            rect,
            corner_radius: corner_radius.into(),
        }
    }

    /// Create a circular clip
    pub fn circle(center: Point, radius: f32) -> Self {
        ClipShape::Circle { center, radius }
    }

    /// Create an elliptical clip
    pub fn ellipse(center: Point, radii: Vec2) -> Self {
        ClipShape::Ellipse { center, radii }
    }

    /// Create a path-based clip
    pub fn path(path: crate::draw::Path) -> Self {
        ClipShape::Path(path)
    }

    /// Get the bounding rect of this clip shape
    pub fn bounds(&self) -> Rect {
        match self {
            ClipShape::Rect(rect) => *rect,
            ClipShape::RoundedRect { rect, .. } => *rect,
            ClipShape::Circle { center, radius } => {
                Rect::from_center(*center, Size::new(*radius * 2.0, *radius * 2.0))
            }
            ClipShape::Ellipse { center, radii } => {
                Rect::from_center(*center, Size::new(radii.x * 2.0, radii.y * 2.0))
            }
            ClipShape::Path(path) => path.bounds(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 3D Scene Types
// ─────────────────────────────────────────────────────────────────────────────

/// Camera projection type
#[derive(Clone, Copy, Debug)]
pub enum CameraProjection {
    Perspective {
        fov_y: f32,
        aspect: f32,
        near: f32,
        far: f32,
    },
    Orthographic {
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        near: f32,
        far: f32,
    },
}

impl Default for CameraProjection {
    fn default() -> Self {
        CameraProjection::Perspective {
            fov_y: std::f32::consts::FRAC_PI_4,
            aspect: 16.0 / 9.0,
            near: 0.1,
            far: 1000.0,
        }
    }
}

/// Camera for 3D scenes
#[derive(Clone, Debug, Default)]
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub projection: CameraProjection,
}

impl Camera {
    pub fn perspective(position: Vec3, target: Vec3, fov_y: f32) -> Self {
        Self {
            position,
            target,
            up: Vec3::UP,
            projection: CameraProjection::Perspective {
                fov_y,
                aspect: 16.0 / 9.0,
                near: 0.1,
                far: 1000.0,
            },
        }
    }

    pub fn orthographic(position: Vec3, target: Vec3, scale: f32) -> Self {
        Self {
            position,
            target,
            up: Vec3::UP,
            projection: CameraProjection::Orthographic {
                left: -scale,
                right: scale,
                bottom: -scale,
                top: scale,
                near: 0.1,
                far: 1000.0,
            },
        }
    }
}

/// Light type for 3D scenes
#[derive(Clone, Debug)]
pub enum Light {
    Directional {
        direction: Vec3,
        color: Color,
        intensity: f32,
        cast_shadows: bool,
    },
    Point {
        position: Vec3,
        color: Color,
        intensity: f32,
        range: f32,
    },
    Spot {
        position: Vec3,
        direction: Vec3,
        color: Color,
        intensity: f32,
        range: f32,
        inner_angle: f32,
        outer_angle: f32,
    },
    Ambient {
        color: Color,
        intensity: f32,
    },
}

/// Environment settings for 3D scenes (skybox, IBL)
#[derive(Clone, Debug, Default)]
pub struct Environment {
    /// HDRI texture path (if any)
    pub hdri: Option<String>,
    /// Environment intensity
    pub intensity: f32,
    /// Background blur amount
    pub blur: f32,
    /// Solid background color (used if no HDRI)
    pub background_color: Option<Color>,
}

/// Parameters for 3D SDF raymarching viewport
///
/// This struct contains all the information needed to render an SDF scene
/// using GPU raymarching.
#[derive(Clone, Debug)]
pub struct Sdf3DViewport {
    /// The generated WGSL shader code containing the SDF scene definition
    pub shader_wgsl: String,
    /// Camera position in world space
    pub camera_pos: Vec3,
    /// Camera look direction (normalized)
    pub camera_dir: Vec3,
    /// Camera up vector (normalized)
    pub camera_up: Vec3,
    /// Camera right vector (normalized)
    pub camera_right: Vec3,
    /// Field of view in radians
    pub fov: f32,
    /// Animation time for time-based effects
    pub time: f32,
    /// Maximum raymarch steps
    pub max_steps: u32,
    /// Maximum ray distance
    pub max_distance: f32,
    /// Surface hit epsilon
    pub epsilon: f32,
    /// Lights in the scene
    pub lights: Vec<Light>,
}

impl Default for Sdf3DViewport {
    fn default() -> Self {
        Self {
            shader_wgsl: String::new(),
            camera_pos: Vec3::new(0.0, 2.0, 5.0),
            camera_dir: Vec3::new(0.0, 0.0, -1.0),
            camera_up: Vec3::new(0.0, 1.0, 0.0),
            camera_right: Vec3::new(1.0, 0.0, 0.0),
            fov: 0.8,
            time: 0.0,
            max_steps: 128,
            max_distance: 100.0,
            epsilon: 0.001,
            lights: Vec::new(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GPU Particle System Types
// ─────────────────────────────────────────────────────────────────────────────

/// Emitter shape for GPU particle systems
#[derive(Clone, Debug, PartialEq)]
pub enum ParticleEmitterShape {
    /// Single point emitter
    Point,
    /// Sphere volume/surface
    Sphere { radius: f32 },
    /// Upper hemisphere
    Hemisphere { radius: f32 },
    /// Cone shape
    Cone { angle: f32, radius: f32 },
    /// Box volume
    Box { half_extents: Vec3 },
    /// Circle (XZ plane)
    Circle { radius: f32 },
}

impl Default for ParticleEmitterShape {
    fn default() -> Self {
        Self::Point
    }
}

/// Force affector for GPU particles
#[derive(Clone, Debug, PartialEq)]
pub enum ParticleForce {
    /// Constant directional force (gravity)
    Gravity(Vec3),
    /// Wind with turbulence
    Wind { direction: Vec3, strength: f32, turbulence: f32 },
    /// Vortex/swirl
    Vortex { axis: Vec3, center: Vec3, strength: f32 },
    /// Velocity damping
    Drag(f32),
    /// Noise-based force
    Turbulence { strength: f32, frequency: f32 },
    /// Point attractor/repeller
    Attractor { position: Vec3, strength: f32 },
}

/// Particle render mode
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ParticleRenderMode {
    /// Camera-facing billboards
    #[default]
    Billboard,
    /// Stretched in velocity direction
    Stretched,
    /// Horizontal billboards
    Horizontal,
    /// Vertical billboards
    Vertical,
}

/// Particle blend mode
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ParticleBlendMode {
    /// Standard alpha blending
    #[default]
    Alpha,
    /// Additive blending (for glowing effects)
    Additive,
    /// Multiplicative blending
    Multiply,
}

/// GPU particle system data for rendering
///
/// This structure contains all the data needed to simulate and render
/// a particle system on the GPU. It's created from blinc_3d's ParticleSystem
/// component for submission to the GPU renderer.
#[derive(Clone, Debug)]
pub struct ParticleSystemData {
    /// Maximum number of particles
    pub max_particles: u32,
    /// Emitter shape
    pub emitter: ParticleEmitterShape,
    /// Emitter world position
    pub emitter_position: Vec3,
    /// Emission rate (particles per second)
    pub emission_rate: f32,
    /// Burst count (particles to spawn in a burst, decrements each frame)
    pub burst_count: f32,
    /// Emission direction
    pub direction: Vec3,
    /// Direction randomness (0 = straight, 1 = fully random)
    pub direction_randomness: f32,
    /// Particle lifetime range (min, max)
    pub lifetime: (f32, f32),
    /// Start speed range (min, max)
    pub start_speed: (f32, f32),
    /// Start size range (min, max)
    pub start_size: (f32, f32),
    /// End size range (min, max)
    pub end_size: (f32, f32),
    /// Start color (base - young particles)
    pub start_color: Color,
    /// Mid color (middle - mid-life particles)
    pub mid_color: Color,
    /// End color (tip - old/dying particles)
    pub end_color: Color,
    /// Force affectors
    pub forces: Vec<ParticleForce>,
    /// Gravity scale
    pub gravity_scale: f32,
    /// Render mode
    pub render_mode: ParticleRenderMode,
    /// Blend mode
    pub blend_mode: ParticleBlendMode,
    /// Current time for simulation
    pub time: f32,
    /// Delta time for this frame
    pub delta_time: f32,
    /// Camera position (for billboarding)
    pub camera_pos: Vec3,
    /// Camera direction (for billboarding)
    pub camera_dir: Vec3,
    /// Camera up vector
    pub camera_up: Vec3,
    /// Camera right vector
    pub camera_right: Vec3,
    /// Whether system is playing
    pub playing: bool,
}

impl Default for ParticleSystemData {
    fn default() -> Self {
        Self {
            max_particles: 10000,
            emitter: ParticleEmitterShape::Point,
            emitter_position: Vec3::ZERO,
            emission_rate: 100.0,
            burst_count: 0.0,
            direction: Vec3::new(0.0, 1.0, 0.0),
            direction_randomness: 0.0,
            lifetime: (1.0, 2.0),
            start_speed: (1.0, 2.0),
            start_size: (0.1, 0.2),
            end_size: (0.0, 0.1),
            start_color: Color::WHITE,
            mid_color: Color::rgba(1.0, 1.0, 1.0, 0.5),
            end_color: Color::rgba(1.0, 1.0, 1.0, 0.0),
            forces: Vec::new(),
            gravity_scale: 1.0,
            render_mode: ParticleRenderMode::Billboard,
            blend_mode: ParticleBlendMode::Alpha,
            time: 0.0,
            delta_time: 0.016,
            camera_pos: Vec3::new(0.0, 2.0, 5.0),
            camera_dir: Vec3::new(0.0, 0.0, -1.0),
            camera_up: Vec3::new(0.0, 1.0, 0.0),
            camera_right: Vec3::new(1.0, 0.0, 0.0),
            playing: true,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Layer Command Types (for Canvas2D and Scene3D)
// ─────────────────────────────────────────────────────────────────────────────

/// Commands for 2D canvas drawing
/// These are recorded and then executed by the Canvas2D renderer
#[derive(Clone, Debug, Default)]
pub struct Canvas2DCommands {
    commands: Vec<Canvas2DCommand>,
}

/// Individual canvas drawing command
#[derive(Clone, Debug)]
pub enum Canvas2DCommand {
    /// Clear the canvas with a color
    Clear(Color),

    /// Save the current state (transform, clip, opacity)
    Save,

    /// Restore the previously saved state
    Restore,

    /// Push a 2D transform
    Transform(Affine2D),

    /// Set the global opacity
    SetOpacity(f32),

    /// Set the blend mode
    SetBlendMode(BlendMode),

    /// Push a clipping shape
    PushClip(ClipShape),

    /// Pop the last clipping shape
    PopClip,

    /// Fill a path with a brush
    FillPath {
        path: crate::draw::Path,
        brush: Brush,
    },

    /// Stroke a path
    StrokePath {
        path: crate::draw::Path,
        stroke: crate::draw::Stroke,
        brush: Brush,
    },

    /// Fill a rectangle (optimized primitive)
    FillRect {
        rect: Rect,
        corner_radius: CornerRadius,
        brush: Brush,
    },

    /// Stroke a rectangle (optimized primitive)
    StrokeRect {
        rect: Rect,
        corner_radius: CornerRadius,
        stroke: crate::draw::Stroke,
        brush: Brush,
    },

    /// Fill a circle (optimized primitive)
    FillCircle {
        center: Point,
        radius: f32,
        brush: Brush,
    },

    /// Stroke a circle (optimized primitive)
    StrokeCircle {
        center: Point,
        radius: f32,
        stroke: crate::draw::Stroke,
        brush: Brush,
    },

    /// Draw text
    DrawText {
        text: String,
        origin: Point,
        style: crate::draw::TextStyle,
    },

    /// Draw an image
    DrawImage {
        image: crate::draw::ImageId,
        rect: Rect,
        options: crate::draw::ImageOptions,
    },

    /// Draw a shadow
    DrawShadow {
        rect: Rect,
        corner_radius: CornerRadius,
        shadow: Shadow,
    },
}

impl Canvas2DCommands {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    pub fn push(&mut self, command: Canvas2DCommand) {
        self.commands.push(command);
    }

    pub fn commands(&self) -> &[Canvas2DCommand] {
        &self.commands
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

/// Commands for 3D scene
#[derive(Clone, Debug, Default)]
pub struct Scene3DCommands {
    commands: Vec<Scene3DCommand>,
}

/// Individual 3D scene command
#[derive(Clone, Debug)]
pub enum Scene3DCommand {
    /// Clear the scene with a color
    Clear(Color),

    /// Set the active camera
    SetCamera(Camera),

    /// Push a 3D transform matrix
    PushTransform(Mat4),

    /// Pop the last transform
    PopTransform,

    /// Draw a mesh with a material
    DrawMesh {
        mesh: crate::draw::MeshId,
        material: crate::draw::MaterialId,
        transform: Mat4,
    },

    /// Draw multiple instances of a mesh
    DrawMeshInstanced {
        mesh: crate::draw::MeshId,
        instances: Vec<crate::draw::MeshInstance>,
    },

    /// Add a light to the scene
    AddLight(Light),

    /// Set the environment (skybox, ambient, etc.)
    SetEnvironment(Environment),

    /// Draw a billboard (2D content in 3D space)
    DrawBillboard {
        /// Size of the billboard in world units
        size: Size,
        /// Transform in world space
        transform: Mat4,
        /// How the billboard faces the camera
        facing: BillboardFacing,
        /// The 2D content to render
        content: Canvas2DCommands,
    },
}

impl Scene3DCommands {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    pub fn push(&mut self, command: Scene3DCommand) {
        self.commands.push(command);
    }

    pub fn commands(&self) -> &[Scene3DCommand] {
        &self.commands
    }

    pub fn clear(&mut self) {
        self.commands.clear();
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UI Node (placeholder for full UI system)
// ─────────────────────────────────────────────────────────────────────────────

/// Reference to a UI node in the layout tree
#[derive(Clone, Copy, Debug)]
pub struct UiNode {
    /// Node identifier
    pub id: u64,
}

impl UiNode {
    pub fn new(id: u64) -> Self {
        Self { id }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Layer Enum - The Core Abstraction
// ─────────────────────────────────────────────────────────────────────────────

/// All visual content is represented as a `Layer`.
///
/// Layers can be 2D UI primitives, 2D canvas drawings, 3D scenes, or
/// composition/transformation of other layers.
#[derive(Clone, Debug)]
pub enum Layer {
    // ─────────────────────────────────────────────────────────────────────────
    // 2D Primitives (SDF Rendered)
    // ─────────────────────────────────────────────────────────────────────────
    /// UI node tree (SDF rendered)
    Ui {
        node: UiNode,
        props: LayerProperties,
    },

    // ─────────────────────────────────────────────────────────────────────────
    // 2D Vector Drawing
    // ─────────────────────────────────────────────────────────────────────────
    /// 2D canvas with vector drawing commands
    Canvas2D {
        size: Size,
        commands: Canvas2DCommands,
        cache_policy: CachePolicy,
        props: LayerProperties,
    },

    // ─────────────────────────────────────────────────────────────────────────
    // 3D Scene
    // ─────────────────────────────────────────────────────────────────────────
    /// 3D scene with meshes, materials, and lighting
    Scene3D {
        viewport: Rect,
        commands: Scene3DCommands,
        camera: Camera,
        environment: Option<Environment>,
        props: LayerProperties,
    },

    // ─────────────────────────────────────────────────────────────────────────
    // Composition
    // ─────────────────────────────────────────────────────────────────────────
    /// Stack of layers composited together
    Stack {
        layers: Vec<Layer>,
        blend_mode: BlendMode,
        props: LayerProperties,
    },

    /// 2D transform applied to a layer
    Transform2D {
        transform: Affine2D,
        layer: Box<Layer>,
        props: LayerProperties,
    },

    /// 3D transform applied to a layer
    Transform3D {
        transform: Mat4,
        layer: Box<Layer>,
        props: LayerProperties,
    },

    /// Clip mask applied to a layer
    Clip {
        shape: ClipShape,
        layer: Box<Layer>,
        props: LayerProperties,
    },

    /// Opacity applied to a layer
    Opacity {
        value: f32,
        layer: Box<Layer>,
        props: LayerProperties,
    },

    // ─────────────────────────────────────────────────────────────────────────
    // Render Target Indirection
    // ─────────────────────────────────────────────────────────────────────────
    /// Layer rendered to an offscreen texture with optional effects
    Offscreen {
        size: Size,
        format: TextureFormat,
        layer: Box<Layer>,
        effects: Vec<PostEffect>,
        props: LayerProperties,
    },

    // ─────────────────────────────────────────────────────────────────────────
    // Dimension Bridging
    // ─────────────────────────────────────────────────────────────────────────
    /// 2D layer placed in 3D space
    Billboard {
        layer: Box<Layer>,
        transform: Mat4,
        facing: BillboardFacing,
        props: LayerProperties,
    },

    /// 3D scene embedded in 2D layout
    Viewport3D {
        rect: Rect,
        scene: Box<Layer>, // Must be Scene3D
        props: LayerProperties,
    },

    /// Reference to another layer's render output
    Portal {
        source: LayerId,
        sample_rect: Rect,
        dest_rect: Rect,
        props: LayerProperties,
    },

    /// Empty layer (useful as placeholder)
    Empty { props: LayerProperties },
}

impl Layer {
    /// Get the layer properties
    pub fn props(&self) -> &LayerProperties {
        match self {
            Layer::Ui { props, .. } => props,
            Layer::Canvas2D { props, .. } => props,
            Layer::Scene3D { props, .. } => props,
            Layer::Stack { props, .. } => props,
            Layer::Transform2D { props, .. } => props,
            Layer::Transform3D { props, .. } => props,
            Layer::Clip { props, .. } => props,
            Layer::Opacity { props, .. } => props,
            Layer::Offscreen { props, .. } => props,
            Layer::Billboard { props, .. } => props,
            Layer::Viewport3D { props, .. } => props,
            Layer::Portal { props, .. } => props,
            Layer::Empty { props } => props,
        }
    }

    /// Get mutable layer properties
    pub fn props_mut(&mut self) -> &mut LayerProperties {
        match self {
            Layer::Ui { props, .. } => props,
            Layer::Canvas2D { props, .. } => props,
            Layer::Scene3D { props, .. } => props,
            Layer::Stack { props, .. } => props,
            Layer::Transform2D { props, .. } => props,
            Layer::Transform3D { props, .. } => props,
            Layer::Clip { props, .. } => props,
            Layer::Opacity { props, .. } => props,
            Layer::Offscreen { props, .. } => props,
            Layer::Billboard { props, .. } => props,
            Layer::Viewport3D { props, .. } => props,
            Layer::Portal { props, .. } => props,
            Layer::Empty { props } => props,
        }
    }

    /// Get the layer ID if set
    pub fn id(&self) -> Option<LayerId> {
        self.props().id
    }

    /// Check if the layer is visible
    pub fn is_visible(&self) -> bool {
        self.props().visible
    }

    /// Create an empty layer
    pub fn empty() -> Self {
        Layer::Empty {
            props: LayerProperties::new(),
        }
    }

    /// Create a stack of layers
    pub fn stack(layers: Vec<Layer>) -> Self {
        Layer::Stack {
            layers,
            blend_mode: BlendMode::Normal,
            props: LayerProperties::new(),
        }
    }

    /// Wrap this layer with a 2D transform
    pub fn with_transform_2d(self, transform: Affine2D) -> Self {
        Layer::Transform2D {
            transform,
            layer: Box::new(self),
            props: LayerProperties::new(),
        }
    }

    /// Wrap this layer with a 3D transform
    pub fn with_transform_3d(self, transform: Mat4) -> Self {
        Layer::Transform3D {
            transform,
            layer: Box::new(self),
            props: LayerProperties::new(),
        }
    }

    /// Wrap this layer with a clip shape
    pub fn with_clip(self, shape: ClipShape) -> Self {
        Layer::Clip {
            shape,
            layer: Box::new(self),
            props: LayerProperties::new(),
        }
    }

    /// Wrap this layer with opacity
    pub fn with_opacity(self, value: f32) -> Self {
        Layer::Opacity {
            value,
            layer: Box::new(self),
            props: LayerProperties::new(),
        }
    }

    /// Check if this is a 3D layer
    pub fn is_3d(&self) -> bool {
        matches!(
            self,
            Layer::Scene3D { .. } | Layer::Billboard { .. } | Layer::Transform3D { .. }
        )
    }

    /// Check if this is a 2D layer
    pub fn is_2d(&self) -> bool {
        matches!(
            self,
            Layer::Ui { .. }
                | Layer::Canvas2D { .. }
                | Layer::Transform2D { .. }
                | Layer::Viewport3D { .. }
        )
    }

    /// Visit all child layers
    pub fn visit_children<F: FnMut(&Layer)>(&self, mut f: F) {
        match self {
            Layer::Stack { layers, .. } => {
                for layer in layers {
                    f(layer);
                }
            }
            Layer::Transform2D { layer, .. }
            | Layer::Transform3D { layer, .. }
            | Layer::Clip { layer, .. }
            | Layer::Opacity { layer, .. }
            | Layer::Offscreen { layer, .. }
            | Layer::Billboard { layer, .. }
            | Layer::Viewport3D { scene: layer, .. } => {
                f(layer);
            }
            _ => {}
        }
    }

    /// Visit all child layers mutably
    pub fn visit_children_mut<F: FnMut(&mut Layer)>(&mut self, mut f: F) {
        match self {
            Layer::Stack { layers, .. } => {
                for layer in layers {
                    f(layer);
                }
            }
            Layer::Transform2D { layer, .. }
            | Layer::Transform3D { layer, .. }
            | Layer::Clip { layer, .. }
            | Layer::Opacity { layer, .. }
            | Layer::Offscreen { layer, .. }
            | Layer::Billboard { layer, .. }
            | Layer::Viewport3D { scene: layer, .. } => {
                f(layer);
            }
            _ => {}
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Scene Graph
// ─────────────────────────────────────────────────────────────────────────────

/// Scene graph containing the root layer and layer index
#[derive(Debug, Default)]
pub struct SceneGraph {
    /// Root layer of the scene
    pub root: Option<Layer>,

    /// Index for fast layer lookup by ID
    layer_index: HashMap<LayerId, usize>,

    /// ID generator
    id_generator: LayerIdGenerator,
}

impl SceneGraph {
    pub fn new() -> Self {
        Self {
            root: None,
            layer_index: HashMap::new(),
            id_generator: LayerIdGenerator::new(),
        }
    }

    /// Set the root layer
    pub fn set_root(&mut self, layer: Layer) {
        self.root = Some(layer);
        self.rebuild_index();
    }

    /// Generate a new unique layer ID
    pub fn new_layer_id(&mut self) -> LayerId {
        self.id_generator.next()
    }

    /// Find a layer by ID (traverses the tree)
    pub fn find_layer(&self, id: LayerId) -> Option<&Layer> {
        fn find_in_layer(layer: &Layer, target_id: LayerId) -> Option<&Layer> {
            if layer.id() == Some(target_id) {
                return Some(layer);
            }

            match layer {
                Layer::Stack { layers, .. } => {
                    for child in layers {
                        if let Some(found) = find_in_layer(child, target_id) {
                            return Some(found);
                        }
                    }
                }
                Layer::Transform2D { layer: child, .. }
                | Layer::Transform3D { layer: child, .. }
                | Layer::Clip { layer: child, .. }
                | Layer::Opacity { layer: child, .. }
                | Layer::Offscreen { layer: child, .. }
                | Layer::Billboard { layer: child, .. }
                | Layer::Viewport3D { scene: child, .. } => {
                    if let Some(found) = find_in_layer(child, target_id) {
                        return Some(found);
                    }
                }
                _ => {}
            }

            None
        }

        self.root.as_ref().and_then(|root| find_in_layer(root, id))
    }

    /// Rebuild the layer index
    fn rebuild_index(&mut self) {
        self.layer_index.clear();
        // Future: implement full index rebuilding
    }

    /// Traverse all layers in depth-first order
    pub fn traverse<F: FnMut(&Layer, usize)>(&self, mut f: F) {
        fn traverse_layer<F: FnMut(&Layer, usize)>(layer: &Layer, depth: usize, f: &mut F) {
            f(layer, depth);
            layer.visit_children(|child| traverse_layer(child, depth + 1, f));
        }

        if let Some(root) = &self.root {
            traverse_layer(root, 0, &mut f);
        }
    }

    /// Count total number of layers
    pub fn layer_count(&self) -> usize {
        let mut count = 0;
        self.traverse(|_, _| count += 1);
        count
    }

    /// Check if the scene contains any 3D layers
    pub fn has_3d(&self) -> bool {
        let mut has_3d = false;
        self.traverse(|layer, _| {
            if layer.is_3d() {
                has_3d = true;
            }
        });
        has_3d
    }

    /// Count all visible layers
    pub fn visible_layer_count(&self) -> usize {
        let mut count = 0;
        self.traverse(|layer, _| {
            if layer.is_visible() {
                count += 1;
            }
        });
        count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layer_creation() {
        let layer = Layer::empty();
        assert!(layer.is_visible());
        assert!(layer.id().is_none());
    }

    #[test]
    fn test_layer_stack() {
        let stack = Layer::stack(vec![Layer::empty(), Layer::empty(), Layer::empty()]);

        let mut count = 0;
        stack.visit_children(|_| count += 1);
        assert_eq!(count, 3);
    }

    #[test]
    fn test_layer_transforms() {
        let layer = Layer::empty()
            .with_transform_2d(Affine2D::translation(10.0, 20.0))
            .with_opacity(0.5);

        assert!(matches!(layer, Layer::Opacity { .. }));
    }

    #[test]
    fn test_scene_graph() {
        let mut scene = SceneGraph::new();

        let id1 = scene.new_layer_id();
        let id2 = scene.new_layer_id();

        assert_ne!(id1, id2);

        scene.set_root(Layer::stack(vec![
            Layer::Empty {
                props: LayerProperties::new().with_id(id1),
            },
            Layer::Empty {
                props: LayerProperties::new().with_id(id2),
            },
        ]));

        assert_eq!(scene.layer_count(), 3); // stack + 2 empty

        let found = scene.find_layer(id1);
        assert!(found.is_some());
    }

    #[test]
    fn test_geometry_types() {
        let p = Point::new(1.0, 2.0);
        let s = Size::new(100.0, 50.0);
        let r = Rect::from_origin_size(p, s);

        assert_eq!(r.center(), Point::new(51.0, 27.0));
        assert!(r.contains(Point::new(50.0, 25.0)));
        assert!(!r.contains(Point::new(200.0, 100.0)));

        // Test Size to Rect conversion
        let size = Size::new(200.0, 100.0);
        let rect: Rect = size.into();
        assert_eq!(rect.x(), 0.0);
        assert_eq!(rect.y(), 0.0);
        assert_eq!(rect.width(), 200.0);
        assert_eq!(rect.height(), 100.0);

        // Test to_rect() method
        let rect2 = size.to_rect();
        assert_eq!(rect, rect2);

        // Test offset and inset
        let offset_rect = rect.offset(10.0, 20.0);
        assert_eq!(offset_rect.x(), 10.0);
        assert_eq!(offset_rect.y(), 20.0);

        let inset_rect = rect.inset(5.0, 10.0);
        assert_eq!(inset_rect.x(), 5.0);
        assert_eq!(inset_rect.y(), 10.0);
        assert_eq!(inset_rect.width(), 190.0);
        assert_eq!(inset_rect.height(), 80.0);
    }

    #[test]
    fn test_color() {
        let c = Color::from_hex(0xFF5500);
        assert_eq!(c.r, 1.0);
        assert!((c.g - 85.0 / 255.0).abs() < 0.001);
        assert_eq!(c.b, 0.0);

        let c2 = c.with_alpha(0.5);
        assert_eq!(c2.a, 0.5);
    }

    #[test]
    fn test_mat4_operations() {
        let t = Mat4::translation(1.0, 2.0, 3.0);
        let s = Mat4::scale(2.0, 2.0, 2.0);
        let result = t.mul(&s);

        // Verify it's a valid combined transform
        assert_eq!(result.cols[3][0], 1.0); // translation preserved
    }
}
