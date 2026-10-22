//! Adjust primitive references when concatenating auxiliary buffers.
use super::{ClipType, GpuPrimitive, PrimitiveType};
impl GpuPrimitive {
    /// Move this primitive's auxiliary references after `shift` existing rows.
    /// Internal offsets within a shape header remain relative to that header.
    pub fn rebase_aux(&mut self, shift: f32) {
        if shift == 0.0 {
            return;
        }
        let group = self.type_info[0] == PrimitiveType::Rect as u32
            && self.perspective[3] == 6.0
            && self.border[1] > 0.5;
        if self.type_info[0] == PrimitiveType::Mesh as u32 || group {
            self.border[2] += shift;
        }
        if self.type_info[2] == ClipType::Polygon as u32 {
            self.clip_radius[3] += shift;
        }
        if self.clip_shape[0] > 0.5 {
            self.clip_shape[0] += shift;
        }
        if self.gradient_table[0] > 0.5 {
            self.gradient_table[0] += shift;
        }
    }
}
