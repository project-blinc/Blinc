//! Bounding volumes for culling and collision

use crate::ecs::Component;
use blinc_core::Vec3;

/// Axis-aligned bounding box
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundingBox {
    pub min: Vec3,
    pub max: Vec3,
}

impl Default for BoundingBox {
    fn default() -> Self {
        Self::empty()
    }
}

impl BoundingBox {
    /// Create an empty bounding box
    pub fn empty() -> Self {
        Self {
            min: Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
            max: Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
        }
    }

    /// Create from min and max points
    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// Create from center and half-extents
    pub fn from_center_half_extents(center: Vec3, half_extents: Vec3) -> Self {
        Self {
            min: Vec3::new(
                center.x - half_extents.x,
                center.y - half_extents.y,
                center.z - half_extents.z,
            ),
            max: Vec3::new(
                center.x + half_extents.x,
                center.y + half_extents.y,
                center.z + half_extents.z,
            ),
        }
    }

    /// Check if the bounding box is empty
    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x || self.min.y > self.max.y || self.min.z > self.max.z
    }

    /// Get the center point
    pub fn center(&self) -> Vec3 {
        Vec3::new(
            (self.min.x + self.max.x) * 0.5,
            (self.min.y + self.max.y) * 0.5,
            (self.min.z + self.max.z) * 0.5,
        )
    }

    /// Get the size (width, height, depth)
    pub fn size(&self) -> Vec3 {
        Vec3::new(
            self.max.x - self.min.x,
            self.max.y - self.min.y,
            self.max.z - self.min.z,
        )
    }

    /// Get half-extents
    pub fn half_extents(&self) -> Vec3 {
        let size = self.size();
        Vec3::new(size.x * 0.5, size.y * 0.5, size.z * 0.5)
    }

    /// Expand to include a point
    pub fn expand_to_include(&mut self, point: Vec3) {
        self.min.x = self.min.x.min(point.x);
        self.min.y = self.min.y.min(point.y);
        self.min.z = self.min.z.min(point.z);
        self.max.x = self.max.x.max(point.x);
        self.max.y = self.max.y.max(point.y);
        self.max.z = self.max.z.max(point.z);
    }

    /// Merge with another bounding box
    pub fn merge(&mut self, other: &BoundingBox) {
        if other.is_empty() {
            return;
        }
        self.min.x = self.min.x.min(other.min.x);
        self.min.y = self.min.y.min(other.min.y);
        self.min.z = self.min.z.min(other.min.z);
        self.max.x = self.max.x.max(other.max.x);
        self.max.y = self.max.y.max(other.max.y);
        self.max.z = self.max.z.max(other.max.z);
    }

    /// Check if a point is inside
    pub fn contains_point(&self, point: Vec3) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    /// Check if this box intersects another
    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }

    /// Get the 8 corner vertices
    pub fn corners(&self) -> [Vec3; 8] {
        [
            Vec3::new(self.min.x, self.min.y, self.min.z),
            Vec3::new(self.max.x, self.min.y, self.min.z),
            Vec3::new(self.min.x, self.max.y, self.min.z),
            Vec3::new(self.max.x, self.max.y, self.min.z),
            Vec3::new(self.min.x, self.min.y, self.max.z),
            Vec3::new(self.max.x, self.min.y, self.max.z),
            Vec3::new(self.min.x, self.max.y, self.max.z),
            Vec3::new(self.max.x, self.max.y, self.max.z),
        ]
    }
}

/// Bounding sphere
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoundingSphere {
    pub center: Vec3,
    pub radius: f32,
}

impl Default for BoundingSphere {
    fn default() -> Self {
        Self {
            center: Vec3::new(0.0, 0.0, 0.0),
            radius: 0.0,
        }
    }
}

impl BoundingSphere {
    /// Create a new bounding sphere
    pub fn new(center: Vec3, radius: f32) -> Self {
        Self { center, radius }
    }

    /// Create from a bounding box
    pub fn from_box(bbox: &BoundingBox) -> Self {
        let center = bbox.center();
        let half_extents = bbox.half_extents();
        let radius = (half_extents.x * half_extents.x
            + half_extents.y * half_extents.y
            + half_extents.z * half_extents.z)
            .sqrt();
        Self { center, radius }
    }

    /// Check if a point is inside
    pub fn contains_point(&self, point: Vec3) -> bool {
        let dx = point.x - self.center.x;
        let dy = point.y - self.center.y;
        let dz = point.z - self.center.z;
        let dist_sq = dx * dx + dy * dy + dz * dz;
        dist_sq <= self.radius * self.radius
    }

    /// Check if this sphere intersects another
    pub fn intersects(&self, other: &BoundingSphere) -> bool {
        let dx = self.center.x - other.center.x;
        let dy = self.center.y - other.center.y;
        let dz = self.center.z - other.center.z;
        let dist_sq = dx * dx + dy * dy + dz * dz;
        let radius_sum = self.radius + other.radius;
        dist_sq <= radius_sum * radius_sum
    }

    /// Check if this sphere intersects a bounding box
    pub fn intersects_box(&self, bbox: &BoundingBox) -> bool {
        // Find closest point on box to sphere center
        let closest = Vec3::new(
            self.center.x.clamp(bbox.min.x, bbox.max.x),
            self.center.y.clamp(bbox.min.y, bbox.max.y),
            self.center.z.clamp(bbox.min.z, bbox.max.z),
        );

        let dx = closest.x - self.center.x;
        let dy = closest.y - self.center.y;
        let dz = closest.z - self.center.z;
        let dist_sq = dx * dx + dy * dy + dz * dz;

        dist_sq <= self.radius * self.radius
    }
}

// Implement Component trait for use in ECS
impl Component for BoundingBox {}
impl Component for BoundingSphere {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounding_box() {
        let bbox = BoundingBox::new(
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
        );

        assert!(bbox.contains_point(Vec3::new(0.0, 0.0, 0.0)));
        assert!(!bbox.contains_point(Vec3::new(2.0, 0.0, 0.0)));

        let center = bbox.center();
        assert!((center.x - 0.0).abs() < 1e-5);
        assert!((center.y - 0.0).abs() < 1e-5);
        assert!((center.z - 0.0).abs() < 1e-5);
    }

    #[test]
    fn test_bounding_sphere() {
        let sphere = BoundingSphere::new(Vec3::new(0.0, 0.0, 0.0), 1.0);

        assert!(sphere.contains_point(Vec3::new(0.0, 0.0, 0.0)));
        assert!(sphere.contains_point(Vec3::new(0.5, 0.0, 0.0)));
        assert!(!sphere.contains_point(Vec3::new(2.0, 0.0, 0.0)));
    }

    #[test]
    fn test_sphere_box_intersection() {
        let sphere = BoundingSphere::new(Vec3::new(2.0, 0.0, 0.0), 1.5);
        let bbox = BoundingBox::new(
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
        );

        assert!(sphere.intersects_box(&bbox));

        let far_sphere = BoundingSphere::new(Vec3::new(10.0, 0.0, 0.0), 1.0);
        assert!(!far_sphere.intersects_box(&bbox));
    }
}
