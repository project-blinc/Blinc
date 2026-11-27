//! Which images are near what shows.
use super::ImageElement;

/// Whether the image is within `margin` of its clip, or of the viewport when it has none.
pub(in crate::context) fn is_near_view(
    image: &ImageElement,
    viewport: (f32, f32),
    margin: f32,
) -> bool {
    let [x, y, width, height] = image
        .clip_bounds
        .unwrap_or([0.0, 0.0, viewport.0, viewport.1]);
    image.x < x + width + margin
        && image.x + image.width > x - margin
        && image.y < y + height + margin
        && image.y + image.height > y - margin
}
