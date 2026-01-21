//! Geometry primitives

mod primitives;
mod vertex;

pub use primitives::{BoxGeometry, CylinderGeometry, PlaneGeometry, SphereGeometry, TorusGeometry};
pub use vertex::{Geometry, GeometryHandle, Vertex};
