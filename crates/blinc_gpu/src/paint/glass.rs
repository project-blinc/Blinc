//! Retain a glass panel in its foreground visual group.
use super::{GpuGlassPrimitive, GpuPaintContext};
use crate::primitives::{DynamicImage, LayerCommand};
use blinc_core::Rect;

/// A panel with owned clip coverage and a tight retained-image footprint.
#[derive(Clone, Debug)]
pub struct GroupGlass {
    pub primitive: GpuGlassPrimitive,
    pub paint: DynamicImage,
    pub frame_size: (u32, u32),
}

impl GpuPaintContext<'_> {
    /// Enable grouped glass when foreground dispatch supplies a prepared backdrop.
    pub fn enable_glass_layers(&mut self) {
        self.glass_layers = true;
    }

    pub(super) fn capture_group_glass(&mut self, primitive: GpuGlassPrimitive) -> bool {
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
                primitive,
                paint,
                frame_size,
            })));
        true
    }
}
