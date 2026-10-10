# CSS Styling

Blinc includes a full-featured CSS engine that lets you style your UI with familiar CSS syntax. Write stylesheets with selectors, animations, transitions, filters, 3D transforms, and more — then apply them with a single `ctx.add_css()` call.

## Quick Start

```rust
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};

fn main() -> Result<()> {
    let mut css_loaded = false;

    WindowedApp::run(WindowConfig::default(), move |ctx| {
        if !css_loaded {
            ctx.add_css(r#"
                #card {
                    background: linear-gradient(135deg, #667eea, #764ba2);
                    border-radius: 16px;
                    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
                    padding: 24px;
                    transition: transform 0.3s ease, box-shadow 0.3s ease;
                }
                #card:hover {
                    transform: scale(1.03);
                    box-shadow: 0 12px 48px rgba(102, 126, 234, 0.5);
                }
            "#);
            css_loaded = true;
        }
        build_ui(ctx)
    })
}

fn build_ui(_ctx: &WindowedContext) -> impl ElementBuilder {
    div().id("card").child(text("Hello, CSS!").size(20.0).color(Color::WHITE))
}
```

Rust code defines **structure**. CSS defines **style**.

---

## Table of Contents

- [Selectors](#selectors)
- [Visual Properties](#visual-properties)
- [Layout Properties](#layout-properties)
- [Text & Typography](#text--typography)
- [Transforms](#transforms)
- [Transitions](#transitions)
- [Animations](#animations)
- [Filters](#filters)
- [Backdrop Filters & Glass Effects](#backdrop-filters--glass-effects)
- [Clip Path](#clip-path)
- [Mask Image](#mask-image)
- [SVG Styling](#svg-styling)
- [3D Shapes & Lighting](#3d-shapes--lighting)
- [CSS Variables](#css-variables)
- [Theme Integration](#theme-integration)
- [Form Styling](#form-styling)
- [Length Units](#length-units)
- [Error Handling](#error-handling)
- [Scoped Style Macros](#scoped-style-macros)
- [How It Works](#how-it-works)
- [Property Comparison](#property-comparison)

---

## Selectors

Blinc supports a wide range of CSS selectors — from simple IDs to complex combinators.

### ID Selectors

The most common way to target elements. Attach an `id` in Rust, then style it in CSS:

```rust
div().id("card")        // Rust
```

```css
#card { background: #3b82f6; }
```

### Class Selectors

Assign CSS classes with `.class()` in Rust:

```rust
div().class("icon-wrapper")
```

```css
.icon-wrapper {
    border-radius: 24px;
    backdrop-filter: blur(12px);
    transition: transform 0.2s ease;
}
.icon-wrapper:hover {
    transform: scale(1.12);
}
```

### Type / Tag Selectors

Target elements by tag name (primarily used for SVG sub-elements):

```css
svg { stroke: #ffffff; fill: none; stroke-width: 2.5; }
path { stroke: #8b5cf6; stroke-width: 5; }
circle { fill: #f3e8ff; stroke: #a78bfa; }
rect { fill: #fef3c7; stroke: #f59e0b; }
```

### Universal Selector

```css
* { opacity: 1.0; }
```

### Pseudo-Classes (States)

Interactive states are matched automatically based on user input:

```css
#button:hover   { transform: scale(1.02); }
#button:active  { transform: scale(0.98); }
#button:focus   { box-shadow: 0 0 0 3px #3b82f6; }
#button:disabled { opacity: 0.5; }
#checkbox:checked { background: #3b82f6; }
```

### Structural Pseudo-Classes

```css
.item:first-child     { border-radius: 12px 12px 0 0; }
.item:last-child      { border-radius: 0 0 12px 12px; }
.item:only-child      { border-radius: 12px; }
.item:nth-child(2)    { background: #f0f0f0; }
.item:nth-last-child(1) { font-weight: bold; }
.item:first-of-type   { color: red; }
.item:last-of-type    { color: blue; }
.item:nth-of-type(3)  { opacity: 0.5; }
.item:only-of-type    { border: 2px solid green; }
:empty                { display: none; }
:root                 { --primary: #3b82f6; }
```

### Functional Pseudo-Classes

```css
:not(.hidden) { opacity: 1; }
:is(#card, .panel) { border-radius: 12px; }
:where(.btn, .link) { cursor: pointer; }
```

### Pseudo-Elements

```css
#input::placeholder { color: #64748b; }
#text::selection    { background: #3b82f6; }
```

### Combinators

Chain selectors for precise targeting:

```css
/* Child combinator — direct children only */
#parent > .child { padding: 8px; }

/* Descendant combinator — any depth */
#list .item { margin: 4px; }

/* Adjacent sibling — next element */
.trigger:hover + .target { opacity: 1; }

/* General sibling — any following sibling */
.trigger:hover ~ .item { background: #e0e0e0; }
```

### Complex Selectors

Combine any of the above:

```css
#card:hover > .title { color: #ffffff; }
#list .item:last-child { border-bottom: none; }
.icon-wrapper:hover #pause { fill: rgba(0, 0, 0, 0.7); }
#progress:hover #time-left { opacity: 1; }
```

---

## Visual Properties

### Background

Supports solid colors, gradients, and image URLs:

```css
/* Solid colors */
#el { background: #3b82f6; }
#el { background: rgb(59, 130, 246); }
#el { background: rgba(255, 255, 255, 0.15); }

/* Linear gradient */
#el { background: linear-gradient(135deg, #667eea 0%, #764ba2 100%); }
#el { background: linear-gradient(to right, red, blue); }
#el { background: linear-gradient(to bottom right, #fff, #000); }

/* Radial gradient */
#el { background: radial-gradient(circle, red, blue); }
#el { background: radial-gradient(circle at 25% 75%, red, blue); }

/* Conic gradient */
#el { background: conic-gradient(from 45deg, red, yellow, green, blue, red); }

/* Background image */
#el { background: url("path/to/image.jpg"); }
```

### Border Radius

```css
#card { border-radius: 12px; }
#avatar { border-radius: 50px; }          /* Circle */
#card { border-radius: theme(radius-lg); } /* Theme token */
```

### Corner Shape

Controls the shape of rounded corners using superellipse exponents. Instead of the standard circular arc, you can create beveled, squircle, scooped, or notched corners.

```css
/* Uniform shape for all corners */
#card { corner-shape: 2; }                /* Squircle (smoother than circular) */
#card { corner-shape: 0; }                /* Bevel (straight diagonal cut) */
#card { corner-shape: -1; }               /* Scoop (concave inward curve) */

/* Functional syntax */
#card { corner-shape: superellipse(2); }   /* Same as corner-shape: 2 */
```

**Value reference:**

| Value  | Shape    | Description                         |
| ------ | -------- | ----------------------------------- |
| `1`    | Round    | Standard circular arc (default)     |
| `0`    | Bevel    | Straight diagonal cut               |
| `2`    | Squircle | Smoother than circular (iOS-style)  |
| `-1`   | Scoop    | Concave inward curve                |
| `100`  | Square   | Sharp corner (ignores border-radius)|
| `-100` | Notch    | Sharp 90-degree inward notch        |

**Rust builder equivalents:**

```rust
div().corner_shape(2.0)                                   // Uniform squircle
div().corner_shapes(0.0, 2.0, 2.0, 0.0)                  // Per-corner: bevel TL, squircle others
div().corner_squircle()                                    // Preset: squircle
div().corner_bevel()                                       // Preset: bevel
div().corner_scoop()                                       // Preset: scoop
```

Corner shape is animatable via transitions and keyframes:

```css
#card {
    border-radius: 20px;
    corner-shape: 1;
    transition: corner-shape 0.3s ease;
}
#card:hover {
    corner-shape: 2;   /* Morph from round to squircle on hover */
}
```

### Border

```css
#el { border-width: 2px; border-color: #3b82f6; }
#el { border-width: 1.5px; border-color: rgba(255, 255, 255, 0.5); }
```

### Box Shadow

```css
#card { box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3); }
#glow { box-shadow: 0 0 40px rgba(139, 92, 246, 0.7); }
#card { box-shadow: none; }
```

### Text Shadow

```css
#heading { text-shadow: 3px 3px 0px rgba(255, 68, 68, 1.0); }
```

### Outline

```css
#el { outline: 3px solid #f59e0b; }
#el { outline-offset: 6px; }
#el { outline-width: 2px; outline-color: #ef4444; }
```

### Opacity

```css
#el { opacity: 0.75; }
```

### Visibility

```css
#el { visibility: hidden; }
```

### Z-Index & Render Layer

```css
#overlay { z-index: 10; }
#el { render-layer: foreground; }  /* foreground | background | glass */
```

---

## Layout Properties

All standard flexbox layout properties can be set from CSS:

### Sizing

```css
#card {
    width: 380px;
    height: 200px;
    min-width: 100px;
    max-width: 600px;
}

/* Percentage values */
#full { width: 100%; }

/* Auto sizing */
#auto { width: auto; }
```

### Spacing

```css
#card {
    padding: 24px;
    padding: 6px 8px;          /* vertical horizontal */
    padding: 8px 12px 16px;    /* top horizontal bottom */
    padding: 8px 12px 16px 4px; /* top right bottom left */
    margin: 16px;
    gap: 20px;
}
```

### Flexbox

```css
#container {
    display: flex;
    flex-direction: row;        /* row | column | row-reverse | column-reverse */
    flex-wrap: wrap;            /* wrap | nowrap */
    align-items: center;        /* center | start | end | stretch | baseline */
    justify-content: space-between; /* center | start | end | space-between | space-around | space-evenly */
    gap: 16px;
}

#item {
    flex-grow: 1;
    flex-shrink: 0;
    align-self: center;
}
```

### Positioning

```css
#el {
    position: absolute;  /* static | relative | absolute | fixed | sticky */
    top: 10px;
    right: 0;
    bottom: 0;
    left: 10px;
    inset: 0;           /* shorthand for all four */
}
```

### Overflow

```css
#scroll { overflow: scroll; }
#clip   { overflow: clip; }
#el     { overflow-x: scroll; overflow-y: hidden; }
```

### Overflow Fade

Replaces the hard clip at overflow boundaries with a smooth fade-to-transparent ramp. Each edge can have an independent fade distance in pixels.

```css
/* Uniform: all 4 edges fade over 24px */
#scroll { overflow-fade: 24px; }

/* Vertical + horizontal: top/bottom 32px, left/right 0 */
#scroll { overflow-fade: 32px 0px; }

/* Per-edge: top, right, bottom, left */
#scroll { overflow-fade: 24px 0px 24px 0px; }
```

**Rust builder equivalents:**

```rust
div().overflow_fade(24.0)                              // Uniform
div().overflow_fade_y(32.0)                            // Vertical only (top + bottom)
div().overflow_fade_x(16.0)                            // Horizontal only (left + right)
div().overflow_fade_edges(24.0, 0.0, 24.0, 0.0)       // Per-edge: top, right, bottom, left
```

Overflow fade is animatable — combine with transitions for hover-triggered soft edges:

```css
#container {
    overflow: clip;
    overflow-fade: 0px;
    transition: overflow-fade 0.3s ease;
}
#container:hover {
    overflow-fade: 32px;
}
```

### Display

```css
#hidden { display: none; }
#flex   { display: flex; }
#block  { display: block; }
```

---

## Text & Typography

```css
#text {
    color: #ffffff;
    font-size: 20px;
    font-weight: 700;               /* 100-900 or thin/light/normal/bold/black */
    line-height: 1.5;
    letter-spacing: 0.5px;
    text-align: center;              /* left | center | right */
    text-decoration: underline;      /* none | underline | line-through */
    text-decoration-color: #ff0000;
    text-decoration-thickness: 2px;
    text-overflow: ellipsis;         /* clip | ellipsis */
    white-space: nowrap;             /* normal | nowrap | pre | pre-wrap */
}
```

---

## Transforms

### 2D Transforms

```css
#el { transform: rotate(15deg); }
#el { transform: scale(1.15); }
#el { transform: scale(1.5, 0.8); }        /* non-uniform */
#el { transform: translate(10px, 20px); }
#el { transform: translateX(10px); }
#el { transform: translateY(20px); }
#el { transform: skewX(-8deg); }
#el { transform: skewY(12deg); }
#el { transform: skew(10deg, 5deg); }
```

### Compound Transforms

Chain multiple transforms in a single property:

```css
#el { transform: rotate(15deg) scale(1.15); }
#el { transform: scale(0.9) translateY(20px); }
```

### Transform Origin

```css
#el { transform-origin: center; }
#el { transform-origin: left top; }
#el { transform-origin: right bottom; }
#el { transform-origin: 50% 50%; }
```

---

## Transitions

Smoothly animate property changes on state transitions (e.g., hover):

```css
#card {
    background: #1e293b;
    transform: scale(1.0);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
    transition: transform 0.3s ease, box-shadow 0.3s ease, background 0.3s ease;
}
#card:hover {
    transform: scale(1.05);
    box-shadow: 0 8px 32px rgba(59, 130, 246, 0.5);
    background: #334155;
}
```

### Transition Syntax

```css
/* Shorthand */
transition: property duration timing-function delay;

/* Multiple properties */
transition: transform 0.3s ease, opacity 0.3s ease, box-shadow 0.25s ease;

/* All properties */
transition: all 0.3s ease;

/* Individual properties */
transition-property: transform;
transition-duration: 300ms;
transition-timing-function: ease-in-out;
transition-delay: 100ms;
```

### Animatable Properties

Almost every visual and layout property can be transitioned:

- **Visual**: `opacity`, `background`, `border-color`, `border-width`, `border-radius`, `corner-shape`, `box-shadow`, `text-shadow`, `outline-color`, `outline-width`
- **Transform**: `transform` (rotate, scale, translate, skew)
- **Layout**: `width`, `height`, `padding`, `margin`, `gap`, `min-width`, `max-width`, `min-height`, `max-height`, `top`, `left`, `flex-grow`
- **Filters**: `filter`, `backdrop-filter`
- **Clip & Overflow**: `clip-path`, `overflow-fade`
- **SVG**: `fill`, `stroke`, `stroke-width`, `stroke-dashoffset`
- **Mask**: `mask-image`
- **3D**: `rotate-x`, `rotate-y`, `perspective`, `translate-z`

### Timing Functions

| Function | Description |
|----------|-------------|
| `ease` | Slow start and end (default) |
| `linear` | Constant speed |
| `ease-in` | Slow start |
| `ease-out` | Slow end |
| `ease-in-out` | Slow start and end |

### Layout Transitions

Layout properties animate with automatic layout recalculation:

```css
#panel {
    width: 120px;
    height: 60px;
    padding: 8px;
    transition: width 0.4s ease, height 0.4s ease, padding 0.3s ease;
}
#panel:hover {
    width: 280px;
    height: 120px;
    padding: 24px;
}
```

---

## Animations

### @keyframes

Define multi-step animations:

```css
@keyframes fade-in {
    from { opacity: 0; transform: translateY(20px); }
    to   { opacity: 1; transform: translateY(0); }
}

@keyframes pulse {
    0%, 100% { opacity: 1; transform: scale(1); }
    50%      { opacity: 0.7; transform: scale(1.05); }
}

@keyframes gradient-cycle {
    0%   { background: linear-gradient(90deg, #d0d0d0, #e0e0e0, #ffffff); }
    33%  { background: linear-gradient(90deg, #d0d0d0, #ffffff, #d0d0d0); }
    66%  { background: linear-gradient(90deg, #ffffff, #e0e0e0, #d0d0d0); }
    100% { background: linear-gradient(90deg, #d0d0d0, #e0e0e0, #ffffff); }
}
```

### Animation Property

```css
/* Shorthand */
#el { animation: pulse 2s ease-in-out infinite; }

/* Full shorthand */
#el { animation: slide-in 300ms ease-out 100ms 1 normal forwards; }
/*               name     duration timing  delay count direction fill-mode */

/* Individual properties */
#el {
    animation-name: pulse;
    animation-duration: 2s;
    animation-timing-function: ease-in-out;
    animation-delay: 100ms;
    animation-iteration-count: infinite;    /* or a number */
    animation-direction: alternate;         /* normal | reverse | alternate | alternate-reverse */
    animation-fill-mode: forwards;          /* none | forwards | backwards | both */
}
```

### Animatable Keyframe Properties

All these properties can be used inside `@keyframes`:

- `opacity`, `transform`, `background`, `border-color`, `border-width`, `border-radius`
- `box-shadow`, `text-shadow`, `outline`, `color`, `font-size`
- `width`, `height`, `padding`, `margin`, `gap`, `min-width`, `max-width`
- `filter` (blur, brightness, contrast, etc.)
- `backdrop-filter`
- `clip-path`
- `fill`, `stroke`, `stroke-width`, `stroke-dasharray`, `stroke-dashoffset`
- `d` (SVG path morphing)
- `rotate-x`, `rotate-y`, `perspective`, `translate-z`
- `light-direction`, `light-intensity`, `ambient`, `specular`

### Automatic Animation

Elements with an `animation` property in the stylesheet start animating automatically:

```css
@keyframes card-enter {
    from { opacity: 0; transform: scale(0.95); }
    to   { opacity: 1; transform: scale(1); }
}

#card { animation: card-enter 300ms ease-out; }
```

```rust
div().id("card").child(content())  // Animates on first render!
```

---

## Filters

Apply visual effects to elements:

```css
#el { filter: grayscale(100%); }
#el { filter: sepia(100%); }
#el { filter: invert(100%); }
#el { filter: brightness(150%); }
#el { filter: contrast(200%); }
#el { filter: saturate(300%); }
#el { filter: hue-rotate(90deg); }
#el { filter: blur(4px); }

/* Combined filters */
#el { filter: grayscale(50%) brightness(120%) contrast(110%); }

/* Filter transitions */
#el {
    filter: blur(0px);
    transition: filter 0.4s ease;
}
#el:hover {
    filter: blur(8px);
}
```

### Filter Animation

```css
@keyframes blur-pulse {
    0%   { filter: blur(0px); }
    50%  { filter: blur(6px); }
    100% { filter: blur(0px); }
}
#el { animation: blur-pulse 3s ease-in-out infinite; }
```

---

## Backdrop Filters & Glass Effects

Apply effects to the area behind an element — essential for glassmorphism:

```css
/* Simple blur */
#panel { backdrop-filter: blur(12px); }

/* Combined */
#panel { backdrop-filter: blur(12px) saturate(180%) brightness(80%); }

/* Named materials */
#glass { backdrop-filter: glass; }
#metal { backdrop-filter: metallic; }
#chrome { backdrop-filter: chrome; }
#gold  { backdrop-filter: gold; }
#wood  { backdrop-filter: wood; }

/* Liquid glass (refracted bevel borders) */
#card {
    backdrop-filter: liquid-glass(
        blur(18px)
        saturate(180%)
        brightness(120%)
        border(4.0)
        tint(rgba(255, 255, 255, 1.0))
    );
}
```

### Backdrop Filter Transitions

```css
#panel {
    backdrop-filter: blur(4px);
    transition: backdrop-filter 0.4s ease;
}
#panel:hover {
    backdrop-filter: blur(20px) saturate(180%);
}
```

---

## Clip Path

Clip elements to geometric shapes:

```css
/* Circle */
#el { clip-path: circle(50% at 50% 50%); }
#el { clip-path: circle(40px at center); }

/* Ellipse */
#el { clip-path: ellipse(50% 35% at 50% 50%); }

/* Inset rectangle */
#el { clip-path: inset(10% 10% 10% 10% round 12px); }

/* Rect / XYWH */
#el { clip-path: rect(10px 90px 90px 10px round 8px); }
#el { clip-path: xywh(10px 10px 80px 80px round 8px); }

/* Polygon */
#hexagon {
    clip-path: polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%);
}
#star {
    clip-path: polygon(
        50% 0%, 61% 35%, 98% 35%, 68% 57%, 79% 91%,
        50% 70%, 21% 91%, 32% 57%, 2% 35%, 39% 35%
    );
}

/* SVG Path */
#el { clip-path: path("M 10 80 C 40 10, 65 10, 95 80 S 150 150, 180 80"); }
```

### Clip Path Animation

```css
@keyframes clip-reveal {
    from { clip-path: inset(0% 50% 100% 50%); }
    to   { clip-path: inset(0% 0% 0% 0%); }
}
#el { animation: clip-reveal 400ms ease-out; }
```

---

## Mask Image

Apply gradient masks to fade or reveal parts of an element:

```css
/* Linear gradient masks */
#el { mask-image: linear-gradient(to bottom, black, transparent); }
#el { mask-image: linear-gradient(to right, black, transparent); }
#el { mask-image: linear-gradient(135deg, black 0%, transparent 100%); }

/* Radial gradient masks */
#el { mask-image: radial-gradient(circle, black, transparent); }

/* URL-based masks (image texture) */
#el { mask-image: url("mask.png"); }

/* Mask mode */
#el { mask-mode: alpha; }      /* default */
#el { mask-mode: luminance; }
```

### Mask Transitions

```css
#reveal {
    mask-image: linear-gradient(to bottom, black, transparent);
    transition: mask-image 0.6s ease;
}
#reveal:hover {
    mask-image: linear-gradient(to bottom, black, black);
}

#radial {
    mask-image: radial-gradient(circle, black, transparent);
    transition: mask-image 0.5s ease;
}
#radial:hover {
    mask-image: radial-gradient(circle, black, black);
}
```

---

## SVG Styling

Style SVG elements using CSS — including fills, strokes, and path animations.

### SVG Properties

```css
svg { stroke: #ffffff; fill: none; stroke-width: 2.5; }

#icon {
    fill: #6366f1;
    stroke: #ffffff;
    stroke-width: 2;
    stroke-dasharray: 251;
    stroke-dashoffset: 0;
}
```

### SVG Tag-Name Selectors

Target specific SVG sub-element types within a parent:

```css
#my-svg path   { stroke: #8b5cf6; stroke-width: 5; }
#my-svg circle { fill: #f3e8ff; stroke: #a78bfa; }
#my-svg rect   { fill: #fef3c7; stroke: #f59e0b; }
```

Supported tags: `path`, `circle`, `rect`, `ellipse`, `line`, `polygon`, `polyline`, `g`.

### SVG Fill & Stroke Animation

```css
@keyframes color-cycle {
    0%   { fill: #ef4444; stroke: #dc2626; }
    33%  { fill: #3b82f6; stroke: #2563eb; }
    66%  { fill: #10b981; stroke: #059669; }
    100% { fill: #ef4444; stroke: #dc2626; }
}
#icon { animation: color-cycle 4s ease-in-out infinite; }

@keyframes glow-stroke {
    0%   { stroke: #fbbf24; stroke-width: 2; }
    50%  { stroke: #f43f5e; stroke-width: 5; }
    100% { stroke: #fbbf24; stroke-width: 2; }
}
#icon { animation: glow-stroke 2s ease-in-out infinite; }
```

### SVG Hover Transitions

```css
#icon {
    fill: #6366f1;
    transition: fill 0.3s ease;
}
#icon:hover { fill: #f43f5e; }

#icon2 {
    stroke: #64748b;
    stroke-width: 2;
    transition: stroke 0.3s ease, stroke-width 0.3s ease;
}
#icon2:hover { stroke: #f59e0b; stroke-width: 5; }
```

### Line Drawing Effect

Animate `stroke-dashoffset` to create a "drawing" effect:

```css
#draw-svg {
    stroke-dasharray: 251;
    animation: draw 3s ease-in-out infinite alternate;
}
@keyframes draw {
    from { stroke-dashoffset: 251; }
    to   { stroke-dashoffset: 0; }
}
```

### SVG Path Morphing

Animate the `d` property to morph between shapes. Both shapes must have the **same number of path segments**:

```css
@keyframes morph {
    0%   { d: path("M20,20 L80,20 L80,80 L50,80 L20,80 Z"); }
    50%  { d: path("M50,10 L90,40 L75,85 L25,85 L10,40 Z"); }
    100% { d: path("M20,20 L80,20 L80,80 L50,80 L20,80 Z"); }
}
#morph-svg { animation: morph 3s ease-in-out infinite; }
```

This enables complex effects like hamburger-to-X menu icon animations:

```css
@keyframes hamburger-to-x {
    0%   { d: path("M20,30 L80,30 M20,50 L80,50 M20,70 L80,70"); }
    100% { d: path("M26,26 L74,74 M50,50 L50,50 M26,74 L74,26"); }
}
#menu-icon { animation: hamburger-to-x 1.5s ease-in-out infinite alternate; }
```

---

## 3D Shapes & Lighting

Blinc can render 3D SDF shapes directly via CSS — no mesh files needed.

### 3D Shape Properties

```css
#sphere {
    shape-3d: sphere;       /* box | sphere | cylinder | torus | capsule */
    depth: 120px;
    perspective: 800px;
    rotate-x: 30deg;
    rotate-y: 45deg;
    background: linear-gradient(45deg, #4488ff, #ff4488);  /* UV-mapped onto surface */
}
```

### 3D Lighting

```css
#lit-shape {
    shape-3d: box;
    depth: 80px;
    perspective: 800px;
    light-direction: (0.0, -1.0, 0.5);  /* x, y, z */
    light-intensity: 1.5;
    ambient: 0.3;
    specular: 64.0;
    translate-z: 20px;
}
```

### 3D Boolean Operations (Group Composition)

Combine multiple 3D shapes with boolean operations:

```css
/* Parent must be a group */
#compound { shape-3d: group; perspective: 800px; depth: 80px; }

/* Children contribute shapes */
#base-shape {
    shape-3d: box;
    depth: 80px;
    3d-op: union;
}
#hole {
    shape-3d: cylinder;
    depth: 120px;
    3d-op: subtract;
    3d-blend: 30px;     /* Smooth blend radius */
}
```

Available operations: `union`, `subtract`, `intersect`, `smooth-union`, `smooth-subtract`, `smooth-intersect`.

### 3D Animation

```css
@keyframes spin-y {
    from { rotate-y: 0deg; }
    to   { rotate-y: 360deg; }
}
#rotating-shape {
    shape-3d: sphere;
    depth: 120px;
    perspective: 800px;
    animation: spin-y 4s linear infinite;
}
```

---

## CSS Variables

Define reusable values with custom properties:

```css
:root {
    --brand-color: #3b82f6;
    --card-radius: 12px;
    --hover-opacity: 0.85;
}

#card {
    background: var(--brand-color);
    border-radius: var(--card-radius);
}

#card:hover {
    opacity: var(--hover-opacity);
}
```

### Fallback Values

```css
#el { background: var(--undefined-color, #333); }
```

### Accessing Variables in Rust

```rust
if let Some(value) = stylesheet.get_variable("brand-color") {
    println!("Brand color: {}", value);
}
```

---

## Theme Integration

The `theme()` function references design tokens that adapt to the current app theme:

```css
#card {
    background: theme(surface);
    border-radius: theme(radius-lg);
    box-shadow: theme(shadow-md);
    color: theme(text-primary);
    border-color: theme(border);
}

#button {
    background: theme(primary);
}
#button:hover {
    background: theme(primary-hover);
}
```

### Available Theme Tokens

**Colors:**

| Token | Description |
|-------|-------------|
| `primary`, `primary-hover`, `primary-active` | Primary brand colors |
| `secondary`, `secondary-hover`, `secondary-active` | Secondary colors |
| `success`, `success-bg` | Success states |
| `warning`, `warning-bg` | Warning states |
| `error`, `error-bg` | Error states |
| `info`, `info-bg` | Info states |
| `background`, `surface`, `surface-elevated`, `surface-overlay` | Background surfaces |
| `text-primary`, `text-secondary`, `text-tertiary`, `text-inverse`, `text-link` | Text colors |
| `border`, `border-secondary`, `border-hover`, `border-focus`, `border-error` | Borders |

**Radii:** `radius-none`, `radius-sm`, `radius-default`, `radius-md`, `radius-lg`, `radius-xl`, `radius-2xl`, `radius-3xl`, `radius-full`

**Shadows:** `shadow-none`, `shadow-sm`, `shadow-default`, `shadow-md`, `shadow-lg`, `shadow-xl`

---

## Form Styling

Inputs, checkboxes, radio buttons, and textareas are all styleable via CSS:

### Text Input

```css
#my-input {
    border-color: #3b82f6;
    border-width: 2px;
    border-radius: 8px;
    color: #ffffff;
    caret-color: #60a5fa;
}
#my-input::placeholder {
    color: #64748b;
}
#my-input:hover {
    border-color: #60a5fa;
}
#my-input:focus {
    border-color: #93c5fd;
    box-shadow: 0 0 0 3px rgba(59, 130, 246, 0.3);
}
```

### Checkbox & Radio

```css
#my-checkbox {
    accent-color: #3b82f6;        /* Checkmark / dot color */
    border-color: #475569;
    border-radius: 4px;
}
#my-checkbox:hover {
    border-color: #3b82f6;
}
#my-checkbox:checked {
    background: #3b82f6;
    border-color: #3b82f6;
}
```

### Scrollbar

```css
#scrollable {
    scrollbar-color: #888 #333;   /* thumb-color track-color */
    scrollbar-width: thin;         /* auto | thin | none */
}
```

---

## Object Fit & Position

Control how images fill their container:

```css
#image-container img {
    object-fit: cover;             /* cover | contain | fill | scale-down | none */
    object-position: 60% 40%;     /* x% y% */
}
```

---

## Interaction Properties

```css
#overlay { pointer-events: none; }  /* auto | none */
#link    { cursor: pointer; }       /* default | pointer | text | move | not-allowed | grab | ... */
#blend   { mix-blend-mode: overlay; } /* normal | multiply | screen | overlay | ... */
```

---

## Length Units

| Unit | Description | Example |
|------|-------------|---------|
| `px` | Pixels (default) | `12px` |
| `%` | Percentage of parent | `50%` |
| `sp` | Spacing units (1sp = 4px) | `4sp` = 16px |
| `deg` | Degrees (angles) | `45deg` |
| `turn` | Full turns (angles) | `0.25turn` = 90deg |
| `rad` | Radians (angles) | `1.5708rad` ≈ 90deg |
| `ms` | Milliseconds (time) | `300ms` |
| `s` | Seconds (time) | `0.3s` |

---

## Calc Expressions & Math Functions

Blinc supports CSS `calc()` with full arithmetic, standard CSS math functions, and shader-inspired extensions. Any property that accepts a numeric value can use `calc()`.

### Basic Arithmetic

Standard `+`, `-`, `*`, `/` with mixed units:

```css
#panel {
    width: calc(100% - 40px);
    padding: calc(2 * 8px);
    margin: calc(100% / 3);
    height: calc(50vh - 20px);
}
```

### Units in Calc

All length units from the table above work inside `calc()`:

```css
width: calc(100vw - 300px);
height: calc(50vh - 2em);
rotate: calc(45deg + 0.25turn);
transition-duration: calc(200ms + 0.1s);
```

Multiply unitless values by a unit literal to produce a dimension:

```css
/* Unitless env var → px */
border-radius: calc(mix(4, 48, 0.5) * 1px);

/* Unitless → degrees */
rotate: calc(env(pointer-x) * 25deg);
```

### CSS Standard Functions

| Function | Syntax | Description |
| --- | --- | --- |
| `min` | `min(a, b)` | Smaller of two values |
| `max` | `max(a, b)` | Larger of two values |
| `clamp` | `clamp(min, val, max)` | Constrain value to range |

```css
#card {
    width: clamp(200px, 50%, 600px);
    font-size: min(2em, 24px);
    padding: max(8px, 2%);
}
```

### Blinc Extension Functions

These shader-inspired functions are available inside `calc()` for expressive, animation-friendly math:

#### `mix(a, b, t)` — Linear Interpolation

Returns `a + (b - a) * t`. When `t = 0` returns `a`, when `t = 1` returns `b`.

```css
/* Opacity: 30% at t=0, 100% at t=1 */
opacity: calc(mix(0.3, 1.0, env(pointer-inside)));

/* Border-radius between 4px and 48px */
border-radius: calc(mix(4, 48, env(pointer-inside)) * 1px);
```

#### `smoothstep(edge0, edge1, x)` — Hermite Interpolation

Returns a smooth 0→1 S-curve. Result is 0 when `x <= edge0`, 1 when `x >= edge1`, and smoothly interpolated between. Uses the Hermite formula: `t*t*(3 - 2*t)`.

When `edge0 > edge1`, the curve inverts — useful for proximity effects (1 when close, 0 when far):

```css
/* Fade in as pointer approaches (distance 1.8→0 maps to opacity 0→1) */
opacity: calc(smoothstep(1.8, 0.0, env(pointer-distance)));

/* Sharp threshold at 0.5 (smoother than step) */
opacity: calc(smoothstep(0.4, 0.6, env(pointer-inside)));
```

#### `step(edge, x)` — Hard Threshold

Returns 0 if `x < edge`, 1 otherwise. Binary on/off switch:

```css
/* Fully visible or fully hidden */
opacity: calc(step(0.5, env(pointer-inside)));
```

#### `remap(val, in_lo, in_hi, out_lo, out_hi)` — Range Mapping

Linearly maps `val` from one range to another:

```css
/* Map pointer-x from [-1, 1] to [10, 50] for border-radius */
border-radius: calc(remap(env(pointer-x), -1, 1, 10, 50) * 1px);
```

### Environment Variables

`env()` references resolve to per-frame dynamic values. Currently used by the [pointer query](../advanced/pointer-query.md) system:

```css
#card {
    pointer-space: self;
    rotate-y: calc(env(pointer-x) * 25deg);
    opacity: calc(mix(0.3, 1.0, env(pointer-inside)));
}
```

See the [Pointer Query](../advanced/pointer-query.md) chapter for the full list of `env(pointer-*)` variables.

### Percentage Values

`%` in calc resolves against the parent dimension (width for horizontal properties, height for vertical):

```css
width: calc(50% - 20px);
margin-left: calc(100% / 6);
```

---

## Error Handling

The CSS parser is resilient — it collects errors without stopping:

```rust
let result = Stylesheet::parse_with_errors(css);

if result.has_errors() {
    result.print_colored_diagnostics();  // Pretty-printed terminal output
    result.print_summary();
}

// Valid properties are still parsed!
let style = result.stylesheet.get("card").unwrap();
```

Individual errors include line/column information:

```rust
for error in &result.errors {
    println!("Line {}, Col {}: {} (property: {:?})",
        error.line, error.column, error.message, error.property);
}
```

---

## Scoped Style Macros

Blinc provides two compile-time macros — `css!` and `style!` — for building `ElementStyle` values directly in Rust. These are ideal for programmatic, scoped styling where you need dynamic values, conditional logic, or simply don't want a global stylesheet.

Both macros produce the same `ElementStyle` type and support all the same properties. The difference is syntax:

| | `css!` | `style!` |
|---|---|---|
| **Naming** | CSS hyphens (`border-radius`) | Rust underscores (`rounded`) |
| **Separator** | Semicolons | Commas |
| **Enum values** | Literal keywords (`position: absolute;`) | Rust expressions (`position: StylePosition::Absolute`) |
| **Best for** | Developers from CSS/web | Rust-native code |

### Quick Example

```rust
use blinc_layout::prelude::*;
use blinc_core::Color;

// CSS-style syntax
let card = css! {
    background: Color::WHITE;
    border-radius: 12.0;
    box-shadow: lg;
    opacity: 0.95;
    padding: 24.0;
};

// Equivalent Rust-style syntax
let card = style! {
    bg: Color::WHITE,
    rounded: 12.0,
    shadow_lg,
    opacity: 0.95,
    p: 24.0,
};
```

Apply a macro style to a `Div` with `.style()`:

```rust
div().style(css! {
    background: Color::rgb(0.1, 0.1, 0.15);
    border-radius: 16.0;
    padding: 20.0;
})
```

### Visual Properties

```rust
// css! macro
let s = css! {
    background: Color::BLUE;
    border-radius: 8.0;
    box-shadow: md;                    // Presets: sm, md, lg, xl, none
    box-shadow: my_shadow;             // Or a Shadow value
    opacity: 0.8;
    clip-path: my_clip_path;           // ClipPath value
    filter: my_filter;                 // CssFilter value
    mask-image: my_mask;               // MaskImage value
    mask-mode: blinc_core::MaskMode::Alpha;
    mix-blend-mode: blinc_core::BlendMode::Overlay;
};

// style! macro
let s = style! {
    bg: Color::BLUE,
    rounded: 8.0,
    shadow_md,                         // Presets as bare keywords
    opacity: 0.8,
    clip_path: my_clip_path,
    filter: my_filter,
    mask_image: my_mask,
    mask_gradient: my_gradient,        // Gradient mask shorthand
    mask_mode: blinc_core::MaskMode::Alpha,
    mix_blend_mode: blinc_core::BlendMode::Overlay,
};
```

#### Corner Radius Presets (style! only)

```rust
let s = style! {
    rounded_sm,     // 2.0
    rounded_md,     // 6.0
    rounded_lg,     // 8.0
    rounded_xl,     // 12.0
    rounded_2xl,    // 16.0
    rounded_full,   // 9999.0 (pill shape)
    rounded_none,   // 0.0
};

// Per-corner control
let s = style! {
    rounded_corners: (12.0, 12.0, 0.0, 0.0),  // top-left, top-right, bottom-right, bottom-left
};
```

#### Shadow Presets (style! only)

```rust
let s = style! { shadow_sm };   // Small, subtle shadow
let s = style! { shadow_md };   // Medium (default card shadow)
let s = style! { shadow_lg };   // Large, elevated
let s = style! { shadow_xl };   // Extra large, floating
let s = style! { shadow_none }; // Remove shadow
```

#### Opacity Presets (style! only)

```rust
let s = style! { opaque };      // 1.0
let s = style! { translucent }; // 0.5
let s = style! { transparent }; // 0.0
```

### Text Properties

```rust
// css! macro
let s = css! {
    color: Color::WHITE;
    font-size: 16.0;
    font-weight: FontWeight::Bold;
    text-decoration: TextDecoration::Underline;
    text-decoration-color: Color::RED;
    text-decoration-thickness: 2.0;
    line-height: 1.5;
    text-align: center;                // Keywords: left, center, right
    letter-spacing: 0.5;
    text-shadow: my_shadow;
    text-overflow: ellipsis;           // Keywords: clip, ellipsis
    white-space: nowrap;               // Keywords: normal, nowrap, pre
};

// style! macro
let s = style! {
    text_color: Color::WHITE,
    font_size: 16.0,
    font_weight: FontWeight::Bold,
    text_decoration: TextDecoration::Underline,
    text_decoration_color: Color::RED,
    text_decoration_thickness: 2.0,
    line_height: 1.5,
    text_align: TextAlign::Center,
    letter_spacing: 0.5,
    text_shadow: my_shadow,
    text_overflow: TextOverflow::Ellipsis,
    white_space: WhiteSpace::Nowrap,
};
```

### Transforms

```rust
// css! macro — function syntax
let s = css! {
    transform: scale(1.05);
    transform: scale(1.5, 0.8);       // Non-uniform
    transform: translate(10.0, 20.0);
    transform: rotate(45.0);
    transform: skewX(15.0);
    transform: skewY(10.0);
    transform-origin: (50.0, 50.0);   // Percentages
};

// css! macro — expression syntax
let s = css! {
    transform: my_transform;          // A Transform value
};

// style! macro — dedicated properties
let s = style! {
    scale: 1.05,
    scale_xy: (1.5, 0.8),
    translate: (10.0, 20.0),
    rotate_deg: 45.0,
    skew_x: 15.0,
    skew_y: 10.0,
    transform_origin: (50.0, 50.0),
};
```

### 3D Properties

```rust
// css! macro
let s = css! {
    rotate-x: 30.0;
    rotate-y: 45.0;
    perspective: 800.0;
    translate-z: 20.0;
    shape-3d: "sphere";
    depth: 120.0;
    light-direction: (0.0, -1.0, 0.5);
    light-intensity: 1.5;
    ambient: 0.3;
    specular: 64.0;
    3d-op: "subtract";
    3d-blend: 30.0;
};

// style! macro
let s = style! {
    rotate_x: 30.0,
    rotate_y: 45.0,
    perspective: 800.0,
    translate_z: 20.0,
    shape_3d: "sphere",
    depth: 120.0,
    light_direction: (0.0, -1.0, 0.5),
    light_intensity: 1.5,
    ambient: 0.3,
    specular: 64.0,
    op_3d: "subtract",
    blend_3d: 30.0,
};
```

### Layout Properties

```rust
// css! macro
let s = css! {
    width: 300.0;
    height: 200.0;
    min-width: 100.0;
    max-width: 600.0;
    padding: 24.0;
    margin: 16.0;
    gap: 12.0;
    display: flex;                     // flex | block | none
    flex-direction: column;            // row | column | row-reverse | column-reverse
    flex-wrap: wrap;
    flex-grow: 1.0;
    flex-shrink: 0.0;
    align-items: center;              // center | start | end | stretch | baseline
    justify-content: space-between;   // center | start | end | space-between | space-around | space-evenly
    align-self: end;                  // center | start | end | stretch | baseline
    overflow: clip;                    // clip | hidden | visible | scroll
    overflow-x: scroll;
    overflow-y: hidden;
};

// style! macro
let s = style! {
    w: 300.0,
    h: 200.0,
    min_w: 100.0,
    max_w: 600.0,
    p: 24.0,
    p_xy: (16.0, 24.0),               // Horizontal, vertical
    m: 16.0,
    m_xy: (8.0, 16.0),
    gap: 12.0,
    flex_col,                          // Bare keyword presets
    flex_wrap,
    flex_grow,                         // Default = 1.0
    flex_grow_value: 2.0,              // Specific value
    flex_shrink_0,                     // flex-shrink: 0
    flex_shrink: 0.5,                  // Specific value
    items_center,
    justify_between,
    self_end,
    overflow_clip,
    overflow_x: StyleOverflow::Scroll,
    overflow_y: StyleOverflow::Clip,
    display_none,
    display_block,
};
```

### Position & Inset

```rust
// css! macro — keyword values
let s = css! {
    position: absolute;               // static | relative | absolute | fixed | sticky
    top: 10.0;
    right: 0.0;
    bottom: 0.0;
    left: 10.0;
    inset: 0.0;                       // Sets all four sides
    z-index: 5;
    visibility: hidden;               // visible | hidden
};

// style! macro — expression values
let s = style! {
    position: StylePosition::Absolute,
    top: 10.0,
    inset: 0.0,
    z_index: 5,
    visibility: StyleVisibility::Hidden,
};
```

### Border & Outline

```rust
// css! macro
let s = css! {
    border: (2.0, Color::RED);        // Shorthand (width, color)
    border-width: 2.0;
    border-color: Color::RED;
    outline: (3.0, Color::BLUE);
    outline-width: 3.0;
    outline-color: Color::BLUE;
    outline-offset: 4.0;
};

// style! macro
let s = style! {
    border: (2.0, Color::RED),
    border_width: 2.0,
    border_color: Color::RED,
    outline: (3.0, Color::BLUE),
    outline_width: 3.0,
    outline_color: Color::BLUE,
    outline_offset: 4.0,
};
```

### Materials & Layers

```rust
// css! macro — keyword presets
let s = css! {
    backdrop-filter: glass;           // glass | metallic | chrome | gold | wood
    render-layer: foreground;         // foreground | background
};

// style! macro — bare keyword presets
let s = style! {
    glass,                            // Also: metallic, chrome, gold, wood
    foreground,
};

// Custom material via expression
let s = style! {
    material: my_material,
    layer: my_layer,
};
```

### Animation & Transition

```rust
// css! macro
let s = css! {
    animation: my_animation;           // CssAnimation value
    animation-name: "pulse";
    animation-duration: 2000;          // milliseconds
    animation-delay: 100;
    animation-timing-function: AnimationTiming::EaseInOut;
    animation-iteration-count: 0;      // 0 = infinite
    animation-direction: AnimationDirection::Alternate;
    animation-fill-mode: AnimationFillMode::Forwards;
    transition: my_transition;         // CssTransitionSet value
};

// style! macro
let s = style! {
    animation: my_animation,
    animation_name: "pulse",
    animation_duration: 2000,
    transition: my_transition,
};
```

### SVG Properties

```rust
// css! macro
let s = css! {
    fill: Color::RED;
    stroke: Color::BLUE;
    stroke-width: 2.0;
    stroke-dasharray: vec![5.0, 3.0];
    stroke-dashoffset: 10.0;
};

// style! macro
let s = style! {
    fill: Color::RED,
    stroke: Color::BLUE,
    stroke_width: 2.0,
    stroke_dasharray: vec![5.0, 3.0],
    stroke_dashoffset: 10.0,
    svg_path_data: "M10,80 L50,20 L90,80",
};
```

### Form & Interaction Properties

```rust
// css! macro
let s = css! {
    caret-color: Color::rgb(0.4, 0.6, 1.0);
    selection-color: Color::BLUE;
    placeholder-color: Color::rgba(1.0, 1.0, 1.0, 0.5);
    accent-color: Color::GREEN;
    scrollbar-color: (Color::rgb(0.5, 0.5, 0.5), Color::rgb(0.2, 0.2, 0.2));
    scrollbar-width: thin;            // auto | thin | none
    pointer-events: none;             // auto | none
    cursor: CursorStyle::Pointer;
};

// style! macro
let s = style! {
    caret_color: Color::rgb(0.4, 0.6, 1.0),
    accent_color: Color::GREEN,
    scrollbar_color: (Color::rgb(0.5, 0.5, 0.5), Color::rgb(0.2, 0.2, 0.2)),
    scrollbar_width: ScrollbarWidth::Thin,
    pointer_events_none,              // Preset keyword
    cursor: CursorStyle::Pointer,
};
```

### Image Properties

```rust
// css! macro (0=cover, 1=contain, 2=fill, 3=scale-down, 4=none)
let s = css! {
    object-fit: 1;
    object-position: (0.5, 0.0);     // x, y in 0.0-1.0 range
};

// style! macro
let s = style! {
    object_fit: 1,
    object_position: (0.5, 0.0),
};
```

### Conditional & Dynamic Styling

The macros shine when combined with Rust control flow:

```rust
fn card_style(is_selected: bool, scale: f32) -> ElementStyle {
    let mut s = css! {
        background: Color::WHITE;
        border-radius: 12.0;
        padding: 16.0;
    };

    if is_selected {
        s = s.merge(&css! {
            border: (2.0, Color::BLUE);
            box-shadow: lg;
        });
    }

    // Dynamic transform
    s = s.scale(scale);
    s
}
```

A class can also follow a signal. `.class_when(name, signal)` adds the
class while the signal holds; the stylesheet's rules for it apply and stop
applying in place, with no rebuild:

```rust
let selected = signal(false);

div()
    .class("card")
    .class_when("card--selected", selected)
    .on_click(move |_| selected.update(|v| !v))
```

```css
.card { border: 1px solid #333; }
.card--selected { border: 2px solid #4a7dff; box-shadow: 0 8px 24px rgba(0,0,0,0.4); }
```

### When to Use Each Approach

| Approach | Best For |
|----------|----------|
| **Global CSS** (`ctx.add_css()`) | Shared styles, hover/focus states, animations, selectors |
| **`css!` / `style!` macros** | Scoped styles, dynamic values, conditional logic |
| **Builder API** (`.w()`, `.bg()`) | One-off overrides, inline on `Div` builders |

The three approaches compose naturally — CSS provides base styles, macros add scoped overrides, and builder methods fine-tune individual elements.

---

## How It Works

Understanding the CSS pipeline helps debug styling issues.

### The Three Styling Approaches

Blinc offers three ways to style elements, in increasing specificity:

1. **Global stylesheet** — `ctx.add_css()` + CSS selectors (recommended for most styling)
2. **Scoped macros** — `css!` / `style!` macros for inline ElementStyle
3. **Builder API** — `.w()`, `.h()`, `.bg()` etc. for direct property setting

All three can be combined. CSS provides base styles; builder methods add dynamic values.

### CSS Pipeline

```text
CSS Text
  ↓  ctx.add_css() / Stylesheet::parse_with_errors()
Stylesheet (parsed selectors + ElementStyle rules)
  ↓  apply_stylesheet_base_styles()
RenderProps (GPU-ready properties on each element)
  ↓  State changes (hover, focus, checked)
  ↓  apply_stylesheet_state_styles()
  ↓  Transition/animation detection & interpolation
  ↓  apply_animated_layout_props() + compute_layout()
GPU Rendering (SDF shader, image shader, text pipeline)
```

### Frame Loop Order

Each frame, CSS processing happens in this order:

1. **Tree build** — Elements are created, `RenderProps` initialized
2. **Base styles** — Non-state CSS rules applied (complex selectors first, then ID selectors for higher specificity)
3. **Layout overrides** — CSS layout properties (width, padding, gap, etc.) modify the flexbox tree
4. **Layout computation** — Flexbox layout calculated via Taffy
5. **State styles** — Hover/focus/checked states matched, transitions detected
6. **Animation tick** — CSS `@keyframes` animations advance
7. **Transition tick** — CSS transitions interpolate toward target
8. **Layout animation** — If animated properties affect layout, re-compute flexbox
9. **Render** — Final RenderProps sent to GPU

### Specificity

Rules follow CSS specificity, applied in order:

1. Type/class/combinator selectors (lowest)
2. ID selectors (highest)
3. Later rules override earlier rules at the same specificity level
4. State styles (`:hover`, etc.) layer on top of base styles

---

## Property Comparison

The same property expressed across all three approaches:

| Global CSS | `css!` macro | `style!` macro | Builder API |
| --- | --- | --- | --- |
| `background: #3498db;` | `background: Color::hex(0x3498db);` | `bg: Color::hex(0x3498db),` | `.bg(Color::hex(0x3498db))` |
| `border-radius: 8px;` | `border-radius: 8.0;` | `rounded: 8.0,` | `.rounded(8.0)` |
| `transform: scale(1.02);` | `transform: scale(1.02);` | `scale: 1.02,` | `.scale(1.02)` |
| `opacity: 0.8;` | `opacity: 0.8;` | `opacity: 0.8,` | `.opacity(0.8)` |
| `width: 200px;` | `width: 200.0;` | `w: 200.0,` | `.w(200.0)` |
| `padding: 16px;` | `padding: 16.0;` | `p: 16.0,` | `.p(16.0)` |
| `gap: 12px;` | `gap: 12.0;` | `gap: 12.0,` | `.gap(12.0)` |
| `flex-direction: column;` | `flex-direction: column;` | `flex_col,` | `.flex_col()` |
| `color: #fff;` | `color: Color::WHITE;` | `text_color: Color::WHITE,` | `.text_color(Color::WHITE)` |
| `font-size: 16px;` | `font-size: 16.0;` | `font_size: 16.0,` | `.font_size(16.0)` |
| `position: absolute;` | `position: absolute;` | `position: StylePosition::Absolute,` | `.position(StylePosition::Absolute)` |
| `fill: red;` | `fill: Color::RED;` | `fill: Color::RED,` | `.fill(Color::RED)` |
| `pointer-events: none;` | `pointer-events: none;` | `pointer_events_none,` | `.pointer_events_none()` |

All three approaches can be combined — CSS provides base styles, macros add scoped overrides, and builder methods fine-tune individual elements.
