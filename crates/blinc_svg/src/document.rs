//! SVG document type and loading

use std::fs;
use std::path::Path as FilePath;

use blinc_core::{Brush, DrawContext, Path, PathCommand, Point, Rect, Stroke};
use usvg::{Options, Tree};

use crate::error::SvgError;
use crate::path::usvg_path_to_blinc;
use crate::style::{fill_to_brush, stroke_to_blinc};

/// A loaded and parsed SVG document
#[derive(Clone)]
pub struct SvgDocument {
    /// The underlying usvg tree
    tree: Tree,
    /// Original viewBox/size of the SVG
    pub width: f32,
    pub height: f32,
}

/// A drawing command extracted from the SVG
#[derive(Clone, Debug)]
pub enum SvgDrawCommand {
    /// Fill a path with a brush
    FillPath { path: Path, brush: Brush },
    /// Stroke a path with a stroke style and brush
    StrokePath {
        path: Path,
        stroke: Stroke,
        brush: Brush,
    },
}

impl SvgDocument {
    /// Load an SVG document from a file
    pub fn from_file(path: impl AsRef<FilePath>) -> Result<Self, SvgError> {
        let data = fs::read(path)?;
        Self::from_data(&data)
    }

    /// Load an SVG document from raw bytes
    pub fn from_data(data: &[u8]) -> Result<Self, SvgError> {
        let options = Options::default();
        let tree = Tree::from_data(data, &options).map_err(|e| SvgError::Parse(e.to_string()))?;

        let size = tree.size();

        Ok(Self {
            tree,
            width: size.width(),
            height: size.height(),
        })
    }

    /// Load an SVG document from a string
    pub fn from_str(svg_str: &str) -> Result<Self, SvgError> {
        Self::from_data(svg_str.as_bytes())
    }

    /// Get the original size of the SVG
    pub fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    /// Get the bounding box of the SVG content
    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width, self.height)
    }

    /// Extract all drawing commands from the SVG
    pub fn commands(&self) -> Vec<SvgDrawCommand> {
        let mut commands = Vec::new();
        self.extract_commands(self.tree.root(), &mut commands);
        commands
    }

    /// Recursively extract commands from the node tree
    fn extract_commands(&self, group: &usvg::Group, commands: &mut Vec<SvgDrawCommand>) {
        for child in group.children() {
            match child {
                usvg::Node::Group(g) => {
                    // Recurse into groups (transforms are handled per-path via abs_transform)
                    self.extract_commands(g, commands);
                }
                usvg::Node::Path(p) => {
                    // Convert path to Blinc path and apply the absolute transform
                    let blinc_path = usvg_path_to_blinc(p.data());
                    let transformed_path = apply_transform(&blinc_path, &p.abs_transform());

                    // Handle fill
                    if let Some(fill) = p.fill() {
                        if let Some(brush) = fill_to_brush(fill) {
                            commands.push(SvgDrawCommand::FillPath {
                                path: transformed_path.clone(),
                                brush,
                            });
                        }
                    }

                    // Handle stroke
                    if let Some(stroke) = p.stroke() {
                        if let Some((blinc_stroke, brush)) = stroke_to_blinc(stroke) {
                            commands.push(SvgDrawCommand::StrokePath {
                                path: transformed_path,
                                stroke: blinc_stroke,
                                brush,
                            });
                        }
                    }
                }
                usvg::Node::Image(_) => {
                    // TODO: Handle embedded images
                }
                usvg::Node::Text(_) => {
                    // Text is converted to paths by usvg
                }
            }
        }
    }

    /// Render the SVG to a DrawContext at the given position and scale
    pub fn render(&self, ctx: &mut dyn DrawContext, x: f32, y: f32, scale: f32) {
        let commands = self.commands();

        for cmd in commands {
            match cmd {
                SvgDrawCommand::FillPath { path, brush } => {
                    let scaled = scale_and_translate_path(&path, x, y, scale);
                    ctx.fill_path(&scaled, brush);
                }
                SvgDrawCommand::StrokePath {
                    path,
                    stroke,
                    brush,
                } => {
                    let scaled = scale_and_translate_path(&path, x, y, scale);
                    // Scale stroke width proportionally
                    let scaled_stroke = Stroke::new(stroke.width * scale)
                        .with_cap(stroke.cap)
                        .with_join(stroke.join);
                    ctx.stroke_path(&scaled, &scaled_stroke, brush);
                }
            }
        }
    }

    /// Render the SVG to fit within a given rectangle, maintaining aspect ratio
    pub fn render_fit(&self, ctx: &mut dyn DrawContext, bounds: Rect) {
        let scale_x = bounds.width() / self.width;
        let scale_y = bounds.height() / self.height;
        let scale = scale_x.min(scale_y);

        // Center within bounds
        let scaled_width = self.width * scale;
        let scaled_height = self.height * scale;
        let x = bounds.x() + (bounds.width() - scaled_width) / 2.0;
        let y = bounds.y() + (bounds.height() - scaled_height) / 2.0;

        self.render(ctx, x, y, scale);
    }
}

/// Apply a usvg Transform to a Blinc Path
fn apply_transform(path: &Path, transform: &usvg::Transform) -> Path {
    if transform.is_identity() {
        return path.clone();
    }

    let (sx, ky, kx, sy, tx, ty) = (
        transform.sx as f32,
        transform.ky as f32,
        transform.kx as f32,
        transform.sy as f32,
        transform.tx as f32,
        transform.ty as f32,
    );

    let transform_point =
        |p: Point| -> Point { Point::new(sx * p.x + kx * p.y + tx, ky * p.x + sy * p.y + ty) };

    let new_commands: Vec<PathCommand> = path
        .commands()
        .iter()
        .map(|cmd| match cmd {
            PathCommand::MoveTo(p) => PathCommand::MoveTo(transform_point(*p)),
            PathCommand::LineTo(p) => PathCommand::LineTo(transform_point(*p)),
            PathCommand::QuadTo { control, end } => PathCommand::QuadTo {
                control: transform_point(*control),
                end: transform_point(*end),
            },
            PathCommand::CubicTo {
                control1,
                control2,
                end,
            } => PathCommand::CubicTo {
                control1: transform_point(*control1),
                control2: transform_point(*control2),
                end: transform_point(*end),
            },
            PathCommand::ArcTo {
                radii,
                rotation,
                large_arc,
                sweep,
                end,
            } => PathCommand::ArcTo {
                radii: *radii,
                rotation: *rotation,
                large_arc: *large_arc,
                sweep: *sweep,
                end: transform_point(*end),
            },
            PathCommand::Close => PathCommand::Close,
        })
        .collect();

    Path::from_commands(new_commands)
}

/// Scale and translate a path for rendering
fn scale_and_translate_path(path: &Path, x: f32, y: f32, scale: f32) -> Path {
    if scale == 1.0 && x == 0.0 && y == 0.0 {
        return path.clone();
    }

    let transform_point = |p: Point| -> Point { Point::new(p.x * scale + x, p.y * scale + y) };

    let new_commands: Vec<PathCommand> = path
        .commands()
        .iter()
        .map(|cmd| match cmd {
            PathCommand::MoveTo(p) => PathCommand::MoveTo(transform_point(*p)),
            PathCommand::LineTo(p) => PathCommand::LineTo(transform_point(*p)),
            PathCommand::QuadTo { control, end } => PathCommand::QuadTo {
                control: transform_point(*control),
                end: transform_point(*end),
            },
            PathCommand::CubicTo {
                control1,
                control2,
                end,
            } => PathCommand::CubicTo {
                control1: transform_point(*control1),
                control2: transform_point(*control2),
                end: transform_point(*end),
            },
            PathCommand::ArcTo {
                radii,
                rotation,
                large_arc,
                sweep,
                end,
            } => PathCommand::ArcTo {
                radii: blinc_core::Vec2::new(radii.x * scale, radii.y * scale),
                rotation: *rotation,
                large_arc: *large_arc,
                sweep: *sweep,
                end: transform_point(*end),
            },
            PathCommand::Close => PathCommand::Close,
        })
        .collect();

    Path::from_commands(new_commands)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_svg() {
        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <rect x="10" y="10" width="80" height="80" fill="red"/>
            </svg>
        "#;

        let doc = SvgDocument::from_str(svg).unwrap();
        assert_eq!(doc.width, 100.0);
        assert_eq!(doc.height, 100.0);

        let commands = doc.commands();
        assert!(!commands.is_empty());
    }

    #[test]
    fn test_parse_path_svg() {
        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <path d="M10,10 L90,10 L90,90 L10,90 Z" fill="blue" stroke="black" stroke-width="2"/>
            </svg>
        "#;

        let doc = SvgDocument::from_str(svg).unwrap();
        let commands = doc.commands();

        // Should have both fill and stroke commands
        assert!(commands.len() >= 2);
    }

    #[test]
    fn test_parse_circle_svg() {
        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
                <circle cx="50" cy="50" r="40" fill="green"/>
            </svg>
        "#;

        let doc = SvgDocument::from_str(svg).unwrap();
        let commands = doc.commands();
        assert!(!commands.is_empty());
    }

    #[test]
    fn test_fill_only_no_stroke() {
        // SVG with fill but no stroke - should NOT have a stroke command
        let svg = r#"
            <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24">
                <path d="M10 8 L18 12 L10 16 Z" fill="white"/>
            </svg>
        "#;

        let doc = SvgDocument::from_str(svg).unwrap();
        let commands = doc.commands();

        // Should have exactly 1 command (fill only)
        let fill_count = commands
            .iter()
            .filter(|c| matches!(c, SvgDrawCommand::FillPath { .. }))
            .count();
        let stroke_count = commands
            .iter()
            .filter(|c| matches!(c, SvgDrawCommand::StrokePath { .. }))
            .count();

        assert_eq!(fill_count, 1, "Should have exactly 1 fill command");
        assert_eq!(
            stroke_count, 0,
            "Should have NO stroke commands when stroke is not specified"
        );
    }
}
