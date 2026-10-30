//! Applying a declaration to an [`ElementStyle`](crate::element_style::ElementStyle).
//!
//! This is where a property name and its unparsed value become a field on
//! the style. Two entry points share that job: `apply_property` applies
//! silently and drops anything it cannot parse, while
//! `apply_property_with_errors` reports what it rejected.

mod apply;
mod border_sides;
mod diagnostic;
mod grid;
mod layout_extras;

pub(crate) use apply::*;
pub(crate) use border_sides::apply_border_side_property;
pub(crate) use diagnostic::*;
pub(crate) use grid::apply_grid_property;
pub(crate) use layout_extras::apply_layout_extra;
