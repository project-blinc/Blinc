// Matches the workspace convention: `'_` vs elided lifetimes are used
// interchangeably across these signatures.
#![allow(mismatched_lifetime_syntaxes)]
//! Blinc's style model and its CSS front end.
//!
//! This crate holds the vocabulary a stylesheet speaks in and the parser
//! that produces it. It sits below the layout engine: nothing here knows
//! about layout trees, elements, or rendering, so a stylesheet can be
//! parsed and inspected without building a UI.
//!
//! # Layout
//!
//! - [`element_style`] — [`ElementStyle`], the resolved style schema every
//!   visual property lands in, plus the `css!` and `style!` macros.
//! - [`parser`] — the CSS subset parser: selectors, at-rules, properties,
//!   value grammars, and the `@flow` shader DAG.
//! - [`calc`] — `calc()` expression trees and their evaluation context.
//! - [`units`] — [`Length`], the length/percentage model.
//! - [`material`] — surface materials (glass, metal, wood) and cursors.
//! - [`motion`] — enter/exit keyframes.
//! - [`text`] — text alignment and font weight.
//! - [`mod@pointer`] — pointer-space configuration for `env(pointer-*)`.
//!
//! # Why the style model lives with the parser
//!
//! [`ElementStyle`] stores CSS animation and transition declarations, and
//! the parser writes every property back into an [`ElementStyle`]. The two
//! reference each other's types, so they form one compilation unit.

pub mod calc;
pub mod element_style;
pub mod material;
pub mod motion;
pub mod parser;
pub mod pointer;
pub mod text;
pub mod units;

pub use calc::{CalcContext, CalcExpr, CalcUnit, parse_calc};
pub use element_style::{ElementStyle, style};
pub use material::{
    CursorStyle, GlassMaterial, Material, MaterialShadow, MetallicMaterial, RenderLayer,
    SolidMaterial, WoodMaterial,
};
pub use motion::{MotionAnimation, MotionKeyframe};
pub use parser::{ParseError, Severity, Stylesheet, active_stylesheet, set_active_stylesheet};
pub use pointer::{PointerOrigin, PointerSpace, PointerSpaceConfig};
pub use text::{FontWeight, TextAlign};
pub use units::{Length, Unit, pct, px, sp};
