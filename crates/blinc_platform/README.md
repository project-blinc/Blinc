# blinc_platform

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Platform abstraction layer for Blinc UI.

## Overview

`blinc_platform` defines the traits and types for cross-platform windowing, input handling, and application lifecycle. Platform-specific implementations are provided by separate crates.

## Features

- **Platform Trait**: Unified API for all platforms
- **Window Management**: Create, configure, and manage windows
- **Event Loop**: Handle platform events and callbacks
- **Input Events**: Mouse, keyboard, touch input
- **Asset Loading**: Platform-agnostic asset access

## Traits

### Platform

```rust
pub trait Platform {
    type Window: Window;
    type EventLoop: EventLoop;

    fn new() -> Self;
    fn create_window(&self, config: WindowConfig) -> Self::Window;
    fn run(self, event_loop: Self::EventLoop);
}
```

### Window

```rust
pub trait Window {
    fn id(&self) -> WindowId;
    fn size(&self) -> (u32, u32);
    fn scale_factor(&self) -> f64;
    fn set_title(&mut self, title: &str);
    fn set_size(&mut self, width: u32, height: u32);
    fn set_visible(&mut self, visible: bool);
    fn request_redraw(&self);
    fn raw_handle(&self) -> RawWindowHandle;
}
```

### EventLoop

```rust
pub trait EventLoop {
    fn run<F>(self, callback: F)
    where
        F: FnMut(Event) -> ControlFlow;

    fn create_proxy(&self) -> EventLoopProxy;
}
```

## Window Configuration

```rust
let config = WindowConfig {
    title: "My App".to_string(),
    width: 800,
    height: 600,
    min_width: Some(400),
    min_height: Some(300),
    max_width: None,
    max_height: None,
    resizable: true,
    decorations: true,
    transparent: false,
    always_on_top: false,
};
```

## Event Types

```rust
pub enum Event {
    // Window events
    WindowEvent { window_id: WindowId, event: WindowEvent },

    // Application events
    Resumed,
    Suspended,
    RedrawRequested(WindowId),
    MainEventsCleared,

    // User events
    UserEvent(Box<dyn Any + Send>),
}

pub enum WindowEvent {
    Resized(u32, u32),
    Moved(i32, i32),
    CloseRequested,
    Focused(bool),
    ScaleFactorChanged(f64),
    ThemeChanged(Theme),
}

pub enum InputEvent {
    MouseMoved { x: f64, y: f64 },
    MouseButton { button: MouseButton, state: ElementState },
    MouseWheel { delta_x: f64, delta_y: f64 },
    KeyboardInput { key: Key, state: ElementState, modifiers: Modifiers },
    TextInput { text: String },
    Touch { id: u64, phase: TouchPhase, x: f64, y: f64 },
}
```

## Platform Implementations

| Crate | Platforms |
|-------|-----------|
| `blinc_platform_desktop` | macOS, Windows, Linux |
| `blinc_platform_ios` | iOS, iPadOS |
| `blinc_platform_android` | Android |

## Asset Loading

```rust
pub trait AssetLoader {
    fn load(&self, path: &str) -> Result<Vec<u8>>;
    fn exists(&self, path: &str) -> bool;
}

// Platform implementations handle:
// - File system access
// - Bundle resources (iOS/macOS)
// - APK assets (Android)
// - Embedded resources (Windows)
```

## License

MIT OR Apache-2.0
