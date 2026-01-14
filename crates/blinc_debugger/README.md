# blinc_debugger

> **Part of the [Blinc UI Framework](https://project-blinc.github.io/Blinc)**
>
> This crate is a component of Blinc, a GPU-accelerated UI framework for Rust.
> For full documentation and guides, visit the [Blinc documentation](https://project-blinc.github.io/Blinc).

Visual debugger application for Blinc UI recordings.

## Overview

`blinc_debugger` is a standalone GUI application for inspecting recorded Blinc UI sessions. It provides visual tools for debugging layout issues, tracking events, and analyzing UI state.

## Features

- **Element Tree**: Hierarchical view of UI elements with diff highlighting
- **UI Preview**: Visual preview with debug overlays
- **Inspector Panel**: Detailed element properties and styles
- **Event Timeline**: Playback controls for recorded events
- **State Viewer**: Track reactive state changes

## Installation

```bash
cargo install blinc_debugger
```

Or build from source:

```bash
cargo build -p blinc_debugger --release
```

## Usage

```bash
# Open a recording file
blinc-debugger recording.json

# Or launch and open via UI
blinc-debugger
```

## Interface

```
┌─────────────────────────────────────────────────────────────┐
│  File  View  Help                                           │
├────────────────┬────────────────────────┬───────────────────┤
│                │                        │                   │
│  Element Tree  │    UI Preview          │   Inspector       │
│                │                        │                   │
│  ▼ Root        │  ┌─────────────────┐  │   Type: div       │
│    ▼ Header    │  │                 │  │   Width: 800      │
│      Logo      │  │   [Preview]     │  │   Height: 600     │
│      Nav       │  │                 │  │   Background: #fff│
│    ▼ Content   │  └─────────────────┘  │   ...             │
│      ...       │                        │                   │
│                │                        │                   │
├────────────────┴────────────────────────┴───────────────────┤
│  Event Timeline                                   [▶][◀][▶] │
│  ═══════════════════●═══════════════════════════════════════│
│  00:00.000  Click (200, 150)                                │
│  00:00.500  KeyDown 'a'                                     │
│  00:01.000  Scroll (0, -50)                                 │
└─────────────────────────────────────────────────────────────┘
```

## Features in Detail

### Element Tree

- Expand/collapse element hierarchy
- Highlight elements on hover
- Filter by element type
- Show/hide hidden elements

### UI Preview

- Zoom and pan
- Debug overlays (bounds, padding, margins)
- Element highlighting on selection
- Snapshot comparison (before/after)

### Inspector Panel

- Element type and ID
- Bounds (x, y, width, height)
- Style properties (background, border, etc.)
- Layout properties (flex, padding, margin)
- Event handlers attached

### Event Timeline

- Play/pause/step through events
- Jump to specific timestamp
- Filter by event type
- Event details on hover

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Space` | Play/Pause |
| `←` / `→` | Step backward/forward |
| `Cmd/Ctrl + O` | Open recording |
| `Cmd/Ctrl + F` | Search elements |
| `Escape` | Deselect |

## Recording Format

The debugger reads JSON files created by `blinc_recorder`:

```json
{
  "version": "1.0",
  "events": [...],
  "snapshots": [...],
  "metadata": {
    "app_name": "My App",
    "recorded_at": "2024-01-01T00:00:00Z"
  }
}
```

## License

MIT OR Apache-2.0
