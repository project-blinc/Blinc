//! When an auto-hiding scrollbar starts its fade.

use super::{ScrollPhysics, ScrollbarState, ScrollbarVisibility};

impl ScrollPhysics {
    /// Whether the scrollbar shows and hides by itself, so its opacity changes while the content
    /// does not.
    pub fn scrollbar_fades(&self) -> bool {
        matches!(
            self.config.scrollbar.visibility,
            ScrollbarVisibility::Auto | ScrollbarVisibility::Hover
        )
    }

    /// Seconds until a shown scrollbar starts to fade, when it is waiting to. The fade starts on
    /// the first tick at or after that, so the runner must draw a frame then even if nothing
    /// else asks for one.
    pub fn scrollbar_hide_in(&self) -> Option<f32> {
        if self.scrollbar_state != ScrollbarState::Scrolling {
            return None;
        }
        let waits = match self.config.scrollbar.visibility {
            ScrollbarVisibility::Auto => true,
            ScrollbarVisibility::Hover => !self.area_hovered,
            _ => false,
        };
        waits.then(|| (self.config.scrollbar.auto_dismiss_delay - self.idle_time).max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(visibility: ScrollbarVisibility) -> ScrollPhysics {
        let mut physics = ScrollPhysics::default();
        physics.config.scrollbar.visibility = visibility;
        physics.on_scroll_activity();
        physics
    }

    #[test]
    fn a_shown_auto_scrollbar_is_due_to_fade_after_the_delay() {
        let physics = shown(ScrollbarVisibility::Auto);
        let delay = physics.config.scrollbar.auto_dismiss_delay;
        assert_eq!(physics.scrollbar_hide_in(), Some(delay));
    }

    #[test]
    fn one_tick_after_a_long_idle_starts_the_fade() {
        let mut physics = shown(ScrollbarVisibility::Auto);
        let delay = physics.config.scrollbar.auto_dismiss_delay;
        physics.tick_scrollbar(delay + 0.5);
        assert_eq!(physics.scrollbar_state, ScrollbarState::FadingOut);
        assert_eq!(physics.scrollbar_hide_in(), None);
    }

    #[test]
    fn scrollbars_that_do_not_hide_by_themselves_are_never_due() {
        for visibility in [ScrollbarVisibility::Always, ScrollbarVisibility::Never] {
            let physics = shown(visibility);
            assert!(!physics.scrollbar_fades());
            assert_eq!(physics.scrollbar_hide_in(), None, "{visibility:?}");
        }
        let mut hovered = shown(ScrollbarVisibility::Hover);
        hovered.area_hovered = true;
        assert_eq!(hovered.scrollbar_hide_in(), None);
    }
}
