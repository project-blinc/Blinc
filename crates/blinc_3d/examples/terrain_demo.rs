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

use blinc_3d::ecs::{Entity, World};
use blinc_3d::geometry::PlaneGeometry;
use blinc_3d::integration::render_scene;
use blinc_3d::lights::{AmbientLight, DirectionalLight, ShadowConfig};
use blinc_3d::materials::StandardMaterial;
use blinc_3d::prelude::*;
use blinc_3d::scene::{Mesh, Object3D, PerspectiveCamera};
use blinc_3d::utils::terrain::*;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_cn::prelude::*;
use blinc_core::events::event_types;
use blinc_layout::stateful::ButtonState;
use blinc_layout::widgets::elapsed_ms;
use std::f32::consts::PI;

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

    fn to_key(&self) -> &'static str {
        match self {
            TerrainPreset::Mountains => "mountains",
            TerrainPreset::Hills => "hills",
            TerrainPreset::Plains => "plains",
            TerrainPreset::Canyons => "canyons",
            TerrainPreset::Dunes => "dunes",
            TerrainPreset::Island => "island",
            TerrainPreset::Flat => "flat",
            TerrainPreset::Custom => "custom",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "mountains" => TerrainPreset::Mountains,
            "hills" => TerrainPreset::Hills,
            "plains" => TerrainPreset::Plains,
            "canyons" => TerrainPreset::Canyons,
            "dunes" => TerrainPreset::Dunes,
            "island" => TerrainPreset::Island,
            "flat" => TerrainPreset::Flat,
            "custom" => TerrainPreset::Custom,
            _ => TerrainPreset::Mountains,
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
    Lake,
    Ocean,
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

    fn to_key(&self) -> &'static str {
        match self {
            WaterPreset::Ocean => "ocean",
            WaterPreset::Lake => "lake",
            WaterPreset::River => "river",
            WaterPreset::Pool => "pool",
            WaterPreset::Swamp => "swamp",
            WaterPreset::None => "none",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "ocean" => WaterPreset::Ocean,
            "lake" => WaterPreset::Lake,
            "river" => WaterPreset::River,
            "pool" => WaterPreset::Pool,
            "swamp" => WaterPreset::Swamp,
            "none" => WaterPreset::None,
            _ => WaterPreset::Lake,
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
        let water_color = water.color;
        let water_geom = PlaneGeometry::new(25.0, 25.0);
        let water_geom_handle = world.add_geometry(water_geom);
        let water_mat_handle = world.add_material(StandardMaterial {
            color: water_color,
            metalness: 0.2,
            roughness: 0.1,
            ..Default::default()
        });

        world
            .spawn()
            .insert(water)
            .insert(Object3D {
                position: Vec3::new(0.0, water_level * 0.1, 0.0),
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
    let mut camera_transform = Object3D::default();
    camera_transform.position = camera_pos;
    camera_transform.look_at(Vec3::ZERO);

    let camera_entity = world
        .spawn()
        .insert(PerspectiveCamera::new(0.8, 16.0 / 9.0, 0.1, 100.0))
        .insert(camera_transform)
        .id();

    // Spawn ambient light
    world
        .spawn()
        .insert(AmbientLight {
            color: Color::WHITE,
            intensity: 0.3,
        });

    // Spawn directional light (sun)
    world
        .spawn()
        .insert(DirectionalLight {
            color: Color::rgb(1.0, 0.95, 0.9),
            intensity: 1.2,
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
    // Create shared state at top level - this is the key fix!
    let terrain_state = ctx.use_state_keyed("terrain_preset", || "mountains".to_string());
    let water_state = ctx.use_state_keyed("water_preset", || "lake".to_string());

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.06, 0.06, 0.1, 1.0))
        .flex_col()
        .child(header())
        .child(main_content(
            ctx.width,
            ctx.height - 80.0,
            terrain_state,
            water_state,
        ))
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

fn main_content(
    width: f32,
    height: f32,
    terrain_state: blinc_core::State<String>,
    water_state: blinc_core::State<String>,
) -> impl ElementBuilder {
    div()
        .w(width)
        .h(height)
        .flex_row()
        .child(viewport_area(
            width - 380.0,
            height,
            terrain_state.clone(),
            water_state.clone(),
        ))
        .child(control_panel(terrain_state, water_state))
}

fn viewport_area(
    width: f32,
    height: f32,
    terrain_state: blinc_core::State<String>,
    water_state: blinc_core::State<String>,
) -> impl ElementBuilder {
    stateful::<ButtonState>().on_state(move |ctx| {
        // Camera angle offset from mouse drag
        let angle = ctx.use_signal("angle", || 0.5f32);

        // Handle mouse drag for camera rotation
        if let Some(event) = ctx.event() {
            if event.event_type == event_types::POINTER_MOVE && ctx.state() == ButtonState::Pressed
            {
                angle.update(|a| a + event.local_x * 0.0003);
            }
        }

        // Get preset values from shared state
        let terrain_key = terrain_state.get();
        let water_key = water_state.get();
        let current_terrain = TerrainPreset::from_key(&terrain_key);
        let current_water = WaterPreset::from_key(&water_key);
        let angle_offset = angle.get();

        div()
            .w(width)
            .h(height)
            .bg(Color::rgba(0.02, 0.02, 0.05, 1.0))
            .cursor_pointer()
            .child(
                canvas(move |draw_ctx, bounds| {
                    // Get current time inside canvas callback - runs every frame
                    let time = elapsed_ms() as f32 / 1000.0;

                    // Calculate camera position with time-based rotation
                    let cam_angle = angle_offset + time * 0.02;
                    let cam_x = cam_angle.sin() * 15.0;
                    let cam_z = cam_angle.cos() * 15.0;
                    let camera_pos = Vec3::new(cam_x, 8.0, cam_z);

                    // Create ECS World with terrain and water
                    let (world, camera_entity) =
                        create_terrain_world(current_terrain, current_water, 30.0, camera_pos);

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
                    .child(code_example(current_terrain, current_water)),
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
        .child(code(&spawn_code))
}

fn control_panel(
    terrain_state: blinc_core::State<String>,
    water_state: blinc_core::State<String>,
) -> impl ElementBuilder {
    scroll()
        .w(380.0)
        .h_full()
        .bg(Color::GRAY)
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
                .child(terrain_radio_group(terrain_state.clone()))
                .child(divider())
                .child(
                    text("Terrain Configuration")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(terrain_properties(terrain_state.clone()))
                .child(divider())
                .child(
                    text("Water Presets")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(water_radio_group(water_state.clone()))
                .child(divider())
                .child(
                    text("Water Configuration")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(water_properties(water_state.clone()))
                .child(divider())
                .child(
                    text("Material Settings")
                        .size(18.0)
                        .weight(FontWeight::SemiBold)
                        .color(Color::WHITE),
                )
                .child(material_info(terrain_state)),
        )
}

fn terrain_radio_group(terrain_state: blinc_core::State<String>) -> impl ElementBuilder {
    cn::radio_group(&terrain_state)
        .option("mountains", "Mountains - Ridged peaks")
        .option("hills", "Hills - Rolling terrain")
        .option("plains", "Plains - Flat with variation")
        .option("canyons", "Canyons - Deep valleys")
        .option("dunes", "Dunes - Sand formations")
        .option("island", "Island - Ocean falloff")
        .option("flat", "Flat - No elevation")
        .option("custom", "Custom - Multiple noise layers")
}

fn water_radio_group(water_state: blinc_core::State<String>) -> impl ElementBuilder {
    cn::radio_group(&water_state)
        .horizontal()
        .option("ocean", "Ocean")
        .option("lake", "Lake")
        .option("river", "River")
        .option("pool", "Pool")
        .option("swamp", "Swamp")
        .option("none", "None")
}

fn terrain_properties(terrain_state: blinc_core::State<String>) -> impl ElementBuilder {
    let terrain_key = terrain_state.get();
    let preset = TerrainPreset::from_key(&terrain_key);
    let terrain = preset.to_terrain(100.0, 10.0);

    div()
        .flex_col()
        .gap(8.0)
        .child(property_row("Size", &format!("{:.0}m", terrain.size)))
        .child(property_row(
            "Max Height",
            &format!("{:.1}m", terrain.max_height),
        ))
        .child(property_row(
            "LOD Levels",
            &format!("{}", terrain.lod_levels),
        ))
        .child(property_row(
            "Resolution",
            &format!("{}x{}", terrain.resolution, terrain.resolution),
        ))
}

fn water_properties(water_state: blinc_core::State<String>) -> impl ElementBuilder {
    let water_key = water_state.get();
    let preset = WaterPreset::from_key(&water_key);

    if let Some(water) = preset.to_water(30.0) {
        div()
            .flex_col()
            .gap(8.0)
            .child(property_row(
                "Water Level",
                &format!("{:.1}m", water.water_level),
            ))
            .child(property_row(
                "Transparency",
                &format!("{:.0}%", water.transparency * 100.0),
            ))
            .child(property_row(
                "Fresnel",
                &format!("{:.0}%", water.fresnel_strength * 100.0),
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
    } else {
        div().child(
            text("No water selected")
                .size(12.0)
                .color(Color::rgba(0.5, 0.5, 0.5, 1.0)),
        )
    }
}

fn material_info(terrain_state: blinc_core::State<String>) -> impl ElementBuilder {
    let terrain_key = terrain_state.get();
    let preset = TerrainPreset::from_key(&terrain_key);
    let terrain = preset.to_terrain(100.0, 10.0);

    div()
        .flex_col()
        .gap(8.0)
        .child(color_swatch("Grass", terrain.material.grass_color))
        .child(color_swatch("Rock", terrain.material.rock_color))
        .child(color_swatch("Snow", terrain.material.snow_color))
        .child(color_swatch("Sand", terrain.material.sand_color))
        .child(property_row(
            "Snow Height",
            &format!("{:.1}m", terrain.material.snow_height),
        ))
        .child(property_row(
            "Rock Slope",
            &format!("{:.1}°", terrain.material.rock_slope.to_degrees()),
        ))
}

fn property_row(prop_label: &'static str, value: &str) -> impl ElementBuilder {
    let value = value.to_string();
    div()
        .flex_row()
        .justify_between()
        .items_center()
        .child(
            text(prop_label)
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

fn color_swatch(swatch_label: &'static str, color: Color) -> impl ElementBuilder {
    div()
        .flex_row()
        .justify_between()
        .items_center()
        .mt(4.0)
        .child(
            text(swatch_label)
                .size(12.0)
                .color(Color::rgba(0.7, 0.7, 0.7, 1.0)),
        )
        .child(
            div()
                .flex_row()
                .gap(8.0)
                .items_center()
                .child(
                    div()
                        .w(24.0)
                        .h(24.0)
                        .rounded(4.0)
                        .bg(color)
                        .border(1.0, Color::rgba(0.4, 0.4, 0.4, 1.0)),
                )
                .child(
                    text(&format!(
                        "rgb({:.0}, {:.0}, {:.0})",
                        color.r * 255.0,
                        color.g * 255.0,
                        color.b * 255.0
                    ))
                    .size(10.0)
                    .color(Color::rgba(0.6, 0.6, 0.6, 1.0)),
                ),
        )
}

fn divider() -> impl ElementBuilder {
    div()
        .w_full()
        .h(1.0)
        .bg(Color::rgba(0.2, 0.2, 0.25, 1.0))
}
