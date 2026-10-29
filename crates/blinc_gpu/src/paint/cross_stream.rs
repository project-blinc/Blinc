//! Capture paired native stream brackets with one logical group owner.
use super::aux_data::AuxBatch;
use super::{GpuPaintContext, LayerState};
use crate::primitives::LayerCommand;
use crate::primitives::cross_stream::{Capture, GroupId};
use blinc_core::{BlendMode, LayerConfig};

#[derive(Clone, Copy)]
enum DeferredNode {
    Text,
    Svg,
    Image,
}

impl GpuPaintContext<'_> {
    /// Enable collected text markers when the caller resolves them before dispatch.
    pub fn enable_deferred_text(&mut self) {
        self.deferred_text = true;
    }

    pub(super) fn capture_text(&mut self, id: u64) -> bool {
        self.capture_deferred_node(id, DeferredNode::Text)
    }

    /// Enable SVG markers when the caller resolves raster images before dispatch.
    pub fn enable_deferred_svg(&mut self) {
        self.deferred_svg = true;
    }

    pub(super) fn capture_svg(&mut self, id: u64) -> bool {
        self.capture_deferred_node(id, DeferredNode::Svg)
    }

    /// Enable image markers when the caller resolves loaded textures before dispatch.
    pub fn enable_deferred_image(&mut self) {
        self.deferred_image = true;
    }

    pub(super) fn capture_image(&mut self, id: u64) -> bool {
        self.capture_deferred_node(id, DeferredNode::Image)
    }

    fn capture_deferred_node(&mut self, id: u64, kind: DeferredNode) -> bool {
        let enabled = match kind {
            DeferredNode::Text => self.deferred_text,
            DeferredNode::Svg => self.deferred_svg,
            DeferredNode::Image => self.deferred_image,
        };
        if !enabled
            || self.active_composite_layer.is_some()
            || !self.layer_stack.iter().any(|s| s.cross_stream.is_some())
        {
            return false;
        }
        let opacity = self.combined_opacity();
        let command = match kind {
            DeferredNode::Text => LayerCommand::DeferredText { id, opacity },
            DeferredNode::Svg => LayerCommand::DeferredSvg { id, opacity },
            DeferredNode::Image => LayerCommand::DeferredImage { id, opacity },
        };
        let foreground = self.emit_foreground();
        let owner = self.layer_command_batch();
        let batch = self.layer_command_batch_mut(owner);
        match kind {
            DeferredNode::Text => {
                batch.cross_stream_groups.text_nodes.insert(id);
            }
            DeferredNode::Svg => {
                batch.cross_stream_groups.svg_nodes.insert(id);
            }
            DeferredNode::Image => {
                batch.cross_stream_groups.image_nodes.insert(id);
            }
        }
        if foreground {
            batch.push_foreground_layer_command(command);
        } else {
            batch.push_layer_command(command);
        }
        true
    }

    pub(super) fn begin_cross_stream_layer(&mut self, id: u64, config: LayerConfig) -> bool {
        let overlay_order = self.overlay_root_pending.take();
        self.copy_overlay_ancestors(id);
        if self.active_composite_layer.is_some()
            || self.rejected_cross_stream_groups.contains(&id)
            || self.layer_stack.iter().any(|s| s.cross_stream.is_none())
        {
            self.rejected_cross_stream_groups.insert(id);
            return false;
        }
        self.push_native_layer(config, Some(GroupId::Node(id)));
        self.layer_stack.last_mut().unwrap().overlay_order = overlay_order;
        true
    }

    pub(super) fn push_native_layer(&mut self, config: LayerConfig, shared: Option<GroupId>) {
        let shared = shared.or_else(|| {
            if self.active_composite_layer.is_none()
                && self.layer_stack.iter().any(|s| s.cross_stream.is_some())
            {
                let id = self.next_cross_stream_local;
                self.next_cross_stream_local += 1;
                Some(GroupId::Local(id))
            } else {
                None
            }
        });
        // Moving groups keep their static paint anchor; portals retain their late owner.
        let retain_static = shared.is_some()
            && self.aux_batch() == AuxBatch::Dynamic
            && self.overlay_subtree_depth == 0;
        if retain_static {
            self.cross_stream_static_depth += 1;
        }
        let owner = self.layer_command_batch();
        let foreground = self.emit_foreground();
        let batch = self.layer_command_batch_mut(owner);
        let cross_stream = shared.map(|id| {
            (
                id,
                batch.layer_commands.len(),
                batch.foreground_layer_commands.len(),
                foreground,
            )
        });
        let state = LayerState {
            captures_glass: false,
            overlay_order: None,
            cross_stream_static: retain_static,
            cross_stream,
            config: config.clone(),
            command_batch: owner,
            foreground,
            primitive_start: batch.primitive_count(),
            foreground_primitive_start: batch.foreground_primitive_count(),
            path_start: batch.path_vertex_count(),
            foreground_path_start: batch.foreground_path_vertex_count(),
            parent_state_indices: (
                self.transform_stack.len(),
                self.opacity_stack.len(),
                self.blend_mode_stack.len(),
                self.clip_stack.len(),
            ),
        };
        self.layer_stack.push(state);
        if config.blend_mode != BlendMode::Normal {
            self.blend_mode_stack.push(config.blend_mode);
        }
        let batch = self.layer_command_batch_mut(owner);
        if shared.is_some() {
            batch.push_layer_command(LayerCommand::Push {
                config: config.clone(),
            });
            batch.push_foreground_layer_command(LayerCommand::Push { config });
        } else if foreground {
            batch.push_foreground_layer_command(LayerCommand::Push { config });
        } else {
            batch.push_layer_command(LayerCommand::Push { config });
        }
    }

    pub(super) fn pop_native_layer(&mut self) {
        let Some(state) = self.layer_stack.pop() else {
            return;
        };
        let (transform, opacity, blend, clip) = state.parent_state_indices;
        if self.transform_stack.len() > transform {
            self.transform_stack.truncate(transform.max(1));
        }
        if self.opacity_stack.len() > opacity {
            self.opacity_stack.truncate(opacity.max(1));
        }
        if self.blend_mode_stack.len() > blend {
            self.blend_mode_stack.truncate(blend.max(1));
        }
        if self.clip_stack.len() > clip {
            self.clip_stack.truncate(clip);
        }
        let batch = self.layer_command_batch_mut(state.command_batch);
        if let Some((id, background, foreground, foreground_owner)) = state.cross_stream {
            batch.push_layer_command(LayerCommand::Pop);
            batch.push_foreground_layer_command(LayerCommand::Pop);
            batch.cross_stream_groups.captures.push(Capture {
                id,
                foreground_owner,
                overlay_order: state.overlay_order,
                background: background..batch.layer_commands.len(),
                foreground: foreground..batch.foreground_layer_commands.len(),
            });
        } else if state.foreground {
            batch.push_foreground_layer_command(LayerCommand::Pop);
        } else {
            batch.push_layer_command(LayerCommand::Pop);
        }
        if state.cross_stream_static {
            self.cross_stream_static_depth -= 1;
        }
    }
}
