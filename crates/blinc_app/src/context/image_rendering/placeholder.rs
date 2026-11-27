//! Which placeholders the frame's glass draws, and which images are near what shows.
use super::ImageElement;
use blinc_core::Brush;

/// Whether the image's placeholder samples what is behind it.
pub(in crate::context) fn has_backdrop_placeholder(image: &ImageElement) -> bool {
    image.placeholder_type == 4
        && matches!(
            image.placeholder_brush.as_deref(),
            Some(Brush::Glass(_) | Brush::Blur(_))
        )
}

/// Whether such a placeholder joins the glass of the frame, for an image the frame draws
/// itself. An image in a 3D layer or a motion container is drawn apart, with no backdrop,
/// and shows the tint, as does one inside a group.
pub(in crate::context) fn blurs_in_the_frame(image: &ImageElement) -> bool {
    has_backdrop_placeholder(image)
        && !image.in_motion_subtree
        && image.transform_3d_layer.is_none()
}

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
