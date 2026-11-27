//! Images that show a glass or blur placeholder while they load.
//!
//! Whether an image has loaded is known to whoever draws it. It says so here before a paint,
//! and the paint walk then groups a loading image as it groups a glass element, so the
//! placeholder samples what is behind it inside groups and motion containers. A loaded image
//! keeps the cheaper paths a plain image takes.

use super::{ElementType, RenderTree};
use crate::tree::{LayoutNodeId, NodeSet};
use blinc_core::Brush;
use std::sync::Arc;

/// The image nodes with such a placeholder and their sources, at a revision of the nodes.
pub(super) type Found = Option<(u64, Arc<[(LayoutNodeId, String)]>)>;

impl RenderTree {
    /// The image nodes whose placeholder samples what is behind it, with their sources.
    pub fn backdrop_placeholder_images(&self) -> Arc<[(LayoutNodeId, String)]> {
        let revision = self.render_nodes.revision();
        if let Some((at, found)) = self.backdrop_placeholders.borrow().as_ref()
            && *at == revision
        {
            return Arc::clone(found);
        }
        let found: Arc<[_]> = self
            .render_nodes
            .iter()
            .filter_map(|(node, render)| {
                let ElementType::Image(image) = &render.element_type else {
                    return None;
                };
                let kind = render
                    .props
                    .placeholder_type
                    .unwrap_or(image.placeholder_type);
                let samples = matches!(
                    image.placeholder_brush.as_deref(),
                    Some(Brush::Glass(_) | Brush::Blur(_))
                );
                (kind == 4 && samples).then(|| (*node, image.source.clone()))
            })
            .collect();
        *self.backdrop_placeholders.borrow_mut() = Some((revision, Arc::clone(&found)));
        found
    }

    /// Says which of [`Self::backdrop_placeholder_images`] have not loaded. The next paint
    /// draws their placeholders as glass.
    pub fn set_pending_backdrop_images(&self, nodes: impl IntoIterator<Item = LayoutNodeId>) {
        *self.pending_backdrop_images.borrow_mut() = nodes.into_iter().collect();
    }
}
