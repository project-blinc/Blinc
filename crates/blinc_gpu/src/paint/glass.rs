//! Retain glass and its backdrop scope in native visual groups.
use super::GpuPaintContext;
use crate::primitives::{DynamicImage, GpuGlassPrimitive, LayerCommand};
use blinc_core::Rect;

/// A recorded glass draw with owned clips and a tight retained-image footprint.
///
/// `GpuPaintContext` creates this command from glass brushes inside an accepted
/// glass layer. Element builders use `.glass()` or `.effect(GlassMaterial)`;
/// they do not need to construct this record or set its sampling scope.
#[derive(Clone, Debug)]
pub struct GroupGlass {
    /// Sample prior paint inside the enclosing glass owner's isolation when true.
    /// First-level panels use false to retain the prepared frame backdrop.
    /// This is CPU command metadata, not a GPU instance field.
    pub nested: bool,
    pub primitive: GpuGlassPrimitive,
    pub paint: DynamicImage,
    pub frame_size: (u32, u32),
}

impl GpuPaintContext<'_> {
    /// Enable retained glass for backdrop-aware ordered dispatch.
    pub fn enable_glass_layers(&mut self) {
        self.glass_layers = true;
    }

    pub(super) fn capture_group_glass(
        &mut self,
        primitive: GpuGlassPrimitive,
        nested: bool,
    ) -> bool {
        if !self.emit_foreground() || !self.layer_stack.iter().any(|s| s.captures_glass) {
            return false;
        }
        let [x, y, w, h] = primitive.bounds;
        let blur = primitive.params2[2] * 3.0 + 2.0;
        let dx = f32::from_bits(primitive.type_info[1]).abs();
        let dy = f32::from_bits(primitive.type_info[2]).abs();
        let min = [
            (x - blur - dx).floor().max(0.0),
            (y - blur - dy).floor().max(0.0),
        ];
        let max = [
            (x + w + blur + dx).ceil().min(self.viewport.width),
            (y + h + blur + dy).ceil().min(self.viewport.height),
        ];
        if max[0] <= min[0] || max[1] <= min[1] {
            return true;
        }
        let Some(mut paint) = self.group_gpu_paint() else {
            return true;
        };
        paint.dest = Rect::new(min[0], min[1], max[0] - min[0], max[1] - min[1]);
        let frame_size = (self.viewport.width as u32, self.viewport.height as u32);
        self.active_batch_mut()
            .push_foreground_layer_command(LayerCommand::Glass(Box::new(GroupGlass {
                nested,
                primitive,
                paint,
                frame_size,
            })));
        true
    }
}
