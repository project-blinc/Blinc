//! ABI clip frames and points, kept together in auxiliary rows.

use super::{Record, Unsupported, row};

impl Record<'_> {
    pub(super) fn clip_rows(&self, scale: f32, list: &[f32]) -> Result<Vec<[f32; 4]>, Unsupported> {
        let info = self.row(row::SHAPE + 1);
        if info[2] == 0.0 {
            return Ok(Vec::new());
        }
        let mut shape = self.row(row::SHAPE + 2);
        let mut points: Vec<[f32; 4]> = Vec::new();
        if info[2] == 3.0 {
            let [first, count, _, _] = shape;
            if !first.is_finite()
                || first < 0.0
                || first.fract() != 0.0
                || !count.is_finite()
                || count < 2.0
                || count.fract() != 0.0
            {
                return Err(Unsupported::ClipShape);
            }
            let start = (first as usize)
                .checked_mul(4)
                .ok_or(Unsupported::ClipShape)?;
            let len = (count as usize)
                .div_ceil(2)
                .checked_mul(4)
                .ok_or(Unsupported::ClipShape)?;
            let end = start.checked_add(len).ok_or(Unsupported::ClipShape)?;
            let data = list.get(start..end).ok_or(Unsupported::ClipShape)?;
            points.extend(data.chunks_exact(4).map(|v| [v[0], v[1], v[2], v[3]]));
            // Relative to this header, so a run's points need no separate rebasing.
            shape[0] = 3.0;
        }
        let mut out = vec![self.row(row::SHAPE).map(|v| v / scale), info, shape];
        out.extend(points);
        Ok(out)
    }
}

/// Move a screen-to-element frame when the target origin moves by `(dx, dy)`.
pub(super) fn translate(rows: &mut [[f32; 4]], dx: f32, dy: f32) {
    let [a, b, c, d] = rows[0];
    rows[1][0] -= a * dx + c * dy;
    rows[1][1] -= b * dx + d * dy;
}
