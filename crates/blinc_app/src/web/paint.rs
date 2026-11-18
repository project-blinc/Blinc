//! Classify browser changes before reusing retained frame content.

use super::WebApp;

impl WebApp {
    pub(super) fn apply_frame_updates(&mut self, source: &'static str) {
        let Some(tree) = self.current_tree.as_mut() else {
            return;
        };
        let updates = tree.take_pending_property_updates();
        if updates.is_empty() && !blinc_layout::has_pending_subtree_rebuilds() {
            return;
        }

        let revision = tree.property_paint_revision();
        let effects = tree.apply_partial_property_updates(updates);
        let rebuilt = tree.process_pending_subtree_rebuilds_routed(Some(&self.ctx.event_router));
        let needs_layout = effects.needs_layout || rebuilt;
        let needs_paint = needs_layout || tree.property_paint_revision() != revision;

        if needs_paint {
            self.blinc_app.invalidate_render_cache_tagged(source);
            self.state_styles.force();
        }
        if needs_layout {
            tree.apply_stylesheet_layout_overrides();
            tree.compute_layout(self.ctx.width, self.ctx.height);
            tree.apply_flip_transitions();
            tree.update_flip_bounds();
            self.render_state.begin_stable_motion_frame();
            tree.initialize_motion_animations(&mut self.render_state);
            self.render_state.end_stable_motion_frame();
            self.render_state.process_global_motion_replays();
            tree.start_all_css_animations();
        }
    }

    pub(super) fn invalidate_explicit_paint(&mut self) {
        if blinc_layout::peek_needs_full_paint() {
            self.blinc_app.invalidate_render_cache_tagged("web_repaint");
        }
    }

    pub(super) fn can_reuse_frame_paint(&self, theme_animating: bool) -> bool {
        self.blinc_app.has_render_cache()
            && !theme_animating
            && !self.render_state.has_active_motions()
            && self.current_tree.as_ref().is_some_and(|tree| {
                !tree.animation_needs_full_paint()
                    && !tree.has_active_visual_animations()
                    && !tree.has_active_layout_animations()
                    && !tree.scroll_paint_stale()
                    && self.ctx.pointer_query.is_empty()
            })
    }
}
