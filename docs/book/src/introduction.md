# Introduction

Blinc is a GPU-accelerated UI framework for Rust. Build desktop, mobile and
browser interfaces with composable components, CSS styling and reactive
property bindings.

This book uses the `blinc` package. Its modules provide the application runner,
layout builders, reactive primitives, themes and optional component APIs.

## Compose an interface

Rust builders describe the content and its structure:

```rust
use blinc::{cn, prelude::*};

fn project_card() -> Div {
    div().class("project-card")
        .child(cn::h2("Website"))
        .child(cn::p("Keep decisions and next steps together."))
}
```

Stylesheets define the appearance:

```css
.project-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-4);
  border-radius: var(--radius-default);
  background: var(--surface);
}
```

Built-in themes supply colours, spacing, corner shapes and motion tokens.
Component classes let an application restyle controls while retaining their
input behaviour.

## Bind data to properties

Signals hold values, Computeds derive them, and Effects connect changes to
application logic. Properties accept reactive sources directly:

```rust
use blinc::{cn, core::{computed, signal}};

let pending = signal(3usize);
let summary = computed(move |g| {
    format!("{} files awaiting review", g.get(pending).unwrap_or_default())
});

let heading = cn::heading(2, summary);
pending.set(5);
```

Changing the Signal updates the bound heading in place. Reactive lists and
branches handle changes to children. Finite state machines remain available
for interactions that need explicit states and transitions.

## Draw beyond standard controls

Blinc combines text shaping, SDF geometry, images, SVG, canvas drawing and
material effects in its GPU renderer. Flow shaders provide custom visual
behaviour. CSS transitions, keyframes and motion containers animate the UI.

## Find the API

| Module | Purpose |
|--------|---------|
| `blinc::app` | Desktop, mobile and browser runners |
| `blinc::layout` | Elements, layout and input routing |
| `blinc::core` | Signals, Computeds, Effects and drawing types |
| `blinc::theme` | Built-in themes and custom theme bundles |
| `blinc::cn` | Components, enabled with `cn` |
| `blinc::icons` | Lucide icons, enabled with `icons` |
| `blinc::svg` | SVG loading APIs, enabled with `svg` |
| `blinc::media` | Playback and capture APIs, enabled with `media` |

The version selector opens documentation for the release you use. The main
`/docs/` entry shows the latest stable book; Development follows the current
source tree.

Start with [Installation](getting-started/installation.md), then build
[Your First App](getting-started/first-app.md).
