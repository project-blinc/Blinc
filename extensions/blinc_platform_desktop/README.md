# blinc_platform_desktop

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Desktop platform implementation for Blinc UI.

## Overview

`blinc_platform_desktop` provides windowing, input handling, and event loop implementation for desktop platforms (macOS, Windows, Linux) using [winit](https://github.com/rust-windowing/winit).

## Supported Platforms

- **macOS**: 11.0+ (Big Sur and later)
- **Windows**: 10+
- **Linux**: X11 and Wayland

## Features

- **Native Windowing**: Platform-native window management
- **Full Input Support**: Mouse, keyboard, trackpad, touch
- **High DPI**: Automatic scale factor handling
- **Multiple Windows**: Create and manage multiple windows
- **Platform Integration**: Native look and feel

## Quick Start

```rust
use blinc_platform_desktop::DesktopPlatform;
use blinc_platform::{Platform, WindowConfig};

fn main() {
    let platform = DesktopPlatform::new();

    let window = platform.create_window(WindowConfig {
        title: "My App".to_string(),
        width: 800,
        height: 600,
        ..Default::default()
    });

    platform.run(|event| {
        match event {
            Event::RedrawRequested(_) => {
                // Render your UI
            }
            Event::WindowEvent { event, .. } => {
                // Handle window events
            }
            _ => {}
        }
        ControlFlow::Poll
    });
}
```

## Platform-Specific Features

### macOS

```rust
#[cfg(target_os = "macos")]
{
    // Native title bar integration
    window.set_titlebar_transparent(true);

    // Vibrancy effects
    window.set_background_appearance(NSVisualEffectMaterial::Sidebar);

    // Full screen support
    window.set_fullscreen(Some(Fullscreen::Borderless));
}
```

### Windows

```rust
#[cfg(target_os = "windows")]
{
    // DWM composition
    window.enable_blur_behind();

    // Taskbar integration
    window.set_taskbar_progress(0.5);
}
```

### Linux

```rust
#[cfg(target_os = "linux")]
{
    // Wayland-specific
    window.set_app_id("com.example.myapp");

    // X11-specific
    window.set_wm_class("MyApp", "myapp");
}
```

## Event Handling

```rust
platform.run(|event| {
    match event {
        // Window lifecycle
        Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
            return ControlFlow::Exit;
        }

        // Input
        Event::WindowEvent { event: WindowEvent::KeyboardInput { input, .. }, .. } => {
            handle_key(input);
        }

        Event::WindowEvent { event: WindowEvent::CursorMoved { position, .. }, .. } => {
            handle_mouse_move(position.x, position.y);
        }

        // Redraw
        Event::RedrawRequested(_) => {
            render();
        }

        _ => {}
    }
    ControlFlow::Poll
});
```

## Requirements

### macOS
- Xcode Command Line Tools

### Windows
- Visual Studio Build Tools 2019+

### Linux
- X11: `libxkbcommon-dev`, `libwayland-dev`, `libxrandr-dev`
- Wayland: `libwayland-dev`, `libxkbcommon-dev`

## License

MIT OR Apache-2.0
