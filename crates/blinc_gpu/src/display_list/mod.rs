//! Reads `blinc_abi` display lists.
//!
//! A scene encoded by `blinc_abi` is a flat run of records, one per
//! primitive in paint order, [`RECORD_FLOATS`] f32s each, in logical pixels
//! whatever display scale the scene is painted at. [`Record`] is a typed
//! view of one, and [`Record::to_primitive`] turns it into the
//! [`GpuPrimitive`] the SDF pipelines draw.
//!
//! The record is a superset of what a primitive carries in some places and a
//! subset in others. A record that needs something `GpuPrimitive` cannot
//! yet express comes back as [`Unsupported`] naming it, never as a
//! primitive that quietly draws something else.
//!
//! Rows of a record, each a vec4:
//!
//! | row   | contents                                                          |
//! |-------|-------------------------------------------------------------------|
//! | 0     | top-left on screen after transforms, width, height                |
//! | 1     | corner radii                                                      |
//! | 2, 3  | fill colour, or gradient start and end                            |
//! | 4     | border widths: top, right, bottom, left                           |
//! | 5     | border colour                                                     |
//! | 6, 7  | shadow offset, blur, spread and colour; or a local clip           |
//! | 8, 9  | screen clip bounds and radii                                      |
//! | 10    | gradient geometry in pixels from the box's top-left               |
//! | 11    | kind, fill type, clip flags, clip corner `n`                      |
//! | 12    | corner shape `n` per corner                                       |
//! | 13, 14| gradient middle stop and stop offsets                             |
//! | 15    | the 2x2 of the box's transform: a, b, c, d                        |
//! | 16-19 | border colour of each side: top, right, bottom, left              |
//! | 20, 21| rect of the innermost fading clip, and its fade distances         |
//! | 22-24 | the innermost `clip-path` in the element's frame                  |
//! | 25-27 | a notch's corners, top edge and bottom edge                       |

pub mod atlas;
pub mod backdrop;
pub mod canvas;
mod clip;
pub mod effects;
mod gaussian;
pub mod image;
pub mod plan;

use crate::primitives::{ClipType, FillType, GpuPrimitive, PrimitiveType};

/// The record schema this module reads, as `blinc_abi::scene::DISPLAY_LIST_VERSION`.
pub const DISPLAY_LIST_VERSION: u32 = 1;

/// Floats in one record.
pub const RECORD_FLOATS: usize = 112;

const ROWS: usize = RECORD_FLOATS / 4;

/// Row numbers of a record.
pub mod row {
    pub const BOUNDS: usize = 0;
    pub const RADII: usize = 1;
    pub const FILL: usize = 2;
    pub const FILL_END: usize = 3;
    pub const BORDER: usize = 4;
    pub const BORDER_COLOR: usize = 5;
    pub const SHADOW: usize = 6;
    pub const SHADOW_COLOR: usize = 7;
    pub const CLIP: usize = 8;
    pub const CLIP_RADII: usize = 9;
    pub const GRADIENT: usize = 10;
    pub const TYPES: usize = 11;
    pub const CORNER_SHAPE: usize = 12;
    pub const GRADIENT_VIA: usize = 13;
    pub const GRADIENT_OFFSETS: usize = 14;
    pub const AFFINE: usize = 15;
    pub const SIDE_COLORS: usize = 16;
    pub const FADE_RECT: usize = 20;
    pub const FADE: usize = 21;
    pub const SHAPE: usize = 22;
    pub const NOTCH: usize = 25;
}

/// `row::TYPES.x`, the record's primitive kind.
const KIND_RECT: f32 = 0.0;
const KIND_SHADOW: f32 = 3.0;
const KIND_TEXT: f32 = 7.0;
const KIND_IMAGE: f32 = 32.0;
const KIND_CANVAS: f32 = 33.0;
const KIND_LAYER_BEGIN: f32 = 40.0;
const KIND_LAYER: f32 = 41.0;
const KIND_BACKDROP: f32 = 42.0;

/// `row::TYPES.z`: bit 0 is a screen clip, bit 1 a local one.
const CLIP_SCREEN: u32 = 1;
const CLIP_LOCAL: u32 = 2;

/// The stop offsets of a plain two-stop gradient, `row::GRADIENT_OFFSETS`.
const TWO_STOPS: [f32; 4] = [0.0, 0.0, 1.0, 0.0];

/// Bounds meaning "no clip", as a primitive carries them.
const NO_CLIP: [f32; 4] = [-10000.0, -10000.0, 100000.0, 100000.0];

/// Why a buffer is not a display list this module can read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The producer's schema is not the one this module reads.
    Version { found: u32 },
    /// Fewer floats than the records claim.
    Short { floats: usize, needed: usize },
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Version { found } => write!(
                f,
                "display list version {found}, expected {DISPLAY_LIST_VERSION}"
            ),
            Self::Short { floats, needed } => {
                write!(f, "display list has {floats} floats, needs {needed}")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

/// What a record needs that a [`GpuPrimitive`] cannot yet express.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Unsupported {
    /// A kind this entry point cannot draw, including ordered passes in primitive-only decoding.
    Kind(f32),
    /// A fill of a kind this does not know, or a gradient whose table is not in the list.
    Gradient,
    /// Invalid shape data, or a shape needing aux data in `to_primitive`.
    ClipShape,
    /// A notch on a record other than a box, shadow or backdrop.
    Notch,
    /// A transform with no area.
    Degenerate,
    /// A text atlas selector other than mask, colour or colour shape.
    GlyphMode,
    /// Invalid image fit, position, repeat axes or intrinsic geometry.
    ImageMapping,
    /// A layer whose mask has more stops than there is room for, or whose table is not in the list.
    LayerEffect,
    /// A gradient whose stops are kept in a table, where the caller gave none to keep it in.
    GradientTable,
    /// Non-finite backdrop data or a negative Gaussian deviation or read margin.
    BackdropEffect,
    /// Invalid canvas geometry, slot or resource scale.
    CanvasData,
}

/// The kind of a record.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Box,
    Shadow,
    /// One glyph, a quad over its rect in a glyph atlas.
    Text,
    /// A resource of the consumer's, named by its slot, over a box.
    Image,
    /// Custom GPU drawing at a consumer-owned slot.
    Canvas,
    /// Opens a layer: the records up to its [`Kind::Layer`] are drawn into it.
    LayerBegin,
    /// Closes a layer, and draws it over the page with what its record asks.
    Layer,
    /// Filters the pixels painted so far in the current layer.
    Backdrop,
    /// An unknown kind, by its code.
    Other(f32),
}

/// One record, as a typed view over its floats.
#[derive(Clone, Copy, Debug)]
pub struct Record<'a>(&'a [f32; RECORD_FLOATS]);

/// The records of a list, with their count checked against its length.
pub fn records(
    floats: &[f32],
    count: usize,
    version: u32,
) -> Result<impl Iterator<Item = Record<'_>>, DecodeError> {
    if version != DISPLAY_LIST_VERSION {
        return Err(DecodeError::Version { found: version });
    }
    let needed = count * RECORD_FLOATS;
    if floats.len() < needed {
        return Err(DecodeError::Short {
            floats: floats.len(),
            needed,
        });
    }
    Ok(floats[..needed]
        .chunks_exact(RECORD_FLOATS)
        .map(|chunk| Record(chunk.try_into().expect("chunks_exact yields whole records"))))
}

/// The primitives of a list, and the records that had none.
#[derive(Debug, Default)]
pub struct Decoded {
    pub primitives: Vec<GpuPrimitive>,
    /// Gradient stop tables and shape clips, addressed by the primitives in `aux_data`.
    pub aux: Vec<[f32; 4]>,
    /// The index of each record left out, and why.
    pub unsupported: Vec<(usize, Unsupported)>,
}

/// Decodes `count` records at `scale` device pixels per logical pixel.
pub fn decode(
    floats: &[f32],
    count: usize,
    version: u32,
    scale: f32,
) -> Result<Decoded, DecodeError> {
    let mut out = Decoded::default();
    for (index, record) in records(floats, count, version)?.enumerate() {
        match record.primitive(scale, floats, out.aux.len()) {
            Ok((p, table)) => {
                out.primitives.push(p);
                out.aux.extend(table);
            }
            Err(why) => out.unsupported.push((index, why)),
        }
    }
    Ok(out)
}

/// Where a record's box lands on a target, in the numbers `GpuPaintContext`
/// produces for the same transform.
struct Placement {
    /// The box, centred where the transform puts its centre, at the transform's uniform scale.
    bounds: [f32; 4],
    /// What is left of the transform's 2x2 once its uniform scale is taken out.
    local_affine: [f32; 4],
    size_scale: f32,
    avg_scale: f32,
    column_x: f32,
    column_y: f32,
}

impl<'a> Record<'a> {
    /// A record over exactly [`RECORD_FLOATS`] floats.
    pub fn new(floats: &'a [f32; RECORD_FLOATS]) -> Self {
        Self(floats)
    }

    fn placement(&self, scale: f32) -> Placement {
        let [x, y, w, h] = self.row(row::BOUNDS);
        let [a, b, c, d] = self.row(row::AFFINE);

        // The transform's uniform scale, and what is left of it.
        let uniform = (a * d - b * c).abs().sqrt();
        let local_affine = [a / uniform, b / uniform, c / uniform, d / uniform];
        let size_scale = scale * uniform;
        let (column_x, column_y) = (a.hypot(b), c.hypot(d));
        let avg_scale = scale * (column_x + column_y) / 2.0;

        // The record places the box's top-left; a primitive places its centre.
        let centre = (x + a * w / 2.0 + c * h / 2.0, y + b * w / 2.0 + d * h / 2.0);
        let (sw, sh) = (w * size_scale, h * size_scale);
        Placement {
            bounds: [
                centre.0 * scale - sw / 2.0,
                centre.1 * scale - sh / 2.0,
                sw,
                sh,
            ],
            local_affine,
            size_scale,
            avg_scale,
            column_x,
            column_y,
        }
    }

    pub fn row(&self, row: usize) -> [f32; 4] {
        debug_assert!(row < ROWS);
        let at = row * 4;
        [self.0[at], self.0[at + 1], self.0[at + 2], self.0[at + 3]]
    }

    pub fn kind(&self) -> Kind {
        match self.row(row::TYPES)[0] {
            k if k == KIND_RECT => Kind::Box,
            k if k == KIND_SHADOW => Kind::Shadow,
            k if k == KIND_TEXT => Kind::Text,
            k if k == KIND_IMAGE => Kind::Image,
            k if k == KIND_CANVAS => Kind::Canvas,
            k if k == KIND_LAYER_BEGIN => Kind::LayerBegin,
            k if k == KIND_LAYER => Kind::Layer,
            k if k == KIND_BACKDROP => Kind::Backdrop,
            k => Kind::Other(k),
        }
    }

    fn fill_type(&self) -> f32 {
        self.row(row::TYPES)[1]
    }

    fn clip_flags(&self) -> u32 {
        self.row(row::TYPES)[2] as u32
    }

    /// The four border colours, top, right, bottom, left.
    fn side_colors(&self) -> [[f32; 4]; 4] {
        std::array::from_fn(|i| self.row(row::SIDE_COLORS + i))
    }

    fn has_notch(&self) -> bool {
        self.row(row::NOTCH).iter().map(|v| v.abs()).sum::<f32>()
            + self.row(row::NOTCH + 1)[0]
            + self.row(row::NOTCH + 2)[0]
            > 0.0
    }

    /// Pack the ABI's signed corners and edge modifiers into the native notch slots.
    fn notch(&self, p: &mut GpuPrimitive, unit: f32) {
        let corners = self.row(row::NOTCH);
        p.corner_radius = corners.map(|v| v.abs() * unit);
        p.light = corners.map(|v| if v < 0.0 { 1.0 } else { 0.0 });
        let edge = |m: [f32; 4]| [m[0], m[1] * unit, m[2] * unit, m[3] * unit];
        p.perspective = edge(self.row(row::NOTCH + 1));
        p.sdf_3d = edge(self.row(row::NOTCH + 2));
        p.notch_info = [unit, 0.0, 0.0, 0.0];
        p.type_info[0] = PrimitiveType::Notch as u32;
    }

    /// The first thing in this record a primitive cannot yet carry.
    pub fn unsupported(&self) -> Option<Unsupported> {
        match self.kind() {
            Kind::Box | Kind::Shadow => {}
            Kind::Text if ![0.0, 1.0, 2.0].contains(&self.fill_type()) => {
                return Some(Unsupported::GlyphMode);
            }
            Kind::Text => {}
            // The producer emits plain images and unresolved bitmap mappings.
            Kind::Image if ![0.0, 3.0].contains(&self.fill_type()) => {
                return Some(Unsupported::ImageMapping);
            }
            Kind::Image => {}
            Kind::Canvas | Kind::LayerBegin | Kind::Layer | Kind::Backdrop => {}
            Kind::Other(k) => return Some(Unsupported::Kind(k)),
        }
        let [a, b, c, d] = self.row(row::AFFINE);
        let det = (a * d - b * c).abs();
        if det.is_nan() || det <= 1e-12 {
            return Some(Unsupported::Degenerate);
        }
        let shape = self.row(row::SHAPE + 1)[2];
        if shape != 0.0 && ![1.0, 2.0, 3.0].contains(&shape) {
            return Some(Unsupported::ClipShape);
        }
        if self.has_notch() && !matches!(self.kind(), Kind::Box | Kind::Shadow | Kind::Backdrop) {
            return Some(Unsupported::Notch);
        }
        if self.kind() == Kind::Box {
            // Solid, linear, radial and conic; anything else is not a fill this knows.
            if self.fill_type() > 3.5 {
                return Some(Unsupported::Gradient);
            }
        }
        None
    }

    /// The primitive that draws this record at `scale` device pixels per
    /// logical pixel, in the numbers `GpuPaintContext` produces for the same
    /// transform: the box keeps its centre and takes the transform's uniform
    /// scale, and the remainder of the transform is the normalized 2x2.
    ///
    /// A gradient with more than two stops, or stops other than from 0 to 1, or
    /// one that spreads past them, keeps them in a table; see
    /// [`Record::primitive`] for those and for shape clips that need auxiliary
    /// rows. This one reports either kind of record.
    pub fn to_primitive(&self, scale: f32) -> Result<GpuPrimitive, Unsupported> {
        if self.row(row::SHAPE + 1)[2] != 0.0 {
            return Err(Unsupported::ClipShape);
        }
        // A table in the list's tail, which there is no list here to find it in.
        if self.fill_type() > 0.5 && self.row(row::GRADIENT_OFFSETS)[3] >= 2.0 {
            return Err(Unsupported::GradientTable);
        }
        match self.primitive(scale, &[], 0)? {
            (p, table) if table.is_empty() => Ok(p),
            _ => Err(Unsupported::GradientTable),
        }
    }

    /// The primitive and its auxiliary rows: a shape clip, then a gradient stop
    /// table, when present. `list` includes the records and their trailing point
    /// and stop tables. `aux_base` is where these rows will begin in `aux_data`.
    pub fn primitive(
        &self,
        scale: f32,
        list: &[f32],
        aux_base: usize,
    ) -> Result<(GpuPrimitive, Vec<[f32; 4]>), Unsupported> {
        match self.kind() {
            // Images, canvases, layers and backdrops keep their paint order.
            Kind::Image => return Err(Unsupported::Kind(KIND_IMAGE)),
            Kind::Canvas => return Err(Unsupported::Kind(KIND_CANVAS)),
            Kind::LayerBegin => return Err(Unsupported::Kind(KIND_LAYER_BEGIN)),
            Kind::Layer => return Err(Unsupported::Kind(KIND_LAYER)),
            Kind::Backdrop => return Err(Unsupported::Kind(KIND_BACKDROP)),
            _ => {}
        }
        if let Some(why) = self.unsupported() {
            return Err(why);
        }
        let Placement {
            bounds,
            local_affine,
            size_scale,
            avg_scale,
            column_x,
            column_y,
        } = self.placement(scale);
        let [a, b] = [self.row(row::AFFINE)[0], self.row(row::AFFINE)[1]];

        let mul = |v: [f32; 4], k: f32| v.map(|f| f * k);
        let mut p = GpuPrimitive {
            bounds,
            corner_radius: mul(self.row(row::RADII), avg_scale),
            corner_shape: self.row(row::CORNER_SHAPE),
            local_affine,
            rotation: [b / column_x.max(1e-6), a / column_x.max(1e-6), 0.0, 1.0],
            ..GpuPrimitive::default()
        };

        let mut aux = self.clip_rows(scale, list)?;
        if !aux.is_empty() {
            p.clip_shape[0] = (aux_base + 1) as f32;
        }

        let flags = self.clip_flags();
        let [kind, _, _, clip_n] = self.row(row::TYPES);
        let clip_type = if flags & CLIP_SCREEN != 0 {
            ClipType::Rect
        } else {
            ClipType::None
        };
        if clip_type == ClipType::Rect {
            p.clip_bounds = mul(self.row(row::CLIP), scale);
            p.clip_radius = mul(self.row(row::CLIP_RADII), scale);
            p.clip_corner_shape = [clip_n; 4];
        } else {
            p.clip_bounds = NO_CLIP;
        }

        p.clip_fade = mul(self.row(row::FADE), scale);
        if p.clip_fade.iter().any(|f| *f > 0.0) {
            p.clip_fade_bounds = mul(self.row(row::FADE_RECT), scale);
        }

        // A clip in the record's own frame, which a turned parent clips its children by.
        // A shadow's rows hold its own, and it has none.
        if flags & CLIP_LOCAL != 0 && kind != KIND_SHADOW {
            p.local_clip_bounds = mul(self.row(row::SHADOW), size_scale);
            p.local_clip_radius = mul(self.row(row::SHADOW_COLOR), size_scale);
            p.clip_corner_shape = [clip_n; 4];
        }

        if kind == KIND_SHADOW {
            p.shadow = mul(self.row(row::SHADOW), size_scale);
            p.shadow_color = self.row(row::SHADOW_COLOR);
            p.type_info = [
                PrimitiveType::Shadow as u32,
                FillType::Solid as u32,
                clip_type as u32,
                0,
            ];
            if self.has_notch() {
                self.notch(&mut p, size_scale);
                p.notch_info[1] = if self.fill_type() > 0.5 { 2.0 } else { 1.0 };
            }
            return Ok((p, aux));
        }

        if kind == KIND_TEXT {
            // The atlas rect, in atlas pixels, is not scaled with the display.
            p.color = self.row(row::FILL);
            p.gradient_params = self.row(row::GRADIENT);
            p.type_info = [
                PrimitiveType::Text as u32,
                // ABI modes clip atlas rectangles; native glyph modes stay 0 and 1.
                match self.fill_type() as u32 {
                    0 => 4,
                    1 => 3,
                    _ => 2,
                },
                clip_type as u32,
                0,
            ];
            return Ok((p, aux));
        }

        p.color = self.row(row::FILL);
        p.color2 = self.row(row::FILL_END);
        let fill = match self.fill_type() {
            1.0 => FillType::LinearGradient,
            2.0 => FillType::RadialGradient,
            3.0 => FillType::ConicGradient,
            _ => FillType::Solid,
        };
        if fill != FillType::Solid {
            // Pixels from the box's top-left, taken to the primitive's frame.
            let g = self.row(row::GRADIENT);
            let (x, y) = (bounds[0] + g[0] * size_scale, bounds[1] + g[1] * size_scale);
            p.gradient_params = match fill {
                FillType::RadialGradient => [x, y, g[2] * size_scale, g[3]],
                // The third is an angle, which a scale does not change.
                FillType::ConicGradient => [x, y, g[2], g[3]],
                _ => [
                    x,
                    y,
                    bounds[0] + g[2] * size_scale,
                    bounds[1] + g[3] * size_scale,
                ],
            };
            if let Some(rows) = self.gradient_rows(list)? {
                p.gradient_table = [(aux_base + aux.len() + 1) as f32, 0.0, 0.0, 0.0];
                aux.extend(rows);
            }
        }

        let widths = self.row(row::BORDER);
        if widths.iter().any(|w| *w > 0.0) {
            p.border_color = self.row(row::BORDER_COLOR);
            // Sides of one colour need no more than `border_color`.
            let colors = self.side_colors();
            if colors.iter().any(|c| *c != colors[0]) {
                p.side_color_top = colors[0];
                p.side_color_right = colors[1];
                p.side_color_bottom = colors[2];
                p.side_color_left = colors[3];
            }
            // One width is the uniform form; the sides are scaled by the
            // axis they run along.
            p.border = if widths.iter().all(|w| *w == widths[0]) {
                [widths[0] * avg_scale, 0.0, 0.0, 0.0]
            } else {
                let (vertical, horizontal) = (scale * column_y, scale * column_x);
                [
                    widths[0] * vertical,
                    widths[1] * horizontal,
                    widths[2] * vertical,
                    widths[3] * horizontal,
                ]
            };
        }

        p.type_info = [PrimitiveType::Rect as u32, fill as u32, clip_type as u32, 0];
        if self.has_notch() {
            self.notch(&mut p, size_scale);
            // The ABI rings a notch with the top side's width and colour.
            p.border = [widths[0] * size_scale, 0.0, 0.0, 0.0];
            p.border_color = self.side_colors()[0];
        }
        Ok((p, aux))
    }

    /// The rows of this record's gradient stop table: its header of the stop
    /// count and spread, the average colour, then each stop's colour and offset.
    /// None for two stops from 0 to 1 that stop there, which the primitive's two
    /// colours say. A table the producer made is in `list` after the records; the
    /// stops it does not make a table of are made one here.
    fn gradient_rows(&self, list: &[f32]) -> Result<Option<Vec<[f32; 4]>>, Unsupported> {
        let [first, middle, last, marker] = self.row(row::GRADIENT_OFFSETS);
        let (c1, c2) = (self.row(row::FILL), self.row(row::FILL_END));
        let built = |stops: &[([f32; 4], f32)]| {
            let mean = std::array::from_fn(|c| {
                stops.iter().map(|s| s.0[c]).sum::<f32>() / stops.len() as f32
            });
            let mut rows = vec![[stops.len() as f32, 0.0, 0.0, 0.0], mean];
            for (color, offset) in stops {
                rows.push(*color);
                rows.push([*offset, 0.0, 0.0, 0.0]);
            }
            rows
        };
        if marker >= 2.0 {
            // Two more than the first row's index from the start of the list.
            let at = (marker - 2.0) as usize * 4;
            let count = *list.get(at).ok_or(Unsupported::Gradient)? as usize;
            let end = at + (2 + 2 * count) * 4;
            if count == 0 || end > list.len() {
                return Err(Unsupported::Gradient);
            }
            return Ok(Some(
                list[at..end]
                    .chunks_exact(4)
                    .map(|c| [c[0], c[1], c[2], c[3]])
                    .collect(),
            ));
        }
        if marker >= 0.5 {
            // A middle stop, kept in the row after the second colour.
            let via = self.row(row::GRADIENT_VIA);
            return Ok(Some(built(&[(c1, first), (via, middle), (c2, last)])));
        }
        if (first, last) == (0.0, 1.0) {
            return Ok(None);
        }
        Ok(Some(built(&[(c1, first), (c2, last)])))
    }
}
