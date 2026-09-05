//! Applying a declaration to an [`ElementStyle`].
//!
//! This is where a property name and its unparsed value become a field on
//! the style. Two entry points share that job: [`apply_property`] applies
//! silently and drops anything it cannot parse, while
//! [`apply_property_with_errors`] reports what it rejected.

mod apply;
mod diagnostic;

pub(crate) use apply::*;
pub(crate) use diagnostic::*;
