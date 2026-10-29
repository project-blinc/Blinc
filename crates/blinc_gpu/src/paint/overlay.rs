//! Give inline portals a late owner with their supported native ancestor state.
use super::{GpuPaintContext, LayerState};
use crate::primitives::cross_stream::GroupId;

pub(super) struct OverlayLayerScope {
    layers: Vec<LayerState>,
    static_motion_depth: Option<u32>,
    static_group_depth: usize,
    copied: usize,
}

impl GpuPaintContext<'_> {
    /// Enter an inline portal, retaining native ancestors in a separate late scope.
    pub fn push_overlay_subtree(&mut self) {
        if self.overlay_subtree_depth == 0
            && self.active_composite_layer.is_none()
            && !self.layer_stack.is_empty()
            && self
                .layer_stack
                .iter()
                .all(|state| matches!(state.cross_stream, Some((GroupId::Node(_), ..))))
        {
            self.overlay_layer_scope = Some(OverlayLayerScope {
                layers: std::mem::take(&mut self.layer_stack),
                static_motion_depth: self.static_motion_depth.take(),
                static_group_depth: std::mem::take(&mut self.cross_stream_static_depth),
                copied: 0,
            });
        }
        self.overlay_subtree_depth = self.overlay_subtree_depth.saturating_add(1);
    }

    pub(super) fn copy_overlay_ancestors(&mut self, root: u64) {
        let Some(scope) = &self.overlay_layer_scope else {
            return;
        };
        if scope.copied > 0 {
            return;
        }
        let ancestors: Vec<_> = scope
            .layers
            .iter()
            .map(|state| (state.cross_stream.unwrap().0, state.config.clone()))
            .collect();
        self.overlay_layer_scope.as_mut().unwrap().copied = ancestors.len();
        for (ancestor, config) in ancestors {
            // Reuse each scope's IDs across the background and foreground walks.
            let id = *self
                .overlay_ancestor_ids
                .entry((root, ancestor))
                .or_insert_with(|| {
                    let id = GroupId::Local(self.next_cross_stream_local);
                    self.next_cross_stream_local += 1;
                    id
                });
            self.push_native_layer(config, Some(id));
        }
    }

    /// Leave the portal and restore ordinary ancestor command ownership.
    pub fn pop_overlay_subtree(&mut self) {
        self.overlay_subtree_depth = self.overlay_subtree_depth.saturating_sub(1);
        if self.overlay_subtree_depth == 0
            && let Some(scope) = self.overlay_layer_scope.take()
        {
            debug_assert_eq!(self.layer_stack.len(), scope.copied);
            for _ in 0..scope.copied {
                self.pop_native_layer();
            }
            self.layer_stack = scope.layers;
            self.static_motion_depth = scope.static_motion_depth;
            self.cross_stream_static_depth = scope.static_group_depth;
        }
    }
}
