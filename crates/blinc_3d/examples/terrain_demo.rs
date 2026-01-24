//! Terrain and Water Demo
//!
//! Demonstrates blinc_3d's ECS-driven procedural terrain and water utilities:
//! - Terrain component with presets: mountains, hills, plains, canyons, dunes, island
//! - Noise layers: Perlin, Simplex, Worley, Ridged, Billow
//! - Terrain materials: grass, rock, snow, sand with height-based blending
//! - WaterBody component with presets: ocean, lake, river, pool, swamp
//! - Wave styles: Still, Calm, Ocean, River
//! - TerrainSystem for LOD management
//! - Proper ECS World setup with render_scene integration
//!
//! Run with: cargo run -p blinc_3d --example terrain_demo --features "utils-terrain"

use blinc_3d::ecs::{Entity, System, SystemContext, World};
use blinc_3d::geometry::{Geometry, PlaneGeometry};
use blinc_3d::integration::{render_scene, CanvasBounds};
use blinc_3d::lights::{AmbientLight, DirectionalLight, ShadowConfig};
use blinc_3d::materials::{StandardMaterial};
use blinc_3d::prelude::*;
use blinc_3d::scene::{Mesh, Object3D, PerspectiveCamera};
use blinc_3d::utils::terrain::*;
use blinc_animation::SpringConfig;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::events::event_types;
use blinc_core::Transform;
use blinc_layout::stateful::ButtonState;
use std::f32::consts::PI;
use std::sync::{Arc, Mutex};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Blinc 3D - Terrain & Water Demo".to_string(),
        width: 1200,
        height: 800,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

// ============================================================================
// Terrain Preset Definitions
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum TerrainPreset {
    #[default]
    Mountains,
    Hills,
    Plains,
    Canyons,
    Dunes,
    Island,
    Flat,
    Custom,
}

impl TerrainPreset {
    fn name(&self) -> &'static str {
        match self {
            TerrainPreset::Mountains => "Mountains",
            TerrainPreset::Hills => "Hills",
            TerrainPreset::Plains => "Plains",
            TerrainPreset::Canyons => "Canyons",
            TerrainPreset::Dunes => "Dunes",
            TerrainPreset::Island => "Island",
            TerrainPreset::Flat => "Flat",
            TerrainPreset::Custom => "Custom",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            TerrainPreset::Mountains => "Ridged peaks with steep slopes",
            TerrainPreset::Hills => "Rolling hills with gentle slopes",
            TerrainPreset::Plains => "Flat terrain with minor variation",
            TerrainPreset::Canyons => "Deep carved valleys and ridges",
            TerrainPreset::Dunes => "Desert sand dune formations",
            TerrainPreset::Island => "Raised center with ocean falloff",
            TerrainPreset::Flat => "Completely flat terrain",
            TerrainPreset::Custom => "Build your own noise layers",
        }
    }

    fn color(&self) -> Color {
        match self {
            TerrainPreset::Mountains => Color::rgb(0.5, 0.6, 0.7),
            TerrainPreset::Hills => Color::rgb(0.4, 0.6, 0.3),
            TerrainPreset::Plains => Color::rgb(0.6, 0.7, 0.4),
            TerrainPreset::Canyons => Color::rgb(0.7, 0.5, 0.3),
            TerrainPreset::Dunes => Color::rgb(0.9, 0.8, 0.5),
            TerrainPreset::Island => Color::rgb(0.3, 0.6, 0.8),
            TerrainPreset::Flat => Color::rgb(0.5, 0.5, 0.5),
            TerrainPreset::Custom => Color::rgb(0.6, 0.4, 0.8),
        }
    }

    /// Get the actual Terrain from blinc_3d utilities
    fn to_terrain(&self, size: f32, height: f32) -> Terrain {
        match self {
            TerrainPreset::Mountains => Terrain::mountains(size, height),
            TerrainPreset::Hills => Terrain::hills(size, height),
            TerrainPreset::Plains => Terrain::plains(size, height * 0.3),
            TerrainPreset::Canyons => Terrain::canyons(size, height),
            TerrainPreset::Dunes => Terrain::dunes(size, height * 0.5),
            TerrainPreset::Island => Terrain::island(size, height),
            TerrainPreset::Flat => Terrain::flat(size),
            TerrainPreset::Custom => Terrain::new(size)
                .with_max_height(height)
                .with_noise(NoiseLayer::ridged(0.005, 0.4))
                .with_noise(NoiseLayer::perlin(0.02, 0.3))
                .with_noise(NoiseLayer::simplex(0.1, 0.2))
                .with_noise(NoiseLayer::worley(0.05, 0.1)),
        }
    }
}

// ============================================================================
// Water Preset Definitions
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum WaterPreset {
    #[default]
    Ocean,
    Lake,
    River,
    Pool,
    Swamp,
    None,
}

impl WaterPreset {
    fn name(&self) -> &'static str {
        match self {
            WaterPreset::Ocean => "Ocean",
            WaterPreset::Lake => "Lake",
            WaterPreset::River => "River",
            WaterPreset::Pool => "Pool",
            WaterPreset::Swamp => "Swamp",
            WaterPreset::None => "None",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            WaterPreset::Ocean => "Deep blue with ocean swells",
            WaterPreset::Lake => "Calm blue-green water",
            WaterPreset::River => "Flowing water with foam",
            WaterPreset::Pool => "Crystal clear still water",
            WaterPreset::Swamp => "Murky opaque water",
            WaterPreset::None => "No water body",
        }
    }

    fn color(&self) -> Color {
        match self {
            WaterPreset::Ocean => Color::rgb(0.1, 0.3, 0.6),
            WaterPreset::Lake => Color::rgb(0.2, 0.4, 0.5),
            WaterPreset::River => Color::rgb(0.3, 0.5, 0.6),
            WaterPreset::Pool => Color::rgb(0.4, 0.7, 0.9),
            WaterPreset::Swamp => Color::rgb(0.2, 0.3, 0.1),
            WaterPreset::None => Color::rgba(0.3, 0.3, 0.3, 0.5),
        }
    }

    /// Get the actual WaterBody from blinc_3d utilities
    fn to_water(&self, water_level: f32) -> Option<WaterBody> {
        match self {
            WaterPreset::Ocean => Some(WaterBody::ocean(water_level)),
            WaterPreset::Lake => Some(WaterBody::lake(water_level)),
            WaterPreset::River => Some(WaterBody::river(water_level)),
            WaterPreset::Pool => Some(WaterBody::pool(water_level)),
            WaterPreset::Swamp => Some(WaterBody::swamp(water_level)),
            WaterPreset::None => None,
        }
    }
}

const ALL_TERRAIN_PRESETS: [TerrainPreset; 8] = [
    TerrainPreset::Mountains,
    TerrainPreset::Hills,
    TerrainPreset::Plains,
    TerrainPreset::Canyons,
    TerrainPreset::Dunes,
    TerrainPreset::Island,
    TerrainPreset::Flat,
    TerrainPreset::Custom,
];

const ALL_WATER_PRESETS: [WaterPreset; 6] = [
    WaterPreset::Ocean,
    WaterPreset::Lake,
    WaterPreset::River,
    WaterPreset::Pool,
    WaterPreset::Swamp,
    WaterPreset::None,
];

// ============================================================================
// ECS World Creation
// ============================================================================

/// Create an ECS World with terrain, water, camera, and lights
fn create_terrain_world(
    terrain_preset: TerrainPreset,
    water_preset: WaterPreset,
    water_level: f32,
    camera_pos: Vec3,
) -> (World, Entity) {
    let mut world = World::new();

    // Create terrain component
    let terrain = terrain_preset.to_terrain(100.0, 10.0);
    let terrain_color = terrain.material.grass_color;

    // Create a plane geometry to represent terrain (GPU pipelines handle actual terrain rendering)
    let terrain_geom = PlaneGeometry::with_segments(20.0, 20.0, 32, 32);
    let terrain_geom_handle = world.add_geometry(terrain_geom);
    let terrain_mat_handle = world.add_material(StandardMaterial {
        color: terrain_color,
        metalness: 0.0,
        roughness: 0.8,
        ..Default::default()
    });

    // Spawn terrain entity with Terrain component and mesh visualization
    world
        .spawn()
        .insert(terrain)
        .insert(Object3D {
            position: Vec3::ZERO,
            visible: true,
            ..Default::default()
        })
        .insert(Mesh {
            geometry: terrain_geom_handle,
            material: terrain_mat_handle,
        })
        .id();

    // Spawn water entity if water preset is selected
    if let Some(water) = water_preset.to_water(water_level) {
        let water_geom = PlaneGeometry::with_segments(25.0, 25.0, 16, 16);
        let water_geom_handle = world.add_geometry(water_geom);
        let water_mat_handle = world.add_material(StandardMaterial {
            color: water.color,
            metalness: 0.2,
            roughness: 0.1,
            ..Default::default()
        });

        world
            .spawn()
            .insert(water)
            .insert(Object3D {
                position: Vec3::new(0.0, water_level * 0.1 - 1.0, 0.0),
                visible: true,
                ..Default::default()
            })
            .insert(Mesh {
                geometry: water_geom_handle,
                material: water_mat_handle,
            })
            .id();
    }

    // Spawn camera entity
    let camera_entity = world
        .spawn()
        .insert(PerspectiveCamera::new(0.8, 16.0 / 9.0, 0.1, 100.0))
        .insert(Object3D {
            position: camera_pos,
            ..Default::default()
        })
        .id();

    // Spawn ambient light
    world
        .spawn()
        .insert(AmbientLight {
            color: Color::WHITE,
            intensity: 0.3,
        })
        .id();

    // Spawn directional light (sun)
    world
        .spawn()
        .insert(DirectionalLight {
            color: Color::WHITE,
            intensity: 0.8,
            cast_shadows: true,
            shadow: ShadowConfig::default(),
            shadow_camera_size: 50.0,
        })
        .insert(Object3D {
            position: Vec3::new(10.0, 20.0, 10.0),
            rotation: Quat::from_euler(-PI / 4.0, PI / 4.0, 0.0),
            ..Default::default()
        })
        .id();

    (world, camera_entity)
}

// ============================================================================
// UI Building
// ============================================================================

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.06, 0.06, 0.1, 1.0))
        .flex_col()
        .child(header())
        .child(main_content(ctx.width, ctx.height - 80.0))
}

fn header() -> impl ElementBuilder {
    div()
        .w_full()
        .h(80.0)
        .px(24.0)
        .flex_row()
        .items_center()
        .justify_between()
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(
                    text("Terrain & Water Demo")
                        .size(24.0)
                        .weight(FontWeight::Bold)
                        .color(Color::WHITE),
                )
                .child(
                    text("ECS-driven procedural terrain with water bodies")
                        .size(14.0)
                        .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
                ),
        )
}

fn main_content(width: f32, height: f32) -> impl ElementBuilder {
    div()
        .w(width)
        .h(height)
        .flex_row()
        .child(viewport_area(width - 380.0, height))
        .child(control_panel())
}

fn viewport_area(width: f32, height: f32) -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(move |ctx| {
        // Selection signals
        let terrain_preset = ctx.use_signal("terrain_preset", || TerrainPreset::Mountains);
        let water_preset = ctx.use_signal("water_preset", || WaterPreset::Lake);
        let water_level = ctx.use_signal("water_level", || 30.0f32);

        // Time for animation
        let time = ctx.use_signal("time", || 0.0f32);
        time.update(|t| t + 0.016);

        // Camera angle
        let angle = ctx.use_signal("angle", || 0.5f32);

        // Handle mouse drag for camera rotation
        if let Some(event) = ctx.event() {
            if event.event_type == event_types::POINTER_MOVE && ctx.state() == ButtonState::Pressed
            {
                angle.update(|a| a + event.local_x * 0.0003);
            }
        }

        // Calculate camera position
        let cam_angle = angle.get() + time.get() * 0.02;
        let cam_x = cam_angle.sin() * 15.0;
        let cam_z = cam_angle.cos() * 15.0;
        let camera_pos = Vec3::new(cam_x, 8.0, cam_z);

        // Create ECS World with terrain and water
        let (world, camera_entity) = create_terrain_world(
            terrain_preset.get(),
            water_preset.get(),
            water_level.get(),
            camera_pos,
        );

        div()
            .w(width)
            .h(height)
            .bg(Color::rgba(0.02, 0.02, 0.05, 1.0))
            .cursor_pointer()
            .child(
                canvas(move |draw_ctx, bounds| {
                    // Use the ECS render_scene function
                    render_scene(draw_ctx, &world, camera_entity, bounds);
                })
                .w_full()
                .h_full(),
            )
            .child(
                // Overlay: usage example
                div()
                    .absolute()
                    .left(16.0)
                    .bottom(16.0)
                    .max_w(width - 32.0)
                    .p(12.0)
                    .bg(Color::rgba(0.0, 0.0, 0.0, 0.85))
                    .rounded(8.0)
                    .flex_col()
                    .gap(8.0)
                    .child(
                        text("ECS Usage Example")
                            .size(14.0)
                            .weight(FontWeight::SemiBold)
                            .color(Color::WHITE),
                    )
                    .child(code_example(terrain_preset.get(), water_preset.get())),
            )
    })
}

fn code_example(terrain: TerrainPreset, water: WaterPreset) -> impl ElementBuilder {
    let terrain_code = match terrain {
        TerrainPreset::Mountains => "let terrain = Terrain::mountains(1000.0, 100.0);",
        TerrainPreset::Hills => "let terrain = Terrain::hills(1000.0, 50.0);",
        TerrainPreset::Plains => "let terrain = Terrain::plains(1000.0, 30.0);",
        TerrainPreset::Canyons => "let terrain = Terrain::canyons(1000.0, 100.0);",
        TerrainPreset::Dunes => "let terrain = Terrain::dunes(1000.0, 50.0);",
        TerrainPreset::Island => "let terrain = Terrain::island(1000.0, 100.0);",
        TerrainPreset::Flat => "let terrain = Terrain::flat(1000.0);",
        TerrainPreset::Custom => "let terrain = Terrain::new(1000.0)\n    .with_max_height(100.0)\n    .with_noise(NoiseLayer::ridged(0.005, 0.4))\n    .with_noise(NoiseLayer::perlin(0.02, 0.3));",
    };

    let water_code = match water {
        WaterPreset::Ocean => "\nlet water = WaterBody::ocean(30.0);",
        WaterPreset::Lake => "\nlet water = WaterBody::lake(30.0);",
        WaterPreset::River => "\nlet water = WaterBody::river(30.0);",
        WaterPreset::Pool => "\nlet water = WaterBody::pool(30.0);",
        WaterPreset::Swamp => "\nlet water = WaterBody::swamp(30.0);",
        WaterPreset::None => "",
    };

    let spawn_code = format!(
        "{}{}\n\n// Spawn entities with components\nworld.spawn()\n    .insert(terrain)\n    .insert(Object3D::default());{}",
        terrain_code,
        water_code,
        if water != WaterPreset::None {
            "\n\nworld.spawn()\n    .insert(water)\n    .insert(Object3D::default());"
        } else {
            ""
        }
    );

    div()
        .px(8.0)
        .py(6.0)
        .bg(Color::rgba(0.1, 0.12, 0.15, 1.0))
        .rounded(4.0)
        .child(
            text(&spawn_code)
                .size(10.0)
                .color(Color::rgba(0.7, 0.9, 0.7, 1.0)),
        )
}

fn control_panel() -> impl ElementBuilder {
    scroll()
        .w(380.0)
        .h_full()
        .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
        .child(
            div()
                .p(16.0)
                .flex_col()
                .gap(16.0)
                .child(
                    text("Terrain Presets")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(terrain_selector())
                .child(divider())
                .child(
                    text("Terrain Configuration")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(terrain_properties())
                .child(divider())
                .child(
                    text("Water Presets")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(water_selector())
                .child(divider())
                .child(
                    text("Water Configuration")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(water_properties())
                .child(divider())
                .child(
                    text("Material Settings")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(material_info()),
        )
}

fn terrain_selector() -> impl ElementBuilder {
    div().flex_col().gap(6.0).children(
        ALL_TERRAIN_PRESETS
            .iter()
            .map(|&preset| terrain_button(preset))
            .collect::<Vec<_>>(),
    )
}

fn terrain_button(preset: TerrainPreset) -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(move |ctx| {
        let current = ctx.use_signal("terrain_preset", || TerrainPreset::Mountains);
        let is_selected = current.get() == preset;

        let (bg, text_color) = match (ctx.state(), is_selected) {
            (_, true) => (preset.color(), Color::WHITE),
            (ButtonState::Hovered, false) => (Color::rgba(0.2, 0.2, 0.25, 1.0), Color::WHITE),            _ => (
                Color::rgba(0.12, 0.12, 0.16, 1.0),
                Color::rgba(0.8, 0.8, 0.8, 1.0),
            ),
        };

        if let Some(event) = ctx.event() {
            if event.event_type == event_types::POINTER_UP {
                current.set(preset);
            }
        }

        let scale = ctx.use_spring(
            "scale",
            if ctx.state() == ButtonState::Pressed {
                0.97
            } else {
                1.0
            },
            SpringConfig::snappy(),
        );

        div()
            .w_full()
            .h(44.0)
            .bg(bg)
            .rounded(6.0)
            .px(12.0)
            .flex_row()
            .items_center()
            .gap(10.0)
            .cursor_pointer()
            .transform(Transform::scale(scale, scale))
            .child(
                div()
                    .w(8.0)
                    .h(8.0)
                    .rounded(4.0)
                    .bg(if is_selected {
                        Color::WHITE
                    } else {
                        preset.color()
                    }),
            )
            .child(
                div()
                    .flex_col()
                    .gap(1.0)
                    .child(
                        text(preset.name())
                            .size(13.0)
                            .weight(FontWeight::Medium)
                            .color(text_color),
                    )
                    .child(
                        text(preset.description())
                            .size(9.0)
                            .color(Color::rgba(0.5, 0.5, 0.5, 1.0)),
                    ),
            )
    })
}

fn water_selector() -> impl ElementBuilder {
    div().flex_row().flex_wrap().gap(6.0).children(
        ALL_WATER_PRESETS
            .iter()
            .map(|&preset| water_chip(preset))
            .collect::<Vec<_>>(),
    )
}

fn water_chip(preset: WaterPreset) -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(move |ctx| {
        let current = ctx.use_signal("water_preset", || WaterPreset::Lake);
        let is_selected = current.get() == preset;

        let bg = if is_selected {
            preset.color()
        } else {
            match ctx.state() {
                ButtonState::Hovered => Color::rgba(0.2, 0.2, 0.25, 1.0),
                _ => Color::rgba(0.12, 0.12, 0.16, 1.0),
            }
        };

        if let Some(event) = ctx.event() {
            if event.event_type == event_types::POINTER_UP {
                current.set(preset);
            }
        }

        div()
            .px(12.0)
            .py(8.0)
            .bg(bg)
            .rounded(16.0)
            .cursor_pointer()
            .child(
                text(preset.name())
                    .size(12.0)
                    .color(if is_selected {
                        Color::WHITE
                    } else {
                        Color::rgba(0.8, 0.8, 0.8, 1.0)
                    }),
            )
    })
}

fn terrain_properties() -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(|ctx| {
        let preset = ctx.use_signal("terrain_preset", || TerrainPreset::Mountains);
        let terrain = preset.get().to_terrain(1000.0, 100.0);

        div()
            .flex_col()
            .gap(8.0)
            .child(property_row("Size", &format!("{:.0} units", terrain.size)))
            .child(property_row(
                "Max Height",
                &format!("{:.0} units", terrain.max_height),
            ))
            .child(property_row(
                "Resolution",
                &format!("{} per chunk", terrain.resolution),
            ))
            .child(property_row(
                "LOD Levels",
                &format!("{}", terrain.lod_levels),
            ))
            .child(noise_layers_info(&terrain))
    })
}

fn noise_layers_info(terrain: &Terrain) -> impl ElementBuilder {
    let mut content = div()
        .flex_col()
        .gap(4.0)
        .mt(8.0)
        .child(
            text("Noise Layers")
                .size(13.0)
                .weight(FontWeight::Medium)
                .color(Color::WHITE),
        );

    // Access noise layers info - terrain struct doesn't expose the array directly
    // We'll show the preset info instead
    content = content.child(
        div()
            .px(8.0)
            .py(4.0)
            .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
            .rounded(4.0)
            .child(
                text("Configured via preset or builder")
                    .size(10.0)
                    .color(Color::rgba(0.6, 0.6, 0.6, 1.0)),
            ),
    );

    content
}

fn water_properties() -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(|ctx| {
        let preset = ctx.use_signal("water_preset", || WaterPreset::Lake);
        let water_level = ctx.use_signal("water_level", || 30.0f32);

        if let Some(water) = preset.get().to_water(water_level.get()) {
            div()
                .flex_col()
                .gap(8.0)
                .child(property_row(
                    "Water Level",
                    &format!("{:.0}", water.water_level),
                ))
                .child(property_row(
                    "Transparency",
                    &format!("{:.0}%", water.transparency * 100.0),
                ))
                .child(property_row(
                    "Wave Intensity",
                    &format!("{:.1}", water.wave_intensity),
                ))
                .child(property_row(
                    "Wave Style",
                    match water.wave_style {
                        WaveStyle::Still => "Still",
                        WaveStyle::Calm => "Calm",
                        WaveStyle::Ocean => "Ocean",
                        WaveStyle::River => "River",
                    },
                ))
                .child(property_row(
                    "Reflections",
                    if water.reflections { "Yes" } else { "No" },
                ))
                .child(property_row(
                    "Refractions",
                    if water.refractions { "Yes" } else { "No" },
                ))
                .child(property_row("Foam", &format!("{:.1}", water.foam)))
                .child(color_swatch("Water Color", water.color))
        } else {
            div().child(
                text("No water body selected")
                    .size(12.0)
                    .color(Color::rgba(0.5, 0.5, 0.5, 1.0)),
            )
        }
    })
}

fn material_info() -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(|ctx| {
        let preset = ctx.use_signal("terrain_preset", || TerrainPreset::Mountains);
        let terrain = preset.get().to_terrain(1000.0, 100.0);
        let material = &terrain.material;

        div()
            .flex_col()
            .gap(8.0)
            .child(color_swatch("Grass", material.grass_color))
            .child(color_swatch("Rock", material.rock_color))
            .child(color_swatch("Snow", material.snow_color))
            .child(color_swatch("Sand", material.sand_color))
            .child(property_row(
                "Snow Height",
                &format!("{:.0}%", material.snow_height * 100.0),
            ))
            .child(property_row(
                "Sand Height",
                &format!("{:.0}%", material.sand_height * 100.0),
            ))
            .child(property_row(
                "Rock Slope",
                &format!("{:.0}%", material.rock_slope * 100.0),
            ))
    })
}

fn property_row(label: &'static str, value: &str) -> impl ElementBuilder {
    let value = value.to_string();
    div()
        .flex_row()
        .justify_between()
        .items_center()
        .child(
            text(label)
                .size(12.0)
                .color(Color::rgba(0.7, 0.7, 0.7, 1.0)),
        )
        .child(
            div()
                .px(8.0)
                .py(3.0)
                .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
                .rounded(4.0)
                .child(text(&value).size(11.0).color(Color::WHITE)),
        )
}

fn color_swatch(label: &'static str, color: Color) -> impl ElementBuilder {
    div()
        .flex_row()
        .justify_between()
        .items_center()
        .child(
            text(label)
                .size(12.0)
                .color(Color::rgba(0.7, 0.7, 0.7, 1.0)),
        )
        .child(
            div()
                .flex_row()
                .gap(6.0)
                .items_center()
                .child(
                    div()
                        .w(20.0)
                        .h(20.0)
                        .rounded(4.0)
                        .bg(color)
                        .border(1.0, Color::rgba(0.3, 0.3, 0.3, 1.0)),
                )
                .child(
                    text(&format!(
                        "({:.0}, {:.0}, {:.0})",
                        color.r * 255.0,
                        color.g * 255.0,
                        color.b * 255.0
                    ))
                    .size(10.0)
                    .color(Color::rgba(0.5, 0.5, 0.5, 1.0)),
                ),
        )
}

fn divider() -> impl ElementBuilder {
    div()
        .w_full()
        .h(1.0)
        .bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
}
