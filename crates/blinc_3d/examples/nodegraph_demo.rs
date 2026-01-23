//! Node Graph Demo
//!
//! This example demonstrates blinc_3d's visual node graph system:
//! - Creating nodes using built-in node types (BuiltinNode trait)
//! - Connecting nodes together with Connection entities
//! - Node execution with Triggered marker and OnTrigger
//! - Data flow through typed ports
//! - NodeGraphSystem for evaluation
//!
//! Run with: cargo run -p blinc_3d --example nodegraph_demo

use blinc_3d::ecs::{System, SystemContext};
use blinc_3d::nodegraph::builtin::{
    AddNode, BuiltinNode, ClampNode, ConstantFloatNode, MultiplyNode,
};
use blinc_3d::nodegraph::{Connection, Node, NodeGraphSystem, NodeValue, Triggered};
use blinc_3d::prelude::*;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::{Brush, Color, CornerRadius, DrawContext, Path, Rect,  Stroke, Vec2};
use std::sync::{Arc, Mutex};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Blinc 3D - Node Graph Demo".to_string(),
        width: 1200,
        height: 800,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

// ============================================================================
// Visual Node Data
// ============================================================================

#[derive(Clone)]
struct VisualPort {
    name: String,
    value: Option<f32>,
}

#[derive(Clone)]
struct VisualNode {
    entity: Entity,
    name: String,
    node_type: &'static str,
    position: Vec2, // In units
    color: Color,
    input_ports: Vec<VisualPort>,
    output_ports: Vec<VisualPort>,
}

impl VisualNode {
    fn from_world(entity: Entity, world: &World, node_type: &'static str, color: Color) -> Self {
        let node = world
            .get::<Node>(entity)
            .expect("Entity must have Node component");

        let position = node.position.unwrap_or(Vec2::ZERO);
        let name = node
            .display_name
            .clone()
            .unwrap_or_else(|| node_type.to_string());

        let input_ports: Vec<VisualPort> = node
            .inputs()
            .map(|p| VisualPort {
                name: p.name.clone(),
                value: node.get_input_as::<f32>(&p.name),
            })
            .collect();

        let output_ports: Vec<VisualPort> = node
            .outputs()
            .map(|p| VisualPort {
                name: p.name.clone(),
                value: node.get_output_as::<f32>(&p.name),
            })
            .collect();

        Self {
            entity,
            name,
            node_type,
            position,
            color,
            input_ports,
            output_ports,
        }
    }
}

/// Connection between two ports for visual rendering
#[derive(Clone, Debug)]
struct VisualConnection {
    from_entity: Entity,
    from_port: String,
    to_entity: Entity,
    to_port: String,
}

/// Port position for connection drawing (computed from node position + port index)
#[derive(Clone, Debug)]
struct PortPosition {
    entity: Entity,
    port_name: String,
    is_output: bool,
    x: f32,
    y: f32,
}

// ============================================================================
// Node Graph Setup
// ============================================================================

fn create_node_graph(world: &mut World) -> (Vec<VisualNode>, Vec<VisualConnection>) {
    let mut visual_nodes = Vec::new();

    // Node layout constants - increased spacing between columns
    const COL_0: f32 = 30.0;
    const COL_1: f32 = 220.0;
    const COL_2: f32 = 410.0;
    const COL_3: f32 = 600.0;
    const COL_4: f32 = 790.0;
    const ROW_TOP: f32 = 40.0;
    const ROW_BOT: f32 = 280.0;
    const ROW_MID: f32 = 160.0;

    let value_a = ConstantFloatNode::spawn(world);
    if let Some(node) = world.get_mut::<Node>(value_a) {
        node.position = Some(Vec2::new(COL_0, ROW_TOP));
        node.set_input("value", NodeValue::Float(10.0));
    }
    visual_nodes.push(VisualNode::from_world(
        value_a,
        world,
        "Constant",
        Color::rgb(0.3, 0.6, 0.9),
    ));

    let value_b = ConstantFloatNode::spawn(world);
    if let Some(node) = world.get_mut::<Node>(value_b) {
        node.position = Some(Vec2::new(COL_0, ROW_BOT));
        node.set_input("value", NodeValue::Float(5.0));
    }
    visual_nodes.push(VisualNode::from_world(
        value_b,
        world,
        "Constant",
        Color::rgb(0.3, 0.6, 0.9),
    ));

    let add_node = AddNode::spawn(world);
    if let Some(node) = world.get_mut::<Node>(add_node) {
        node.position = Some(Vec2::new(COL_1, ROW_TOP));
    }
    visual_nodes.push(VisualNode::from_world(
        add_node,
        world,
        "Add",
        Color::rgb(0.9, 0.5, 0.3),
    ));

    let multiplier = ConstantFloatNode::spawn(world);
    if let Some(node) = world.get_mut::<Node>(multiplier) {
        node.position = Some(Vec2::new(COL_1, ROW_BOT));
        node.set_input("value", NodeValue::Float(2.0));
    }
    visual_nodes.push(VisualNode::from_world(
        multiplier,
        world,
        "Constant",
        Color::rgb(0.3, 0.6, 0.9),
    ));

    let multiply_node = MultiplyNode::spawn(world);
    if let Some(node) = world.get_mut::<Node>(multiply_node) {
        node.position = Some(Vec2::new(COL_2, ROW_MID));
    }
    visual_nodes.push(VisualNode::from_world(
        multiply_node,
        world,
        "Multiply",
        Color::rgb(0.9, 0.5, 0.3),
    ));

    let clamp_node = ClampNode::spawn(world);
    if let Some(node) = world.get_mut::<Node>(clamp_node) {
        node.position = Some(Vec2::new(COL_3, ROW_MID));
        node.set_input("min", NodeValue::Float(0.0));
        node.set_input("max", NodeValue::Float(25.0));
    }
    visual_nodes.push(VisualNode::from_world(
        clamp_node,
        world,
        "Clamp",
        Color::rgb(0.5, 0.9, 0.4),
    ));

    let output_node = world
        .spawn()
        .insert(
            Node::new()
                .with_input::<f32>("final")
                .with_output::<f32>("out")
                .with_name("Output")
                .with_position(Vec2::new(COL_4, ROW_MID)),
        )
        .insert(blinc_3d::nodegraph::OnTrigger::passthrough())
        .id();
    visual_nodes.push(VisualNode::from_world(
        output_node,
        world,
        "Output",
        Color::rgb(0.9, 0.3, 0.5),
    ));

    // Create connections
    let mut visual_connections = Vec::new();

    // Helper to create both ECS connection and visual connection
    let mut add_connection = |from_entity: Entity,
                              from_port: &str,
                              to_entity: Entity,
                              to_port: &str| {
        world
            .spawn()
            .insert(Connection::new(from_entity, from_port, to_entity, to_port));
        visual_connections.push(VisualConnection {
            from_entity,
            from_port: from_port.to_string(),
            to_entity,
            to_port: to_port.to_string(),
        });
    };

    // value_a.out -> add.a
    add_connection(value_a, "out", add_node, "a");
    // value_b.out -> add.b
    add_connection(value_b, "out", add_node, "b");
    // add.result -> multiply.a
    add_connection(add_node, "result", multiply_node, "a");
    // multiplier.out -> multiply.b
    add_connection(multiplier, "out", multiply_node, "b");
    // multiply.result -> clamp.value
    add_connection(multiply_node, "result", clamp_node, "value");
    // clamp.result -> output.final
    add_connection(clamp_node, "result", output_node, "final");

    // Execute graph
    for vn in &visual_nodes {
        world.insert(vn.entity, Triggered);
    }

    (visual_nodes, visual_connections)
}

fn execute_graph(world: &mut World) {
    let mut system = NodeGraphSystem;
    let mut ctx = SystemContext {
        world,
        delta_time: 0.016,
        elapsed_time: 0.0,
        frame: 0,
    };
    system.run(&mut ctx);
}

fn update_visual_nodes(world: &World, visual_nodes: &mut [VisualNode]) {
    for vn in visual_nodes.iter_mut() {
        if let Some(node) = world.get::<Node>(vn.entity) {
            // Update input port values
            for port in &mut vn.input_ports {
                port.value = node.get_input_as::<f32>(&port.name);
            }
            // Update output port values
            for port in &mut vn.output_ports {
                port.value = node.get_output_as::<f32>(&port.name);
            }
        }
    }
}

// ============================================================================
// UI Building
// ============================================================================

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let mut world = World::new();
    let (mut visual_nodes, visual_connections) = create_node_graph(&mut world);
    execute_graph(&mut world);
    update_visual_nodes(&world, &mut visual_nodes);

    let _world = Arc::new(Mutex::new(world));

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.06, 0.06, 0.1, 1.0))
        .flex_col()
        .p(4.0)
        .gap(4.0)
        .child(header_section())
        .child(
            div()
                .flex_1()
                .flex_row()
                .gap(4.0)
                .justify_between()
                .child(div().w_full().h(800.0).overflow_clip().child(graph_area(
                    ctx,
                    visual_nodes,
                    visual_connections,
                )))
                .child(side_panel()),
        )
}

fn header_section() -> Div {
    div()
        .flex_row()
        .justify_between()
        .items_center()
        .child(
            div()
                .flex_col()
                .gap(1.0)
                .child(
                    text("Blinc 3D - Node Graph System")
                        .size(28.0)
                        .color(Color::WHITE),
                )
                .child(
                    text("Visual programming with typed ports and data flow")
                        .size(14.0)
                        .color(Color::rgba(0.6, 0.6, 0.7, 1.0)),
                ),
        )
        .child(
            div()
                .px(3.0)
                .py(2.0)
                .bg(Color::rgba(0.2, 0.5, 0.3, 0.3))
                .rounded(2.0)
                .flex_row()
                .gap(2.0)
                .items_center()
                .child(
                    div()
                        .w(2.0)
                        .h(2.0)
                        .rounded(1.0)
                        .bg(Color::rgb(0.3, 1.0, 0.5)),
                )
                .child(
                    text("Graph Executed")
                        .size(11.0)
                        .color(Color::rgb(0.5, 1.0, 0.7)),
                ),
        )
}

// ============================================================================
// Graph Area - Stack with canvas (grid) + div layer (nodes)
// ============================================================================

    // Compute port positions from node positions
    // Node layout: header (~24px) + body padding (8px) + ports
    // Each port row is ~22px tall with 4px gap
    const NODE_WIDTH: f32 = 150.0;
    const HEADER_HEIGHT: f32 = 24.0;
    const BODY_PADDING: f32 = 8.0;
    const PORT_HEIGHT: f32 = 22.0;
    const PORT_GAP: f32 = 4.0 * 4.0;
fn graph_area(
    _ctx: &WindowedContext,
    nodes: Vec<VisualNode>,
    connections: Vec<VisualConnection>,
) -> Div {
    let mut port_positions = Vec::new();

    for node in &nodes {
        let node_x = node.position.x;
        let node_y = node.position.y;

        // Input ports: left edge of node
        for (i, port) in node.input_ports.iter().enumerate() {
            let port_y =
                node_y + HEADER_HEIGHT + BODY_PADDING + (i as f32) * (PORT_HEIGHT + PORT_GAP) + PORT_HEIGHT / 2.0;
            port_positions.push(PortPosition {
                entity: node.entity,
                port_name: port.name.clone(),
                is_output: false,
                x: node_x,
                y: port_y,
            });
        }

        // Output ports: right edge of node
        for (i, port) in node.output_ports.iter().enumerate() {
            let port_y =
                node_y + HEADER_HEIGHT + BODY_PADDING + (i as f32) * (PORT_HEIGHT + PORT_GAP) + PORT_HEIGHT / 2.0;
            port_positions.push(PortPosition {
                entity: node.entity,
                port_name: port.name.clone(),
                is_output: true,
                x: node_x + NODE_WIDTH,
                y: port_y,
            });
        }
    }

    let _nodes = Arc::new(nodes);
    div()
        .w_full()
        .h_full()
        .absolute()
        .child(
            stack()
                .flex_grow()
                .h_full()
                .bg(Color::rgba(0.08, 0.08, 0.12, 1.0))
                .rounded(2.0)
                .overflow_clip()
                // Layer 1: Canvas for grid and connections
                .child(
                    div()
                        .w_full()
                        .h_full()
                        .child(grid_canvas(port_positions, connections)),
                )
                // Layer 2: Node divs positioned absolutely
                .child(nodes_layer(_nodes.to_vec())),
        )
}

fn grid_canvas(port_positions: Vec<PortPosition>, connections: Vec<VisualConnection>) -> Canvas {
    canvas(move |ctx: &mut dyn DrawContext, bounds| {
        let marker_radius = 4.0;

        // Layer 0: Grid (background)
        ctx.set_z_layer(0);
        let grid_color = Color::rgba(0.15, 0.15, 0.2, 1.0);
        let grid_size = 20.0;

        for x in (0..(bounds.width as i32)).step_by(grid_size as usize) {
            ctx.fill_rect(
                Rect::new(x as f32, 0.0, 1.0, bounds.height),
                CornerRadius::ZERO,
                Brush::Solid(grid_color),
            );
        }
        for y in (0..(bounds.height as i32)).step_by(grid_size as usize) {
            ctx.fill_rect(
                Rect::new(0.0, y as f32, bounds.width, 1.0),
                CornerRadius::ZERO,
                Brush::Solid(grid_color),
            );
        }

        // Layer 1: Connections (middle)
        ctx.set_z_layer(1);
        let connection_color = Color::rgba(0.4, 0.7, 1.0, 0.8);

        for conn in &connections {
            // Find source port (output)
            let from_port = port_positions.iter().find(|p| {
                p.entity == conn.from_entity && p.port_name == conn.from_port && p.is_output
            });

            // Find target port (input)
            let to_port = port_positions.iter().find(|p| {
                p.entity == conn.to_entity && p.port_name == conn.to_port && !p.is_output
            });

            if let (Some(from), Some(to)) = (from_port, to_port) {
                let from_x = from.x + marker_radius;
                let from_y = from.y;
                let to_x = to.x - marker_radius;
                let to_y = to.y;

                // Draw bezier curve connection
                let control_offset = ((to_x - from_x).abs() / 2.0).max(50.0);
                let ctrl1_x = from_x + control_offset;
                let ctrl1_y = from_y;
                let ctrl2_x = to_x - control_offset;
                let ctrl2_y = to_y;

                draw_bezier_curve(
                    ctx,
                    from_x,
                    from_y,
                    ctrl1_x,
                    ctrl1_y,
                    ctrl2_x,
                    ctrl2_y,
                    to_x,
                    to_y,
                    connection_color,
                    2.0,
                );
            }
        }

        // Layer 2: Port markers (foreground)
        ctx.set_z_layer(2);
        let input_marker_color = Color::rgba(0.3, 0.8, 0.5, 1.0);
        let output_marker_color = Color::rgba(0.8, 0.5, 0.3, 1.0);

        for port in &port_positions {
            let color = if port.is_output {
                output_marker_color
            } else {
                input_marker_color
            };

            ctx.fill_rect(
                Rect::new(
                    port.x - marker_radius,
                    port.y - marker_radius,
                    marker_radius * 2.0,
                    marker_radius * 2.0,
                ),
                CornerRadius::uniform(marker_radius),
                Brush::Solid(color),
            );
        }
    })
    .w_full()
    .h_full()
}

/// Draw a smooth cubic bezier curve using the Path API
fn draw_bezier_curve(
    ctx: &mut dyn DrawContext,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    x3: f32,
    y3: f32,
    color: Color,
    thickness: f32,
) {
    let path = Path::new()
        .move_to(x0, y0)
        .cubic_to(x1, y1, x2, y2, x3, y3);

    ctx.stroke_path(&path, &Stroke::new(thickness), Brush::Solid(color));
}

// ============================================================================
// Node Layer
// ============================================================================

fn nodes_layer(nodes: Vec<VisualNode>) -> Div {
    let mut container = div().w_full().h_full();

    for node in nodes.iter() {
        container = container.child(node_div(node));
    }

    container
}

fn node_div(node: &VisualNode) -> Div {
    // Build input ports - left side, each port is a flex-row of label and value
    let mut inputs_col = div().flex_col().gap(4.0).items_start();
    for port in &node.input_ports {
        let port_id = format!("inport-{:?}-{}", node.entity, port.name);
        let value_str = port.value.map(|v| format!("{:.1}", v)).unwrap_or_default();
        inputs_col = inputs_col.child(
            div()
                .id(port_id)
                .flex_row()
                .gap(2.0)
                .items_center()
                .child(
                    text(port.name.as_str())
                        .size(10.0)
                        .color(Color::rgba(0.7, 0.7, 0.8, 1.0)),
                )
                .when(!value_str.is_empty(), |d| {
                    d.child(
                        div()
                            .px(1.0)
                            .py(1.0)
                            .bg(Color::rgba(0.0, 0.4, 0.25, 0.6))
                            .rounded(2.0)
                            .child(
                                text(value_str.clone())
                                    .size(10.0)
                                    .color(Color::rgb(0.5, 1.0, 0.7)),
                            ),
                    )
                })
                .when(value_str.is_empty(), |d| d.child(div().px(1.0).py(1.0))),
        );
    }

    // Build output ports - right side, each port is a flex-row of value and label
    let mut outputs_col = div().flex_col().gap(4.0).items_end();
    for port in &node.output_ports {
        let port_id = format!("outport-{:?}-{}", node.entity, port.name);
        let value_str = port.value.map(|v| format!("{:.1}", v)).unwrap_or_default();
        outputs_col = outputs_col.child(
            div()
                .id(port_id)
                .flex_row()
                .gap(2.0)
                .items_center()
                .when(!value_str.is_empty(), |d| {
                    d.child(
                        div()
                            .px(1.0)
                            .py(1.0)
                            .bg(Color::rgba(0.0, 0.4, 0.25, 0.6))
                            .rounded(2.0)
                            .child(
                                text(value_str.clone())
                                    .size(10.0)
                                    .color(Color::rgb(0.5, 1.0, 0.7)),
                            ),
                    )
                })
                .when(value_str.is_empty(), |d| d.child(div().px(1.0).py(1.0)))
                .child(
                    text(port.name.as_str())
                        .size(10.0)
                        .color(Color::rgba(0.7, 0.7, 0.8, 1.0)),
                ),
        );
    }

    div()
        .id(format!("node-{:?}", node.entity))
        .absolute()
        .left(node.position.x)
        .top(node.position.y)
        .shadow_lg()
        .bg(Color::rgba(0.15, 0.15, 0.2, 0.95))
        .rounded(2.0)
        .flex_col()
        .w(150.0)
        // Header
        .child(
            div()
                .flex_row()
                .w_full()
                .px(2.0)
                .py(1.0)
                .bg(node.color)
                .rounded(2.0)
                .child(
                    text(node.name.as_str())
                        .size(11.0)
                        .color(Color::WHITE)
                        .bold(),
                ),
        )
        // Body with ports
        .child(
            div()
                .w_full()
                .p(2.0)
                .flex_row()
                .justify_between()
                .child(inputs_col)
                .child(outputs_col),
        )
}

// ============================================================================
// Side Panel
// ============================================================================

fn side_panel() -> Scroll {
    scroll()
        .h_full()
        .bg(Color::rgba(0.1, 0.1, 0.14, 1.0))
        .rounded(2.0)
        .p(4.0)
        .child(
            div()
                .w_full()
                .flex_col()
                .gap(4.0)
                .child(text("Node Graph API").size(16.0).color(Color::WHITE).bold())
                .child(
                    code("// Create nodes\nlet add = AddNode::spawn(&mut world);\n\n// Connect ports\nworld.spawn().insert(\n  Connection::new(\n    value_a, \"out\",\n    add_node, \"a\"\n  )\n);\n\n// Execute graph\nworld.insert(entity, Triggered);")
                        .font_size(10.0)
                        .rounded(1.0),
                )
                .child(text("Data Flow").size(16.0).color(Color::WHITE).bold())
                .child(flow_info())
                .child(text("Node Types").size(16.0).color(Color::WHITE).bold())
                .child(node_types_info()),
        )
}

fn flow_info() -> Div {
    div()
        .flex_col()
        .gap(2.0)
        .p(3.0)
        .bg(Color::rgba(0.12, 0.12, 0.16, 1.0))
        .rounded(1.0)
        .child(flow_step("1", "Value A = 10.0"))
        .child(flow_step("2", "Value B = 5.0"))
        .child(flow_step("3", "Add: 10 + 5 = 15"))
        .child(flow_step("4", "Multiply: 15 × 2 = 30"))
        .child(flow_step("5", "Clamp(0, 25): 30 → 25"))
        .child(flow_step("✓", "Output = 25.0"))
}

fn flow_step(num: &'static str, desc: &'static str) -> Div {
    div()
        .flex_row()
        .gap(2.0)
        .items_center()
        .child(
            div()
                .w(5.0)
                .h(5.0)
                .rounded(2.5)
                .bg(Color::rgba(0.3, 0.5, 0.8, 0.5))
                .flex_row()
                .justify_center()
                .items_center()
                .child(text(num).size(10.0).color(Color::WHITE)),
        )
        .child(text(desc).size(11.0).color(Color::rgba(0.7, 0.7, 0.8, 1.0)))
}

fn node_types_info() -> Div {
    div()
        .flex_col()
        .gap(2.0)
        .child(node_type_badge(
            "Constant",
            "Outputs a fixed value",
            Color::rgb(0.3, 0.6, 0.9),
        ))
        .child(node_type_badge(
            "Add",
            "Adds two inputs",
            Color::rgb(0.9, 0.5, 0.3),
        ))
        .child(node_type_badge(
            "Multiply",
            "Multiplies two inputs",
            Color::rgb(0.9, 0.5, 0.3),
        ))
        .child(node_type_badge(
            "Clamp",
            "Clamps value to range",
            Color::rgb(0.5, 0.9, 0.4),
        ))
        .child(node_type_badge(
            "Output",
            "Final result node",
            Color::rgb(0.9, 0.3, 0.5),
        ))
}

fn node_type_badge(name: &'static str, desc: &'static str, color: Color) -> Div {
    div()
        .flex_row()
        .gap(2.0)
        .items_center()
        .child(div().w(3.0).h(3.0).rounded(0.5).bg(color))
        .child(
            div()
                .flex_col()
                .child(text(name).size(11.0).color(Color::WHITE))
                .child(text(desc).size(9.0).color(Color::rgba(0.5, 0.5, 0.6, 1.0))),
        )
}
