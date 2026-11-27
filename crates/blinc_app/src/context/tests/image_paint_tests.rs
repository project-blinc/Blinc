//! What a frame draws for an image: its shadow, its place and its placeholder.
use super::*;

fn state() -> RenderState {
    RenderState::new(Arc::new(Mutex::new(
        blinc_animation::AnimationScheduler::new(),
    )))
}

/// A frame by each path: direct, with motion, through the window compositor and as an overlay.
fn frame(
    context: &mut RenderContext,
    tree: &RenderTree,
    state: &RenderState,
    mode: u8,
) -> image::RgbaImage {
    if mode < 3 {
        return super::gpu_pass_group_tests::frame(context, tree, state, mode);
    }
    let scale = tree.scale_factor() as u32;
    let (texture, target) = super::gpu_pass_group_tests::target(context, scale);
    context.renderer.clear_target(&target, wgpu::Color::BLUE);
    context
        .render_overlay_tree_with_motion(tree, state, 160 * scale, 120 * scale, &target)
        .unwrap();
    crate::tests::read_rendered_pixels(
        context.device(),
        context.queue(),
        &texture,
        160 * scale,
        120 * scale,
    )
}

fn laid_out(root: Div, scale: u32) -> RenderTree {
    let mut tree = RenderTree::from_element(&root);
    tree.set_scale_factor(scale as f32);
    tree.compute_layout(160.0, 120.0);
    tree
}

fn page() -> Div {
    div().w(160.0).h(120.0).bg(Color::BLUE)
}

fn at(x: f32, y: f32, child: impl ElementBuilder + 'static) -> Div {
    div().absolute().left(x).top(y).child(child)
}

#[test]
#[ignore] // Requires GPU
fn an_image_shadow_is_drawn_once() {
    let shadow = blinc_core::Shadow::new(0.0, 40.0, 0.0, Color::rgba(1.0, 0.0, 0.0, 0.5));
    for mode in 0..4 {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let green = super::overlay_group_tests::IMAGE;
        let element = blinc_layout::image(green)
            .size(40.0, 30.0)
            .fade_in(std::time::Duration::ZERO)
            .shadow(shadow);
        let background = div()
            .w(40.0)
            .h(30.0)
            .background(blinc_core::Brush::Image(blinc_core::ImageBrush::new(green)))
            .shadow(shadow);
        let tree = laid_out(
            page()
                .child(at(20.0, 20.0, element))
                .child(at(90.0, 20.0, background)),
            1,
        );
        let drawn = frame(&mut context, &tree, &state(), mode);
        for x in [40, 110] {
            super::opacity_group_tests::pixel(&drawn, (x, 75), [128, 0, 127, 255]);
        }
    }
}

/// A page with a background, one image with what `picture` adds, and a glass element apart
/// from it.
fn beside_glass(picture: Div) -> Div {
    page().child(picture).child(
        div()
            .absolute()
            .left(110.0)
            .top(80.0)
            .w(40.0)
            .h(30.0)
            .glass(),
    )
}

fn green() -> blinc_layout::image::Image {
    blinc_layout::image(super::overlay_group_tests::IMAGE)
        .size(40.0, 30.0)
        .fade_in(std::time::Duration::ZERO)
}

#[test]
#[ignore] // Requires GPU
fn glass_leaves_the_images_beside_it_drawn() {
    for mode in 0..4 {
        for scale in [1, 2] {
            let mut context = super::main_text_clip_tests::context(true, 1);
            let moved = green().transform(blinc_core::Transform::translate(40.0, 0.0));
            let tree = laid_out(beside_glass(at(20.0, 20.0, moved)), scale);
            let state = state();
            for _ in 0..2 {
                let drawn = frame(&mut context, &tree, &state, mode);
                for (x, expected) in [(30, [0, 0, 255, 255]), (80, [0, 255, 0, 255])] {
                    super::opacity_group_tests::pixel(&drawn, (x * scale, 35 * scale), expected);
                }
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_blurs_the_image_under_it() {
    for mode in 0..4 {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let tree = laid_out(
            page().child(at(40.0, 30.0, green())).child(
                div()
                    .absolute()
                    .left(45.0)
                    .top(35.0)
                    .w(30.0)
                    .h(20.0)
                    .glass(),
            ),
            1,
        );
        let drawn = frame(&mut context, &tree, &state(), mode);
        let inside = drawn.get_pixel(60, 45).0;
        assert!(inside[1] > 100, "mode {mode}: {inside:?}");
    }
}

#[test]
#[ignore] // Requires GPU
fn glass_leaves_the_placeholders_beside_it_drawn() {
    use blinc_layout::image::Placeholder;
    let thumbnail = Placeholder::Image(super::overlay_group_tests::IMAGE.into());
    for (placeholder, expected) in [
        (Placeholder::Color(Color::CYAN), [0, 255, 255, 255]),
        (thumbnail, [0, 255, 0, 255]),
    ] {
        for mode in 0..4 {
            let mut context = super::main_text_clip_tests::context(true, 1);
            let picture = blinc_layout::image("missing-beside-glass.png")
                .size(40.0, 30.0)
                .placeholder(placeholder.clone());
            let tree = laid_out(beside_glass(at(20.0, 20.0, picture)), 1);
            let state = state();
            for _ in 0..2 {
                let drawn = frame(&mut context, &tree, &state, mode);
                super::opacity_group_tests::pixel(&drawn, (40, 35), expected);
            }
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn a_skeleton_placeholder_moves_in_every_frame_path() {
    for mode in 0..3 {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let picture = blinc_layout::image("missing-skeleton.png")
            .size(120.0, 40.0)
            .skeleton();
        let tree = laid_out(page().child(at(20.0, 20.0, picture)), 1);
        let state = state();
        let mut row = |n: u64| {
            context.frame_count = n;
            let drawn = frame(&mut context, &tree, &state, mode);
            assert!(context.has_active_flows(), "mode {mode}: no next frame");
            (20..140)
                .map(|x| drawn.get_pixel(x, 40).0)
                .collect::<Vec<_>>()
        };
        assert_ne!(row(30), row(90), "mode {mode}");
    }
}

/// A red bar under the left half of the box of an image that has a glass placeholder, with
/// the image at `opacity`, or inside a motion container.
fn glass_placeholder_page(tint: Color, opacity: f32, in_motion: bool) -> RenderTree {
    use blinc_core::{Brush, GlassStyle};
    let picture = blinc_layout::image("missing-glass-placeholder.png")
        .size(80.0, 60.0)
        .placeholder_brush(Brush::Glass(GlassStyle::new().blur(20.0).tint(tint)));
    let bar = div()
        .absolute()
        .left(20.0)
        .top(20.0)
        .w(40.0)
        .h(60.0)
        .bg(Color::RED);
    let holder = at(20.0, 20.0, picture).id("holder").opacity(opacity);
    let root = page().child(bar);
    let root = if in_motion {
        root.child(blinc_layout::motion::motion().child(holder))
    } else {
        root.child(holder)
    };
    laid_out(root, 1)
}

#[test]
#[ignore] // Requires GPU
fn a_glass_placeholder_blurs_what_is_behind_it() {
    for mode in 0..4 {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let tree = glass_placeholder_page(Color::TRANSPARENT, 1.0, false);
        let state = state();
        for n in 0..3 {
            let drawn = frame(&mut context, &tree, &state, mode);
            // Blur spreads the bar past its edge, where a flat tint would leave it hard.
            let past_the_bar = drawn.get_pixel(63, 50).0;
            assert!(
                past_the_bar[0] > 20 && past_the_bar[2] > 20,
                "mode {mode} frame {n}: {past_the_bar:?}"
            );
            super::opacity_group_tests::pixel(&drawn, (110, 50), [0, 0, 255, 255]);
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn a_glass_placeholder_blurs_inside_a_group() {
    for (opacity, in_motion) in [(0.5, false), (1.0, true), (0.5, true)] {
        for mode in 0..4 {
            let mut context = super::main_text_clip_tests::context(true, 1);
            let tree = glass_placeholder_page(Color::TRANSPARENT, opacity, in_motion);
            let state = state();
            for n in 0..3 {
                let drawn = frame(&mut context, &tree, &state, mode);
                // The blurred bar, faded by the group, over the sharp page.
                let past_the_bar = drawn.get_pixel(63, 50).0;
                assert!(
                    past_the_bar[0] > 10 && past_the_bar[2] > 120,
                    "mode {mode} frame {n} opacity {opacity} motion {in_motion}: {past_the_bar:?}"
                );
                super::opacity_group_tests::pixel(&drawn, (110, 50), [0, 0, 255, 255]);
            }
        }
    }
}

/// The glass placeholder page with the image in a container whose translate is `x`.
fn moving_placeholder_page(
    source: &str,
    x: blinc_layout::motion::SharedAnimatedValue,
) -> RenderTree {
    use blinc_core::{Brush, GlassStyle};
    let glass = Brush::Glass(GlassStyle::new().blur(20.0).tint(Color::TRANSPARENT));
    let picture = blinc_layout::image(source)
        .size(80.0, 60.0)
        .fade_in(std::time::Duration::ZERO)
        .placeholder_brush(glass);
    let bar = div()
        .absolute()
        .left(20.0)
        .top(20.0)
        .w(40.0)
        .h(60.0)
        .bg(Color::RED);
    laid_out(
        page()
            .child(bar)
            .child(at(20.0, 20.0, picture).motion().translate_x(x)),
        1,
    )
}

fn animated(start: f32) -> blinc_layout::motion::SharedAnimatedValue {
    let scheduler = blinc_animation::AnimationScheduler::new();
    Arc::new(Mutex::new(blinc_animation::AnimatedValue::with_default(
        scheduler.handle(),
        start,
    )))
}

#[test]
#[ignore] // Requires GPU
fn a_glass_placeholder_blurs_under_a_moving_binding() {
    for mode in 0..4 {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let x = animated(0.0);
        let tree = moving_placeholder_page("missing-moving-placeholder.png", x.clone());
        let state = state();
        // At each place, the bar's far edge is blurred only where the image is over it.
        for (offset, left_edge_blurred) in [(0.0, true), (30.0, false), (0.0, true)] {
            x.lock().unwrap().set_immediate(offset);
            let drawn = frame(&mut context, &tree, &state, mode);
            let past_the_bar = drawn.get_pixel(63, 50).0;
            assert!(
                past_the_bar[0] > 20 && past_the_bar[2] > 20,
                "mode {mode} at {offset}: {past_the_bar:?}"
            );
            let left = drawn.get_pixel(23, 50).0;
            assert_eq!(
                left != [255, 0, 0, 255],
                left_edge_blurred,
                "mode {mode} at {offset}: {left:?}"
            );
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn a_moving_image_leaves_its_glass_when_it_loads() {
    for mode in 0..4 {
        let path = std::env::temp_dir().join(format!(
            "blinc-moving-glass-{}-{mode}.png",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let mut context = super::main_text_clip_tests::context(true, 1);
        let x = animated(0.0);
        let tree = moving_placeholder_page(&path.to_string_lossy(), x.clone());
        let state = state();
        frame(&mut context, &tree, &state, mode);
        assert_eq!(tree.backdrop_placeholder_images().len(), 1);

        image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 255, 0, 255]))
            .save(&path)
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut drawn = frame(&mut context, &tree, &state, mode);
        while drawn.get_pixel(90, 50).0[1] < 200 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
            drawn = frame(&mut context, &tree, &state, mode);
        }
        x.lock().unwrap().set_immediate(30.0);
        for _ in 0..2 {
            let drawn = frame(&mut context, &tree, &state, mode);
            super::opacity_group_tests::pixel(&drawn, (120, 50), [0, 255, 0, 255]);
            super::opacity_group_tests::pixel(&drawn, (23, 50), [255, 0, 0, 255]);
        }
        let _ = std::fs::remove_file(path);
    }
}

#[test]
#[ignore] // Requires GPU
fn a_glass_placeholder_blurs_in_a_3d_layer() {
    for mode in 0..4 {
        let mut context = super::main_text_clip_tests::context(true, 1);
        let mut tree = glass_placeholder_page(Color::TRANSPARENT, 1.0, false);
        let holder = tree.query_by_id("holder").unwrap();
        tree.update_render_props(holder, |p| p.rotate_y = Some(0.01));
        let state = state();
        for n in 0..3 {
            let drawn = frame(&mut context, &tree, &state, mode);
            let past_the_bar = drawn.get_pixel(63, 50).0;
            assert!(
                past_the_bar[0] > 20 && past_the_bar[2] > 20,
                "mode {mode} frame {n}: {past_the_bar:?}"
            );
        }
    }
}

#[test]
#[ignore] // Requires GPU
fn a_glass_placeholder_keeps_later_content_above_it() {
    use blinc_core::{Brush, GlassStyle};
    for (bound, opacity) in [(false, 1.0), (false, 0.5), (true, 1.0)] {
        for mode in 0..4 {
            let mut context = super::main_text_clip_tests::context(true, 1);
            let glass = Brush::Glass(GlassStyle::new().blur(20.0).tint(Color::TRANSPARENT));
            let picture = blinc_layout::image("missing-under-badge.png")
                .size(80.0, 60.0)
                .placeholder_brush(glass);
            let holder = at(20.0, 20.0, picture).opacity(opacity);
            let badge = div()
                .absolute()
                .left(80.0)
                .top(30.0)
                .w(30.0)
                .h(20.0)
                .bg(Color::GREEN);
            let root = if bound {
                page().child(holder.motion().translate_x(animated(0.0)))
            } else {
                page().child(holder)
            };
            let tree = laid_out(root.child(badge), 1);
            let state = state();
            for _ in 0..2 {
                let drawn = frame(&mut context, &tree, &state, mode);
                super::opacity_group_tests::pixel(&drawn, (90, 40), [0, 255, 0, 255]);
            }
        }
    }
}
