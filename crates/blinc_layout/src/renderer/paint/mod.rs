//! Painting + render-walker machinery on `RenderTree`.
//!
//! Build-side concerns (tree creation, change analysis, subtree
//! rebuilds) live in [`crate::renderer::build`] —
//! this module is everything that walks the populated `RenderTree`
//! and emits draw calls or collects artefacts.
//!
//! Six submodules organised by walker:
//!
//! - [`basic`] — `render` + `render_node` + `render_layered_simple`.
//!   Single-context walker, no motion-pre-replay. Glass passes
//!   through `Brush::Glass`.
//! - [`layered`] — `render_layered` + `render_to_layer` +
//!   `render_layer`. Layer-aware walker that splits primitives across
//!   background / glass / foreground contexts.
//! - [`motion`] — `render_with_motion` + `render_layer_with_motion`.
//!   Production desktop / mobile paint surface — folds in motion
//!   pre-replay, CSS layer effects, viewport culling, 3D-SDF group
//!   collection, mask gradient + clip-path resolution.
//! - [`layout_renderer`] — `render_to<R: LayoutRenderer>` +
//!   `render_layer_with_content` + the LayoutRenderer-trait text /
//!   svg recursives. Alternate paint surface routed through a
//!   user-supplied frontend instead of a `DrawContext`.
//! - [`collect`] — non-painting walkers that gather per-node
//!   artefacts (`text_elements`, `svg_elements`, the deprecated
//!   `collect_glass_panels`).
//! - [`helpers`] — small shared helpers (`extract_mask_alphas`,
//!   `has_glass`, `apply_opacity_to_brush`).

pub mod basic;
mod canvas_order;
pub mod collect;
mod cross_stream;
pub mod helpers;
pub mod layered;
pub mod layout_renderer;
pub mod motion;
