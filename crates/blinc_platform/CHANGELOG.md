# Changelog

All notable changes to `blinc_platform` will be documented in this file.

## [Unreleased]

## [0.6.0] - 2026-11-04

### Added
- `WindowConfig::max_frame_latency: u32` (default `2`, clamped `1..=3`) and the matching builder method. Caps how many frames the GPU is allowed to queue ahead of the currently-presented frame; lowering it halves the in-flight GPU memory pipelined for surfaces / command buffers / bind groups, at the cost of slightly higher input latency.
- `WindowConfig::animation_fps_cap: Option<u32>` (default `None`) and the matching builder method. Caps the redraw rate when the chain is firing only because of animation progress (CSS keyframe / transition / motion / theme / flow signals — never input, scroll, drag, or cursor). `None` keeps animation frames at native vsync (right for games / video / scrubbing UIs); `Some(30)` roughly halves wake-ups while a slow keyframe is on screen, with a small smoothness cost on fast animations. Backed by `WakeProxy::wake_at` on the desktop shim. Per-property smoothness override is automatic — the cap bypasses to native vsync when any visible animation is touching a transform / 3D rotation / layout sizing / font-size / clip-path property, so a screen mixing slow fades with a rotate-y keyframe gets each at its appropriate rate.
