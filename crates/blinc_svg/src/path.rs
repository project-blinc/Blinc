//! SVG path conversion to Blinc Path

use blinc_core::{Path, PathCommand, Point};

/// Convert usvg path data to Blinc Path
pub fn usvg_path_to_blinc(path_data: &usvg::tiny_skia_path::Path) -> Path {
    let mut commands = Vec::new();

    for segment in path_data.segments() {
        match segment {
            usvg::tiny_skia_path::PathSegment::MoveTo(p) => {
                commands.push(PathCommand::MoveTo(Point::new(p.x, p.y)));
            }
            usvg::tiny_skia_path::PathSegment::LineTo(p) => {
                commands.push(PathCommand::LineTo(Point::new(p.x, p.y)));
            }
            usvg::tiny_skia_path::PathSegment::QuadTo(c, e) => {
                commands.push(PathCommand::QuadTo {
                    control: Point::new(c.x, c.y),
                    end: Point::new(e.x, e.y),
                });
            }
            usvg::tiny_skia_path::PathSegment::CubicTo(c1, c2, e) => {
                commands.push(PathCommand::CubicTo {
                    control1: Point::new(c1.x, c1.y),
                    control2: Point::new(c2.x, c2.y),
                    end: Point::new(e.x, e.y),
                });
            }
            usvg::tiny_skia_path::PathSegment::Close => {
                commands.push(PathCommand::Close);
            }
        }
    }

    Path::from_commands(commands)
}
