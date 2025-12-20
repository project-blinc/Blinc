# BLINC Canvas Architecture

**B**lended **L**ayers for **IN**teractive **C**anvas

A unified rendering architecture for visual and interactive applications spanning 2D UI, vector graphics, and 3D scenes.

---

## Design Philosophy

BLINC treats all visual content as composable layers rendered to a unified canvas. Rather than bolting 3D onto a 2D framework or vice versa, BLINC provides first-class primitives for each domain with seamless composition.

```
┌─────────────────────────────────────────────────────────────────┐
│                                                                 │
│   "A settings panel, a vector illustration, a 3D product       │
│    viewer, and a game HUD are all the same thing:              │
│    layers of content composed onto a canvas."                  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Core Principles

1. **Layer Unification** — 2D primitives, vector paths, and 3D meshes are all layers with consistent composition semantics

2. **Context Adaptation** — A single `DrawContext` adapts to the current layer type, providing appropriate operations

3. **Dimension Bridging** — 2D content can be placed in 3D space (billboards), 3D content can be embedded in 2D layout (viewports)

4. **AOT Optimization** — Static analysis at compile time enables render target pre-allocation, shader variant selection, and update minimization

5. **Progressive Capability** — Applications pay only for the rendering features they use; a pure UI app excludes 3D entirely

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        BLINC Canvas                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                     Scene Graph                          │   │
│  │                                                          │   │
│  │   SceneRoot                                              │   │
│  │      ├── Layer2D (UI)                                    │   │
│  │      │     ├── Container                                 │   │
│  │      │     │     ├── Text                                │   │
│  │      │     │     └── Button                              │   │
│  │      │     └── Canvas2D { paint | ... }                  │   │
│  │      │                                                   │   │
│  │      ├── Layer3D (Scene)                                 │   │
│  │      │     ├── Camera                                    │   │
│  │      │     ├── Mesh                                      │   │
│  │      │     ├── Light                                     │   │
│  │      │     └── Billboard → Layer2D                       │   │
│  │      │                                                   │   │
│  │      └── LayerComposite                                  │   │
│  │            ├── Layer2D                                   │   │
│  │            └── Layer3D                                   │   │
│  │                                                          │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                  │
│                              ▼                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    Frame Graph                           │   │
│  │                                                          │   │
│  │   Pass Dependencies → Render Target Allocation →         │   │
│  │   Command Encoding → GPU Submission                      │   │
│  │                                                          │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                  │
│                              ▼                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │                    Render Backends                       │   │
│  │                                                          │   │
│  │   ┌──────────┐  ┌──────────┐  ┌──────────┐             │   │
│  │   │   SDF    │  │ Canvas2D │  │ Scene3D  │             │   │
│  │   │ Renderer │  │ Renderer │  │ Renderer │             │   │
│  │   └──────────┘  └──────────┘  └──────────┘             │   │
│  │                                                          │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                  │
│                              ▼                                  │
│                         Swapchain                               │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Layer Model

### Layer Enum

All visual content is represented as a `Layer`:

```rust
enum Layer {
    // ─────────────────────────────────────────────────────────
    // 2D Primitives (SDF Rendered)
    // ─────────────────────────────────────────────────────────
    Ui(UiNode),
    
    // ─────────────────────────────────────────────────────────
    // 2D Vector Drawing
    // ─────────────────────────────────────────────────────────
    Canvas2D {
        size: Size,
        commands: Canvas2DCommands,
        cache_policy: CachePolicy,
    },
    
    // ─────────────────────────────────────────────────────────
    // 3D Scene
    // ─────────────────────────────────────────────────────────
    Scene3D {
        viewport: Rect,
        scene: Scene3DCommands,
        camera: Camera,
        environment: Option<Environment>,
    },
    
    // ─────────────────────────────────────────────────────────
    // Composition
    // ─────────────────────────────────────────────────────────
    Stack {
        layers: Vec<Layer>,
        blend_mode: BlendMode,
    },
    
    Transform2D {
        transform: Affine2D,
        layer: Box<Layer>,
    },
    
    Transform3D {
        transform: Mat4,
        layer: Box<Layer>,
    },
    
    Clip {
        shape: ClipShape,
        layer: Box<Layer>,
    },
    
    Opacity {
        value: f32,
        layer: Box<Layer>,
    },
    
    // ─────────────────────────────────────────────────────────
    // Render Target Indirection
    // ─────────────────────────────────────────────────────────
    Offscreen {
        size: Size,
        format: TextureFormat,
        layer: Box<Layer>,
        effects: Vec<PostEffect>,
    },
    
    // ─────────────────────────────────────────────────────────
    // Dimension Bridging
    // ─────────────────────────────────────────────────────────
    
    /// 2D layer placed in 3D space
    Billboard {
        layer: Box<Layer>,
        transform: Mat4,
        facing: BillboardFacing,
    },
    
    /// 3D scene embedded in 2D layout
    Viewport3D {
        rect: Rect,
        scene: Box<Layer>,  // Must be Scene3D
    },
    
    /// Reference to another layer's render output
    Portal {
        source: LayerId,
        sample_rect: Rect,
        dest_rect: Rect,
    },
}
```

### Layer Properties

```rust
struct LayerProperties {
    /// Unique identifier for referencing
    id: Option<LayerId>,
    
    /// Layout participation
    layout: LayoutConfig,
    
    /// Visibility (skips render entirely when false)
    visible: bool,
    
    /// Pointer event behavior
    pointer_events: PointerEvents,
    
    /// Render order hint (within same Z-level)
    order: i32,
}

enum PointerEvents {
    Auto,           // Normal hit testing
    None,           // Transparent to input
    PassThrough,    // Receive events but don't block
}

enum BillboardFacing {
    Camera,         // Always faces camera
    CameraY,        // Faces camera but stays upright
    Fixed,          // Uses transform rotation
}
```

---

## Draw Context

A unified interface that adapts to the current layer type:

```rust
trait DrawContext {
    // ─────────────────────────────────────────────────────────
    // Transform Stack (available everywhere)
    // ─────────────────────────────────────────────────────────
    
    fn push_transform(&mut self, transform: Transform);
    fn pop_transform(&mut self);
    fn current_transform(&self) -> Transform;
    
    // ─────────────────────────────────────────────────────────
    // State Stack
    // ─────────────────────────────────────────────────────────
    
    fn push_clip(&mut self, shape: ClipShape);
    fn pop_clip(&mut self);
    
    fn push_opacity(&mut self, opacity: f32);
    fn pop_opacity(&mut self);
    
    fn push_blend_mode(&mut self, mode: BlendMode);
    fn pop_blend_mode(&mut self);
    
    // ─────────────────────────────────────────────────────────
    // 2D Drawing Operations
    // ─────────────────────────────────────────────────────────
    
    /// Fill a path with a brush
    fn fill(&mut self, path: impl Into<Path>, brush: impl Into<Brush>);
    
    /// Stroke a path
    fn stroke(&mut self, path: impl Into<Path>, stroke: Stroke, brush: impl Into<Brush>);
    
    /// Draw text
    fn draw_text(&mut self, text: &TextLayout, origin: Point);
    
    /// Draw an image
    fn draw_image(&mut self, image: &Image, rect: Rect, options: ImageOptions);
    
    /// SDF shape composition (optimized path for UI primitives)
    fn sdf<F: FnOnce(&mut SdfBuilder)>(&mut self, f: F);
    
    // ─────────────────────────────────────────────────────────
    // 3D Drawing Operations
    // ─────────────────────────────────────────────────────────
    
    /// Set the camera for 3D rendering
    fn set_camera(&mut self, camera: Camera);
    
    /// Draw a mesh with material
    fn draw_mesh(
        &mut self,
        mesh: &Mesh,
        material: &Material,
        transform: Mat4,
    );
    
    /// Draw instanced meshes
    fn draw_mesh_instanced(
        &mut self,
        mesh: &Mesh,
        material: &Material,
        instances: &[Mat4],
    );
    
    /// Add a light to the scene
    fn add_light(&mut self, light: Light);
    
    /// Set environment (skybox, IBL)
    fn set_environment(&mut self, env: Environment);
    
    // ─────────────────────────────────────────────────────────
    // Dimension Bridging
    // ─────────────────────────────────────────────────────────
    
    /// Embed 2D content in current 3D context
    fn billboard<F>(&mut self, size: Size, transform: Mat4, facing: BillboardFacing, f: F)
    where
        F: FnOnce(&mut dyn DrawContext);
    
    /// Embed 3D viewport in current 2D context
    fn viewport_3d<F>(&mut self, rect: Rect, camera: Camera, f: F)
    where
        F: FnOnce(&mut dyn DrawContext);
    
    // ─────────────────────────────────────────────────────────
    // Layer Management
    // ─────────────────────────────────────────────────────────
    
    /// Begin an offscreen layer with optional effects
    fn push_layer(&mut self, config: LayerConfig);
    fn pop_layer(&mut self);
    
    /// Access the output of a named layer
    fn sample_layer(&mut self, id: LayerId, rect: Rect) -> Image;
}
```

### SDF Builder (Optimized 2D Primitives)

```rust
trait SdfBuilder {
    // ─────────────────────────────────────────────────────────
    // Primitives
    // ─────────────────────────────────────────────────────────
    
    fn rect(&mut self, rect: Rect, corner_radius: CornerRadius) -> ShapeId;
    fn circle(&mut self, center: Point, radius: f32) -> ShapeId;
    fn ellipse(&mut self, center: Point, radii: Vec2) -> ShapeId;
    fn line(&mut self, from: Point, to: Point, width: f32) -> ShapeId;
    fn arc(&mut self, center: Point, radius: f32, start: f32, end: f32, width: f32) -> ShapeId;
    
    // Quadratic Bézier (has closed-form SDF)
    fn quad_bezier(&mut self, p0: Point, p1: Point, p2: Point, width: f32) -> ShapeId;
    
    // ─────────────────────────────────────────────────────────
    // Boolean Operations
    // ─────────────────────────────────────────────────────────
    
    fn union(&mut self, a: ShapeId, b: ShapeId) -> ShapeId;
    fn subtract(&mut self, a: ShapeId, b: ShapeId) -> ShapeId;
    fn intersect(&mut self, a: ShapeId, b: ShapeId) -> ShapeId;
    
    fn smooth_union(&mut self, a: ShapeId, b: ShapeId, radius: f32) -> ShapeId;
    fn smooth_subtract(&mut self, a: ShapeId, b: ShapeId, radius: f32) -> ShapeId;
    fn smooth_intersect(&mut self, a: ShapeId, b: ShapeId, radius: f32) -> ShapeId;
    
    // ─────────────────────────────────────────────────────────
    // Modifiers
    // ─────────────────────────────────────────────────────────
    
    fn round(&mut self, shape: ShapeId, radius: f32) -> ShapeId;
    fn outline(&mut self, shape: ShapeId, width: f32) -> ShapeId;
    fn offset(&mut self, shape: ShapeId, distance: f32) -> ShapeId;
    
    // ─────────────────────────────────────────────────────────
    // Rendering
    // ─────────────────────────────────────────────────────────
    
    fn fill(&mut self, shape: ShapeId, brush: impl Into<Brush>);
    fn stroke(&mut self, shape: ShapeId, stroke: Stroke, brush: impl Into<Brush>);
    fn shadow(&mut self, shape: ShapeId, shadow: Shadow);
}
```

---

## DSL Syntax

### Basic Structure

```
@component ComponentName {
    // State declarations
    @state name: Type = initial_value
    
    // Computed properties
    @computed derived: Type = expression
    
    // Lifecycle hooks
    @on_mount { ... }
    @on_unmount { ... }
    
    // Render declaration
    @render {
        // Layer tree
    }
}
```

### Complete Example

```
@component ProductViewer {
    @state rotation: Vec3 = Vec3::ZERO
    @state selected_part: Option<PartId> = None
    @state show_annotations: bool = true
    @state zoom: f32 = 1.0
    
    @computed camera_position: Vec3 = {
        let distance = 5.0 / zoom;
        Vec3(
            distance * rotation.y.cos() * rotation.x.cos(),
            distance * rotation.x.sin(),
            distance * rotation.y.sin() * rotation.x.cos()
        )
    }
    
    @render {
        ZStack {
            // ─────────────────────────────────────────────────
            // Layer 0: 3D Product Scene
            // ─────────────────────────────────────────────────
            Scene3D(id: "product_scene") {
                Environment(
                    hdri: asset!("studio.hdr"),
                    intensity: 0.8,
                    blur: 0.3
                )
                
                Camera(
                    kind: .perspective(fov: 45.deg),
                    position: camera_position,
                    target: Vec3::ZERO
                )
                
                // Main product model
                Model(source: asset!("product.gltf")) { node |
                    on_click: (hit) => {
                        selected_part.set(Some(hit.node_id))
                    }
                    
                    on_hover: (hit) => {
                        // Highlight on hover
                    }
                }
                
                // Lighting
                DirectionalLight(
                    direction: Vec3(-1, -1, -1).normalize(),
                    color: Color::WHITE,
                    intensity: 1.0,
                    cast_shadows: true
                )
                
                AmbientLight(color: Color::WHITE, intensity: 0.2)
                
                // 3D Annotations as billboards
                if show_annotations.get() {
                    for annotation in product.annotations {
                        Billboard(
                            position: annotation.world_pos,
                            facing: .camera
                        ) {
                            AnnotationBadge(
                                label: annotation.label,
                                expanded: selected_part.get() == Some(annotation.part_id)
                            )
                        }
                    }
                }
            }
            
            // ─────────────────────────────────────────────────
            // Layer 1: 2D Canvas Overlay (guides, measurements)
            // ─────────────────────────────────────────────────
            Canvas(full_size: true, pointer_events: .pass_through) { paint |
                if show_annotations.get() {
                    // Draw measurement lines
                    for measurement in product.measurements {
                        let start = project_to_screen(measurement.start)
                        let end = project_to_screen(measurement.end)
                        
                        paint.stroke(
                            Path::line(start, end),
                            Stroke(width: 1.px, dash: [4, 2]),
                            Color::ACCENT
                        )
                        
                        let midpoint = (start + end) / 2.0
                        paint.draw_text(
                            "{measurement.value} mm",
                            midpoint,
                            TextStyle(size: 12.px, color: Color::WHITE)
                        )
                    }
                }
            }
            
            // ─────────────────────────────────────────────────
            // Layer 2: UI Controls
            // ─────────────────────────────────────────────────
            
            // Top-left: Part info panel
            Align(to: .top_leading, padding: 16.px) {
                if let Some(part_id) = selected_part.get() {
                    let part = product.get_part(part_id)
                    
                    Panel(style: .floating) {
                        VStack(spacing: 8.px) {
                            Text(content: part.name, style: .headline)
                            Text(content: part.description, style: .body)
                            
                            Divider()
                            
                            HStack(spacing: 12.px) {
                                Text(content: "Material:")
                                Text(content: part.material, style: .bold)
                            }
                            
                            HStack(spacing: 12.px) {
                                Text(content: "Weight:")
                                Text(content: "{part.weight}g", style: .bold)
                            }
                        }
                    }
                }
            }
            
            // Bottom: Control bar
            Align(to: .bottom, padding: 16.px) {
                Panel(style: .floating) {
                    HStack(spacing: 16.px, align: .center) {
                        // Zoom slider
                        HStack(spacing: 8.px) {
                            Icon(name: "zoom_out")
                            Slider(
                                value: zoom,
                                range: 0.5..3.0,
                                width: 120.px
                            )
                            Icon(name: "zoom_in")
                        }
                        
                        Divider(orientation: .vertical, height: 24.px)
                        
                        // Toggle annotations
                        Toggle(value: show_annotations) {
                            Icon(name: "annotations")
                        }
                        
                        Divider(orientation: .vertical, height: 24.px)
                        
                        // Reset view
                        Button(style: .icon) {
                            Icon(name: "reset_view")
                            
                            on_click: () => {
                                rotation.set(Vec3::ZERO)
                                zoom.set(1.0)
                                selected_part.set(None)
                            }
                        }
                    }
                }
            }
            
            // Thumbnail in corner
            Align(to: .bottom_trailing, padding: 16.px) {
                Container(
                    width: 80.px,
                    height: 80.px,
                    corner_radius: 8.px,
                    border: Border(width: 1.px, color: Color::BORDER),
                    overflow: .hidden
                ) {
                    // Mini 3D view (orthographic)
                    Scene3D {
                        Camera(
                            kind: .orthographic(scale: 2.0),
                            position: Vec3(0, 10, 0),
                            target: Vec3::ZERO
                        )
                        
                        Model(source: asset!("product.gltf"))
                        
                        // Show current view frustum
                        Canvas3D { paint |
                            paint.stroke(
                                view_frustum_path(),
                                Stroke(width: 2.px),
                                Color::ACCENT
                            )
                        }
                    }
                }
            }
        }
    }
    
    // ─────────────────────────────────────────────────────────
    // Gesture Handling
    // ─────────────────────────────────────────────────────────
    
    @gesture(on: "product_scene") {
        Drag { delta |
            rotation.update(|r| {
                r.y += delta.x * 0.01
                r.x = (r.x + delta.y * 0.01).clamp(-PI/2, PI/2)
            })
        }
        
        Pinch { scale |
            zoom.update(|z| (z * scale).clamp(0.5, 3.0))
        }
        
        Scroll { delta |
            zoom.update(|z| (z - delta.y * 0.001).clamp(0.5, 3.0))
        }
    }
}
```

### Reusable Components

```
@component AnnotationBadge {
    @prop label: String
    @prop expanded: bool = false
    
    @render {
        Container(
            padding: @if expanded { 12.px } else { 6.px },
            background: Color::BLACK.opacity(0.8),
            corner_radius: @if expanded { 8.px } else { 12.px },
            shadow: Shadow::sm()
        ) {
            @if expanded {
                VStack(spacing: 4.px) {
                    Text(content: label, style: .caption, color: Color::WHITE)
                    // Extended content...
                }
            } else {
                Circle(size: 8.px, fill: Color::ACCENT)
            }
        }
        
        @animate(expanded, spring: .bouncy)
    }
}

@component Panel {
    @prop style: PanelStyle = .default
    @children content
    
    @render {
        Container(
            padding: 16.px,
            background: @match style {
                .default => Color::SURFACE,
                .floating => Color::SURFACE.opacity(0.95),
                .solid => Color::SURFACE_SOLID,
            },
            corner_radius: 12.px,
            shadow: @match style {
                .floating => Shadow::lg(),
                _ => Shadow::none(),
            },
            backdrop_blur: @match style {
                .floating => 20.px,
                _ => 0.px,
            }
        ) {
            @content
        }
    }
}
```

---

## Rendering Pipeline

### Frame Graph

The frame graph automatically schedules render passes based on dependencies:

```rust
struct FrameGraph {
    passes: Vec<RenderPass>,
    targets: Vec<RenderTarget>,
    dependencies: DependencyGraph,
}

enum RenderPass {
    // 3D Passes
    ShadowMap { light_index: usize, cascade: usize },
    GBuffer { scene: LayerId },
    Lighting { scene: LayerId },
    Transparent3D { scene: LayerId },
    
    // 2D Passes
    Canvas2D { layer: LayerId },
    SdfPrimitives { batch: BatchId },
    Text { batch: BatchId },
    
    // Composition
    Composite { layers: Vec<LayerId>, blend: BlendMode },
    PostProcess { effect: PostEffect, input: TargetId },
    
    // Final
    Present { source: TargetId },
}
```

### Pass Execution Order

```
┌─────────────────────────────────────────────────────────────────┐
│                     Per-Frame Execution                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  1. Scene Graph Traversal                                       │
│     └─ Collect visible layers, compute transforms               │
│                                                                 │
│  2. Frustum Culling (3D layers)                                 │
│     └─ Build per-layer visible object sets                      │
│                                                                 │
│  3. Render Target Allocation                                    │
│     └─ Allocate/reuse targets for offscreen layers              │
│                                                                 │
│  4. Command Encoding                                            │
│     │                                                           │
│     ├─ Shadow Passes (parallel per light)                       │
│     │   └─ Render depth for shadow-casting meshes               │
│     │                                                           │
│     ├─ 3D Opaque Pass                                           │
│     │   └─ Depth pre-pass → GBuffer → Lighting                  │
│     │                                                           │
│     ├─ 3D Transparent Pass                                      │
│     │   └─ Sorted back-to-front, blended                        │
│     │                                                           │
│     ├─ Canvas2D Passes                                          │
│     │   └─ Tessellated paths, SDF curves                        │
│     │                                                           │
│     ├─ SDF UI Pass                                              │
│     │   └─ Batched primitives: shadows → fills → borders        │
│     │                                                           │
│     ├─ Text Pass                                                │
│     │   └─ Glyph atlas sampling, subpixel positioning           │
│     │                                                           │
│     └─ Composite Pass                                           │
│         └─ Layer blending, post effects                         │
│                                                                 │
│  5. GPU Submission                                              │
│     └─ Submit command buffer, present swapchain                 │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Renderer Backends

```rust
struct BlincRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    
    // Specialized renderers
    sdf: SdfRenderer,
    canvas2d: Canvas2DRenderer,
    scene3d: Scene3DRenderer,
    text: TextRenderer,
    compositor: Compositor,
    
    // Shared resources
    texture_cache: TextureCache,
    mesh_cache: MeshCache,
    material_cache: MaterialCache,
    glyph_atlas: GlyphAtlas,
}

impl SdfRenderer {
    /// Renders UI primitives using signed distance fields
    /// 
    /// Primitive types:
    /// - Rectangles with corner radii
    /// - Circles, ellipses
    /// - Shadows (Gaussian blur via error function)
    /// - Quadratic Bézier curves
    /// - Boolean combinations
    
    fn render(&mut self, primitives: &[SdfPrimitive], target: &wgpu::TextureView);
}

impl Canvas2DRenderer {
    /// Renders arbitrary vector paths
    /// 
    /// Strategies (selected per-path):
    /// - Tessellation via Lyon (arbitrary paths)
    /// - Analytical SDF (quadratic Béziers)
    /// - MSDF lookup (cached complex paths)
    
    fn render(&mut self, commands: &Canvas2DCommands, target: &wgpu::TextureView);
}

impl Scene3DRenderer {
    /// Renders 3D scenes with PBR materials
    /// 
    /// Features:
    /// - Deferred or forward rendering (configurable)
    /// - Cascaded shadow maps
    /// - Image-based lighting
    /// - Instanced rendering
    
    fn render(&mut self, scene: &Scene3D, camera: &Camera, target: &RenderTargets);
}
```

---

## Input System

### Unified Hit Testing

```rust
struct InputManager {
    /// Process a pointer event through the layer stack
    fn hit_test(&self, screen_pos: Point, event_type: EventType) -> HitResult;
}

enum HitResult {
    // 2D UI hit
    Ui {
        node_id: NodeId,
        local_pos: Point,
        layer_id: LayerId,
    },
    
    // Canvas2D hit
    Canvas2D {
        layer_id: LayerId,
        local_pos: Point,
    },
    
    // 3D mesh hit
    Mesh {
        node_id: NodeId,
        mesh_id: MeshId,
        world_pos: Vec3,
        normal: Vec3,
        uv: Option<Vec2>,
        distance: f32,
        layer_id: LayerId,
    },
    
    // Billboard hit (2D content in 3D space)
    Billboard {
        billboard_id: NodeId,
        local_pos: Point,     // Position in billboard's 2D space
        world_pos: Vec3,      // Position in 3D
        layer_id: LayerId,
    },
    
    // No hit
    None,
}
```

### Hit Test Pipeline

```
┌─────────────────────────────────────────────────────────────────┐
│                      Hit Test Pipeline                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Screen Position                                                │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────────────┐                                           │
│  │ 1. UI Layer     │  Test front-to-back                       │
│  │    Hit Test     │  Point-in-rect with transforms            │
│  └────────┬────────┘                                           │
│           │ miss                                                │
│           ▼                                                     │
│  ┌─────────────────┐                                           │
│  │ 2. Canvas2D     │  Test painted regions                     │
│  │    Hit Test     │  Path containment or bounds               │
│  └────────┬────────┘                                           │
│           │ miss                                                │
│           ▼                                                     │
│  ┌─────────────────┐                                           │
│  │ 3. Billboard    │  Project billboards to screen             │
│  │    Hit Test     │  Test as 2D rects in screen space         │
│  └────────┬────────┘                                           │
│           │ miss                                                │
│           ▼                                                     │
│  ┌─────────────────┐                                           │
│  │ 4. 3D Ray Cast  │  Screen → Ray via camera                  │
│  │                 │  Intersect with scene BVH                 │
│  └────────┬────────┘                                           │
│           │                                                     │
│           ▼                                                     │
│      HitResult                                                  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Gesture Recognition

```rust
enum Gesture {
    // Pointer gestures
    Tap { position: Point, count: u32 },
    LongPress { position: Point, duration: Duration },
    Drag { start: Point, current: Point, delta: Vec2, velocity: Vec2 },
    
    // Multi-touch
    Pinch { center: Point, scale: f32, rotation: f32 },
    Pan { translation: Vec2, velocity: Vec2 },
    
    // 3D-specific
    Orbit { delta: Vec2, pivot: Vec3 },
    Dolly { delta: f32 },
    
    // Keyboard
    KeyPress { key: Key, modifiers: Modifiers },
}

trait GestureHandler {
    fn on_gesture(&mut self, gesture: Gesture, hit: HitResult) -> Handled;
}
```

---

## AOT Compilation

### Static Analysis Passes

```
┌─────────────────────────────────────────────────────────────────┐
│                    Compilation Pipeline                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  DSL Source                                                     │
│       │                                                         │
│       ▼                                                         │
│  ┌─────────────────┐                                           │
│  │ 1. Parse        │  Build AST                                │
│  └────────┬────────┘                                           │
│           │                                                     │
│           ▼                                                     │
│  ┌─────────────────┐                                           │
│  │ 2. Type Check   │  Validate props, state, expressions       │
│  └────────┬────────┘                                           │
│           │                                                     │
│           ▼                                                     │
│  ┌─────────────────┐                                           │
│  │ 3. Layer        │  Determine static layer topology          │
│  │    Analysis     │  Count render targets needed              │
│  │                 │  Identify 3D vs 2D-only components        │
│  └────────┬────────┘                                           │
│           │                                                     │
│           ▼                                                     │
│  ┌─────────────────┐                                           │
│  │ 4. Dependency   │  Track state → layer dependencies         │
│  │    Analysis     │  Build minimal update graph               │
│  └────────┬────────┘                                           │
│           │                                                     │
│           ▼                                                     │
│  ┌─────────────────┐                                           │
│  │ 5. Resource     │  Enumerate textures, meshes, fonts        │
│  │    Collection   │  Generate asset manifest                  │
│  └────────┬────────┘                                           │
│           │                                                     │
│           ▼                                                     │
│  ┌─────────────────┐                                           │
│  │ 6. Shader       │  Select required shader variants          │
│  │    Selection    │  Compile specialized pipelines            │
│  └────────┬────────┘                                           │
│           │                                                     │
│           ▼                                                     │
│  ┌─────────────────┐                                           │
│  │ 7. Code Gen     │  Emit Rust render functions               │
│  │                 │  Generate update dispatchers              │
│  └────────┬────────┘                                           │
│           │                                                     │
│           ▼                                                     │
│  Optimized Rust Code + Asset Bundle                            │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Optimization Opportunities

```rust
struct CompileTimeOptimizations {
    /// Static layer count → pre-allocate render targets
    render_target_slots: usize,
    
    /// No 3D layers detected → exclude 3D renderer
    exclude_3d: bool,
    
    /// No Canvas2D → exclude tessellation
    exclude_canvas2d: bool,
    
    /// Static canvas content → pre-tessellate paths
    pre_tessellated_paths: Vec<TessellatedPath>,
    
    /// Static MSDF content → pre-generate atlases
    msdf_atlases: Vec<MsdfAtlas>,
    
    /// State dependency map → minimal update sets
    update_graph: UpdateGraph,
    
    /// Shader variants needed
    shader_variants: ShaderVariantSet,
}

struct UpdateGraph {
    /// Maps state variables to affected layers
    state_to_layers: HashMap<StateId, Vec<LayerId>>,
    
    /// Which layers need full re-render vs transform-only update
    layer_update_modes: HashMap<LayerId, UpdateMode>,
}

enum UpdateMode {
    /// Only transform changed, reuse cached content
    TransformOnly,
    
    /// Content changed, re-render layer
    ContentChanged,
    
    /// Layer visibility toggled
    VisibilityChanged,
}
```

---

## Module Structure

```
blinc/
├── Cargo.toml
│
├── core/
│   ├── lib.rs
│   ├── layer.rs              # Layer enum, properties
│   ├── scene_graph.rs        # Tree structure, traversal
│   ├── transform.rs          # 2D/3D transform stack
│   ├── color.rs              # Color, gradients, brushes
│   └── geometry.rs           # Point, Rect, Vec2, Vec3, Mat4
│
├── render/
│   ├── lib.rs
│   ├── context.rs            # DrawContext trait
│   ├── frame_graph.rs        # Pass scheduling
│   │
│   ├── sdf/
│   │   ├── mod.rs
│   │   ├── primitives.rs     # SdfPrimitive enum
│   │   ├── builder.rs        # SdfBuilder implementation
│   │   ├── shader.wgsl       # SDF fragment shader
│   │   └── renderer.rs       # SdfRenderer
│   │
│   ├── canvas2d/
│   │   ├── mod.rs
│   │   ├── path.rs           # Path builder
│   │   ├── commands.rs       # Canvas2DCommands
│   │   ├── tessellation.rs   # Lyon integration
│   │   ├── sdf_curves.rs     # Analytical Bézier SDF
│   │   ├── msdf.rs           # Multi-channel SDF cache
│   │   └── renderer.rs       # Canvas2DRenderer
│   │
│   ├── scene3d/
│   │   ├── mod.rs
│   │   ├── mesh.rs           # Mesh, vertex formats
│   │   ├── material.rs       # PBR material system
│   │   ├── camera.rs         # Perspective, orthographic
│   │   ├── lighting.rs       # Directional, point, IBL
│   │   ├── shadow.rs         # Shadow mapping
│   │   ├── environment.rs    # Skybox, HDRI
│   │   └── renderer.rs       # Scene3DRenderer
│   │
│   ├── text/
│   │   ├── mod.rs
│   │   ├── shaping.rs        # Platform text shaping
│   │   ├── layout.rs         # Text layout engine
│   │   ├── atlas.rs          # Glyph atlas management
│   │   └── renderer.rs       # TextRenderer
│   │
│   ├── compositor/
│   │   ├── mod.rs
│   │   ├── blending.rs       # Blend modes
│   │   ├── effects.rs        # Post-processing effects
│   │   └── renderer.rs       # Final composition
│   │
│   └── resources/
│       ├── mod.rs
│       ├── texture_cache.rs
│       ├── mesh_cache.rs
│       └── material_cache.rs
│
├── input/
│   ├── lib.rs
│   ├── hit_test.rs           # Unified hit testing
│   ├── gestures.rs           # Gesture recognition
│   ├── ray_cast.rs           # 3D ray casting
│   └── focus.rs              # Focus management
│
├── ui/
│   ├── lib.rs
│   ├── node.rs               # UiNode enum
│   ├── layout/
│   │   ├── mod.rs
│   │   ├── flex.rs           # Flexbox via Taffy
│   │   └── constraints.rs    # Size constraints
│   │
│   ├── primitives/
│   │   ├── mod.rs
│   │   ├── container.rs
│   │   ├── text.rs
│   │   ├── image.rs
│   │   ├── button.rs
│   │   ├── slider.rs
│   │   ├── toggle.rs
│   │   └── scroll.rs
│   │
│   └── styling/
│       ├── mod.rs
│       ├── theme.rs
│       └── tokens.rs
│
├── widgets/
│   ├── lib.rs
│   ├── canvas.rs             # Canvas { paint | ... }
│   ├── scene3d.rs            # Scene3D { ctx | ... }
│   ├── model3d.rs            # Inline 3D model
│   ├── billboard.rs          # 2D UI in 3D space
│   └── viewport.rs           # 3D embedded in 2D
│
├── animation/
│   ├── lib.rs
│   ├── spring.rs             # Spring physics
│   ├── tween.rs              # Easing functions
│   ├── keyframe.rs           # Keyframe animation
│   └── driver.rs             # Animation scheduler
│
├── compiler/
│   ├── lib.rs
│   ├── lexer.rs
│   ├── parser.rs
│   ├── ast.rs
│   ├── type_check.rs
│   ├── analysis/
│   │   ├── mod.rs
│   │   ├── layers.rs         # Layer topology analysis
│   │   ├── dependencies.rs   # State → layer deps
│   │   └── resources.rs      # Asset collection
│   │
│   ├── codegen/
│   │   ├── mod.rs
│   │   ├── rust.rs           # Rust code emission
│   │   └── shaders.rs        # Shader variant selection
│   │
│   └── bundle.rs             # Asset bundling
│
└── runtime/
    ├── lib.rs
    ├── app.rs                # Application lifecycle
    ├── window.rs             # Window management
    ├── event_loop.rs         # Platform event loop
    └── platform/
        ├── mod.rs
        ├── macos.rs
        ├── windows.rs
        ├── linux.rs
        └── web.rs
```

---

## Implementation Phases

| Phase | Focus | Deliverables | Unlocks |
|-------|-------|--------------|---------|
| **1** | SDF UI | Rect, circle, shadow, text; Flexbox layout | Forms, dashboards, settings |
| **2** | Canvas2D | Path tessellation, SDF curves, MSDF cache | Charts, diagrams, custom drawing |
| **3** | Scene3D | PBR materials, directional light, shadows | Product viewers, 3D previews |
| **4** | Composition | Layer blending, post effects, portals | Polished visual effects |
| **5** | Bridging | Billboard, Viewport3D, unified input | Game UI, spatial apps, CAD |
| **6** | Animation | Spring, tween, keyframe systems | Fluid interactions |
| **7** | Compiler | Full AOT pipeline, optimizations | Production performance |
| **8** | Polish | Platform integration, accessibility | Production readiness |

---

## Appendix: Shader Reference

### SDF Primitives (WGSL)

```wgsl
// Core SDF functions

fn sd_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> f32 {
    let half_size = size * 0.5;
    let center = origin + half_size;
    let q = abs(p - center) - half_size;
    
    // Select corner radius based on quadrant
    let r = select(
        select(radius.z, radius.w, q.x > 0.0),
        select(radius.x, radius.y, q.x > 0.0),
        q.y > 0.0
    );
    
    return length(max(q - vec2(r), vec2(0.0))) + min(max(q.x - r, q.y - r), 0.0);
}

fn sd_circle(p: vec2<f32>, center: vec2<f32>, radius: f32) -> f32 {
    return length(p - center) - radius;
}

fn sd_quad_bezier(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, c: vec2<f32>) -> f32 {
    let ba = b - a;
    let ca = c - a;
    let pa = p - a;
    
    let h = dot(ba, ba) - 2.0 * dot(ba, ca);
    let t = clamp((dot(ba, pa) - dot(ca, pa)) / h, 0.0, 1.0);
    
    let q = mix(mix(a, b, t), mix(b, c, t), t);
    return length(p - q);
}

// Boolean operations

fn op_union(d1: f32, d2: f32) -> f32 {
    return min(d1, d2);
}

fn op_subtract(d1: f32, d2: f32) -> f32 {
    return max(d1, -d2);
}

fn op_intersect(d1: f32, d2: f32) -> f32 {
    return max(d1, d2);
}

fn op_smooth_union(d1: f32, d2: f32, k: f32) -> f32 {
    let h = clamp(0.5 + 0.5 * (d2 - d1) / k, 0.0, 1.0);
    return mix(d2, d1, h) - k * h * (1.0 - h);
}

// Shadow (Gaussian blur via erf approximation)

fn erf(x: f32) -> f32 {
    let s = sign(x);
    let a = abs(x);
    let t = 1.0 / (1.0 + 0.3275911 * a);
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-a * a);
    return s * y;
}

fn shadow_rect(p: vec2<f32>, origin: vec2<f32>, size: vec2<f32>, sigma: f32) -> f32 {
    let d = 0.5 * sqrt(2.0) * sigma;
    let half = size * 0.5;
    let center = origin + half;
    let rel = p - center;
    
    let x = 0.5 * (erf((half.x - rel.x) / d) + erf((half.x + rel.x) / d));
    let y = 0.5 * (erf((half.y - rel.y) / d) + erf((half.y + rel.y) / d));
    
    return x * y;
}
```

---

*BLINC Canvas Architecture v0.1*
*For the Zyntax UI Framework*