//! The declarative style macros.
//!
//! All three are `#[macro_export]`ed, so they live at the crate root
//! whichever module defines them. They reach each other through `$crate::`,
//! which resolves at expansion, so definition order does not matter.

mod css;
mod flow;
mod style;
