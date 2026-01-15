# Fuchsia Integration Gaps - Completion Plan

## Overview

Complete the remaining integration gaps to make Blinc fully functional on Fuchsia OS.

## Current State

| Component | Status | Location |
|-----------|--------|----------|
| Platform traits | ✅ Complete | `extensions/blinc_platform_fuchsia/src/` |
| Flatland session | ✅ Stubs | `flatland.rs` |
| ImagePipe2 client | ✅ Stubs | `gpu.rs` |
| Input types | ✅ Complete | `input.rs` |
| Event loop | ✅ Structure | `event_loop.rs` |
| FuchsiaApp runner | ✅ Structure | `crates/blinc_app/src/fuchsia.rs` |

---

## Gap 1: ViewProvider Component

**Priority:** Critical
**Effort:** Medium

Fuchsia GUI apps must implement `fuchsia.ui.app.ViewProvider` to receive a View from the system.

### Files to Create/Modify

- `extensions/blinc_platform_fuchsia/src/view_provider.rs` (new)
- `extensions/blinc_platform_fuchsia/src/lib.rs` (add module)

### Implementation

```rust
// view_provider.rs

/// ViewProvider service implementation
///
/// Fuchsia calls CreateView2 when launching the app, providing tokens
/// to create our View in the compositor.
pub struct ViewProvider {
    /// Flatland session for this view
    flatland: FlatlandSession,
    /// View creation token from system
    view_creation_token: Option<ViewCreationToken>,
    /// Parent viewport watcher
    parent_viewport_watcher: Option<ParentViewportWatcherProxy>,
}

impl ViewProvider {
    /// Create a new ViewProvider
    pub fn new() -> Self { ... }

    /// Handle CreateView2 request from system
    ///
    /// Called by Fuchsia when app is launched. Provides:
    /// - ViewCreationToken: Create our View with Flatland
    /// - ViewRefFocused: Watch for focus changes
    /// - ViewRef: Our view's identity
    #[cfg(target_os = "fuchsia")]
    pub async fn create_view2(
        &mut self,
        args: CreateView2Args,
    ) -> Result<(), ViewProviderError> {
        // 1. Store view creation token
        self.view_creation_token = Some(args.view_creation_token);

        // 2. Create View with Flatland using the token
        // flatland.create_view(token, parent_viewport_watcher)?;

        // 3. Watch parent viewport for layout info
        // Spawns async task to receive ViewProperties updates

        Ok(())
    }
}

/// Serve ViewProvider protocol
#[cfg(target_os = "fuchsia")]
pub async fn serve_view_provider(
    stream: ViewProviderRequestStream,
    sender: mpsc::Sender<ViewEvent>,
) -> Result<(), Error> {
    while let Some(request) = stream.try_next().await? {
        match request {
            ViewProviderRequest::CreateView2 { args, .. } => {
                sender.send(ViewEvent::CreateView(args)).await?;
            }
        }
    }
    Ok(())
}
```

### Verification

- [ ] ViewProvider compiles with FIDL bindings
- [ ] App launches and receives CreateView2
- [ ] View appears in Scenic compositor

---

## Gap 2: ParentViewportWatcher Integration

**Priority:** Critical
**Effort:** Medium

Receive layout size and device pixel ratio from the parent.

### Files to Modify

- `extensions/blinc_platform_fuchsia/src/view_provider.rs`
- `crates/blinc_app/src/fuchsia.rs`

### Implementation

```rust
// In view_provider.rs

/// Watch parent viewport for layout changes
#[cfg(target_os = "fuchsia")]
pub async fn watch_parent_viewport(
    watcher: ParentViewportWatcherProxy,
    sender: mpsc::Sender<ViewEvent>,
) {
    loop {
        match watcher.get_layout().await {
            Ok(layout_info) => {
                let logical_size = layout_info.logical_size.unwrap_or_default();
                let device_pixel_ratio = layout_info.device_pixel_ratio.unwrap_or(1.0);

                sender.send(ViewEvent::LayoutChanged {
                    width: logical_size.width,
                    height: logical_size.height,
                    scale_factor: device_pixel_ratio,
                }).await.ok();
            }
            Err(e) => {
                tracing::error!("ParentViewportWatcher error: {}", e);
                break;
            }
        }
    }
}
```

### In FuchsiaApp::run

```rust
// Handle ViewEvent::LayoutChanged
ViewEvent::LayoutChanged { width, height, scale_factor } => {
    ctx.width = width;
    ctx.height = height;
    ctx.scale_factor = scale_factor;

    // Resize ImagePipe buffers
    image_pipe.resize(
        (width * scale_factor) as u32,
        (height * scale_factor) as u32,
    ).await?;

    needs_rebuild = true;
}
```

### Verification

- [ ] App receives initial layout
- [ ] App responds to window resize
- [ ] Scale factor correctly applied

---

## Gap 3: Focus Handling

**Priority:** High
**Effort:** Low

Track focus state for keyboard input.

### Files to Modify

- `extensions/blinc_platform_fuchsia/src/scenic.rs`
- `crates/blinc_app/src/fuchsia.rs`

### Implementation

```rust
// In scenic.rs - already have FocusState, add watcher

#[cfg(target_os = "fuchsia")]
pub async fn watch_focus(
    view_ref_focused: ViewRefFocusedProxy,
    sender: mpsc::Sender<ViewEvent>,
) {
    loop {
        match view_ref_focused.watch().await {
            Ok(state) => {
                let focused = state.focused.unwrap_or(false);
                sender.send(ViewEvent::FocusChanged(focused)).await.ok();
            }
            Err(_) => break,
        }
    }
}
```

### Verification

- [ ] App tracks focus state
- [ ] Keyboard input only processed when focused
- [ ] Visual focus indicators work

---

## Gap 4: TouchSource/MouseSource Integration

**Priority:** Critical
**Effort:** Medium

Receive actual input events from fuchsia.ui.pointer.

### Files to Modify

- `extensions/blinc_platform_fuchsia/src/input.rs`
- `crates/blinc_app/src/fuchsia.rs`

### Implementation

```rust
// In input.rs

#[cfg(target_os = "fuchsia")]
pub async fn watch_touch_source(
    touch_source: TouchSourceProxy,
    sender: mpsc::Sender<InputEvent>,
) {
    let mut responses = Vec::new();

    loop {
        match touch_source.watch(&responses).await {
            Ok(events) => {
                responses.clear();

                for event in events {
                    // Convert FIDL TouchEvent to our TouchInteraction
                    let interaction = TouchInteraction::from_fidl(&event);

                    // Respond with Yes to claim this interaction
                    responses.push(TouchResponse {
                        interaction_id: event.interaction_id,
                        status: TouchResponseType::Yes,
                    });

                    sender.send(InputEvent::Touch(interaction)).await.ok();
                }
            }
            Err(_) => break,
        }
    }
}

#[cfg(target_os = "fuchsia")]
pub async fn watch_mouse_source(
    mouse_source: MouseSourceProxy,
    sender: mpsc::Sender<InputEvent>,
) {
    loop {
        match mouse_source.watch().await {
            Ok(events) => {
                for event in events {
                    let interaction = MouseInteraction::from_fidl(&event);
                    sender.send(InputEvent::Mouse(interaction)).await.ok();
                }
            }
            Err(_) => break,
        }
    }
}
```

### Verification

- [ ] Touch events route to UI
- [ ] Multi-touch works
- [ ] Mouse hover/click works
- [ ] Scroll events work

---

## Gap 5: Keyboard Integration

**Priority:** High
**Effort:** Medium

Receive keyboard input from fuchsia.ui.input3.Keyboard.

### Files to Modify

- `extensions/blinc_platform_fuchsia/src/input.rs`
- `crates/blinc_app/src/fuchsia.rs`

### Implementation

```rust
// In input.rs

#[cfg(target_os = "fuchsia")]
pub async fn serve_keyboard_listener(
    stream: KeyboardListenerRequestStream,
    sender: mpsc::Sender<InputEvent>,
) {
    while let Some(request) = stream.try_next().await.ok().flatten() {
        match request {
            KeyboardListenerRequest::OnKeyEvent { event, responder } => {
                let key_event = KeyboardListenerRequest::from_fidl(&event);
                sender.send(InputEvent::Keyboard(key_event)).await.ok();

                // Tell system we handled it
                let _ = responder.send(KeyEventStatus::Handled);
            }
        }
    }
}
```

### Verification

- [ ] Key presses route to focused element
- [ ] Modifier keys tracked
- [ ] Text input works in text fields
- [ ] Keyboard shortcuts work

---

## Gap 6: GPU Surface Connection

**Priority:** Critical
**Effort:** High

Connect ImagePipe2 buffers to wgpu for actual rendering.

### Files to Modify

- `extensions/blinc_platform_fuchsia/src/gpu.rs`
- `crates/blinc_app/src/fuchsia.rs`

### Implementation

```rust
// In gpu.rs

#[cfg(target_os = "fuchsia")]
impl ImagePipeClient {
    /// Create wgpu surface from ImagePipe
    pub fn create_wgpu_surface(
        &self,
        instance: &wgpu::Instance,
    ) -> Result<wgpu::Surface<'static>, ImagePipeError> {
        // Use VK_FUCHSIA_imagepipe_surface extension
        // This requires raw Vulkan handle access

        // For wgpu, we need to create via raw surface:
        // 1. Get ImagePipe2 token as zx::Channel
        // 2. Create VkSurfaceKHR via vkCreateImagePipeSurfaceFUCHSIA
        // 3. Wrap in wgpu::Surface

        todo!("Requires wgpu Fuchsia surface support")
    }

    /// Get Vulkan image for current buffer
    pub fn get_vulkan_image(&self, buffer_index: u32) -> VulkanImage {
        // Returns VkImage from BufferCollection
        // Used for rendering via blinc_gpu
    }
}
```

### Alternative: Texture-based Rendering

If wgpu doesn't support Fuchsia surfaces directly:

```rust
// Render to texture, then blit to ImagePipe

pub struct OffscreenRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    render_texture: wgpu::Texture,
}

impl OffscreenRenderer {
    pub fn render_frame(&self, render_tree: &RenderTree) -> wgpu::Texture {
        // Render to internal texture
    }

    pub fn copy_to_image_pipe(&self, image_pipe: &ImagePipeClient) {
        // Copy texture data to sysmem buffer
    }
}
```

### Verification

- [ ] wgpu initializes on Fuchsia
- [ ] Rendering produces visible output
- [ ] Frame timing correct (vsync)
- [ ] No visual artifacts

---

## Gap 7: Async Event Loop

**Priority:** Critical
**Effort:** Medium

Replace placeholder sleep loop with proper fuchsia-async integration.

### Files to Modify

- `crates/blinc_app/src/fuchsia.rs`

### Implementation

```rust
#[cfg(target_os = "fuchsia")]
pub fn run<F, E>(mut ui_builder: F) -> Result<()>
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder + 'static,
{
    // Create fuchsia-async executor
    let mut executor = fuchsia_async::LocalExecutor::new();

    executor.run_singlethreaded(async move {
        // Event channels
        let (view_tx, mut view_rx) = mpsc::channel(16);
        let (input_tx, mut input_rx) = mpsc::channel(64);

        // Spawn ViewProvider service
        let view_provider = ViewProvider::new();
        let view_provider_fut = serve_view_provider(/* ... */, view_tx.clone());

        // Main loop with select!
        loop {
            futures::select! {
                // View events (layout, focus)
                view_event = view_rx.next() => {
                    match view_event {
                        Some(ViewEvent::LayoutChanged { .. }) => { /* resize */ }
                        Some(ViewEvent::FocusChanged(focused)) => { /* update focus */ }
                        None => break,
                    }
                }

                // Input events (touch, mouse, keyboard)
                input_event = input_rx.next() => {
                    match input_event {
                        Some(InputEvent::Touch(t)) => handle_touch(&t, ...),
                        Some(InputEvent::Mouse(m)) => handle_mouse(&m, ...),
                        Some(InputEvent::Keyboard(k)) => handle_key(&k, ...),
                        None => break,
                    }
                }

                // Frame scheduling (OnNextFrameBegin)
                frame_info = flatland.on_next_frame_begin() => {
                    if needs_rebuild { rebuild_ui(); }
                    render_frame();
                    flatland.present().await?;
                }

                // Animation wake
                _ = wake_receiver.next() => {
                    needs_rebuild = true;
                }
            }
        }

        Ok(())
    })
}
```

### Verification

- [ ] Event loop doesn't busy-wait
- [ ] All event sources integrated
- [ ] Animations smooth at 60fps
- [ ] No event starvation

---

## Gap 8: Component Manifest

**Priority:** High
**Effort:** Low

Create proper .cml manifest for Fuchsia apps.

### Files to Create

- `extensions/blinc_platform_fuchsia/meta/blinc_app.cml`

### Implementation

```json5
// blinc_app.cml
{
    include: [
        "syslog/client.shard.cml",
    ],

    program: {
        runner: "elf",
        binary: "bin/blinc_app",
    },

    capabilities: [
        {
            protocol: "fuchsia.ui.app.ViewProvider",
        },
    ],

    use: [
        // Compositor
        {
            protocol: "fuchsia.ui.composition.Flatland",
        },
        {
            protocol: "fuchsia.ui.composition.Allocator",
        },

        // Input
        {
            protocol: "fuchsia.ui.pointer.TouchSource",
            availability: "optional",
        },
        {
            protocol: "fuchsia.ui.pointer.MouseSource",
            availability: "optional",
        },
        {
            protocol: "fuchsia.ui.input3.Keyboard",
        },

        // GPU
        {
            protocol: "fuchsia.vulkan.loader.Loader",
        },
        {
            protocol: "fuchsia.sysmem2.Allocator",
        },

        // System
        {
            protocol: "fuchsia.logger.LogSink",
        },
    ],

    expose: [
        {
            protocol: "fuchsia.ui.app.ViewProvider",
            from: "self",
        },
    ],
}
```

### Verification

- [ ] Manifest validates with `cmc`
- [ ] Component starts in Fuchsia
- [ ] All capabilities available

---

## Gap 9: Asset Loading

**Priority:** Medium
**Effort:** Low

Load assets from Fuchsia package namespace.

### Files to Modify

- `extensions/blinc_platform_fuchsia/src/assets.rs`

### Current State

Already implemented with `/pkg/data/` path prefix.

### Verification

- [ ] Fonts load from package
- [ ] Images load from package
- [ ] Error handling for missing assets

---

## Implementation Order

1. **Gap 7: Async Event Loop** - Foundation for everything else
2. **Gap 1: ViewProvider** - Required to launch app
3. **Gap 2: ParentViewportWatcher** - Get window size
4. **Gap 6: GPU Surface** - Rendering
5. **Gap 4: TouchSource/MouseSource** - Input
6. **Gap 3: Focus Handling** - Keyboard prerequisite
7. **Gap 5: Keyboard** - Text input
8. **Gap 8: Component Manifest** - Packaging
9. **Gap 9: Asset Loading** - Already done, verify

---

## Testing Strategy

### Unit Tests (Host)

Run with placeholder implementations:
```bash
cargo test -p blinc_platform_fuchsia
cargo test -p blinc_app --features fuchsia
```

### Integration Tests (Emulator)

```bash
# Start Fuchsia emulator
ffx emu start --headless

# Build and deploy
fx set core.x64 --with //examples/blinc_hello
fx build
fx run fuchsia-pkg://fuchsia.com/blinc_hello#meta/blinc_hello.cm

# Check logs
ffx log
```

### Manual Testing

- [ ] App launches and displays UI
- [ ] Touch interaction works
- [ ] Keyboard input works
- [ ] Animations smooth
- [ ] No memory leaks
- [ ] Graceful shutdown

---

## Dependencies

### Required Fuchsia SDK Crates

```toml
[target.'cfg(target_os = "fuchsia")'.dependencies]
fuchsia-async = "0.1"
fuchsia-component = "0.1"
fidl = "0.1"
fidl_fuchsia_ui_composition = "0.1"
fidl_fuchsia_ui_pointer = "0.1"
fidl_fuchsia_ui_input3 = "0.1"
fidl_fuchsia_ui_views = "0.1"
fidl_fuchsia_ui_app = "0.1"
fidl_fuchsia_sysmem2 = "0.1"
```

### Build Configuration

```toml
# .cargo/config.toml
[target.x86_64-unknown-fuchsia]
linker = "path/to/fuchsia-sdk/tools/x64/lld"
rustflags = ["-L", "path/to/fuchsia-sdk/arch/x64/lib"]

[target.aarch64-unknown-fuchsia]
linker = "path/to/fuchsia-sdk/tools/arm64/lld"
rustflags = ["-L", "path/to/fuchsia-sdk/arch/arm64/lib"]
```

---

## Success Criteria

- [ ] Blinc apps compile for Fuchsia targets
- [ ] Apps run in Fuchsia emulator
- [ ] Touch/mouse input works
- [ ] Keyboard input works
- [ ] 60fps rendering
- [ ] Correct window sizing
- [ ] Assets load from package
- [ ] Component manifest valid
