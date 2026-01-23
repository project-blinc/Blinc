//! ECS (Entity Component System) Demo
//!
//! This example demonstrates blinc_3d's ECS system:
//! - Creating a World and spawning entities
//! - Adding components to entities
//! - Querying entities by component type
//! - Running systems that process entities
//! - Integration with Blinc's animation scheduler for smooth updates
//!
//! Run with: cargo run -p blinc_3d --example ecs_demo

use blinc_3d::prelude::*;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::{Brush, Color, CornerRadius, DrawContext, Rect};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Blinc 3D - ECS Demo".to_string(),
        width: 1000,
        height: 700,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

// ============================================================================
// Custom Components
// ============================================================================

/// A simple position component
#[derive(Clone, Debug)]
struct Position {
    x: f32,
    y: f32,
}

impl Component for Position {
    const STORAGE: blinc_3d::ecs::StorageType = blinc_3d::ecs::StorageType::Dense;
}

/// A velocity component for movement
#[derive(Clone, Debug)]
struct Velocity {
    dx: f32,
    dy: f32,
}

impl Component for Velocity {
    const STORAGE: blinc_3d::ecs::StorageType = blinc_3d::ecs::StorageType::Dense;
}

/// A color component for rendering
#[derive(Clone, Debug)]
struct ColorComponent {
    color: Color,
}

impl Component for ColorComponent {
    const STORAGE: blinc_3d::ecs::StorageType = blinc_3d::ecs::StorageType::Dense;
}

/// A size component
#[derive(Clone, Debug)]
struct Size {
    width: f32,
    height: f32,
}

impl Component for Size {
    const STORAGE: blinc_3d::ecs::StorageType = blinc_3d::ecs::StorageType::Dense;
}

/// A name component for identification
#[derive(Clone, Debug)]
struct Name(String);

impl Component for Name {
    const STORAGE: blinc_3d::ecs::StorageType = blinc_3d::ecs::StorageType::Sparse;
}

// ============================================================================
// Systems
// ============================================================================

/// Movement system that updates positions based on velocity
struct MovementSystem;

impl System for MovementSystem {
    fn run(&mut self, ctx: &mut SystemContext) {
        // Query all entities with both Position and Velocity
        // Collect entities first since we need to mutate
        let entities: Vec<_> = ctx.world.query::<(&Position, &Velocity)>()
            .iter()
            .map(|(e, _)| e)
            .collect();

        for entity in entities {
            let vel = ctx.world.get::<Velocity>(entity).map(|v| (v.dx, v.dy));
            if let Some((dx, dy)) = vel {
                if let Some(pos) = ctx.world.get_mut::<Position>(entity) {
                    pos.x += dx * ctx.delta_time;
                    pos.y += dy * ctx.delta_time;
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        "MovementSystem"
    }

    fn stage(&self) -> SystemStage {
        SystemStage::Update
    }
}

/// Boundary bounce system
struct BounceSystem {
    bounds: (f32, f32, f32, f32), // (min_x, min_y, max_x, max_y)
}

impl System for BounceSystem {
    fn run(&mut self, ctx: &mut SystemContext) {
        // Collect entities first since we need to mutate
        let entities: Vec<_> = ctx.world.query::<(&Position, &Velocity, &Size)>()
            .iter()
            .map(|(e, _)| e)
            .collect();

        for entity in entities {
            // Get current state
            let size = ctx.world.get::<Size>(entity).map(|s| (s.width, s.height));
            let pos_state = ctx.world.get::<Position>(entity).map(|p| (p.x, p.y));

            if let (Some((width, height)), Some((px, py))) = (size, pos_state) {
                let mut new_dx = None;
                let mut new_dy = None;
                let mut new_x = px;
                let mut new_y = py;

                // Bounce off left/right walls
                if px < self.bounds.0 || px + width > self.bounds.2 {
                    if let Some(vel) = ctx.world.get::<Velocity>(entity) {
                        new_dx = Some(-vel.dx);
                    }
                    new_x = px.clamp(self.bounds.0, self.bounds.2 - width);
                }
                // Bounce off top/bottom walls
                if py < self.bounds.1 || py + height > self.bounds.3 {
                    if let Some(vel) = ctx.world.get::<Velocity>(entity) {
                        new_dy = Some(-vel.dy);
                    }
                    new_y = py.clamp(self.bounds.1, self.bounds.3 - height);
                }

                // Apply updates
                if let Some(pos) = ctx.world.get_mut::<Position>(entity) {
                    pos.x = new_x;
                    pos.y = new_y;
                }
                if let Some(vel) = ctx.world.get_mut::<Velocity>(entity) {
                    if let Some(dx) = new_dx {
                        vel.dx = dx;
                    }
                    if let Some(dy) = new_dy {
                        vel.dy = dy;
                    }
                }
            }
        }
    }

    fn name(&self) -> &'static str {
        "BounceSystem"
    }

    fn stage(&self) -> SystemStage {
        SystemStage::Update
    }
}

// ============================================================================
// World Setup
// ============================================================================

fn create_demo_world() -> World {
    let mut world = World::new();

    // Spawn several entities with different component combinations
    let colors = [
        Color::rgb(0.9, 0.3, 0.3), // Red
        Color::rgb(0.3, 0.9, 0.3), // Green
        Color::rgb(0.3, 0.3, 0.9), // Blue
        Color::rgb(0.9, 0.9, 0.3), // Yellow
        Color::rgb(0.9, 0.3, 0.9), // Magenta
        Color::rgb(0.3, 0.9, 0.9), // Cyan
    ];

    for i in 0..6 {
        let x = 100.0 + (i as f32 * 120.0);
        let y = 200.0 + (i as f32 % 3.0) * 80.0;
        let dx = 50.0 + (i as f32 * 20.0) * if i % 2 == 0 { 1.0 } else { -1.0 };
        let dy = 30.0 + (i as f32 * 15.0) * if i % 3 == 0 { 1.0 } else { -1.0 };

        world
            .spawn()
            .insert(Position { x, y })
            .insert(Velocity { dx, dy })
            .insert(ColorComponent { color: colors[i] })
            .insert(Size {
                width: 40.0 + (i as f32 * 5.0),
                height: 40.0 + (i as f32 * 5.0),
            })
            .insert(Name(format!("Entity {}", i)));
    }

    // Add a static entity (no velocity)
    world
        .spawn()
        .insert(Position { x: 450.0, y: 300.0 })
        .insert(ColorComponent {
            color: Color::rgb(1.0, 1.0, 1.0),
        })
        .insert(Size {
            width: 60.0,
            height: 60.0,
        })
        .insert(Name("Static Entity".to_string()));

    world
}

// ============================================================================
// Shared bounds for the ECS world (updated by canvas, read by tick callback)
// ============================================================================

use std::sync::atomic::{AtomicU32, Ordering};

/// Shared canvas bounds for the bounce system
static CANVAS_WIDTH: AtomicU32 = AtomicU32::new(700);
static CANVAS_HEIGHT: AtomicU32 = AtomicU32::new(500);

// ============================================================================
// UI Building
// ============================================================================

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    // Create the world using persisted state (survives UI rebuilds)
    let world = ctx.use_state_keyed("ecs_world", || {
        std::sync::Arc::new(std::sync::Mutex::new(create_demo_world()))
    });

    // Register tick callback to run ECS systems at 120fps
    // This runs on the animation scheduler's background thread
    let world_for_tick = world.get();
    ctx.use_tick_callback(move |dt| {
        if let Ok(mut world) = world_for_tick.lock() {
            // Get current canvas bounds
            let width = CANVAS_WIDTH.load(Ordering::Relaxed) as f32;
            let height = CANVAS_HEIGHT.load(Ordering::Relaxed) as f32;

            // Run ECS systems
            let mut movement = MovementSystem;
            let mut bounce = BounceSystem {
                bounds: (0.0, 0.0, width, height),
            };

            let mut sys_ctx = SystemContext {
                world: &mut world,
                delta_time: dt.min(0.1), // Cap at 100ms to avoid large jumps
                elapsed_time: 0.0,
                frame: 0,
            };

            movement.run(&mut sys_ctx);
            bounce.run(&mut sys_ctx);
        }
    });

    let world_for_canvas = world.get();

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.06, 0.06, 0.1, 1.0))
        .flex_col()
        .p(16.0)
        .gap(16.0)
        // Title
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(text("Blinc 3D - ECS Demo").size(28.0).color(Color::WHITE))
                .child(
                    text("Entity Component System with custom components and systems")
                        .size(14.0)
                        .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
                ),
        )
        // Main content
        .child(
            div()
                .flex_1()
                .flex_row()
                .gap(16.0)
                // Canvas for ECS visualization
                .child(ecs_canvas(world_for_canvas))
                // Static info panel (describes the demo)
                .child(info_panel()),
        )
}

fn ecs_canvas(world: std::sync::Arc<std::sync::Mutex<World>>) -> Canvas {
    canvas(move |ctx: &mut dyn DrawContext, bounds| {
        // Update shared bounds for the tick callback's bounce system
        CANVAS_WIDTH.store(bounds.width as u32, Ordering::Relaxed);
        CANVAS_HEIGHT.store(bounds.height as u32, Ordering::Relaxed);

        // Lock world for rendering only (systems run in tick callback)
        let world = world.lock().unwrap();

        // Draw background
        ctx.fill_rect(
            Rect::new(0.0, 0.0, bounds.width, bounds.height),
            CornerRadius::ZERO,
            Brush::Solid(Color::rgba(0.1, 0.1, 0.15, 1.0)),
        );

        // Draw grid
        let grid_color = Color::rgba(0.2, 0.2, 0.25, 1.0);
        let grid_spacing = 50.0;
        for x in (0..(bounds.width as i32)).step_by(grid_spacing as usize) {
            ctx.fill_rect(
                Rect::new(x as f32, 0.0, 1.0, bounds.height),
                CornerRadius::ZERO,
                Brush::Solid(grid_color),
            );
        }
        for y in (0..(bounds.height as i32)).step_by(grid_spacing as usize) {
            ctx.fill_rect(
                Rect::new(0.0, y as f32, bounds.width, 1.0),
                CornerRadius::ZERO,
                Brush::Solid(grid_color),
            );
        }

        // Query and draw all entities with Position, Size, and Color
        for (_entity, (pos, size, color)) in world
            .query::<(&Position, &Size, &ColorComponent)>()
            .iter()
        {
            ctx.fill_rect(
                Rect::new(pos.x, pos.y, size.width, size.height),
                CornerRadius::ZERO,
                Brush::Solid(color.color),
            );

            // Draw a border
            let border_color = Color::rgba(1.0, 1.0, 1.0, 0.3);
            ctx.fill_rect(
                Rect::new(pos.x, pos.y, size.width, 2.0),
                CornerRadius::ZERO,
                Brush::Solid(border_color),
            );
            ctx.fill_rect(
                Rect::new(pos.x, pos.y + size.height - 2.0, size.width, 2.0),
                CornerRadius::ZERO,
                Brush::Solid(border_color),
            );
            ctx.fill_rect(
                Rect::new(pos.x, pos.y, 2.0, size.height),
                CornerRadius::ZERO,
                Brush::Solid(border_color),
            );
            ctx.fill_rect(
                Rect::new(pos.x + size.width - 2.0, pos.y, 2.0, size.height),
                CornerRadius::ZERO,
                Brush::Solid(border_color),
            );
        }
    })
    .w(700.0)
    .h_full()
}

/// Static info panel describing the ECS demo
fn info_panel() -> Div {
    div()
        .w(250.0)
        .h_full()
        .bg(Color::rgba(0.12, 0.12, 0.16, 1.0))
        .rounded(8.0)
        .p(16.0)
        .flex_col()
        .gap(16.0)
        // Components header
        .child(text("Components Used").size(16.0).color(Color::WHITE))
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(component_badge("Position", "x, y coordinates"))
                .child(component_badge("Velocity", "dx, dy movement"))
                .child(component_badge("ColorComponent", "RGBA color"))
                .child(component_badge("Size", "width, height"))
                .child(component_badge("Name", "Entity identifier")),
        )
        // Systems header
        .child(text("Active Systems").size(16.0).color(Color::WHITE))
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(system_badge("MovementSystem", "Updates positions"))
                .child(system_badge("BounceSystem", "Boundary collision")),
        )
        // Demo info
        .child(text("Demo Info").size(16.0).color(Color::WHITE))
        .child(
            div()
                .flex_col()
                .gap(4.0)
                .child(info_item("7 entities spawned"))
                .child(info_item("6 with velocity (moving)"))
                .child(info_item("1 static (white square)"))
                .child(info_item("Systems run each frame"))
                .child(info_item("Queries filter by components")),
        )
}

fn component_badge(name: &'static str, desc: &'static str) -> Div {
    div()
        .flex_col()
        .gap(2.0)
        .child(
            div()
                .px(8.0)
                .py(4.0)
                .bg(Color::rgba(0.2, 0.4, 0.8, 0.3))
                .rounded(4.0)
                .child(
                    text(name)
                        .size(12.0)
                        .color(Color::rgba(0.5, 0.7, 1.0, 1.0)),
                ),
        )
        .child(
            text(desc)
                .size(10.0)
                .color(Color::rgba(0.5, 0.5, 0.6, 1.0)),
        )
}

fn system_badge(name: &'static str, desc: &'static str) -> Div {
    div()
        .flex_col()
        .gap(2.0)
        .child(
            div()
                .px(8.0)
                .py(4.0)
                .bg(Color::rgba(0.4, 0.8, 0.2, 0.3))
                .rounded(4.0)
                .child(
                    text(name)
                        .size(12.0)
                        .color(Color::rgba(0.5, 1.0, 0.5, 1.0)),
                ),
        )
        .child(
            text(desc)
                .size(10.0)
                .color(Color::rgba(0.5, 0.5, 0.6, 1.0)),
        )
}

fn info_item(content: &'static str) -> Div {
    div()
        .flex_row()
        .gap(6.0)
        .items_center()
        .child(
            div()
                .w(4.0)
                .h(4.0)
                .rounded(2.0)
                .bg(Color::rgba(0.5, 0.5, 0.6, 0.8)),
        )
        .child(
            text(content)
                .size(11.0)
                .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
        )
}
