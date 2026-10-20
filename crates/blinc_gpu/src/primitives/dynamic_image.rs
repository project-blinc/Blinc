//! Screen-space geometry for native raw-image quads.
use super::DynamicImage;
use blinc_core::Rect;

fn bounds(dest: Rect, transform: [f32; 4]) -> Rect {
    let [a, b, c, d] = transform;
    let width = a.abs() * dest.width() + c.abs() * dest.height();
    let height = b.abs() * dest.width() + d.abs() * dest.height();
    Rect::new(
        dest.x() + dest.width() * 0.5 - width * 0.5,
        dest.y() + dest.height() * 0.5 - height * 0.5,
        width,
        height,
    )
}

impl DynamicImage {
    /// Axis-aligned screen bounds of the transformed quad, including reflection.
    pub fn screen_bounds(&self) -> Rect {
        bounds(self.dest, self.transform)
    }

    /// Reject invalid geometry before allocating pixels or GPU resources.
    pub(crate) fn valid_geometry(dest: Rect, transform: [f32; 4]) -> bool {
        let [a, b, c, d] = transform;
        let determinant = a * d - b * c;
        let rect = bounds(dest, transform);
        dest.width() > 0.0
            && dest.height() > 0.0
            && determinant.is_finite()
            && determinant.abs() > 1e-12
            && transform.iter().all(|v| v.is_finite())
            && [
                dest.x(),
                dest.y(),
                dest.width(),
                dest.height(),
                rect.x(),
                rect.y(),
                rect.width(),
                rect.height(),
                rect.x() + rect.width(),
                rect.y() + rect.height(),
            ]
            .iter()
            .all(|v| v.is_finite())
    }
}
