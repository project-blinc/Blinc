//! Update retained owning fills without touching their children or collected text.

use super::{RenderContext, RenderTree};

impl RenderContext {
    pub(super) fn apply_retained_background_colors(&mut self, tree: &RenderTree) {
        let Some(mut colors) = tree.retained_background_colors() else {
            self.invalidate_render_cache_tagged("retained_background_source_changed");
            return;
        };
        if colors.is_empty() {
            return;
        }
        colors.sort_unstable_by_key(|(id, _, dynamic)| (*dynamic, *id));
        let mut static_changed = false;
        for (dynamic, batch) in [
            (false, self.cached_bg_batch.as_mut()),
            (true, self.cached_dynamic_batch.as_mut()),
        ] {
            let Some(batch) = batch else { continue };
            for p in &mut batch.primitives {
                if p.node_slot[3] >= 0.0 {
                    continue;
                }
                let owner = (-p.node_slot[3]) as u32;
                if let Ok(index) =
                    colors.binary_search_by_key(&(dynamic, owner), |(id, _, d)| (*d, *id))
                    && p.color != colors[index].1
                {
                    p.color = colors[index].1;
                    static_changed |= !dynamic;
                }
            }
        }
        if static_changed {
            self.renderer.invalidate_static_layer();
        }
    }
}
