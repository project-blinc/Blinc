//! SDF (Signed Distance Field) Demo
//!
//! This example demonstrates blinc_3d's SDF system:
//! - SDF primitives (sphere, box, torus, cylinder, cone, capsule)
//! - Boolean operations (union, subtract, intersect)
//! - Smooth blending operations
//! - GPU raymarching via `SdfScene::render` and `DrawContext`
//! - WGSL code generation for GPU raymarching
//!
//! The `SdfScene::render` method renders SDF scenes through the standard
//! Canvas/DrawContext pipeline, enabling real-time GPU raymarching without
//! requiring direct wgpu API calls.
//!
//! Run with: cargo run -p blinc_3d --example sdf_demo --features sdf

use blinc_3d::prelude::*;
use blinc_3d::sdf::{SdfNodeContent, SdfOp, SdfPrimitive, SdfScene};
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use std::f32::consts::PI;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Blinc 3D - SDF Demo".to_string(),
        width: 1200,
        height: 800,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

// ============================================================================
// SDF Demo Definitions
// ============================================================================

struct SdfDemo {
    name: &'static str,
    description: &'static str,
    scene: SdfScene,
}

fn get_sdf_demos() -> Vec<SdfDemo> {
    vec![
        // Primitive: Sphere
        {
            let mut scene = SdfScene::new();
            scene.set_root(SdfScene::sphere(1.0).with_color(Color::rgb(0.3, 0.7, 1.0)));
            SdfDemo {
                name: "Sphere",
                description: "Basic SDF primitive",
                scene,
            }
        },
        // Primitive: Box
        {
            let mut scene = SdfScene::new();
            scene.set_root(SdfScene::cube(1.5).with_color(Color::rgb(1.0, 0.5, 0.3)));
            SdfDemo {
                name: "Box",
                description: "Rectangular SDF",
                scene,
            }
        },
        // Primitive: Torus
        {
            let mut scene = SdfScene::new();
            scene.set_root(
                SdfScene::torus(1.0, 0.3)
                    .rotated(Vec3::new(PI / 4.0, 0.0, 0.0))
                    .with_color(Color::rgb(0.5, 1.0, 0.5)),
            );
            SdfDemo {
                name: "Torus",
                description: "Ring/donut shape",
                scene,
            }
        },
        // Boolean: Union
        {
            let mut scene = SdfScene::new();
            let sphere = SdfScene::sphere(0.8)
                .at(Vec3::new(-0.5, 0.0, 0.0))
                .with_color(Color::rgb(0.9, 0.9, 0.3));
            let box_sdf = SdfScene::cube(1.0)
                .at(Vec3::new(0.5, 0.0, 0.0))
                .with_color(Color::rgb(0.3, 0.9, 0.9));
            scene.set_root(SdfScene::union(sphere, box_sdf));
            SdfDemo {
                name: "Union",
                description: "min(d1, d2) - combine shapes",
                scene,
            }
        },
        // Boolean: Subtract
        {
            let mut scene = SdfScene::new();
            let box_sdf = SdfScene::cube(1.5).with_color(Color::rgb(0.9, 0.3, 0.5));
            let sphere = SdfScene::sphere(1.0).at(Vec3::new(0.5, 0.5, 0.5));
            scene.set_root(SdfScene::subtract(box_sdf, sphere));
            SdfDemo {
                name: "Subtract",
                description: "max(d1, -d2) - carve out",
                scene,
            }
        },
        // Boolean: Intersect
        {
            let mut scene = SdfScene::new();
            let sphere = SdfScene::sphere(1.0)
                .at(Vec3::new(-0.3, 0.0, 0.0))
                .with_color(Color::rgb(0.5, 0.3, 0.9));
            let box_sdf = SdfScene::cube(1.2).at(Vec3::new(0.3, 0.0, 0.0));
            scene.set_root(SdfScene::intersect(sphere, box_sdf));
            SdfDemo {
                name: "Intersect",
                description: "max(d1, d2) - overlap only",
                scene,
            }
        },
        // Smooth Union
        {
            let mut scene = SdfScene::new();
            let sphere1 = SdfScene::sphere(0.7)
                .at(Vec3::new(-0.5, 0.0, 0.0))
                .with_color(Color::rgb(0.3, 0.9, 0.7));
            let sphere2 = SdfScene::sphere(0.7)
                .at(Vec3::new(0.5, 0.0, 0.0))
                .with_color(Color::rgb(0.9, 0.7, 0.3));
            scene.set_root(SdfScene::smooth_union(sphere1, sphere2, 0.3));
            SdfDemo {
                name: "Smooth Union",
                description: "Blended combination (k=0.3)",
                scene,
            }
        },
        // Complex: Cylinder + Torus
        {
            let mut scene = SdfScene::new();
            let cylinder = SdfScene::cylinder(2.0, 0.5).with_color(Color::rgb(1.0, 0.7, 0.3));
            let torus = SdfScene::torus(0.8, 0.2)
                .at(Vec3::new(0.0, 0.8, 0.0))
                .rotated(Vec3::new(PI / 2.0, 0.0, 0.0))
                .with_color(Color::rgb(0.3, 0.7, 1.0));
            scene.set_root(SdfScene::union(cylinder, torus));
            SdfDemo {
                name: "Complex",
                description: "Cylinder + Torus union",
                scene,
            }
        },
    ]
}

// ============================================================================
// UI Building
// ============================================================================

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let demos = get_sdf_demos();

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.06, 0.06, 0.1, 1.0))
        .flex_col()
        .p(16.0)
        .gap(16.0)
        // Header
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(
                    text("Blinc 3D - Signed Distance Fields")
                        .size(28.0)
                        .color(Color::WHITE),
                )
                .child(
                    text("GPU raymarching via Canvas DrawContext")
                        .size(14.0)
                        .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
                ),
        )
        // Main content
        .child(
            div()
                .flex_1()
                .flex_row()
                .justify_between()
                .gap(2.0)
                // SDF demos grid
                .child(sdf_demos_grid(demos))
                // Info panel
                .child(info_panel()),
        )
}

fn sdf_demos_grid(demos: Vec<SdfDemo>) -> Scroll {
    let mut grid = div()
        .flex_1()
        .w_full()
        .flex_row()
        .flex_wrap()
        .gap(8.0)
        .items_start()
        .content_start();

    for demo in demos {
        grid = grid.child(sdf_demo_card(demo));
    }

    scroll().w_full().h(600.0).child(grid)
}

fn sdf_demo_card(demo: SdfDemo) -> Div {
    // Clone the scene for the canvas closure
    let scene = demo.scene.clone();

    // Create camera positioned to view the scene
    let camera = SdfCamera {
        position: Vec3::new(0.0, 0.0, 4.0),
        target: Vec3::ZERO,
        up: Vec3::new(0.0, 1.0, 0.0),
        fov: 0.8,
    };

    div()
        .w(280.0)
        .bg(Color::rgba(0.1, 0.1, 0.14, 1.0))
        .rounded(8.0)
        .flex_col()
        .overflow_clip()
        // GPU-rendered SDF preview via DrawContext
        .child(
            canvas(move |ctx, bounds| {
                // Use GPU raymarching through the DrawContext
                scene.render(ctx, &camera, bounds, 0.0);
            })
            .w_full()
            .h(135.0),
        )
        // Info section
        .child(
            div()
                .w_full()
                .p(8.0)
                .flex_col()
                .gap(4.0)
                .child(
                    div()
                        .flex_row()
                        .justify_between()
                        .items_center()
                        .child(text(demo.name).size(14.0).color(Color::WHITE))
                        .child(scene_type_badge(&demo.scene)),
                )
                .child(
                    text(demo.description)
                        .size(11.0)
                        .color(Color::rgba(0.5, 0.5, 0.6, 1.0)),
                ),
        )
}

fn scene_type_badge(scene: &SdfScene) -> Div {
    let (label, color) = match scene.root() {
        Some(node) => match &node.content {
            SdfNodeContent::Primitive(prim) => {
                let name = match prim {
                    SdfPrimitive::Sphere { .. } => "Sphere",
                    SdfPrimitive::Box { .. } => "Box",
                    SdfPrimitive::Torus { .. } => "Torus",
                    SdfPrimitive::Cylinder { .. } => "Cylinder",
                    SdfPrimitive::Cone { .. } => "Cone",
                    SdfPrimitive::Plane { .. } => "Plane",
                    SdfPrimitive::Capsule { .. } => "Capsule",
                    SdfPrimitive::RoundedBox { .. } => "RoundedBox",
                    SdfPrimitive::Ellipsoid { .. } => "Ellipsoid",
                    SdfPrimitive::TriPrism { .. } => "TriPrism",
                    SdfPrimitive::HexPrism { .. } => "HexPrism",
                    SdfPrimitive::Octahedron { .. } => "Octahedron",
                    SdfPrimitive::Pyramid { .. } => "Pyramid",
                };
                (name, Color::rgba(0.3, 0.6, 0.9, 0.8))
            }
            SdfNodeContent::Operation { op, .. } => {
                let name = match op {
                    SdfOp::Union => "Union",
                    SdfOp::Subtract => "Subtract",
                    SdfOp::Intersect => "Intersect",
                    SdfOp::SmoothUnion { .. } => "SmoothUnion",
                    SdfOp::SmoothSubtract { .. } => "SmoothSub",
                    SdfOp::SmoothIntersect { .. } => "SmoothInt",
                };
                (name, Color::rgba(0.9, 0.6, 0.3, 0.8))
            }
        },
        None => ("Empty", Color::rgba(0.5, 0.5, 0.5, 0.8)),
    };

    div()
        .px(6.0)
        .py(2.0)
        .bg(color)
        .rounded(4.0)
        .child(text(label).size(9.0).color(Color::WHITE))
}

fn info_panel() -> Scroll {
    // Generate example WGSL for display
    let mut example_scene = SdfScene::new();
    example_scene.set_root(SdfScene::sphere(1.0));
    let example_wgsl = example_scene.to_wgsl();
    let map_scene_fn = extract_map_scene(&example_wgsl);
    scroll().w(300.0).h(600.00).bg(Color::rgba(0.1, 0.1, 0.14, 1.0))
        .rounded(8.0)
        .p(4.0).child( div()
        .w_full()
        .h_full()
        .rounded(8.0)
        .flex_col()
        .gap(4.0)
        // Primitives
        .child(text("SDF Primitives").size(16.0).color(Color::WHITE))
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(primitive_item("Sphere", "length(p) - r"))
                .child(primitive_item("Box", "max(abs(p) - b)"))
                .child(primitive_item("Torus", "len(vec2(len(p.xz)-R, p.y))-r"))
                .child(primitive_item("Cylinder", "length(p.xz) - r")),
        )
        // Operations
        .child(text("Boolean Operations").size(16.0).color(Color::WHITE))
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(op_item("Union", "min(d1, d2)"))
                .child(op_item("Subtract", "max(-d2, d1)"))
                .child(op_item("Intersect", "max(d1, d2)"))
                .child(op_item("SmoothUnion", "blend with k")),
        )
        // API Usage
        .child(text("API Usage").size(16.0).color(Color::WHITE))
        .child(
            code("let mut scene = SdfScene::new();\nscene.set_root(\n  SdfScene::sphere(1.0)\n);")
                .font_size(9.0)
                .rounded(4.0),
        )
        // Generated WGSL
        .child(text("Generated WGSL").size(16.0).color(Color::WHITE))
        .child(code(&map_scene_fn).font_size(8.0).rounded(4.0))
        // Rendering info
        .child(text("Rendering").size(16.0).color(Color::WHITE))
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(render_item("Sphere tracing / raymarching"))
                .child(render_item("Gradient for surface normals"))
                .child(render_item("WGSL code generation"))
                .child(render_item("GPU pipeline ready")),
        ))
}

fn extract_map_scene(wgsl: &str) -> String {
    let lines: Vec<&str> = wgsl.lines().collect();
    let mut in_function = false;
    let mut result = Vec::new();

    for line in lines {
        if line.contains("fn map_scene") {
            in_function = true;
        }
        if in_function {
            result.push(line);
            if line.trim() == "}" {
                break;
            }
        }
    }

    if result.is_empty() {
        "fn map_scene(p: vec3f) -> f32 {\n  return sdf_sphere(p, 1.0);\n}".to_string()
    } else {
        result.join("\n")
    }
}

fn primitive_item(name: &'static str, formula: &'static str) -> Div {
    div()
        .flex_col()
        .gap(1.0)
        .child(text(name).size(12.0).color(Color::rgba(0.8, 0.8, 0.9, 1.0)))
        .child(
            text(formula)
                .size(9.0)
                .color(Color::rgba(0.4, 0.6, 0.8, 0.8)),
        )
}

fn op_item(name: &'static str, formula: &'static str) -> Div {
    div()
        .flex_row()
        .justify_between()
        .child(text(name).size(11.0).color(Color::rgba(0.7, 0.7, 0.8, 1.0)))
        .child(
            text(formula)
                .size(10.0)
                .color(Color::rgba(0.5, 0.7, 1.0, 0.8)),
        )
}

fn render_item(info: &'static str) -> Div {
    div()
        .flex_row()
        .gap(6.0)
        .items_center()
        .child(
            div()
                .w(4.0)
                .h(4.0)
                .rounded(2.0)
                .bg(Color::rgba(0.3, 0.8, 0.5, 0.8)),
        )
        .child(text(info).size(11.0).color(Color::rgba(0.6, 0.6, 0.7, 1.0)))
}
