//! Stylesheet matching + application on `RenderTree`.
//!
//! The stylesheet pass is layered across five submodules:
//!
//! - [`selectors`] — matching engine: `compound_matches`,
//!   `complex_selector_matches`, `selector_specificity`, plus the
//!   per-frame `apply_complex_selector_styles` /
//!   `apply_svg_tag_styles` driver passes that consume them.
//! - [`apply`] — `ElementStyle` → `RenderProps` / `taffy::Style`
//!   projection (`apply_element_style_to_props`,
//!   `apply_element_style_to_taffy`), keyframe-property snapshots
//!   for transition detection, and `clip-path` resolution.
//! - [`base`] — `apply_stylesheet_base_styles` (full-tree pass after
//!   stylesheet set) and `apply_stylesheet_base_styles_for_subtree`
//!   (post-rebuild pass). Apply complex non-state rules sorted by
//!   specificity, then `#id` rules, then SVG tag rules, then
//!   propagate inherited text properties parent → child.
//! - [`state`] — `apply_state_styles` (single-node), the per-frame
//!   `apply_stylesheet_state_styles` driver, `apply_pointer_styles`
//!   (`calc(env(pointer-x), ...)` evaluation), and the
//!   `css_has_visible_transitions` redraw gate.
//! - [`layout`] — `apply_stylesheet_layout_overrides` (CSS layout
//!   props → taffy style before `compute_layout`),
//!   `apply_all_stylesheet_styles` (combined visual + layout fast
//!   path), and `auto_create_css_scroll_physics` for nodes with
//!   `overflow: scroll`.
//!
//! `set_stylesheet` / `set_stylesheet_arc` / `stylesheet()` accessors
//! stay in `renderer/mod.rs` next to the other small accessors on the
//! struct.

pub mod apply;
pub mod base;
pub mod classes;
pub mod layout;
pub mod selectors;
pub mod state;
