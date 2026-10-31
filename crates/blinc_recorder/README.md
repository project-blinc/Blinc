# blinc_recorder

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Recording and debugging infrastructure for Blinc applications.

## Overview

`blinc_recorder` provides tools for recording user interactions, capturing UI state, and enabling visual regression testing.

## Features

- **Event Recording**: Capture user interactions (clicks, keys, etc.)
- **Tree Snapshots**: Capture UI element tree state
- **Session Management**: Start/pause/stop recording lifecycle
- **Replay**: Play back recorded sessions
- **Visual Testing**: Screenshot comparison for regression testing
- **Debug Server**: Live inspection of running applications

## Recording Sessions

```rust
use blinc_recorder::{SharedRecordingSession, RecordingConfig};

// Create a recording session
let session = SharedRecordingSession::new(RecordingConfig {
    capture_events: true,
    capture_snapshots: true,
    snapshot_interval: Duration::from_millis(100),
    ..Default::default()
});

// Start recording
session.start();

// ... application runs ...

// Stop and get recorded data
session.stop();
let events = session.events();
let snapshots = session.snapshots();
```

## Event Types

```rust
use blinc_recorder::RecordedEvent;

// Recorded events include:
RecordedEvent::MouseMove { x, y, timestamp }
RecordedEvent::MouseDown { x, y, button, timestamp }
RecordedEvent::MouseUp { x, y, button, timestamp }
RecordedEvent::Click { x, y, button, timestamp }
RecordedEvent::KeyDown { key, modifiers, timestamp }
RecordedEvent::KeyUp { key, modifiers, timestamp }
RecordedEvent::Scroll { x, y, delta_x, delta_y, timestamp }
RecordedEvent::TextInput { text, timestamp }
```

## Tree Snapshots

```rust
use blinc_recorder::TreeSnapshot;

// Snapshots capture the UI element tree
let snapshot: TreeSnapshot = session.latest_snapshot();

// Access element data
for element in snapshot.elements() {
    println!("Element: {} at ({}, {})",
        element.type_name,
        element.bounds.x,
        element.bounds.y
    );
}
```

## Replay

```rust
use blinc_recorder::ReplayPlayer;

// Load recorded session
let session = SharedRecordingSession::load("recording.json")?;

// Create replay player
let player = ReplayPlayer::new(session);

// Play back events
player.play();

// Or step through manually
player.step();
```

## Visual Testing

With the `headless` feature, `Headless` builds a real UI offscreen, with no
window, through the renderer a window uses. You drive the pointer and the
clock, and read back frames:

```rust
use blinc_recorder::Headless;

let mut ui = Headless::new(240, 100)?;
ui.theme(HybridTheme::bundle().with_css(CN_STYLES), ColorScheme::Dark);
ui.mount(|_| div().w(240.0).h(100.0).child(cn::button("Save")));

ui.hover(".cn-button").settle();   // move the pointer, let transitions finish
ui.assert_snapshot("button_hover");

ui.advance(64);                    // or step the clock by hand
let frame = ui.snapshot()?;        // a `CapturedFrame`
```

Goldens are PNGs in `.blinc/snapshots/<crate>/<os>/` at the workspace root
(`BLINC_SNAPSHOT_DIR` moves it). `.blinc/` is not tracked: each machine
records its own with `BLINC_SNAPSHOT=bless`. A missing golden fails and the
actual and diff images are written to `failed/` beside them. A platform with
no goldens skips, and `BLINC_SNAPSHOT=off` skips everywhere.

A headless run records like a live one. The session is the format the
debugger opens, and it replays to the same frame:

```rust
ui.start_recording();
ui.click(".cn-button").settle();
let export = ui.stop_recording().unwrap();   // events and tree snapshots

other.replay(&export);                       // reproduces the input
```

## Debug Server

```rust
use blinc_recorder::DebugServer;

// Start debug server for live inspection
let server = DebugServer::start(8080)?;

// Connect via browser at http://localhost:8080
// View live element tree, events, and snapshots
```

## Optional Features

- `png` - Enable PNG screenshot export
- `headless` - `Headless`, `Golden`: offscreen builds, input, frames and goldens

## Architecture

```
blinc_recorder
├── session.rs        # Recording session management
├── events.rs         # Event types and recording
├── snapshot.rs       # Tree snapshot capture
├── replay/           # Replay infrastructure
├── testing/          # Frames, goldens and the offscreen `Headless` driver
└── server.rs         # Debug server
```

## License

MIT OR Apache-2.0
