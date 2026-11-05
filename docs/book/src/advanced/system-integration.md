# System Integration

Blinc provides APIs for common desktop system features: file dialogs, system tray, notifications, drag-and-drop, and global keyboard shortcuts.

## Feature flags

Each surface is a separate `blinc` feature so apps that don't use them don't pay the transitive dep cost (each pulls 25–130 crates on Linux):

| Feature | Module | Backed by |
|---|---|---|
| `dialogs` | `blinc::app::dialog` | `rfd` |
| `tray` | `blinc::app::tray` | `tray-icon` + `muda` |
| `notifications` | `blinc::app::notify` | `notify-rust` |
| `hotkeys` | `blinc::app::hotkey` | `global-hotkey` |

Enable in `Cargo.toml`:

```toml
[dependencies]
blinc = { version = "0.6", features = ["dialogs", "tray", "notifications", "hotkeys"] }
```

Or use the umbrella `windowed-full` to enable all four:

```toml
blinc = { version = "0.6", features = ["windowed-full"] }
```

Without the corresponding feature, the matching module's APIs compile to no-op stubs — calls return `None` / silently do nothing — so you can prototype against them and turn the flag on later.

## File Dialogs

Open, save, and folder picker dialogs via the `rfd` crate (requires the `dialogs` feature):

```rust
use blinc::app::dialog::{open_file, save_file, pick_folder, FileFilter};

// Open a file
if let Some(path) = open_file()
    .title("Open Image")
    .filter(FileFilter::new("Images").ext("png").ext("jpg"))
    .filter(FileFilter::new("All Files").ext("*"))
    .pick()
{
    println!("Selected: {}", path.display());
}

// Open multiple files
let paths = open_file()
    .title("Select Files")
    .filter(FileFilter::new("Rust").ext("rs"))
    .pick_many();

// Save dialog
if let Some(path) = save_file()
    .title("Save As")
    .file_name("untitled.txt")
    .filter(FileFilter::new("Text").ext("txt"))
    .save()
{
    println!("Save to: {}", path.display());
}

// Folder picker
if let Some(dir) = pick_folder()
    .title("Choose Directory")
    .pick()
{
    println!("Directory: {}", dir.display());
}
```

## System Tray

Create a tray icon with a context menu for background apps:

```rust
use blinc::app::tray::{TrayIconBuilder, TrayMenuItem};

let _tray = TrayIconBuilder::new()
    .tooltip("My App v1.0")
    .menu(vec![
        TrayMenuItem::item("Show Window", || {
            // bring window to front
        }),
        TrayMenuItem::separator(),
        TrayMenuItem::submenu("Recent", vec![
            TrayMenuItem::item("File 1", || {}),
            TrayMenuItem::item("File 2", || {}),
        ]),
        TrayMenuItem::separator(),
        TrayMenuItem::item("Quit", || std::process::exit(0)),
    ])
    .build();
// Keep `_tray` alive — dropping it removes the icon
```

You can provide a custom icon:

```rust
let rgba = vec![100, 150, 255, 255].repeat(32 * 32); // 32x32 blue icon
TrayIconBuilder::new()
    .icon_rgba(rgba, 32, 32)
    .tooltip("My App")
    .build();
```

## Notifications

Send native desktop notifications:

```rust
use blinc::app::notify::Notification;

Notification::new("Download Complete")
    .body("Your file has been saved to ~/Downloads")
    .show();
```

## Drag and Drop

### Window-Level

Register a global file drop handler:

```rust
use blinc::app::dnd::{on_file_drop, DropEvent};

on_file_drop(|event| match event {
    DropEvent::Hovered(paths) => println!("Dragging: {:?}", paths),
    DropEvent::Dropped(paths) => {
        for path in paths {
            println!("Dropped: {}", path.display());
        }
    }
    DropEvent::Cancelled => println!("Drag cancelled"),
});
```

### Element-Level

Make any element a drop target:

```rust
div()
    .w(300.0).h(200.0)
    .bg(Color::rgb(0.15, 0.15, 0.2))
    .rounded(8.0)
    .on_file_drop(|ctx| {
        println!("File dropped on this element!");
    })
    .on_file_drag_over(|ctx| {
        // Show visual feedback
    })
    .on_file_drag_leave(|ctx| {
        // Remove visual feedback
    })
    .child(text("Drop files here"))
```

## Global Keyboard Shortcuts

Register system-wide hotkeys that work even when the app isn't focused:

```rust
use blinc::app::hotkey::GlobalHotkey;

// Active until `_hotkey` is dropped
let _hotkey = GlobalHotkey::new("Ctrl+Shift+P", || {
    println!("Global shortcut triggered!");
});

// macOS uses Cmd
let _hotkey2 = GlobalHotkey::new("Cmd+Shift+Space", || {
    println!("Quick search!");
});
```

Accelerator format: `Ctrl`, `Shift`, `Alt`, `Cmd`/`Super` + key name (e.g., `A`, `F1`, `Space`, `Enter`).
