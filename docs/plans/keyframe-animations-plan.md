# Keyframe Animations Integration Plan

## Overview

This plan outlines the integration of keyframe animations for element entry/exit animations and demonstrates keyframe animation usage with the canvas element.

## Current State

The animation system already has solid foundations:
- **Spring physics** - RK4 integration, velocity inheritance, presets (wobbly, stiff, snappy)
- **KeyframeAnimation** - Basic keyframe support with easing functions
- **AnimationScheduler** - Background/foreground scheduling with SlotMap storage
- **Timeline** - Multi-animation orchestration with looping
- **AnimatedValue/AnimatedKeyframe** - High-level wrappers
- **Canvas** - Direct GPU drawing with animation callback support

## Goals

1. **Entry/Exit Animations** - Declarative API for animating elements as they appear/disappear
2. **Keyframe Canvas Demo** - Demonstrate keyframe animations with canvas element
3. **Animation Presets** - Common animation patterns (fadeIn, slideIn, scale, bounce)
4. **Lifecycle Integration** - Hook animations into element mount/unmount cycle

---

## Phase 1: Keyframe Animation Enhancements

### 1.1 Multi-Property Keyframes

Extend keyframe system to animate multiple properties simultaneously.

**New Types in `blinc_animation/src/keyframe.rs`:**

```rust
/// A keyframe with multiple animated properties
pub struct MultiKeyframe {
    pub time: f32,  // 0.0 to 1.0
    pub properties: KeyframeProperties,
    pub easing: Easing,
}

/// Properties that can be animated in a keyframe
#[derive(Clone, Default)]
pub struct KeyframeProperties {
    pub opacity: Option<f32>,
    pub scale_x: Option<f32>,
    pub scale_y: Option<f32>,
    pub translate_x: Option<f32>,
    pub translate_y: Option<f32>,
    pub rotate: Option<f32>,  // degrees
}

/// Multi-property keyframe animation
pub struct MultiKeyframeAnimation {
    duration_ms: u32,
    keyframes: Vec<MultiKeyframe>,
    current_time: f32,
    playing: bool,
    direction: PlayDirection,
    fill_mode: FillMode,
}

pub enum PlayDirection {
    Forward,
    Reverse,
    Alternate,  // Forward then reverse
}

pub enum FillMode {
    None,      // Reset to initial after animation
    Forwards,  // Hold final value
    Backwards, // Apply initial value before start
    Both,
}
```

### 1.2 Animation Presets

**New file: `blinc_animation/src/presets.rs`**

```rust
/// Pre-built animation presets for common patterns
pub struct AnimationPreset;

impl AnimationPreset {
    /// Fade in from transparent
    pub fn fade_in(duration_ms: u32) -> MultiKeyframeAnimation {
        MultiKeyframeAnimation::new(duration_ms)
            .keyframe(0.0, KeyframeProperties { opacity: Some(0.0), ..default() }, Easing::EaseOut)
            .keyframe(1.0, KeyframeProperties { opacity: Some(1.0), ..default() }, Easing::EaseOut)
    }

    /// Fade out to transparent
    pub fn fade_out(duration_ms: u32) -> MultiKeyframeAnimation {
        MultiKeyframeAnimation::new(duration_ms)
            .keyframe(0.0, KeyframeProperties { opacity: Some(1.0), ..default() }, Easing::EaseIn)
            .keyframe(1.0, KeyframeProperties { opacity: Some(0.0), ..default() }, Easing::EaseIn)
    }

    /// Scale up from small
    pub fn scale_in(duration_ms: u32) -> MultiKeyframeAnimation {
        MultiKeyframeAnimation::new(duration_ms)
            .keyframe(0.0, KeyframeProperties {
                scale_x: Some(0.0),
                scale_y: Some(0.0),
                opacity: Some(0.0),
                ..default()
            }, Easing::EaseOutCubic)
            .keyframe(1.0, KeyframeProperties {
                scale_x: Some(1.0),
                scale_y: Some(1.0),
                opacity: Some(1.0),
                ..default()
            }, Easing::EaseOutCubic)
    }

    /// Slide in from left
    pub fn slide_in_left(duration_ms: u32, distance: f32) -> MultiKeyframeAnimation {
        MultiKeyframeAnimation::new(duration_ms)
            .keyframe(0.0, KeyframeProperties {
                translate_x: Some(-distance),
                opacity: Some(0.0),
                ..default()
            }, Easing::EaseOutCubic)
            .keyframe(1.0, KeyframeProperties {
                translate_x: Some(0.0),
                opacity: Some(1.0),
                ..default()
            }, Easing::EaseOutCubic)
    }

    /// Slide in from right
    pub fn slide_in_right(duration_ms: u32, distance: f32) -> MultiKeyframeAnimation;

    /// Slide in from top
    pub fn slide_in_top(duration_ms: u32, distance: f32) -> MultiKeyframeAnimation;

    /// Slide in from bottom
    pub fn slide_in_bottom(duration_ms: u32, distance: f32) -> MultiKeyframeAnimation;

    /// Bounce in with overshoot
    pub fn bounce_in(duration_ms: u32) -> MultiKeyframeAnimation {
        MultiKeyframeAnimation::new(duration_ms)
            .keyframe(0.0, KeyframeProperties {
                scale_x: Some(0.0),
                scale_y: Some(0.0),
                ..default()
            }, Easing::Linear)
            .keyframe(0.6, KeyframeProperties {
                scale_x: Some(1.1),
                scale_y: Some(1.1),
                ..default()
            }, Easing::EaseOut)
            .keyframe(0.8, KeyframeProperties {
                scale_x: Some(0.95),
                scale_y: Some(0.95),
                ..default()
            }, Easing::EaseInOut)
            .keyframe(1.0, KeyframeProperties {
                scale_x: Some(1.0),
                scale_y: Some(1.0),
                ..default()
            }, Easing::EaseOut)
    }

    /// Shake horizontally (for error feedback)
    pub fn shake(duration_ms: u32, intensity: f32) -> MultiKeyframeAnimation;

    /// Pulse (scale up and down)
    pub fn pulse(duration_ms: u32) -> MultiKeyframeAnimation;

    /// Spin rotation
    pub fn spin(duration_ms: u32) -> MultiKeyframeAnimation;
}
```

---

## Phase 2: Element Entry/Exit Animations

### 2.1 Motion Container Element

**New file: `blinc_layout/src/motion.rs`**

A style-less container that only applies animations to its child:

```rust
/// Style-less motion container
///
/// Wraps a child element and applies entry/exit animations without
/// adding any visual styling of its own.
pub struct Motion {
    child: Option<Box<dyn ElementBuilder>>,
    enter: Option<ElementAnimation>,
    exit: Option<ElementAnimation>,
}

/// Create a motion container
pub fn motion() -> Motion {
    Motion {
        child: None,
        enter: None,
        exit: None,
    }
}

impl Motion {
    /// Set the child element to animate
    pub fn child(mut self, child: impl ElementBuilder + 'static) -> Self {
        self.child = Some(Box::new(child));
        self
    }

    /// Set animation to play when element enters the tree
    pub fn enter_animation(mut self, animation: impl Into<ElementAnimation>) -> Self {
        self.enter = Some(animation.into());
        self
    }

    /// Set animation to play when element exits the tree
    pub fn exit_animation(mut self, animation: impl Into<ElementAnimation>) -> Self {
        self.exit = Some(animation.into());
        self
    }

    /// Convenience: fade in on enter
    pub fn fade_in(self, duration_ms: u32) -> Self {
        self.enter_animation(AnimationPreset::fade_in(duration_ms))
    }

    /// Convenience: fade out on exit
    pub fn fade_out(self, duration_ms: u32) -> Self {
        self.exit_animation(AnimationPreset::fade_out(duration_ms))
    }

    /// Convenience: slide in from direction
    pub fn slide_in(self, direction: SlideDirection, duration_ms: u32) -> Self {
        let anim = match direction {
            SlideDirection::Left => AnimationPreset::slide_in_left(duration_ms, 50.0),
            SlideDirection::Right => AnimationPreset::slide_in_right(duration_ms, 50.0),
            SlideDirection::Top => AnimationPreset::slide_in_top(duration_ms, 50.0),
            SlideDirection::Bottom => AnimationPreset::slide_in_bottom(duration_ms, 50.0),
        };
        self.enter_animation(anim)
    }

    /// Convenience: slide out to direction
    pub fn slide_out(self, direction: SlideDirection, duration_ms: u32) -> Self {
        let anim = match direction {
            SlideDirection::Left => AnimationPreset::slide_out_left(duration_ms, 50.0),
            SlideDirection::Right => AnimationPreset::slide_out_right(duration_ms, 50.0),
            SlideDirection::Top => AnimationPreset::slide_out_top(duration_ms, 50.0),
            SlideDirection::Bottom => AnimationPreset::slide_out_bottom(duration_ms, 50.0),
        };
        self.exit_animation(anim)
    }

    /// Convenience: bounce in on enter
    pub fn bounce_in(self, duration_ms: u32) -> Self {
        self.enter_animation(AnimationPreset::bounce_in(duration_ms))
    }

    /// Convenience: scale in on enter
    pub fn scale_in(self, duration_ms: u32) -> Self {
        self.enter_animation(AnimationPreset::scale_in(duration_ms))
    }

    /// Convenience: scale out on exit
    pub fn scale_out(self, duration_ms: u32) -> Self {
        self.exit_animation(AnimationPreset::scale_out(duration_ms))
    }

    /// Enable stagger animations for multiple children
    pub fn stagger(mut self, config: StaggerConfig) -> Self {
        self.stagger_config = Some(config);
        self
    }

    /// Add multiple children with stagger animation support
    pub fn children(mut self, children: impl IntoIterator<Item = impl ElementBuilder + 'static>) -> Self {
        self.children = children.into_iter()
            .map(|c| Box::new(c) as Box<dyn ElementBuilder>)
            .collect();
        self
    }
}

/// Configuration for stagger animations
#[derive(Clone)]
pub struct StaggerConfig {
    /// Delay between each child's animation start (ms)
    pub delay: u32,
    /// Animation to apply to each child
    pub animation: ElementAnimation,
    /// Direction of stagger (first-to-last or last-to-first)
    pub direction: StaggerDirection,
    /// Optional: limit stagger to first N items, then animate rest together
    pub limit: Option<usize>,
}

impl StaggerConfig {
    /// Create a new stagger config with delay between items
    pub fn new(delay_ms: u32, animation: impl Into<ElementAnimation>) -> Self {
        Self {
            delay: delay_ms,
            animation: animation.into(),
            direction: StaggerDirection::Forward,
            limit: None,
        }
    }

    /// Stagger from last to first
    pub fn reverse(mut self) -> Self {
        self.direction = StaggerDirection::Reverse;
        self
    }

    /// Stagger from center outward
    pub fn from_center(mut self) -> Self {
        self.direction = StaggerDirection::FromCenter;
        self
    }

    /// Limit stagger to first N items
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }
}

pub enum StaggerDirection {
    Forward,      // First to last
    Reverse,      // Last to first
    FromCenter,   // Center outward
}

pub enum SlideDirection {
    Left,
    Right,
    Top,
    Bottom,
}
```

### 2.2 Div Animation Methods

**Extend `blinc_layout/src/div.rs`:**

Div elements can also have animations directly (for cases where a wrapper isn't desired):

```rust
impl Div {
    /// Set animation to play when element enters the tree
    pub fn enter_animation(mut self, animation: impl Into<ElementAnimation>) -> Self {
        self.style.enter_animation = Some(animation.into());
        self
    }

    /// Set animation to play when element exits the tree
    pub fn exit_animation(mut self, animation: impl Into<ElementAnimation>) -> Self {
        self.style.exit_animation = Some(animation.into());
        self
    }

    /// Convenience: fade in on enter
    pub fn fade_in(self, duration_ms: u32) -> Self {
        self.enter_animation(AnimationPreset::fade_in(duration_ms))
    }

    /// Convenience: fade out on exit
    pub fn fade_out(self, duration_ms: u32) -> Self {
        self.exit_animation(AnimationPreset::fade_out(duration_ms))
    }
}
```

### 2.2 ElementAnimation Wrapper

**New file: `blinc_layout/src/element_animation.rs`**

```rust
/// Animation configuration for element lifecycle
pub struct ElementAnimation {
    pub animation: MultiKeyframeAnimation,
    pub delay_ms: u32,
    pub on_complete: Option<Box<dyn Fn()>>,
}

impl From<MultiKeyframeAnimation> for ElementAnimation {
    fn from(animation: MultiKeyframeAnimation) -> Self {
        Self {
            animation,
            delay_ms: 0,
            on_complete: None,
        }
    }
}

impl ElementAnimation {
    pub fn with_delay(mut self, delay_ms: u32) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    pub fn on_complete<F: Fn() + 'static>(mut self, f: F) -> Self {
        self.on_complete = Some(Box::new(f));
        self
    }
}
```

### 2.3 RenderTree Animation Tracking

**Extend `blinc_layout/src/renderer.rs`:**

```rust
pub struct RenderTree {
    // ... existing fields ...

    /// Active entry animations (node_id -> animation state)
    entering_animations: HashMap<LayoutNodeId, ActiveAnimation>,

    /// Active exit animations (node_id -> animation state + cached render data)
    exiting_animations: HashMap<LayoutNodeId, ExitingElement>,
}

struct ActiveAnimation {
    animation: MultiKeyframeAnimation,
    started_at: u64,  // timestamp ms
}

struct ExitingElement {
    animation: ActiveAnimation,
    cached_bounds: Rect,
    cached_style: Style,
    // Render data cached at time of exit
}

impl RenderTree {
    /// Called during tree diff when new nodes appear
    fn on_node_enter(&mut self, node_id: LayoutNodeId) {
        if let Some(enter_anim) = self.get_enter_animation(node_id) {
            self.entering_animations.insert(node_id, ActiveAnimation {
                animation: enter_anim,
                started_at: elapsed_ms(),
            });
        }
    }

    /// Called during tree diff when nodes are removed
    fn on_node_exit(&mut self, node_id: LayoutNodeId) {
        if let Some(exit_anim) = self.get_exit_animation(node_id) {
            // Cache render data before node is removed
            let cached = self.cache_node_for_exit(node_id);
            self.exiting_animations.insert(node_id, ExitingElement {
                animation: ActiveAnimation {
                    animation: exit_anim,
                    started_at: elapsed_ms(),
                },
                cached_bounds: cached.bounds,
                cached_style: cached.style,
            });
        }
    }

    /// Tick all active animations, returns true if any are still running
    pub fn tick_animations(&mut self, current_time: u64) -> bool {
        let mut any_active = false;

        // Tick entry animations
        self.entering_animations.retain(|_, anim| {
            let elapsed = current_time - anim.started_at;
            anim.animation.tick(elapsed as f32);
            let playing = anim.animation.is_playing();
            any_active |= playing;
            playing
        });

        // Tick exit animations
        self.exiting_animations.retain(|_, elem| {
            let elapsed = current_time - elem.animation.started_at;
            elem.animation.animation.tick(elapsed as f32);
            let playing = elem.animation.animation.is_playing();
            any_active |= playing;
            playing
        });

        any_active
    }

    /// Get animated transform for a node (entry animation)
    pub fn get_animation_transform(&self, node_id: LayoutNodeId) -> Option<AnimatedTransform> {
        self.entering_animations.get(&node_id).map(|anim| {
            AnimatedTransform::from_keyframe_properties(anim.animation.current_properties())
        })
    }
}

pub struct AnimatedTransform {
    pub opacity: f32,
    pub scale: (f32, f32),
    pub translate: (f32, f32),
    pub rotate: f32,
}
```

---

## Phase 3: Canvas Keyframe Demo

### 3.1 Example: Animated Loading Spinner

**New file: `blinc_app/examples/keyframe_canvas.rs`**

```rust
//! Keyframe Animation Canvas Demo
//!
//! Demonstrates keyframe animations with the canvas element for:
//! - Loading spinner with rotation keyframes
//! - Pulsing dots animation
//! - Progress bar with eased fill
//! - Bouncing logo animation
//!
//! Run with: cargo run -p blinc_app --example keyframe_canvas --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_animation::{
    AnimatedKeyframe, Keyframe, Easing, KeyframeAnimation,
    AnimatedTimeline, Timeline,
};
use blinc_core::Color;
use std::cell::RefCell;
use std::rc::Rc;
use std::f32::consts::PI;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Keyframe Canvas Animations".to_string(),
        width: 800,
        height: 600,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::rgba(0.1, 0.1, 0.15, 1.0))
        .flex_col()
        .gap(20.0)
        .p(40.0)
        .items_center()
        .child(
            text("Keyframe Canvas Animations")
                .size(32.0)
                .weight(FontWeight::Bold)
                .color(Color::WHITE),
        )
        .child(
            div()
                .flex_row()
                .gap(30.0)
                .flex_wrap()
                .justify_center()
                .child(spinning_loader_demo(ctx))
                .child(pulsing_dots_demo(ctx))
                .child(progress_bar_demo(ctx))
                .child(bouncing_logo_demo(ctx)),
        )
}

/// Demo 1: Spinning loader using rotation keyframes
fn spinning_loader_demo(ctx: &WindowedContext) -> Div {
    // Create a looping rotation animation
    let rotation = Rc::new(RefCell::new({
        let mut timeline = AnimatedTimeline::new(ctx.animation_handle());
        let entry = timeline.add(0, 1000, 0.0, 360.0);  // 1 second full rotation
        timeline.set_loop(-1);  // Infinite loop
        timeline.start();
        (timeline, entry)
    }));

    let render_rotation = Rc::clone(&rotation);

    demo_card("Spinning Loader")
        .child(
            canvas(move |ctx, bounds| {
                let (timeline, entry) = &*render_rotation.borrow();
                let angle_deg = timeline.get(*entry).unwrap_or(0.0);
                let angle_rad = angle_deg * PI / 180.0;

                let cx = bounds.width / 2.0;
                let cy = bounds.height / 2.0;
                let radius = 30.0;
                let thickness = 4.0;

                // Draw spinning arc
                let arc_length = PI * 1.5;  // 270 degrees

                // Calculate arc endpoints
                let start_angle = angle_rad;
                let end_angle = angle_rad + arc_length;

                // Draw arc using line segments
                let segments = 32;
                for i in 0..segments {
                    let t1 = i as f32 / segments as f32;
                    let t2 = (i + 1) as f32 / segments as f32;

                    let a1 = start_angle + t1 * arc_length;
                    let a2 = start_angle + t2 * arc_length;

                    let x1 = cx + radius * a1.cos();
                    let y1 = cy + radius * a1.sin();
                    let x2 = cx + radius * a2.cos();
                    let y2 = cy + radius * a2.sin();

                    ctx.stroke_line(
                        Point::new(x1, y1),
                        Point::new(x2, y2),
                        thickness,
                        Color::rgba(0.4, 0.8, 1.0, 1.0),
                    );
                }
            })
            .w(100.0)
            .h(100.0),
        )
}

/// Demo 2: Pulsing dots with staggered keyframes
fn pulsing_dots_demo(ctx: &WindowedContext) -> Div {
    // Create three dots with staggered pulse animations
    let dots: Vec<_> = (0..3).map(|i| {
        let mut timeline = AnimatedTimeline::new(ctx.animation_handle());
        // Stagger start by 200ms per dot
        let offset = i as i32 * 200;
        let scale_entry = timeline.add(offset, 600, 0.5, 1.0);
        let opacity_entry = timeline.add(offset, 600, 0.3, 1.0);
        timeline.set_loop(-1);
        timeline.start();
        Rc::new(RefCell::new((timeline, scale_entry, opacity_entry)))
    }).collect();

    let dots_clone = dots.clone();

    demo_card("Pulsing Dots")
        .child(
            canvas(move |ctx, bounds| {
                let cx = bounds.width / 2.0;
                let cy = bounds.height / 2.0;
                let dot_radius = 8.0;
                let spacing = 25.0;

                for (i, dot) in dots_clone.iter().enumerate() {
                    let (timeline, scale_entry, opacity_entry) = &*dot.borrow();
                    let scale = timeline.get(*scale_entry).unwrap_or(1.0);
                    let opacity = timeline.get(*opacity_entry).unwrap_or(1.0);

                    let x = cx + (i as f32 - 1.0) * spacing;
                    let r = dot_radius * scale;

                    ctx.fill_circle(
                        Point::new(x, cy),
                        r,
                        Color::rgba(0.4, 1.0, 0.8, opacity),
                    );
                }
            })
            .w(100.0)
            .h(100.0),
        )
}

/// Demo 3: Progress bar with eased fill animation
fn progress_bar_demo(ctx: &WindowedContext) -> Div {
    // Keyframe animation for progress (with ease-in-out)
    let progress = Rc::new(RefCell::new({
        let mut anim = AnimatedKeyframe::new(ctx.animation_handle(), 2000)
            .keyframe(0.0, 0.0, Easing::Linear)
            .keyframe(0.3, 0.1, Easing::EaseInCubic)
            .keyframe(0.7, 0.9, Easing::EaseOutCubic)
            .keyframe(1.0, 1.0, Easing::EaseOut)
            .build();
        anim.start();
        anim
    }));

    let render_progress = Rc::clone(&progress);
    let click_progress = Rc::clone(&progress);

    demo_card("Progress Bar")
        .child(
            canvas(move |ctx, bounds| {
                let progress_val = render_progress.borrow().get().unwrap_or(0.0);

                let bar_width = bounds.width - 20.0;
                let bar_height = 12.0;
                let bar_x = 10.0;
                let bar_y = (bounds.height - bar_height) / 2.0;

                // Background
                ctx.fill_rounded_rect(
                    Rect::new(bar_x, bar_y, bar_width, bar_height),
                    6.0,
                    Color::rgba(0.2, 0.2, 0.25, 1.0),
                );

                // Filled portion
                let fill_width = bar_width * progress_val;
                if fill_width > 0.0 {
                    ctx.fill_rounded_rect(
                        Rect::new(bar_x, bar_y, fill_width, bar_height),
                        6.0,
                        Color::rgba(0.4, 0.8, 1.0, 1.0),
                    );
                }

                // Percentage text
                let percent = (progress_val * 100.0) as i32;
                ctx.fill_text(
                    &format!("{}%", percent),
                    Point::new(bounds.width / 2.0, bar_y + bar_height + 15.0),
                    14.0,
                    Color::WHITE,
                );
            })
            .w(150.0)
            .h(60.0)
            .on_click(move |_| {
                // Restart animation on click
                click_progress.borrow_mut().start();
            }),
        )
        .child(
            text("Click to restart")
                .size(12.0)
                .color(Color::rgba(0.5, 0.5, 0.5, 1.0)),
        )
}

/// Demo 4: Bouncing logo with complex keyframes
fn bouncing_logo_demo(ctx: &WindowedContext) -> Div {
    // Bounce animation with squash and stretch
    let bounce = Rc::new(RefCell::new({
        let mut timeline = AnimatedTimeline::new(ctx.animation_handle());

        // Y position (bounce)
        let y_entry = timeline.add(0, 800, 0.0, 1.0);

        // Scale X (squash on impact)
        let scale_x_entry = timeline.add(0, 800, 1.0, 1.0);

        // Scale Y (stretch during fall, squash on impact)
        let scale_y_entry = timeline.add(0, 800, 1.0, 1.0);

        timeline.set_loop(-1);
        timeline.start();

        (timeline, y_entry, scale_x_entry, scale_y_entry)
    }));

    let render_bounce = Rc::clone(&bounce);

    demo_card("Bouncing Ball")
        .child(
            canvas(move |ctx, bounds| {
                let (timeline, y_entry, _, _) = &*render_bounce.borrow();

                // Get normalized bounce progress
                let t = timeline.get(*y_entry).unwrap_or(0.0);

                // Apply bounce easing (quadratic for gravity feel)
                let bounce_height = 50.0;
                let ground_y = bounds.height - 25.0;

                // Simple parabolic bounce
                let y = if t < 0.5 {
                    // Falling
                    let fall_t = t * 2.0;
                    ground_y - bounce_height * (1.0 - fall_t * fall_t)
                } else {
                    // Rising
                    let rise_t = (t - 0.5) * 2.0;
                    ground_y - bounce_height * (1.0 - (1.0 - rise_t) * (1.0 - rise_t))
                };

                // Squash/stretch based on velocity
                let (scale_x, scale_y) = if t < 0.45 || t > 0.55 {
                    // In air - slight stretch
                    (0.9, 1.1)
                } else {
                    // Near ground - squash
                    (1.2, 0.8)
                };

                let cx = bounds.width / 2.0;
                let radius = 15.0;

                // Draw shadow
                let shadow_scale = 1.0 - (ground_y - y) / bounce_height * 0.5;
                ctx.fill_ellipse(
                    Point::new(cx, ground_y + 5.0),
                    radius * shadow_scale,
                    radius * 0.3 * shadow_scale,
                    Color::rgba(0.0, 0.0, 0.0, 0.3 * shadow_scale),
                );

                // Draw ball with squash/stretch
                ctx.fill_ellipse(
                    Point::new(cx, y),
                    radius * scale_x,
                    radius * scale_y,
                    Color::rgba(1.0, 0.5, 0.3, 1.0),
                );
            })
            .w(100.0)
            .h(120.0),
        )
}

/// Helper to create a demo card
fn demo_card(title: &str) -> Div {
    div()
        .w(180.0)
        .flex_col()
        .gap(10.0)
        .p(16.0)
        .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
        .rounded(12.0)
        .items_center()
        .child(
            text(title)
                .size(14.0)
                .weight(FontWeight::SemiBold)
                .color(Color::WHITE),
        )
}
```

---

## Phase 4: Implementation Steps

### Step 1: Extend KeyframeAnimation (2-3 hours)
- [ ] Add `MultiKeyframe` and `KeyframeProperties` types
- [ ] Implement `MultiKeyframeAnimation` with property interpolation
- [ ] Add `PlayDirection` and `FillMode` support
- [ ] Update scheduler to handle multi-property keyframes

### Step 2: Create Animation Presets (1-2 hours)
- [ ] Create `presets.rs` module
- [ ] Implement fade_in, fade_out presets
- [ ] Implement scale_in, scale_out presets
- [ ] Implement slide_in variants (left, right, top, bottom)
- [ ] Implement bounce_in, shake, pulse, spin presets

### Step 3: Element Lifecycle Integration (3-4 hours)
- [ ] Create `motion()` container element in `blinc_layout/src/motion.rs`
- [ ] Add `enter_animation` and `exit_animation` to Style
- [ ] Extend Div with `enter_animation()`, `exit_animation()` methods
- [ ] Add convenience methods (`fade_in()`, `slide_in()`, etc.)
- [ ] Implement `StaggerConfig` and stagger animation support
- [ ] Add `.stagger()` and `.children()` methods to Motion
- [ ] Implement RenderTree animation tracking
- [ ] Handle exiting element caching and delayed removal

### Step 4: Canvas Keyframe Demo (2 hours)
- [ ] Create `keyframe_canvas.rs` example
- [ ] Implement spinning loader demo
- [ ] Implement pulsing dots demo
- [ ] Implement progress bar demo
- [ ] Implement bouncing ball demo

### Step 5: Testing & Documentation (1-2 hours)
- [ ] Write unit tests for multi-property keyframes
- [ ] Write integration tests for entry/exit animations
- [ ] Add documentation to animation modules
- [ ] Update CHANGELOG

---

## API Summary

### Keyframe Animation Usage

```rust
// Simple keyframe animation
let anim = AnimatedKeyframe::new(handle, 500)
    .keyframe(0.0, 0.0, Easing::EaseOut)
    .keyframe(1.0, 100.0, Easing::EaseOut)
    .build();
anim.start();
let value = anim.get();

// Multi-property keyframe
let anim = MultiKeyframeAnimation::new(300)
    .keyframe(0.0, KeyframeProperties {
        opacity: Some(0.0),
        scale_x: Some(0.8),
        scale_y: Some(0.8),
        ..default()
    }, Easing::EaseOutCubic)
    .keyframe(1.0, KeyframeProperties {
        opacity: Some(1.0),
        scale_x: Some(1.0),
        scale_y: Some(1.0),
        ..default()
    }, Easing::EaseOutCubic);
```

### Entry/Exit Animation Usage

```rust
// Using motion() container with presets
motion()
    .fade_in(300)
    .fade_out(200)
    .child(my_content)

// Using explicit animations on motion() container
motion()
    .enter_animation(AnimationPreset::slide_in_left(300, 50.0))
    .exit_animation(AnimationPreset::fade_out(200))
    .child(my_content)

// Direct on Div (without wrapper)
div()
    .enter_animation(AnimationPreset::bounce_in(400))
    .exit_animation(AnimationPreset::fade_out(200))
    .child(content)

// With delay and callback
motion()
    .enter_animation(
        AnimationPreset::bounce_in(400)
            .with_delay(100)
            .on_complete(|| println!("Animation done!"))
    )
    .child(content)
```

### Stagger Animation Usage

```rust
// Stagger a list of items with 50ms delay between each
motion()
    .stagger(StaggerConfig::new(50, AnimationPreset::fade_in(300)))
    .children(items.iter().map(|item| {
        div().child(text(item))
    }))

// Stagger with slide animation, reverse order
motion()
    .stagger(
        StaggerConfig::new(80, AnimationPreset::slide_in_left(400, 30.0))
            .reverse()
    )
    .children(menu_items)

// Stagger from center outward (great for grids)
motion()
    .stagger(
        StaggerConfig::new(30, AnimationPreset::scale_in(250))
            .from_center()
    )
    .children(grid_items)

// Limit stagger to first 5 items, rest animate together
motion()
    .stagger(
        StaggerConfig::new(100, AnimationPreset::fade_in(300))
            .limit(5)
    )
    .children(long_list)
```

### Canvas with Keyframes

```rust
let rotation = AnimatedTimeline::new(handle);
let entry = rotation.add(0, 1000, 0.0, 360.0);
rotation.set_loop(-1);
rotation.start();

canvas(move |ctx, bounds| {
    let angle = rotation.get(entry).unwrap_or(0.0);
    // Draw with animated rotation...
})
```

---

## Future Enhancements

1. **CSS-like Animation Strings** - Parse animation definitions from strings
2. **Animation Groups** - Coordinate multiple element animations
3. **Gesture-Driven Animations** - Connect swipe/drag to animation progress
4. **Shared Element Transitions** - Animate elements between different views
5. **Spring-Keyframe Hybrid** - Use springs for interruptible keyframe animations
