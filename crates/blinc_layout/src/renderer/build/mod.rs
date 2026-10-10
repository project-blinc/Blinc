//! Tree construction + incremental update on `RenderTree`.
//!
//! Build-side concerns split across six submodules:
//!
//! - [`entry`] — public entry points: `from_element`,
//!   `from_element_with_registry`, `update_if_changed`,
//!   `incremental_update`. These are the user-facing build surface.
//! - [`collect`] — `build_element` + the `collect_render_props*`
//!   family that walk an `ElementBuilder` and populate `RenderTree`.
//! - [`element_type`] — `determine_element_type` / `_boxed`
//!   projection from an `ElementBuilder` to the `ElementType` enum
//!   stored on every render node.
//! - [`text`] — text-property inheritance from parent → child
//!   (`inherit_text_props_from_parent`) and `TextData` materialisation
//!   (`build_text_data`).
//! - [`diff`] — `analyze_changes` change classification,
//!   `props_visually_equal` per-node fast comparator, and
//!   `update_render_props_in_place` / `rebuild_children_in_place`
//!   patches that follow the diff.
//! - [`regions`] — `reconcile_region`, bringing the rows of a list or
//!   branch up to date in place.
//! - [`subtree`] — `rebuild_changed_subtrees`, `rebuild_children`,
//!   `remove_subtree_nodes`, `process_pending_subtree_rebuilds`, and
//!   the `update_subtree_props_*` props-only fast path.

pub mod collect;
pub mod diff;
pub mod element_type;
pub mod entry;
pub mod regions;
pub mod subtree;
pub mod text;
