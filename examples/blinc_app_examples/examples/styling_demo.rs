//! Unified Styling API Demo
//!
//! Demonstrates all styling approaches in Blinc:
//! - `css_style!` macro: CSS-like syntax with hyphenated property names
//! - `style!` macro: Rust-friendly syntax with underscored names
//! - `ElementStyle` builder: Programmatic style construction
//! - CSS Parser: Runtime CSS string parsing
//!
//! All approaches produce `ElementStyle` - a unified schema for visual properties.
//!
//! Run with: cargo run -p blinc_app_examples --example styling_demo

use blinc_app::prelude::*;
use blinc_app::windowed::WindowedApp;
use blinc_app::windowed::WindowedContext;
use blinc_core::{Color, Shadow, Transform};
use blinc_layout::css_parser::Stylesheet;
use blinc_layout::css_style;
use blinc_layout::element_style::ElementStyle;
use blinc_layout::style;
use blinc_layout::widgets::radio_group;
use blinc_theme::{ColorToken, ThemeState};

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = WindowConfig {
        title: "Blinc Styling API Demo".to_string(),
        width: 1000,
        height: 800,
        resizable: true,
        fullscreen: false,
        animation_fps_cap: Some(30),
        ..Default::default()
    };

    WindowedApp::run(config, build_ui)
}

const STYLESHEET: &str = r#"
            #css-card {
                background: #3b82f6;
                border-radius: 12px;
                box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
            }
            #css-card:hover {
                background: #60a5fa;
                box-shadow: 0 8px 16px rgba(59, 130, 246, 0.4);
            }

            #css-alert {
                background: #ef4444;
                border-radius: 8px;
                opacity: 0.95;
            }
            #css-alert:hover {
                opacity: 1.0;
                background: #f87171;
            }

            #css-glass {
                background: rgba(255, 255, 255, 0.15);
                border-radius: 16px;
                backdrop-filter: blur(10px);
            }

            #hover-blue {
                background: #3b82f6;
                border-radius: 8px;
            }
            #hover-blue:hover {
                background: #2563eb;
                box-shadow: 0 4px 12px rgba(37, 99, 235, 0.5);
            }

            #hover-green {
                background: #22c55e;
                border-radius: 8px;
            }
            #hover-green:hover {
                background: #16a34a;
                opacity: 0.9;
            }

            #hover-purple {
                background: #a855f7;
                border-radius: 12px;
            }
            #hover-purple:hover {
                background: #9333ea;
                box-shadow: 0 6px 20px rgba(147, 51, 234, 0.5);
            }

            #hover-orange {
                background: #f97316;
                border-radius: 16px;
            }
            #hover-orange:hover {
                background: #ea580c;
            }

            @keyframes pulse {
                0% { opacity: 0.5; }
                50% { opacity: 1.0; }
                100% { opacity: 0.5; }
            }
            #anim-pulse {
                background: #ec4899;
                border-radius: 8px;
                animation: pulse 2000ms ease-in-out infinite;
            }

            @keyframes glow {
                0% { opacity: 0.6; }
                50% { opacity: 1.0; }
                100% { opacity: 0.6; }
            }
            #anim-glow {
                background: #8b5cf6;
                border-radius: 12px;
                animation: glow 3000ms ease-in-out infinite;
            }

            @keyframes spin-y {
                0% { rotate-y: 0deg; }
                100% { rotate-y: 360deg; }
            }
            #anim-spin-3d {
                background: #3b82f6;
                border-radius: 8px;
                perspective: 800px;
                animation: spin-y 4000ms linear infinite;
            }

            @keyframes wobble-3d {
                0% { rotate-x: -15deg; rotate-y: -15deg; }
                50% { rotate-x: 15deg; rotate-y: 15deg; }
                100% { rotate-x: -15deg; rotate-y: -15deg; }
            }
            #anim-wobble-3d {
                background: #22c55e;
                border-radius: 12px;
                perspective: 800px;
                animation: wobble-3d 3000ms ease-in-out infinite;
            }

            @keyframes float-z {
                0% { translate-z: 0px; }
                50% { translate-z: 40px; }
                100% { translate-z: 0px; }
            }
            #anim-float-3d {
                shape-3d: box;
                depth: 80px;
                perspective: 600px;
                rotate-y: 15deg;
                background: #f97316;
                border-radius: 8px;
                animation: float-z 2000ms ease-in-out infinite;
            }

            #uv-box-gradient {
                shape-3d: box;
                depth: 80px;
                perspective: 800px;
                rotate-x: 15deg;
                rotate-y: 20deg;
                background: linear-gradient(45deg, #4488ff, #ff4488);
            }
            #uv-sphere-gradient {
                shape-3d: sphere;
                depth: 120px;
                perspective: 800px;
                background: linear-gradient(0deg, #00ff88, #0088ff);
            }
            #uv-cylinder-gradient {
                shape-3d: cylinder;
                depth: 120px;
                perspective: 800px;
                background: radial-gradient(circle, #ffaa00, #ff4400);
            }

            /* --- 3D Group Composition --- */

            /* Box with cylinder hole — cylinder cuts a round tunnel through the box */
            #group-subtract {
                shape-3d: group;
                perspective: 800px;
                rotate-x: 25deg;
                rotate-y: 35deg;
            }
            #group-subtract-base {
                shape-3d: box;
                depth: 140px;
                background: #3b82f6;
            }
            #group-subtract-hole {
                shape-3d: cylinder;
                depth: 200px;
                3d-op: subtract;
                background: #ef4444;
            }

            /* Smooth blob union — two spheres merging with smooth blend */
            #group-smooth-union {
                shape-3d: group;
                perspective: 800px;
                rotate-x: 15deg;
                rotate-y: -25deg;
            }
            #group-smooth-union-a {
                shape-3d: sphere;
                depth: 130px;
                background: #22c55e;
            }
            #group-smooth-union-b {
                shape-3d: sphere;
                depth: 130px;
                3d-op: smooth-union;
                3d-blend: 30px;
                background: #22c55e;
            }

            /* Rounded intersection — box ∩ sphere gives pillow shape */
            #group-intersect {
                shape-3d: group;
                perspective: 800px;
                rotate-x: 20deg;
                rotate-y: 30deg;
            }
            #group-intersect-box {
                shape-3d: box;
                depth: 160px;
                background: #a855f7;
            }
            #group-intersect-sphere {
                shape-3d: sphere;
                depth: 200px;
                3d-op: intersect;
                background: #9333ea;
            }

            /* Smooth scoop — sphere scooped from box with smooth blend */
            #group-smooth-subtract {
                shape-3d: group;
                perspective: 800px;
                rotate-x: 25deg;
                rotate-y: -20deg;
            }
            #group-smooth-subtract-base {
                shape-3d: box;
                depth: 140px;
                background: #f97316;
            }
            #group-smooth-subtract-scoop {
                shape-3d: sphere;
                depth: 160px;
                3d-op: smooth-subtract;
                3d-blend: 20px;
                background: #ea580c;
            }

            /* --- CSS position demos --- */
            #pos-container {
                position: relative;
                width: 300px;
                height: 200px;
                background: #1e293b;
            }
            #pos-static-child {
                width: 100px;
                height: 40px;
                background: #3b82f6;
            }
            #pos-floating {
                position: absolute;
                top: 12px;
                right: 12px;
                width: 80px;
                height: 80px;
                background: #ef4444;
            }
            #pos-bottom-bar {
                position: absolute;
                bottom: 0px;
                left: 0px;
                right: 0px;
                height: 32px;
                background: #22c55e;
            }

            /* Fixed & Sticky positioning — using inline styles for now */

            /* --- CSS overflow:scroll demo --- */
            #css-scroll-container {
                overflow: scroll;
                height: 200px;
                width: 300px;
                background: #0f172a;
                border-radius: 8px;
            }

            /* --- CSS clip-path demos --- */
            #clip-circle {
                background: #3b82f6;
                clip-path: circle(50% at 50% 50%);
            }
            #clip-ellipse {
                background: #22c55e;
                clip-path: ellipse(50% 35% at 50% 50%);
            }
            #clip-inset {
                background: #a855f7;
                clip-path: inset(10% 10% 10% 10% round 12px);
            }
            #clip-polygon-hex {
                background: #f97316;
                clip-path: polygon(50% 0%, 100% 25%, 100% 75%, 50% 100%, 0% 75%, 0% 25%);
            }
            #clip-polygon-star {
                background: #ec4899;
                clip-path: polygon(50% 0%, 61% 35%, 98% 35%, 68% 57%, 79% 91%, 50% 70%, 21% 91%, 32% 57%, 2% 35%, 39% 35%);
            }
            #clip-polygon-arrow {
                background: #ef4444;
                clip-path: polygon(40% 0%, 40% 40%, 0% 40%, 50% 100%, 100% 40%, 60% 40%, 60% 0%);
            }

            @keyframes clip-reveal-center {
                from { clip-path: inset(50% 50% 50% 50%); }
                to { clip-path: inset(0% 0% 0% 0%); }
            }
            #clip-over-center {
                position: absolute;
                top: 0px;
                left: 0px;
                width: 200px;
                height: 150px;
                background: #3b82f6;
                border-radius: 12px;
                clip-path: inset(50% 50% 50% 50%);
            }
            #clip-over-center:hover {
                animation: clip-reveal-center 400ms ease-out forwards;
                clip-path: inset(0% 0% 0% 0%);
            }

            @keyframes clip-reveal-top {
                from { clip-path: inset(100% 0% 0% 0%); }
                to { clip-path: inset(0% 0% 0% 0%); }
            }
            #clip-over-top {
                position: absolute;
                top: 0px;
                left: 0px;
                width: 200px;
                height: 150px;
                background: #22c55e;
                border-radius: 12px;
                clip-path: inset(100% 0% 0% 0%);
            }
            #clip-over-top:hover {
                animation: clip-reveal-top 400ms ease-out forwards;
                clip-path: inset(0% 0% 0% 0%);
            }

            @keyframes clip-reveal-left {
                from { clip-path: inset(0% 100% 0% 0%); }
                to { clip-path: inset(0% 0% 0% 0%); }
            }
            #clip-over-left {
                position: absolute;
                top: 0px;
                left: 0px;
                width: 200px;
                height: 150px;
                background: #a855f7;
                border-radius: 12px;
                clip-path: inset(0% 100% 0% 0%);
            }
            #clip-over-left:hover {
                animation: clip-reveal-left 400ms ease-out forwards;
                clip-path: inset(0% 0% 0% 0%);
            }

            /* --- CSS Selector Hierarchy demos --- */

            /* Class selector: .card applies to all elements with class="card" */
            .card {
                background: #1e3a5f;
                border-radius: 12px;
                transition: all 300ms ease;
            }
            .card:hover {
                background: #3b82f6;
                border-radius: 24px;
                box-shadow: 0 8px 20px rgba(0, 0, 0, 0.3);
            }

            /* Child combinator: #parent > .child */
            #selector-parent > .child-item {
                background: #374151;
                border-radius: 8px;
            }
            #selector-parent:hover > .child-item {
                background: #6366f1;
            }

            /* Structural pseudo-classes */
            .list-item:first-child {
                background: #22c55e;
                border-radius: 8px 8px 0 0;
            }
            .list-item:last-child {
                background: #ef4444;
                border-radius: 0 0 8px 8px;
            }

            /* Filter + transition demo */
            .filter-card {
                border-radius: 12px;
                transition: all 400ms ease;
            }
            .filter-card:hover {
                filter: brightness(1.8) saturate(2.0) contrast(1.3);
            }

            /* --- Advanced Selector demos --- */

            /* Adjacent sibling combinator: .trigger:hover + .target */
            .sib-trigger {
                background: #475569;
                border-radius: 8px;
                transition: all 300ms ease;
            }
            .sib-trigger:hover + .sib-target {
                background: #f59e0b;
                border-radius: 16px;
            }
            .sib-target {
                background: #64748b;
                border-radius: 8px;
                transition: all 300ms ease;
            }

            /* General sibling combinator: .gs-trigger:hover ~ .gs-item */
            .gs-trigger {
                background: #7c3aed;
                border-radius: 8px;
                transition: all 300ms ease;
            }
            .gs-trigger:hover ~ .gs-item {
                background: #a78bfa;
                border-radius: 16px;
            }
            .gs-item {
                background: #4c1d95;
                border-radius: 8px;
                transition: all 300ms ease;
            }

            /* :not() pseudo-class */
            .not-demo {
                border-radius: 8px;
                transition: all 300ms ease;
            }
            .not-demo:not(:first-child) {
                background: #0891b2;
            }
            .not-demo:first-child {
                background: #f43f5e;
            }

            /* :empty pseudo-class */
            .empty-demo {
                border-radius: 8px;
                border-width: 2px;
                border-color: #94a3b8;
            }
            .empty-demo:empty {
                background: #22d3ee;
                border-color: #22d3ee;
            }

            /* Universal selector: * inside #universal-parent */
            #universal-parent > * {
                background: #059669;
                border-radius: 8px;
            }
            #universal-parent > *:hover {
                background: #10b981;
                box-shadow: 0 4px 12px rgba(16, 185, 129, 0.4);
            }

            /* :is() pseudo-class: matches any of the listed selectors */
            .is-card {
                background: #374151;
                border-radius: 8px;
                transition: all 300ms ease;
            }
            :is(.is-primary, .is-accent):hover {
                background: #3b82f6;
                box-shadow: 0 4px 12px rgba(59, 130, 246, 0.4);
            }

            /* :where() pseudo-class (same as :is but zero specificity) */
            :where(.is-secondary):hover {
                background: #8b5cf6;
                box-shadow: 0 4px 12px rgba(139, 92, 246, 0.4);
            }

            /* :first-of-type / :last-of-type */
            .type-item {
                background: #374151;
                border-radius: 8px;
            }
            .type-item:first-of-type {
                background: #059669;
            }
            .type-item:last-of-type {
                background: #dc2626;
            }

            /* :nth-of-type */
            .nth-type-item {
                background: #374151;
                border-radius: 8px;
            }
            .nth-type-item:nth-of-type(2) {
                background: #d97706;
            }
            .nth-type-item:only-of-type {
                background: #7c3aed;
            }

            /* --- Gradient Animation demos --- */

            /* Gradient transition on hover */
            #grad-hover {
                background: linear-gradient(135deg, #3b82f6, #8b5cf6);
                border-radius: 12px;
                transition: all 500ms ease;
            }
            #grad-hover:hover {
                background: linear-gradient(135deg, #f59e0b, #ef4444);
            }

            /* Gradient angle transition */
            #grad-angle {
                background: linear-gradient(0deg, #06b6d4, #8b5cf6);
                border-radius: 12px;
                transition: all 800ms ease;
            }
            #grad-angle:hover {
                background: linear-gradient(180deg, #06b6d4, #8b5cf6);
            }

            /* Gradient keyframe animation */
            @keyframes gradient-cycle {
                0% { background: linear-gradient(90deg, #ef4444, #f97316); }
                33% { background: linear-gradient(90deg, #22c55e, #06b6d4); }
                66% { background: linear-gradient(90deg, #8b5cf6, #ec4899); }
                100% { background: linear-gradient(90deg, #ef4444, #f97316); }
            }
            #grad-anim {
                border-radius: 12px;
                animation: gradient-cycle 4000ms linear infinite;
            }

            /* --- Text Shadow demos --- */

            #text-shadow-basic {
                text-shadow: 3px 3px 0px rgba(255, 68, 68, 1.0);
            }
            #text-shadow-glow {
                text-shadow: 2px 2px 0px rgba(34, 197, 94, 0.9);
            }
            #text-shadow-hover {
                text-shadow: 2px 2px 0px rgba(0, 0, 0, 0.6);
                transition: text-shadow 500ms ease;
            }
            #text-shadow-hover:hover {
                text-shadow: 3px 3px 0px rgba(239, 68, 68, 1.0);
            }

            /* --- Filter blur & drop-shadow demos --- */

            /* Static blur */
            #filter-blur-static {
                background: #3b82f6;
                border-radius: 12px;
                filter: blur(4px);
            }

            /* Blur transition on hover */
            #filter-blur-hover {
                background: #22c55e;
                border-radius: 12px;
                filter: blur(0px);
                transition: filter 400ms ease;
            }
            #filter-blur-hover:hover {
                filter: blur(8px);
            }

            /* Drop-shadow */
            #filter-drop-shadow {
                background: #a855f7;
                border-radius: 12px;
                filter: drop-shadow(4px 4px 8px rgba(0, 0, 0, 0.5));
            }

            /* Combined blur + color filter */
            #filter-blur-combo {
                background: #f97316;
                border-radius: 12px;
                filter: blur(6px) brightness(1.3);
                transition: filter 500ms ease;
            }
            #filter-blur-combo:hover {
                filter: blur(0px) brightness(1.0);
            }

            /* Blur keyframe animation */
            @keyframes blur-pulse {
                0% { filter: blur(0px); }
                50% { filter: blur(6px); }
                100% { filter: blur(0px); }
            }
            #filter-blur-anim {
                background: #ec4899;
                border-radius: 12px;
                animation: blur-pulse 3000ms ease-in-out infinite;
            }

            /* --- Backdrop-filter animation demos --- */

            /* Static backdrop blur */
            #backdrop-static {
                backdrop-filter: blur(12px);
                border-radius: 12px;
            }

            /* Backdrop blur transition on hover */
            #backdrop-hover {
                backdrop-filter: blur(0px);
                border-radius: 12px;
                transition: backdrop-filter 400ms ease;
            }
            #backdrop-hover:hover {
                backdrop-filter: blur(20px);
            }

            /* Backdrop blur + saturate transition */
            #backdrop-combo {
                backdrop-filter: blur(8px) saturate(1.0);
                border-radius: 12px;
                transition: backdrop-filter 500ms ease;
            }
            #backdrop-combo:hover {
                backdrop-filter: blur(20px) saturate(1.8);
            }

            /* Backdrop blur keyframe animation */
            @keyframes backdrop-pulse {
                0% { backdrop-filter: blur(4px); }
                50% { backdrop-filter: blur(24px); }
                100% { backdrop-filter: blur(4px); }
            }
            #backdrop-anim {
                border-radius: 12px;
                animation: backdrop-pulse 3000ms ease-in-out infinite;
            }

            /* --- Layout Property Animation demos --- */

            /* Width transition on hover */
            #layout-width {
                background: #3b82f6;
                border-radius: 8px;
                width: 120px;
                height: 60px;
                transition: width 400ms ease;
            }
            #layout-width:hover {
                width: 280px;
            }

            /* Height transition on hover */
            #layout-height {
                background: #22c55e;
                border-radius: 8px;
                width: 120px;
                height: 60px;
                transition: height 400ms ease;
            }
            #layout-height:hover {
                height: 120px;
            }

            /* Padding transition on hover */
            #layout-padding {
                background: #a855f7;
                border-radius: 8px;
                padding: 8px;
                transition: padding 300ms ease;
            }
            #layout-padding:hover {
                padding: 24px;
            }

            /* Combined width + height via @keyframes */
            @keyframes grow-shrink {
                0% { width: 80px; height: 50px; }
                50% { width: 200px; height: 100px; }
                100% { width: 80px; height: 50px; }
            }
            #layout-anim {
                background: #f97316;
                border-radius: 12px;
                animation: grow-shrink 3000ms ease-in-out infinite;
            }

            /* --- Constraint & Position Animation demos --- */

            /* min-width transition */
            #constraint-minw {
                background: #6366f1;
                border-radius: 8px;
                width: 80px;
                height: 60px;
                min-width: 80px;
                transition: min-width 400ms ease;
            }
            #constraint-minw:hover {
                min-width: 200px;
            }

            /* max-width transition */
            #constraint-maxw {
                background: #8b5cf6;
                border-radius: 8px;
                width: 300px;
                height: 60px;
                max-width: 300px;
                transition: max-width 400ms ease;
            }
            #constraint-maxw:hover {
                max-width: 120px;
            }

            /* top/left position animation */
            #pos-anim-container {
                position: relative;
                width: 280px;
                height: 140px;
                background: #1e293b;
                border-radius: 8px;
            }
            #pos-anim-dot {
                position: absolute;
                top: 10px;
                left: 10px;
                width: 40px;
                height: 40px;
                background: #f43f5e;
                border-radius: 20px;
                transition: all 500ms ease;
            }
            #pos-anim-dot:hover {
                top: 90px;
                left: 228px;
            }

            /* flex-grow transition */
            .flex-grow-item {
                background: #0ea5e9;
                border-radius: 6px;
                height: 50px;
                flex-grow: 1;
                transition: flex-grow 400ms ease;
            }
            .flex-grow-item:hover {
                flex-grow: 4;
            }

            /* @keyframes with min/max constraints */
            @keyframes constraint-pulse {
                0% { min-width: 60px; max-width: 60px; }
                50% { min-width: 180px; max-width: 180px; }
                100% { min-width: 60px; max-width: 60px; }
            }
            #constraint-anim {
                background: #14b8a6;
                border-radius: 8px;
                height: 50px;
                animation: constraint-pulse 2500ms ease-in-out infinite;
            }

            /* --- Text Color Animation demos --- */

            /* Text color transition on hover */
            #text-color-hover {
                color: #94a3b8;
                transition: color 400ms ease;
            }
            #text-color-hover:hover {
                color: #3b82f6;
            }

            /* Text color + background combined transition */
            #text-color-combo {
                background: #1e293b;
                border-radius: 8px;
                transition: all 400ms ease;
            }
            #text-color-combo:hover {
                background: #3b82f6;
            }
            #text-color-combo-label {
                color: #94a3b8;
                transition: color 400ms ease;
            }
            #text-color-combo-label:hover {
                color: #ffffff;
            }

            /* @keyframes text color cycling */
            @keyframes color-cycle {
                0% { color: #ef4444; }
                33% { color: #22c55e; }
                66% { color: #3b82f6; }
                100% { color: #ef4444; }
            }
            #text-color-anim {
                animation: color-cycle 3000ms linear infinite;
            }

            /* --- Transform Extensions demos --- */

            /* SkewX on hover */
            #skew-x-demo {
                background: #3b82f6;
                border-radius: 8px;
                transition: all 400ms ease;
            }
            #skew-x-demo:hover {
                transform: skewX(-12deg);
            }

            /* SkewY on hover */
            #skew-y-demo {
                background: #22c55e;
                border-radius: 8px;
                transition: all 400ms ease;
            }
            #skew-y-demo:hover {
                transform: skewY(12deg);
            }

            /* Transform-origin: top-left rotation */
            #origin-tl-demo {
                background: #a855f7;
                border-radius: 8px;
                transform-origin: left top;
                transition: all 400ms ease;
            }
            #origin-tl-demo:hover {
                transform: rotate(15deg);
            }

            /* Transform-origin: bottom-right rotation */
            #origin-br-demo {
                background: #f97316;
                border-radius: 8px;
                transform-origin: right bottom;
                transition: all 400ms ease;
            }
            #origin-br-demo:hover {
                transform: rotate(-15deg);
            }

            /* @keyframes skew wobble */
            @keyframes skew-wobble {
                0% { transform: skew(0deg, 0deg); }
                25% { transform: skew(10deg, 5deg); }
                50% { transform: skew(0deg, 0deg); }
                75% { transform: skew(-10deg, -5deg); }
                100% { transform: skew(0deg, 0deg); }
            }
            #skew-wobble-demo {
                background: #ec4899;
                border-radius: 8px;
                animation: skew-wobble 2000ms ease-in-out infinite;
            }

            /* --- Font Size Animation demos --- */

            /* Font size transition on hover */
            #font-size-hover {
                font-size: 14px;
                color: #94a3b8;
                transition: all 400ms ease;
            }
            #font-size-hover:hover {
                font-size: 28px;
                color: #3b82f6;
            }

            /* @keyframes font size pulsing */
            @keyframes font-pulse {
                0% { font-size: 14px; }
                50% { font-size: 24px; }
                100% { font-size: 14px; }
            }
            #font-size-anim {
                color: #22c55e;
                animation: font-pulse 2000ms ease-in-out infinite;
            }

            /* --- Outline demos --- */

            /* Static outline */
            #outline-static {
                background: #3b82f6;
                border-radius: 12px;
                outline: 3px solid #f59e0b;
            }

            /* Outline with offset */
            #outline-offset {
                background: #22c55e;
                border-radius: 12px;
                outline: 2px solid #ef4444;
                outline-offset: 6px;
            }

            /* Outline transition on hover */
            #outline-hover {
                background: #a855f7;
                border-radius: 12px;
                outline: 0px solid rgba(168, 85, 247, 0);
                outline-offset: 0px;
                transition: all 400ms ease;
            }
            #outline-hover:hover {
                outline: 3px solid #c084fc;
                outline-offset: 4px;
            }

            /* Outline color transition */
            #outline-color-hover {
                background: #1e293b;
                border-radius: 12px;
                outline: 3px solid #64748b;
                transition: outline-color 400ms ease;
            }
            #outline-color-hover:hover {
                outline-color: #f43f5e;
            }

            /* @keyframes outline animation */
            @keyframes outline-pulse {
                0% { outline-width: 1px; outline-color: rgba(59, 130, 246, 0.3); outline-offset: 0px; }
                50% { outline-width: 4px; outline-color: rgba(59, 130, 246, 1.0); outline-offset: 6px; }
                100% { outline-width: 1px; outline-color: rgba(59, 130, 246, 0.3); outline-offset: 0px; }
            }
            #outline-anim {
                background: #0f172a;
                border-radius: 12px;
                outline: 1px solid rgba(59, 130, 246, 0.3);
                animation: outline-pulse 2500ms ease-in-out infinite;
            }

            /* --- Form Input Styling --- */

            /* Base input style */
            #demo-input {
                background: #1e293b;
                border-color: #475569;
                border-width: 2px;
                border-radius: 8px;
                color: #f1f5f9;
                caret-color: #60a5fa;
            }
            #demo-input:hover {
                border-color: #64748b;
                background: #283548;
            }
            #demo-input:focus {
                border-color: #3b82f6;
                background: #1e293b;
                outline: 2px solid rgba(59, 130, 246, 0.4);
                outline-offset: 2px;
            }
            #demo-input::placeholder {
                color: #64748b;
            }

            /* Accent-colored input */
            #accent-input {
                background: #fefce8;
                border-color: #eab308;
                border-width: 2px;
                border-radius: 12px;
                color: #713f12;
                caret-color: #ca8a04;
            }
            #accent-input:hover {
                border-color: #facc15;
                background: #fef9c3;
            }
            #accent-input:focus {
                border-color: #eab308;
                outline: 2px solid rgba(234, 179, 8, 0.4);
                outline-offset: 2px;
            }
            #accent-input::placeholder {
                color: #a16207;
            }

            /* Disabled input */
            #disabled-input {
                background: #1e293b;
                border-color: #334155;
                border-width: 1px;
                border-radius: 8px;
                color: #64748b;
                opacity: 0.5;
            }

            /* Text area with CSS styling */
            #demo-textarea {
                background: #1e293b;
                border-color: #475569;
                border-width: 2px;
                border-radius: 8px;
                color: #f1f5f9;
                caret-color: #a78bfa;
            }
            #demo-textarea:hover {
                border-color: #64748b;
            }
            #demo-textarea:focus {
                border-color: #8b5cf6;
                outline: 2px solid rgba(139, 92, 246, 0.4);
                outline-offset: 2px;
            }
            #demo-textarea::placeholder {
                color: #64748b;
            }

            /* Checkbox CSS styling */
            #demo-checkbox {
                border-color: #475569;
                border-radius: 4px;
                accent-color: #3b82f6;
            }
            #demo-checkbox:hover {
                border-color: #3b82f6;
            }
            #demo-checkbox:checked {
                background: #3b82f6;
                border-color: #3b82f6;
            }

            #accent-checkbox {
                border-color: #eab308;
                accent-color: #eab308;
            }
            #accent-checkbox:hover {
                border-color: #facc15;
            }
            #accent-checkbox:checked {
                background: #eab308;
                border-color: #eab308;
            }

            #disabled-checkbox {
                opacity: 0.5;
            }

            /* Radio CSS styling */
            #theme-radio-light {
                border-color: #475569;
                accent-color: #3b82f6;
            }
            #theme-radio-light:hover {
                border-color: #3b82f6;
            }
            #theme-radio-light:checked {
                accent-color: #3b82f6;
                border-color: #3b82f6;
            }
            #theme-radio-dark {
                border-color: #475569;
                accent-color: #3b82f6;
            }
            #theme-radio-dark:hover {
                border-color: #3b82f6;
            }
            #theme-radio-dark:checked {
                accent-color: #3b82f6;
                border-color: #3b82f6;
            }
            #theme-radio-system {
                border-color: #475569;
                accent-color: #3b82f6;
            }
            #theme-radio-system:hover {
                border-color: #3b82f6;
            }
            #theme-radio-system:checked {
                accent-color: #3b82f6;
                border-color: #3b82f6;
            }

            /* ============================================================ */
            /* Global tag-name selectors (CSS for all widgets)     */
            /* ============================================================ */

            /* Style ALL h1 headings globally — uses theme CSS variables */
            h1 {
                color: var(--text-primary);
                letter-spacing: -0.5px;
            }

            /* Style ALL paragraphs — uses theme CSS variables */
            p {
                color: var(--text-secondary);
                line-height: 1.6;
            }

            /* Style ALL blockquotes */
            blockquote {
                background: var(--surface-elevated);
                border-color: var(--primary);
                border-radius: 4px;
            }

            /* Style ALL SVG icons — uses theme text-tertiary for muted icon color */
            svg {
                fill: var(--text-tertiary);
            }

            /* SVG stroke on hover */
            #stroke-svg {
                fill: none;
                stroke: var(--border);
                stroke-width: 2px;
            }
            #stroke-svg:hover {
                stroke: var(--primary);
                stroke-width: 3px;
            }

            /* Per-instance override — higher specificity than tag selector */
            #accent-svg {
                fill: var(--warning);
            }

            /* --- Corner Shape demos --- */

            #cs-transition {
                background: #3b82f6;
                border-radius: 32px;
                corner-shape: round;
                transition: corner-shape 500ms ease;
            }
            #cs-transition:hover {
                corner-shape: squircle;
            }

            @keyframes cs-morph {
                0% { corner-shape: superellipse(1.0); }
                25% { corner-shape: superellipse(0.0); }
                50% { corner-shape: superellipse(2.0); }
                75% { corner-shape: superellipse(-1.0); }
                100% { corner-shape: superellipse(1.0); }
            }
            #cs-anim {
                background: #8b5cf6;
                border-radius: 32px;
                animation: cs-morph 4000ms ease-in-out infinite;
            }

            "#;

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    if ctx.rebuild_count == 0 {
        ctx.add_css(STYLESHEET);
    }

    let theme = ThemeState::get();
    let bg = theme.color(ColorToken::Background);

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(bg)
        .flex_col()
        .child(header())
        .child(
            scroll().w_full().h(ctx.height - 80.0).child(
                div()
                    .w_full()
                    .p(theme.spacing().space_6)
                    .flex_col()
                    .gap(theme.spacing().space_8)
                    // Layout property animation
                    .child(layout_animation_section())
                    // Constraint & position animation
                    .child(constraint_position_animation_section())
                    // Text color animation
                    .child(text_color_animation_section())
                    // Transform extensions
                    .child(transform_extensions_section())
                    // Font size animation
                    .child(font_size_animation_section())
                    // CSS Selector Hierarchy
                    .child(selector_hierarchy_section())
                    // Advanced selectors (+, ~, :not(), :empty, *)
                    .child(advanced_selectors_section())
                    // :is()/:where() and *-of-type selectors
                    .child(is_where_of_type_section())
                    // Filter blur & drop-shadow
                    .child(filter_blur_section())
                    // Backdrop-filter animation
                    .child(backdrop_filter_section())
                    // Gradient animation
                    .child(gradient_animation_section())
                    // Text shadow
                    .child(text_shadow_section())
                    // Outline
                    .child(outline_section())
                    // Form Input CSS Styling
                    .child(form_input_section())
                    // Checkbox & Radio CSS Styling
                    .child(form_controls_section(ctx))
                    // Global tag-name CSS selectors
                    .child(global_tag_selectors_section())
                    // CSS Stylesheet integration
                    .child(css_stylesheet_section())
                    .child(css_hover_section())
                    .child(css_animation_section())
                    // Styling API sections
                    .child(css_macro_section())
                    .child(style_macro_section())
                    .child(builder_pattern_section())
                    .child(css_parser_section())
                    .child(style_merging_section())
                    .child(backgrounds_section())
                    .child(corner_radius_section())
                    .child(corner_shape_section())
                    .child(shadows_section())
                    .child(transforms_section())
                    .child(opacity_section())
                    .child(materials_section())
                    // 3D features
                    .child(transforms_3d_section())
                    .child(sdf_3d_shapes_section())
                    .child(lighting_3d_section())
                    .child(translate_z_section())
                    .child(uv_mapping_3d_section())
                    .child(animation_3d_section())
                    .child(group_composition_section())
                    .child(clip_path_section())
                    .child(clip_path_animation_section())
                    .child(css_position_section())
                    .child(api_comparison_section()),
            ),
        )
}

fn header() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let surface = theme.color(ColorToken::Surface);
    let text_primary = theme.color(ColorToken::TextPrimary);
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let border = theme.color(ColorToken::Border);

    div()
        .w_full()
        .h(80.0)
        .bg(surface)
        .border_bottom(1.0, border)
        .flex_row()
        .items_center()
        .justify_center()
        .gap(16.0)
        .child(
            text("Blinc Styling API")
                .size(28.0)
                .weight(FontWeight::Bold)
                .color(text_primary),
        )
        .child(
            text("Stylesheets | Hover | Animations | css_style! | style! | 3D SDF | Groups | clip-path")
                .size(14.0)
                .color(text_secondary),
        )
}

// ============================================================================
// Section Container Helpers
// ============================================================================

fn section_container() -> Div {
    let theme = ThemeState::get();
    let surface = theme.color(ColorToken::Surface);
    let border = theme.color(ColorToken::Border);

    div()
        .w_full()
        .bg(surface)
        .border(1.0, border)
        .rounded(12.0)
        .p(24.0)
        .flex_col()
        .gap(16.0)
}

fn section_title(title: &str) -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_primary = theme.color(ColorToken::TextPrimary);

    text(title)
        .size(20.0)
        .weight(FontWeight::SemiBold)
        .color(text_primary)
}

fn section_description(desc: &str) -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    text(desc).size(14.0).color(text_secondary)
}

fn code_label(label: &str) -> impl ElementBuilder + use<> {
    inline_code(label).size(12.0)
}

// ============================================================================
// LAYOUT PROPERTY ANIMATION SECTION
// ============================================================================

fn layout_animation_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Layout Property Animation"))
        .child(section_description(
            "Animate width, height, and padding via CSS transitions and @keyframes. Layout is recomputed each frame.",
        ))
        .child(
            div()
                .flex_col()
                .gap(16.0)
                // 1. Width transition
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#layout-width { width: 120px; transition: width 400ms ease; } :hover { width: 280px; }"))
                        .child(
                            div()
                                .id("layout-width")
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Hover to grow width").size(12.0).color(Color::WHITE)),
                        ),
                )
                // 2. Height transition
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#layout-height { height: 60px; transition: height 400ms ease; } :hover { height: 120px; }"))
                        .child(
                            div()
                                .id("layout-height")
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Hover to grow height").size(12.0).color(Color::WHITE)),
                        ),
                )
                // 3. Padding transition
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#layout-padding { padding: 8px; transition: padding 300ms ease; } :hover { padding: 24px; }"))
                        .child(
                            div()
                                .id("layout-padding")
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Hover to expand padding").size(12.0).color(Color::WHITE)),
                        ),
                )
                // 4. @keyframes layout animation
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("@keyframes grow-shrink { 0% { width: 80px; height: 50px; } 50% { width: 200px; height: 100px; } }"))
                        .child(
                            div()
                                .w(200.0)
                                .h(100.0)
                                .child(
                                    div()
                                        .id("layout-anim")
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Animated").size(12.0).color(Color::WHITE)),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// CONSTRAINT & POSITION ANIMATION SECTION
// ============================================================================

fn constraint_position_animation_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Constraint & Position Animation"))
        .child(section_description(
            "Animate min/max-width/height, top/left positioning, flex-grow, and z-index via CSS transitions and @keyframes.",
        ))
        .child(
            div()
                .flex_col()
                .gap(16.0)
                // 1. min-width transition
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#constraint-minw { min-width: 80px; transition: min-width 400ms; } :hover { min-width: 200px }"))
                        .child(
                            div()
                                .id("constraint-minw")
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Hover: min-width grows").size(11.0).color(Color::WHITE)),
                        ),
                )
                // 2. max-width transition
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#constraint-maxw { max-width: 300px; transition: max-width 400ms; } :hover { max-width: 120px }"))
                        .child(
                            div()
                                .id("constraint-maxw")
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Hover: max-width shrinks").size(11.0).color(Color::WHITE)),
                        ),
                )
                // 3. top/left position transition
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#pos-anim-dot { top: 10px; left: 10px; transition: all 500ms; } :hover { top: 90px; left: 228px }"))
                        .child(
                            div()
                                .id("pos-anim-container")
                                .child(
                                    div()
                                        .id("pos-anim-dot"),
                                ),
                        ),
                )
                // 4. flex-grow transition
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(".flex-grow-item { flex-grow: 1; transition: flex-grow 400ms; } :hover { flex-grow: 4 }"))
                        .child(
                            div()
                                .flex_row()
                                .gap(4.0)
                                .w(400.0)
                                .child(
                                    div()
                                        .class("flex-grow-item")
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("A").size(12.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("flex-grow-item")
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("B").size(12.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("flex-grow-item")
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("C").size(12.0).color(Color::WHITE)),
                                ),
                        ),
                )
                // 5. @keyframes constraint animation
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("@keyframes constraint-pulse { 0% { min-width: 60px } 50% { min-width: 180px } }"))
                        .child(
                            div()
                                .w(200.0)
                                .h(50.0)
                                .child(
                                    div()
                                        .id("constraint-anim")
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Pulsing").size(12.0).color(Color::WHITE)),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// TEXT COLOR ANIMATION SECTION
// ============================================================================

fn text_color_animation_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Text Color Animation"))
        .child(section_description(
            "CSS color property with transitions and @keyframes. Hover to see smooth text color transitions.",
        ))
        .child(
            div()
                .flex_row()
                .gap(24.0)
                .flex_wrap()
                // 1. Text color transition on hover
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("color: #94a3b8; transition: color 400ms ease"))
                        .child(
                            div()
                                .w(200.0)
                                .h(60.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(
                                    text("Hover me!")
                                        .id("text-color-hover")
                                        .size(18.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::rgba(0.58, 0.64, 0.72, 1.0)),
                                ),
                        ),
                )
                // 2. Text + background combined transition
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("background + text color transition combined"))
                        .child(
                            div()
                                .id("text-color-combo")
                                .w(200.0)
                                .h(60.0)
                                .p(16.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(
                                    text("Combo hover")
                                        .id("text-color-combo-label")
                                        .size(16.0)
                                        .weight(FontWeight::SemiBold)
                                        .color(Color::rgba(0.58, 0.64, 0.72, 1.0)),
                                ),
                        ),
                )
                // 3. @keyframes text color cycling
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("@keyframes color-cycle { 0% red, 33% green, 66% blue }"))
                        .child(
                            div()
                                .w(200.0)
                                .h(60.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(
                                    text("Color cycling")
                                        .id("text-color-anim")
                                        .size(18.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::rgba(0.94, 0.27, 0.27, 1.0)),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// FONT SIZE ANIMATION SECTION
// ============================================================================

fn font_size_animation_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Font Size Animation"))
        .child(section_description(
            "CSS font-size property with transitions and @keyframes. Hover to see smooth font size transitions.",
        ))
        .child(
            div()
                .flex_row()
                .gap(24.0)
                .flex_wrap()
                .items_center()
                // 1. Font size transition on hover
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("font-size: 14px → 28px on hover"))
                        .child(
                            div()
                                .w(200.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(
                                    text("Hover to grow")
                                        .id("font-size-hover")
                                        .size(14.0)
                                        .color(Color::rgba(0.58, 0.64, 0.72, 1.0)),
                                ),
                        ),
                )
                // 2. @keyframes font size pulsing
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("@keyframes font-pulse { 14px → 24px }"))
                        .child(
                            div()
                                .w(200.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(
                                    text("Pulsing size")
                                        .id("font-size-anim")
                                        .size(14.0)
                                        .color(Color::rgba(0.13, 0.77, 0.37, 1.0)),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// TRANSFORM EXTENSIONS SECTION
// ============================================================================

fn transform_extensions_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Transform Extensions"))
        .child(section_description(
            "CSS skewX/skewY and transform-origin support. Hover to see skew transitions and custom pivot points.",
        ))
        .child(
            div()
                .flex_row()
                .gap(24.0)
                .flex_wrap()
                .items_center()
                // 1. SkewX on hover
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("transform: skewX(-12deg)"))
                        .child(
                            div()
                                .id("skew-x-demo")
                                .w(120.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("SkewX").size(14.0).color(Color::WHITE)),
                        ),
                )
                // 2. SkewY on hover
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("transform: skewY(12deg)"))
                        .child(
                            div()
                                .id("skew-y-demo")
                                .w(120.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("SkewY").size(14.0).color(Color::WHITE)),
                        ),
                )
                // 3. Transform-origin: top-left
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("transform-origin: left top"))
                        .child(
                            div()
                                .id("origin-tl-demo")
                                .w(120.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Top-Left").size(14.0).color(Color::WHITE)),
                        ),
                )
                // 4. Transform-origin: bottom-right
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("transform-origin: right bottom"))
                        .child(
                            div()
                                .id("origin-br-demo")
                                .w(120.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Bot-Right").size(14.0).color(Color::WHITE)),
                        ),
                )
                // 5. @keyframes skew wobble
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("@keyframes skew-wobble"))
                        .child(
                            div()
                                .id("skew-wobble-demo")
                                .w(120.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Wobble").size(14.0).color(Color::WHITE)),
                        ),
                ),
        )
}

// ============================================================================
// CSS SELECTOR HIERARCHY SECTION
// ============================================================================

fn selector_hierarchy_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_color = theme.color(ColorToken::TextPrimary);

    section_container()
        .child(section_title("CSS Selector Hierarchy"))
        .child(section_description(
            "Class selectors (.class), child combinators (>), structural pseudo-classes (:first-child, :last-child), and transitions.",
        ))
        .child(
            div()
                .flex_col()
                .gap(16.0)
                // 1. Class selectors with transitions
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(".card { transition: all 300ms ease; }  .card:hover { ... }"))
                        .child(
                            div()
                                .flex_row()
                                .gap(12.0)
                                .child(
                                    div()
                                        .class("card")
                                        .w(100.0)
                                        .h(80.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Card A").size(13.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("card")
                                        .w(100.0)
                                        .h(80.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Card B").size(13.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("card")
                                        .w(100.0)
                                        .h(80.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Card C").size(13.0).color(Color::WHITE)),
                                ),
                        ),
                )
                // 2. Child combinator: #parent:hover > .child-item
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#selector-parent:hover > .child-item { background: #6366f1 }"))
                        .child(
                            div()
                                .id("selector-parent")
                                .bg(Color::rgba(0.1, 0.1, 0.15, 1.0))
                                .rounded(12.0)
                                .p(12.0)
                                .flex_row()
                                .gap(8.0)
                                .child(
                                    div()
                                        .class("child-item")
                                        .w(70.0)
                                        .h(50.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("1").size(14.0).color(text_color)),
                                )
                                .child(
                                    div()
                                        .class("child-item")
                                        .w(70.0)
                                        .h(50.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("2").size(14.0).color(text_color)),
                                )
                                .child(
                                    div()
                                        .class("child-item")
                                        .w(70.0)
                                        .h(50.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("3").size(14.0).color(text_color)),
                                ),
                        ),
                )
                // 3. Structural pseudo-classes
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(".list-item:first-child / :last-child"))
                        .child(
                            div()
                                .id("pseudo-list")
                                .flex_col()
                                .w(200.0)
                                .child(
                                    div()
                                        .class("list-item")
                                        .h(40.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("First").size(13.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("list-item")
                                        .h(40.0)
                                        .bg(Color::rgba(0.3, 0.3, 0.35, 1.0))
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Middle").size(13.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("list-item")
                                        .h(40.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Last").size(13.0).color(Color::WHITE)),
                                ),
                        ),
                )
                // 4. Filter + transition on class
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(".filter-card:hover { filter: brightness(1.8) saturate(2.0) contrast(1.3) }"))
                        .child(
                            div()
                                .flex_row()
                                .gap(12.0)
                                .child(
                                    div()
                                        .class("filter-card")
                                        .w(100.0)
                                        .h(80.0)
                                        .bg(Color::rgba(0.9, 0.2, 0.3, 1.0))
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Hover me").size(12.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("filter-card")
                                        .w(100.0)
                                        .h(80.0)
                                        .bg(Color::rgba(0.2, 0.6, 0.9, 1.0))
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Hover me").size(12.0).color(Color::WHITE)),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// ADVANCED SELECTORS SECTION (+, ~, :not(), :empty, *, :root)
// ============================================================================

fn advanced_selectors_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Advanced CSS Selectors"))
        .child(section_description(
            "Adjacent sibling (+), general sibling (~), :not(), :empty, and universal (*) selectors.",
        ))
        .child(
            div()
                .flex_col()
                .gap(16.0)
                // 1. Adjacent sibling combinator: hover trigger highlights next sibling
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(".sib-trigger:hover + .sib-target { background: #f59e0b }"))
                        .child(
                            div()
                                .flex_row()
                                .gap(12.0)
                                .child(
                                    div()
                                        .class("sib-trigger")
                                        .w(100.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Hover me").size(12.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("sib-target")
                                        .w(100.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("I change!").size(12.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("sib-target")
                                        .w(100.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Not me").size(12.0).color(Color::WHITE)),
                                ),
                        ),
                )
                // 2. General sibling combinator: hover trigger highlights all later siblings
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(".gs-trigger:hover ~ .gs-item { background: #a78bfa }"))
                        .child(
                            div()
                                .flex_row()
                                .gap(12.0)
                                .child(
                                    div()
                                        .class("gs-trigger")
                                        .w(80.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Hover").size(12.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("gs-item")
                                        .w(80.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("A").size(14.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("gs-item")
                                        .w(80.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("B").size(14.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("gs-item")
                                        .w(80.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("C").size(14.0).color(Color::WHITE)),
                                ),
                        ),
                )
                // 3. :not() pseudo-class
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(".not-demo:not(:first-child) { bg: #0891b2 }  :first-child { bg: #f43f5e }"))
                        .child(
                            div()
                                .flex_row()
                                .gap(12.0)
                                .child(
                                    div()
                                        .class("not-demo")
                                        .w(80.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("1st").size(13.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("not-demo")
                                        .w(80.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("2nd").size(13.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("not-demo")
                                        .w(80.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("3rd").size(13.0).color(Color::WHITE)),
                                ),
                        ),
                )
                // 4. :empty pseudo-class
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(".empty-demo:empty { background: #22d3ee }"))
                        .child(
                            div()
                                .flex_row()
                                .gap(12.0)
                                .child(
                                    div()
                                        .class("empty-demo")
                                        .w(80.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Has child").size(11.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("empty-demo")
                                        .w(80.0)
                                        .h(60.0),
                                )
                                .child(
                                    div()
                                        .class("empty-demo")
                                        .w(80.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Has child").size(11.0).color(Color::WHITE)),
                                ),
                        ),
                )
                // 5. Universal selector: * matches any element
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#universal-parent > * { background: #059669 }"))
                        .child(
                            div()
                                .id("universal-parent")
                                .bg(Color::rgba(0.1, 0.1, 0.15, 1.0))
                                .rounded(12.0)
                                .p(12.0)
                                .flex_row()
                                .gap(8.0)
                                .child(
                                    div()
                                        .w(70.0)
                                        .h(50.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("A").size(14.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .w(70.0)
                                        .h(50.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("B").size(14.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .w(70.0)
                                        .h(50.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("C").size(14.0).color(Color::WHITE)),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// :is()/:where() AND *-of-type SELECTORS SECTION
// ============================================================================

fn is_where_of_type_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title(":is() / :where() / *-of-type"))
        .child(section_description(
            "Functional pseudo-classes :is(), :where() and structural *-of-type selectors.",
        ))
        .child(
            div()
                .flex_col()
                .gap(16.0)
                // 1. :is() — hover highlights primary or accent cards
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(
                            ":is(.is-primary, .is-accent):hover { bg: blue }",
                        ))
                        .child(
                            div()
                                .flex_row()
                                .gap(12.0)
                                .child(
                                    div()
                                        .class("is-card")
                                        .class("is-primary")
                                        .w(90.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Primary").size(12.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("is-card")
                                        .class("is-secondary")
                                        .w(90.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Secondary").size(12.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("is-card")
                                        .class("is-accent")
                                        .w(90.0)
                                        .h(60.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Accent").size(12.0).color(Color::WHITE)),
                                ),
                        ),
                )
                // 2. :where() — secondary gets purple hover
                .child(
                    div()
                        .flex_col()
                        .gap(4.0)
                        .child(code_label(":where(.is-secondary):hover { bg: purple }")),
                )
                // 3. :first-of-type / :last-of-type
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(
                            ":first-of-type { green }  :last-of-type { red }",
                        ))
                        .child(
                            div()
                                .flex_row()
                                .gap(12.0)
                                .child(
                                    div()
                                        .class("type-item")
                                        .w(80.0)
                                        .h(50.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("First").size(12.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("type-item")
                                        .w(80.0)
                                        .h(50.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Middle").size(12.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("type-item")
                                        .w(80.0)
                                        .h(50.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("Last").size(12.0).color(Color::WHITE)),
                                ),
                        ),
                )
                // 4. :nth-of-type(2) — highlights 2nd item
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(":nth-of-type(2) { orange }"))
                        .child(
                            div()
                                .flex_row()
                                .gap(12.0)
                                .child(
                                    div()
                                        .class("nth-type-item")
                                        .w(80.0)
                                        .h(50.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("1st").size(12.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("nth-type-item")
                                        .w(80.0)
                                        .h(50.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("2nd").size(12.0).color(Color::WHITE)),
                                )
                                .child(
                                    div()
                                        .class("nth-type-item")
                                        .w(80.0)
                                        .h(50.0)
                                        .flex_col()
                                        .justify_center()
                                        .items_center()
                                        .child(text("3rd").size(12.0).color(Color::WHITE)),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// FILTER BLUR & DROP-SHADOW SECTION
// ============================================================================

fn filter_blur_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Filter: blur() & drop-shadow()"))
        .child(section_description(
            "CSS filter: blur(Npx) renders via Kawase multi-pass GPU blur. drop-shadow() adds offset shadow. Both support transitions and @keyframes.",
        ))
        .child(
            div()
                .flex_col()
                .gap(16.0)
                // 1. Static blur
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("filter: blur(4px)"))
                        .child(
                            div()
                                .id("filter-blur-static")
                                .w(200.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Blurred").size(14.0).color(Color::WHITE)),
                        ),
                )
                // 2. Blur transition on hover
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("hover: blur(0px) → blur(8px)"))
                        .child(
                            div()
                                .id("filter-blur-hover")
                                .w(200.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Hover me").size(14.0).color(Color::WHITE)),
                        ),
                )
                // 3. Drop-shadow
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("filter: drop-shadow(4px 4px 8px rgba(0,0,0,0.5))"))
                        .child(
                            div()
                                .id("filter-drop-shadow")
                                .w(200.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Drop Shadow").size(14.0).color(Color::WHITE)),
                        ),
                )
                // 4. Combined blur + brightness
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("hover: blur(2px) brightness(1.2) → blur(0px) brightness(1.0)"))
                        .child(
                            div()
                                .id("filter-blur-combo")
                                .w(200.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Combo filter").size(14.0).color(Color::WHITE)),
                        ),
                )
                // 5. Blur keyframe animation
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("@keyframes blur-pulse { 0%→0px 50%→6px 100%→0px }"))
                        .child(
                            div()
                                .id("filter-blur-anim")
                                .w(200.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Pulsing blur").size(14.0).color(Color::WHITE)),
                        ),
                ),
        )
}

// ============================================================================
// BACKDROP-FILTER ANIMATION SECTION
// ============================================================================

fn backdrop_filter_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Backdrop-Filter Animation"))
        .child(section_description(
            "Animatable backdrop-filter: blur(), saturate(), brightness() on glass materials.",
        ))
        .child(
            div()
                .flex_col()
                .gap(16.0)
                // Colorful background strip behind glass elements
                .child(
                    div()
                        .flex_row()
                        .gap(12.0)
                        .child(
                            // Static backdrop blur
                            div()
                                .flex_col()
                                .gap(8.0)
                                .child(code_label("backdrop-filter: blur(12px)"))
                                .child(
                                    div()
                                        .w(140.0)
                                        .h(80.0)
                                        .rounded(12.0)
                                        .bg(Color::rgba(0.2, 0.6, 1.0, 0.3))
                                        .child(
                                            div()
                                                .id("backdrop-static")
                                                .w(140.0)
                                                .h(80.0)
                                                .flex_col()
                                                .justify_center()
                                                .items_center()
                                                .child(
                                                    text("Static Blur")
                                                        .size(12.0)
                                                        .color(Color::WHITE),
                                                ),
                                        ),
                                ),
                        )
                        .child(
                            // Hover → blur transition
                            div()
                                .flex_col()
                                .gap(8.0)
                                .child(code_label("hover: blur(0→20px)"))
                                .child(
                                    div()
                                        .w(140.0)
                                        .h(80.0)
                                        .rounded(12.0)
                                        .bg(Color::rgba(1.0, 0.4, 0.2, 0.4))
                                        .child(
                                            div()
                                                .id("backdrop-hover")
                                                .w(140.0)
                                                .h(80.0)
                                                .flex_col()
                                                .justify_center()
                                                .items_center()
                                                .child(
                                                    text("Hover Me").size(12.0).color(Color::WHITE),
                                                ),
                                        ),
                                ),
                        )
                        .child(
                            // Combo: blur + saturate transition
                            div()
                                .flex_col()
                                .gap(8.0)
                                .child(code_label("hover: blur+saturate"))
                                .child(
                                    div()
                                        .w(140.0)
                                        .h(80.0)
                                        .rounded(12.0)
                                        .bg(Color::rgba(0.4, 0.9, 0.3, 0.4))
                                        .child(
                                            div()
                                                .id("backdrop-combo")
                                                .w(140.0)
                                                .h(80.0)
                                                .flex_col()
                                                .justify_center()
                                                .items_center()
                                                .child(
                                                    text("Hover Combo")
                                                        .size(12.0)
                                                        .color(Color::WHITE),
                                                ),
                                        ),
                                ),
                        )
                        .child(
                            // Keyframe animation
                            div()
                                .flex_col()
                                .gap(8.0)
                                .child(code_label("@keyframes backdrop-pulse"))
                                .child(
                                    div()
                                        .w(140.0)
                                        .h(80.0)
                                        .rounded(12.0)
                                        .bg(Color::rgba(0.8, 0.3, 0.9, 0.4))
                                        .child(
                                            div()
                                                .id("backdrop-anim")
                                                .w(140.0)
                                                .h(80.0)
                                                .flex_col()
                                                .justify_center()
                                                .items_center()
                                                .child(
                                                    text("Animated").size(12.0).color(Color::WHITE),
                                                ),
                                        ),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// GRADIENT ANIMATION SECTION
// ============================================================================

fn gradient_animation_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Gradient Animation"))
        .child(section_description(
            "Gradient color stop and angle interpolation via transitions and @keyframes.",
        ))
        .child(
            div()
                .flex_col()
                .gap(16.0)
                // 1. Gradient hover transition (color change)
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("transition: all 500ms ease — gradient colors"))
                        .child(
                            div()
                                .id("grad-hover")
                                .w(200.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Hover me").size(14.0).color(Color::WHITE)),
                        ),
                )
                // 2. Gradient angle transition
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(
                            "transition: all 800ms ease — gradient angle 0deg → 180deg",
                        ))
                        .child(
                            div()
                                .id("grad-angle")
                                .w(200.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Hover me").size(14.0).color(Color::WHITE)),
                        ),
                )
                // 3. Gradient keyframe animation
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(
                            "@keyframes gradient-cycle { 0%..100% gradient colors }",
                        ))
                        .child(
                            div()
                                .id("grad-anim")
                                .w(200.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Cycling gradient").size(14.0).color(Color::WHITE)),
                        ),
                ),
        )
}

// ============================================================================
// TEXT SHADOW SECTION
// ============================================================================

fn text_shadow_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Text Shadow"))
        .child(section_description(
            "CSS text-shadow property with offset, blur, and color. Supports transitions and @keyframes.",
        ))
        .child(
            div()
                .flex_row()
                .gap(24.0)
                .flex_wrap()
                // 1. Basic drop shadow
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("text-shadow: 3px 3px 0px rgba(255, 68, 68, 1.0)"))
                        .child(
                            div()
                                .w(200.0)
                                .h(60.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(
                                    text("Drop Shadow")
                                        .id("text-shadow-basic")
                                        .size(22.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        ),
                )
                // 2. Green offset shadow
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("text-shadow: 2px 2px 0px rgba(34, 197, 94, 0.9)"))
                        .child(
                            div()
                                .w(200.0)
                                .h(60.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(
                                    text("Green Shadow")
                                        .id("text-shadow-glow")
                                        .size(22.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::rgba(0.23, 0.51, 0.96, 1.0)),
                                ),
                        ),
                )
                // 3. Hover transition
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("text-shadow: 2px 2px 0px (dark) → 3px 3px (red) on :hover"))
                        .child(
                            div()
                                .w(200.0)
                                .h(60.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(
                                    text("Hover me")
                                        .id("text-shadow-hover")
                                        .size(22.0)
                                        .weight(FontWeight::Bold)
                                        .color(Color::WHITE),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// OUTLINE SECTION
// ============================================================================

fn outline_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Outline Properties"))
        .child(section_description(
            "CSS outline with width, color, and offset. Outlines don't affect layout. Supports transitions and @keyframes.",
        ))
        .child(
            div()
                .flex_row()
                .gap(24.0)
                .flex_wrap()
                .items_center()
                // 1. Static outline
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("outline: 3px solid #f59e0b"))
                        .child(
                            div()
                                .id("outline-static")
                                .w(120.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Static").size(14.0).color(Color::WHITE)),
                        ),
                )
                // 2. Outline with offset
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("outline-offset: 6px"))
                        .child(
                            div()
                                .id("outline-offset")
                                .w(120.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Offset").size(14.0).color(Color::WHITE)),
                        ),
                )
                // 3. Outline transition on hover
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("outline + offset on :hover"))
                        .child(
                            div()
                                .id("outline-hover")
                                .w(120.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Hover me").size(14.0).color(Color::WHITE)),
                        ),
                )
                // 4. Outline color transition
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("outline-color transition"))
                        .child(
                            div()
                                .id("outline-color-hover")
                                .w(120.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Color hover").size(12.0).color(Color::WHITE)),
                        ),
                )
                // 5. @keyframes outline animation
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("@keyframes outline-pulse"))
                        .child(
                            div()
                                .id("outline-anim")
                                .w(120.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Animated").size(14.0).color(Color::WHITE)),
                        ),
                ),
        )
}

// ============================================================================
// FORM INPUT CSS STYLING SECTION
// ============================================================================

fn form_input_section() -> impl ElementBuilder + use<> {
    use std::sync::OnceLock;

    static INPUT_DATA: OnceLock<SharedTextInputState> = OnceLock::new();
    static ACCENT_DATA: OnceLock<SharedTextInputState> = OnceLock::new();
    static DISABLED_DATA: OnceLock<SharedTextInputState> = OnceLock::new();
    static TEXTAREA_DATA: OnceLock<SharedTextAreaState> = OnceLock::new();

    let input_data =
        INPUT_DATA.get_or_init(|| text_input_state_with_placeholder("Type something..."));
    let accent_data =
        ACCENT_DATA.get_or_init(|| text_input_state_with_placeholder("Accent styled..."));
    let disabled_data =
        DISABLED_DATA.get_or_init(|| text_input_state_with_placeholder("Cannot edit"));
    let textarea_data = TEXTAREA_DATA
        .get_or_init(|| text_area_state_with_placeholder("Write your thoughts here..."));

    section_container()
        .child(section_title("Form Input CSS Styling"))
        .child(section_description(
            "TextInput and TextArea styled via CSS selectors. Supports :hover, :focus, ::placeholder, caret-color, and outline.",
        ))
        .child(
            div()
                .flex_col()
                .gap(20.0)
                // 1. CSS-styled text input
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#demo-input { background: #1e293b; border-color: #475569; caret-color: #60a5fa; }"))
                        .child(
                            div()
                                .flex_row()
                                .gap(16.0)
                                .items_center()
                                .child(
                                    text_input(input_data)
                                        .id("demo-input")
                                        .w(300.0),
                                )
                                .child(
                                    text("Hover and click to see :hover / :focus / outline transitions")
                                        .size(12.0)
                                        .color(Color::rgba(0.5, 0.5, 0.5, 1.0)),
                                ),
                        ),
                )
                // 2. Accent-colored input
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#accent-input { background: #fefce8; border-color: #eab308; caret-color: #ca8a04; }"))
                        .child(
                            text_input(accent_data)
                                .id("accent-input")
                                .w(300.0),
                        ),
                )
                // 3. Disabled input
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#disabled-input { opacity: 0.5; } + .disabled(true)"))
                        .child(
                            text_input(disabled_data)
                                .id("disabled-input")
                                .w(300.0)
                                .disabled(true),
                        ),
                )
                // 4. CSS-styled text area
                .child(
                    div()
                        .flex_col()
                        .w_full()
                        .h_fit()
                        .gap(8.0)
                        .child(code_label("#demo-textarea { caret-color: #a78bfa; } :focus { outline + border-color: #8b5cf6; }"))
                        .child(
                            text_area(textarea_data)
                                .id("demo-textarea")
                                .w(400.0)
                                .h(120.0),
                        ),
                ),
        )
}

// ============================================================================
// CHECKBOX & RADIO CSS STYLING SECTION
// ============================================================================

fn form_controls_section(ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    let cb_state = ctx.use_state_keyed("demo_checkbox", || false);
    let cb_accent_state = ctx.use_state_keyed("accent_checkbox", || true);
    let cb_disabled_state = ctx.use_state_keyed("disabled_checkbox", || false);

    let radio_state = ctx.use_state_keyed("theme_radio", || "light".to_string());

    section_container()
        .child(section_title("Checkbox & Radio CSS Styling"))
        .child(section_description(
            "Checkbox and Radio widgets styled via CSS selectors. Supports :hover, :checked, :disabled, and accent-color.",
        ))
        .child(
            div()
                .flex_col()
                .gap(24.0)
                // Checkbox demos
                .child(
                    div()
                        .flex_col()
                        .gap(12.0)
                        .child(code_label("#demo-checkbox { accent-color: #3b82f6; } :hover { border-color: #3b82f6; } :checked { background: #3b82f6; }"))
                        .child(
                            div()
                                .flex_row()
                                .gap(32.0)
                                .items_center()
                                .child(
                                    checkbox(&cb_state)
                                        .id("demo-checkbox")
                                        .label("Accept terms"),
                                )
                                .child(
                                    text("Hover / click to see :hover and :checked CSS")
                                        .size(12.0)
                                        .color(Color::rgba(0.5, 0.5, 0.5, 1.0)),
                                ),
                        ),
                )
                // Accent checkbox
                .child(
                    div()
                        .flex_col()
                        .gap(12.0)
                        .child(code_label("#accent-checkbox { accent-color: #eab308; } :checked { background: #eab308; }"))
                        .child(
                            checkbox(&cb_accent_state)
                                .id("accent-checkbox")
                                .label("Amber accent checkbox (pre-checked)"),
                        ),
                )
                // Disabled checkbox
                .child(
                    div()
                        .flex_col()
                        .gap(12.0)
                        .child(code_label("#disabled-checkbox { opacity: 0.5; } + .disabled(true)"))
                        .child(
                            checkbox(&cb_disabled_state)
                                .id("disabled-checkbox")
                                .disabled(true)
                                .label("Disabled checkbox"),
                        ),
                )
                // Radio demos
                .child(
                    div()
                        .flex_col()
                        .gap(12.0)
                        .child(code_label("#theme-radio-*:checked { accent-color: #3b82f6; } :hover { border-color: #3b82f6; }"))
                        .child(
                            div()
                                .flex_row()
                                .gap(32.0)
                                .items_center()
                                .child(
                                    radio_group(&radio_state)
                                        .id("theme-radio")
                                        .label("Theme")
                                        .horizontal()
                                        .option("light", "Light")
                                        .option("dark", "Dark")
                                        .option("system", "System"),
                                )
                                .child(
                                    text("Hover / click to see :hover and :checked CSS on radio")
                                        .size(12.0)
                                        .color(Color::rgba(0.5, 0.5, 0.5, 1.0)),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// CSS STYLESHEET SECTION (automatic style application via ctx.add_css)
// ============================================================================

// ============================================================================
// GLOBAL TAG-NAME CSS SELECTORS
// ============================================================================

fn global_tag_selectors_section() -> impl ElementBuilder + use<> {
    // Simple inline SVG icons for the demo
    let star_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/></svg>"#;
    let heart_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/></svg>"#;

    section_container()
        .child(section_title("Global Tag-Name CSS Selectors"))
        .child(section_description(
            "Type selectors style ALL instances of a widget without needing #id. Per-instance #id overrides take higher priority.",
        ))
        .child(
            div()
                .flex_col()
                .gap(24.0)
                // --- Typography: h1 { } and p { } ---
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("h1 { color: var(--text-primary); letter-spacing: -0.5px; }"))
                        .child(h1("Heading Styled by Tag Selector"))
                        .child(code_label("p { color: var(--text-secondary); line-height: 1.6; }"))
                        .child(p("This paragraph is styled globally via the p { } type selector. All paragraphs get the same base styling without needing individual IDs.")),
                )
                // --- Blockquote: blockquote { } ---
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("blockquote { background: var(--surface-elevated); border-color: var(--primary); }"))
                        .child(
                            blockquote()
                                .child(p("This blockquote is styled by the global blockquote { } selector. No ID needed.")),
                        ),
                )
                // --- SVG: svg { } + #accent-svg override ---
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("svg { fill: var(--text-tertiary); }  +  #accent-svg { fill: var(--warning); }"))
                        .child(
                            div()
                                .flex_row()
                                .gap(16.0)
                                .items_center()
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(svg(star_svg).size(32.0, 32.0))
                                        .child(code_label("svg { }")),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(svg(heart_svg).size(32.0, 32.0))
                                        .child(code_label("svg { }")),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(svg(star_svg).size(32.0, 32.0).id("accent-svg"))
                                        .child(code_label("#accent-svg")),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(svg(star_svg).size(32.0, 32.0).id("stroke-svg"))
                                        .child(code_label(":hover stroke")),
                                ),
                        ),
                )
                // --- Explanation ---
                .child(
                    div()
                        .flex_col()
                        .gap(4.0)
                        .child(
                            text("Specificity: #id > type selector > universal *")
                                .size(12.0)
                                .color(Color::rgba(0.5, 0.6, 0.7, 1.0)),
                        )
                        .child(
                            text("The accent star uses #accent-svg { fill: var(--warning) } which overrides the global svg { fill: var(--text-tertiary) }.")
                                .size(12.0)
                                .color(Color::rgba(0.5, 0.6, 0.7, 1.0)),
                        ),
                ),
        )
}

fn css_stylesheet_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    section_container()
        .child(section_title("CSS Stylesheet (ctx.add_css)"))
        .child(section_description(
            "Styles applied automatically via ctx.add_css(). Elements get #id selectors — no manual wiring needed.",
        ))
        .child(
            div()
                .flex_col()
                .gap(8.0)
                .child(
                    text("ctx.add_css(\"#css-card { background: #3b82f6; border-radius: 12px; ... }\")")
                        .size(12.0)
                        .color(text_secondary),
                )
                .child(
                    div()
                        .flex_row()
                        .flex_wrap()
                        .gap(16.0)
                        // Card styled by stylesheet
                        .child(
                            div()
                                .flex_col()
                                .gap(8.0)
                                .child(code_label("#css-card"))
                                .child(div().w(80.0).h(80.0).id("css-card")),
                        )
                        // Alert styled by stylesheet
                        .child(
                            div()
                                .flex_col()
                                .gap(8.0)
                                .child(code_label("#css-alert"))
                                .child(div().w(80.0).h(80.0).id("css-alert")),
                        )
                        // Glass styled by stylesheet
                        .child(
                            div()
                                .flex_col()
                                .gap(8.0)
                                .child(code_label("#css-glass"))
                                .child(
                                    div()
                                        .w(80.0)
                                        .h(80.0)
                                        .id("css-glass")
                                        .bg(Color::rgb(0.3, 0.4, 0.6)),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// CSS HOVER SECTION (automatic :hover state styles)
// ============================================================================

fn css_hover_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("CSS :hover States"))
        .child(section_description(
            "Hover over boxes to see automatic :hover styles. Defined in stylesheet, applied by the framework.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#hover-blue"))
                        .child(div().w(80.0).h(80.0).id("hover-blue")),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#hover-green"))
                        .child(div().w(80.0).h(80.0).id("hover-green")),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#hover-purple"))
                        .child(div().w(80.0).h(80.0).id("hover-purple")),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#hover-orange"))
                        .child(div().w(80.0).h(80.0).id("hover-orange")),
                ),
        )
}

// ============================================================================
// CSS ANIMATION SECTION (@keyframes + animation property)
// ============================================================================

fn css_animation_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("CSS @keyframes Animations"))
        .child(section_description(
            "CSS animations via @keyframes. Defined in stylesheet, ticked automatically each frame.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#anim-pulse (2s infinite)"))
                        .child(div().w(80.0).h(80.0).id("anim-pulse")),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#anim-glow (3s infinite)"))
                        .child(div().w(80.0).h(80.0).id("anim-glow")),
                ),
        )
}

// ============================================================================
// CSS MACRO SECTION
// ============================================================================

fn css_macro_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("css_style! Macro"))
        .child(section_description(
            "CSS-like syntax with hyphenated property names and semicolon separators.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                // Basic card with CSS properties
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(
                            "css_style! { background: ...; border-radius: ...; }",
                        ))
                        .child(styled_box_with_element_style(css_style! {
                            background: Color::BLUE;
                            border-radius: 8.0;
                            opacity: 0.9;
                        })),
                )
                // Shadow presets
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("css_style! { box-shadow: md; }"))
                        .child(styled_box_with_element_style(css_style! {
                            background: Color::WHITE;
                            border-radius: 12.0;
                            box-shadow: md;
                        })),
                )
                // Custom shadow
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("css_style! { box-shadow: Shadow::new(...); }"))
                        .child(styled_box_with_element_style(css_style! {
                            background: Color::GREEN;
                            border-radius: 8.0;
                            box-shadow: Shadow::new(4.0, 8.0, 12.0, Color::BLACK.with_alpha(0.3));
                        })),
                )
                // Backdrop filter (glass)
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("css_style! { backdrop-filter: glass; }"))
                        .child(styled_box_with_element_style(css_style! {
                            background: Color::WHITE.with_alpha(0.2);
                            border-radius: 16.0;
                            backdrop-filter: glass;
                        })),
                ),
        )
}

// ============================================================================
// STYLE MACRO SECTION
// ============================================================================

fn style_macro_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("style! Macro"))
        .child(section_description(
            "Rust-friendly syntax with underscored names and comma separators.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                // Basic card
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("style! { bg: ..., rounded: ... }"))
                        .child(styled_box_with_element_style(style! {
                            bg: Color::PURPLE,
                            rounded: 8.0,
                            opacity: 0.9,
                        })),
                )
                // Preset methods
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("style! { rounded_lg, shadow_md }"))
                        .child(styled_box_with_element_style(style! {
                            bg: Color::WHITE,
                            rounded_lg,
                            shadow_md,
                        })),
                )
                // Transform shortcuts
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("style! { scale: 1.1 }"))
                        .child(styled_box_with_element_style(style! {
                            bg: Color::ORANGE,
                            rounded: 8.0,
                            scale: 1.1,
                        })),
                )
                // Material presets
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("style! { gold, rounded_xl }"))
                        .child(styled_box_with_element_style(style! {
                            bg: Color::from_hex(0xD4AF37), // Gold color
                            gold,
                            rounded_xl,
                        })),
                ),
        )
}

// ============================================================================
// BUILDER PATTERN SECTION
// ============================================================================

fn builder_pattern_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("ElementStyle Builder"))
        .child(section_description(
            "Programmatic construction using method chaining.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                // Basic builder
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("ElementStyle::new().bg().rounded()"))
                        .child(styled_box_with_element_style(
                            ElementStyle::new().bg(Color::CYAN).rounded(8.0).shadow_sm(),
                        )),
                )
                // Advanced builder
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(".rounded_corners().shadow_lg()"))
                        .child(styled_box_with_element_style(
                            ElementStyle::new()
                                .bg(Color::MAGENTA)
                                .rounded_corners(16.0, 16.0, 0.0, 0.0)
                                .shadow_lg(),
                        )),
                )
                // With transform
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(".rotate_deg(10.0)"))
                        .child(styled_box_with_element_style(
                            ElementStyle::new()
                                .bg(Color::from_hex(0x008080)) // Teal
                                .rounded(12.0)
                                .rotate_deg(10.0),
                        )),
                )
                // With material
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(".chrome().rounded(24.0)"))
                        .child(styled_box_with_element_style(
                            ElementStyle::new()
                                .bg(Color::from_hex(0xC0C0C8))
                                .chrome()
                                .rounded(24.0),
                        )),
                ),
        )
}

// ============================================================================
// CSS PARSER SECTION
// ============================================================================

fn css_parser_section() -> impl ElementBuilder + use<> {
    // Define CSS as a string using #id selectors
    let css_string = r#"
        #parser-card {
            background: #3b82f6;
            border-radius: 12px;
            box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
        }

        #parser-alert {
            background: #ef4444;
            border-radius: 8px;
            opacity: 0.95;
        }

        #parser-glass {
            background: rgba(255, 255, 255, 0.15);
            border-radius: 16px;
            backdrop-filter: blur(10px);
        }
    "#;

    // Parse at runtime
    let stylesheet = Stylesheet::parse(css_string).expect("valid CSS");
    let card_style = stylesheet.get("parser-card");
    let alert_style = stylesheet.get("parser-alert");
    let glass_style = stylesheet.get("parser-glass");

    section_container()
        .child(section_title("CSS Parser (Runtime)"))
        .child(section_description(
            "Parse CSS strings at runtime using Stylesheet::parse(). Uses #id selectors.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                // Card style from CSS
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#parser-card { ... }"))
                        .child(if let Some(s) = card_style {
                            styled_box_with_element_style(s.clone())
                        } else {
                            styled_box_with_element_style(ElementStyle::new().bg(Color::GRAY))
                        }),
                )
                // Alert style from CSS
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#parser-alert { ... }"))
                        .child(if let Some(s) = alert_style {
                            styled_box_with_element_style(s.clone())
                        } else {
                            styled_box_with_element_style(ElementStyle::new().bg(Color::GRAY))
                        }),
                )
                // Glass style from CSS
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("#parser-glass { ... }"))
                        .child(if let Some(s) = glass_style {
                            styled_box_with_element_style(s.clone())
                        } else {
                            styled_box_with_element_style(ElementStyle::new().bg(Color::GRAY))
                        }),
                ),
        )
}

// ============================================================================
// STYLE MERGING SECTION
// ============================================================================

fn style_merging_section() -> impl ElementBuilder + use<> {
    // Base style
    let base = style! {
        bg: Color::BLUE,
        rounded: 12.0,
        shadow_md,
    };

    // Hover override
    let hover_overlay = style! {
        bg: Color::from_hex(0x3B82F6), // Lighter blue
        scale: 1.05,
    };

    // Merged result
    let merged = base.merge(&hover_overlay);

    section_container()
        .child(section_title("Style Merging"))
        .child(section_description(
            "Merge styles to create state-specific variants. Properties from overlay override base.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                .items_end()
                // Base style
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("Base style"))
                        .child(styled_box_with_element_style(base.clone())),
                )
                // Plus sign
                .child(
                    text("+")
                        .size(24.0)
                        .color(ThemeState::get().color(ColorToken::TextSecondary)),
                )
                // Hover overlay
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("Hover overlay"))
                        .child(styled_box_with_element_style(hover_overlay)),
                )
                // Equals sign
                .child(
                    text("=")
                        .size(24.0)
                        .color(ThemeState::get().color(ColorToken::TextSecondary)),
                )
                // Merged result
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("Merged result"))
                        .child(styled_box_with_element_style(merged)),
                ),
        )
}

// ============================================================================
// BACKGROUNDS SECTION
// ============================================================================

fn backgrounds_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Backgrounds"))
        .child(section_description(
            "Solid colors with various construction methods.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                // Solid color
                .child(labeled_box(
                    "Solid RED",
                    style! { bg: Color::RED, rounded: 8.0 },
                ))
                // With alpha
                .child(labeled_box(
                    "With Alpha",
                    style! { bg: Color::GREEN.with_alpha(0.6), rounded: 8.0 },
                ))
                // From hex
                .child(labeled_box(
                    "from_hex(0x9333EA)",
                    style! { bg: Color::from_hex(0x9333EA), rounded: 8.0 },
                ))
                // From hex (orange)
                .child(labeled_box(
                    "from_hex(0xF97316)",
                    style! { bg: Color::from_hex(0xF97316), rounded: 8.0 },
                ))
                // rgb() constructor
                .child(labeled_box(
                    "rgb(0.2, 0.6, 0.9)",
                    style! { bg: Color::rgb(0.2, 0.6, 0.9), rounded: 8.0 },
                )),
        )
}

// ============================================================================
// CORNER RADIUS SECTION
// ============================================================================

fn corner_radius_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Corner Radius"))
        .child(section_description(
            "Uniform and per-corner radii with theme presets.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                // None
                .child(labeled_box(
                    "rounded_none",
                    style! { bg: Color::BLUE, rounded_none },
                ))
                // Small
                .child(labeled_box(
                    "rounded_sm",
                    style! { bg: Color::BLUE, rounded_sm },
                ))
                // Medium
                .child(labeled_box(
                    "rounded_md",
                    style! { bg: Color::BLUE, rounded_md },
                ))
                // Large
                .child(labeled_box(
                    "rounded_lg",
                    style! { bg: Color::BLUE, rounded_lg },
                ))
                // XL
                .child(labeled_box(
                    "rounded_xl",
                    style! { bg: Color::BLUE, rounded_xl },
                ))
                // 2XL
                .child(labeled_box(
                    "rounded_2xl",
                    style! { bg: Color::BLUE, rounded_2xl },
                ))
                // Full (pill)
                .child(labeled_box(
                    "rounded_full",
                    style! { bg: Color::BLUE, rounded_full },
                ))
                // Custom per-corner
                .child(labeled_box(
                    "Top only",
                    style! { bg: Color::BLUE, rounded_corners: (16.0, 16.0, 0.0, 0.0) },
                ))
                // Custom uniform
                .child(labeled_box(
                    "rounded: 20.0",
                    css_style! { background: Color::BLUE; border-radius: 20.0; },
                )),
        )
}

// ============================================================================
// CORNER SHAPE SECTION (superellipse)
// ============================================================================

fn corner_shape_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Corner Shape (Superellipse)"))
        .child(section_description(
            "Control corner curvature via superellipse math. Works with border-radius. \
             Values: round (1.0), bevel (0.0), squircle (2.0), scoop (-1.0), notch, square.",
        ))
        .child(
            div()
                .flex_col()
                .gap(16.0)
                // Static shapes via Div builder
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("Div builder: .rounded(32.0).corner_shape(n)"))
                        .child(
                            div()
                                .flex_row()
                                .flex_wrap()
                                .gap(16.0)
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("round (1.0)"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(32.0)
                                                .corner_shape(1.0)
                                                .bg(Color::rgba(0.23, 0.51, 0.96, 1.0)),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("bevel (0.0)"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(32.0)
                                                .corner_bevel()
                                                .bg(Color::rgba(0.13, 0.77, 0.37, 1.0)),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("squircle (2.0)"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(32.0)
                                                .corner_squircle()
                                                .bg(Color::rgba(0.66, 0.33, 0.97, 1.0)),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("scoop (-1.0)"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(32.0)
                                                .corner_scoop()
                                                .bg(Color::rgba(0.98, 0.45, 0.09, 1.0)),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("notch (-100)"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(10.0)
                                                .corner_shape(-100.0)
                                                .bg(Color::rgba(0.93, 0.27, 0.61, 1.0)),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("square (100)"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(32.0)
                                                .corner_shape(100.0)
                                                .bg(Color::rgba(0.07, 0.55, 0.45, 1.0)),
                                        ),
                                ),
                        ),
                )
                // Per-corner shapes
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(
                            ".corner_shapes(2.0, 0.0, 2.0, 0.0) — mixed per-corner",
                        ))
                        .child(
                            div()
                                .flex_row()
                                .gap(16.0)
                                .child(
                                    div()
                                        .w(80.0)
                                        .h(80.0)
                                        .rounded(32.0)
                                        .corner_shapes(2.0, 0.0, 2.0, 0.0)
                                        .bg(Color::rgba(0.39, 0.40, 0.95, 1.0)),
                                )
                                .child(
                                    div()
                                        .w(80.0)
                                        .h(80.0)
                                        .rounded(32.0)
                                        .corner_shapes(0.0, 2.0, 0.0, 2.0)
                                        .bg(Color::rgba(0.85, 0.53, 0.05, 1.0)),
                                )
                                .child(
                                    div()
                                        .w(80.0)
                                        .h(80.0)
                                        .rounded(32.0)
                                        .corner_shapes(-1.0, 2.0, -1.0, 2.0)
                                        .bg(Color::rgba(0.95, 0.24, 0.24, 1.0)),
                                ),
                        ),
                )
                // Bordered shapes — verifies border follows corner_shape
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(
                            "Bordered shapes: .border(3.0, color).corner_shape(n)",
                        ))
                        .child(
                            div()
                                .flex_row()
                                .flex_wrap()
                                .gap(16.0)
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("round border"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(32.0)
                                                .corner_shape(1.0)
                                                .border(3.0, Color::rgba(0.23, 0.51, 0.96, 1.0))
                                                .bg(Color::rgba(0.93, 0.95, 1.0, 1.0)),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("squircle border"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(32.0)
                                                .corner_squircle()
                                                .border(3.0, Color::rgba(0.66, 0.33, 0.97, 1.0))
                                                .bg(Color::rgba(0.96, 0.93, 1.0, 1.0)),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("bevel border"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(32.0)
                                                .corner_bevel()
                                                .border(3.0, Color::rgba(0.13, 0.77, 0.37, 1.0))
                                                .bg(Color::rgba(0.93, 1.0, 0.95, 1.0)),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("scoop border"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(32.0)
                                                .corner_scoop()
                                                .border(3.0, Color::rgba(0.98, 0.45, 0.09, 1.0))
                                                .bg(Color::rgba(1.0, 0.95, 0.93, 1.0)),
                                        ),
                                ),
                        ),
                )
                // Bordered shapes with shadow — verifies shadow matches corner_shape
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(
                            "Shadow + border + corner_shape (shadow must follow shape)",
                        ))
                        .child(
                            div()
                                .flex_row()
                                .flex_wrap()
                                .gap(24.0)
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("round shadow"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(32.0)
                                                .corner_shape(1.0)
                                                .border(2.0, Color::rgba(0.2, 0.2, 0.2, 0.3))
                                                .shadow_lg()
                                                .bg(Color::WHITE),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("squircle shadow"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(32.0)
                                                .corner_squircle()
                                                .border(2.0, Color::rgba(0.2, 0.2, 0.2, 0.3))
                                                .shadow_lg()
                                                .bg(Color::WHITE),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("scoop shadow"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(32.0)
                                                .corner_scoop()
                                                .border(2.0, Color::rgba(0.2, 0.2, 0.2, 0.3))
                                                .shadow_lg()
                                                .bg(Color::WHITE),
                                        ),
                                ),
                        ),
                )
                // Bordered shapes with overflow_clip — verifies clip doesn't double border
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("Border + overflow_clip (no doubled corners)"))
                        .child(
                            div()
                                .flex_row()
                                .flex_wrap()
                                .gap(16.0)
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("round clip"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(32.0)
                                                .corner_shape(1.0)
                                                .border(3.0, Color::rgba(0.23, 0.51, 0.96, 1.0))
                                                .bg(Color::rgba(0.93, 0.95, 1.0, 1.0))
                                                .overflow_clip(),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .items_center()
                                        .child(code_label("squircle clip"))
                                        .child(
                                            div()
                                                .w(80.0)
                                                .h(80.0)
                                                .rounded(32.0)
                                                .corner_squircle()
                                                .border(3.0, Color::rgba(0.66, 0.33, 0.97, 1.0))
                                                .bg(Color::rgba(0.96, 0.93, 1.0, 1.0))
                                                .overflow_clip(),
                                        ),
                                ),
                        ),
                )
                // CSS transition demo
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(
                            "CSS transition: corner-shape round → squircle on hover",
                        ))
                        .child(
                            div()
                                .id("cs-transition")
                                .w(120.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Hover me").size(13.0).color(Color::WHITE)),
                        ),
                )
                // CSS keyframe animation
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label(
                            "@keyframes cs-morph: round → bevel → squircle → scoop",
                        ))
                        .child(
                            div()
                                .id("cs-anim")
                                .w(120.0)
                                .h(80.0)
                                .flex_col()
                                .justify_center()
                                .items_center()
                                .child(text("Morphing").size(13.0).color(Color::WHITE)),
                        ),
                ),
        )
}

// ============================================================================
// SHADOWS SECTION
// ============================================================================

fn shadows_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Box Shadows"))
        .child(section_description(
            "Shadow presets (sm, md, lg, xl) and custom shadows.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                // Shadow presets
                .child(labeled_box(
                    "shadow_sm",
                    style! { bg: Color::WHITE, rounded: 8.0, shadow_sm },
                ))
                .child(labeled_box(
                    "shadow_md",
                    style! { bg: Color::WHITE, rounded: 8.0, shadow_md },
                ))
                .child(labeled_box(
                    "shadow_lg",
                    style! { bg: Color::WHITE, rounded: 8.0, shadow_lg },
                ))
                .child(labeled_box(
                    "shadow_xl",
                    style! { bg: Color::WHITE, rounded: 8.0, shadow_xl },
                ))
                // CSS syntax presets
                .child(labeled_box(
                    "box-shadow: md",
                    css_style! { background: Color::WHITE; border-radius: 8.0; box-shadow: md; },
                ))
                // Custom shadow
                .child(labeled_box(
                    "Custom shadow",
                    ElementStyle::new()
                        .bg(Color::WHITE)
                        .rounded(8.0)
                        .shadow(Shadow::new(8.0, 8.0, 16.0, Color::PURPLE.with_alpha(0.4))),
                ))
                // No shadow (explicit)
                .child(labeled_box(
                    "shadow_none",
                    style! { bg: Color::WHITE, rounded: 8.0, shadow_none },
                )),
        )
}

// ============================================================================
// TRANSFORMS SECTION
// ============================================================================

fn transforms_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Transforms"))
        .child(section_description(
            "Scale, rotate, and translate transformations.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(32.0)
                // Scale up
                .child(labeled_box(
                    "scale: 1.2",
                    style! { bg: Color::GREEN, rounded: 8.0, scale: 1.2 },
                ))
                // Scale down
                .child(labeled_box(
                    "scale: 0.8",
                    style! { bg: Color::GREEN, rounded: 8.0, scale: 0.8 },
                ))
                // Non-uniform scale
                .child(labeled_box(
                    "scale_xy",
                    style! { bg: Color::GREEN, rounded: 8.0, scale_xy: (1.3, 0.8) },
                ))
                // Rotate
                .child(labeled_box(
                    "rotate_deg: 15",
                    style! { bg: Color::ORANGE, rounded: 8.0, rotate_deg: 15.0 },
                ))
                // Rotate negative
                .child(labeled_box(
                    "rotate_deg: -10",
                    style! { bg: Color::ORANGE, rounded: 8.0, rotate_deg: -10.0 },
                ))
                // Translate
                .child(labeled_box(
                    "translate: (10, 5)",
                    style! { bg: Color::PURPLE, rounded: 8.0, translate: (10.0, 5.0) },
                ))
                // CSS transform syntax
                .child(labeled_box(
                    "CSS transform",
                    css_style! {
                        background: Color::CYAN;
                        border-radius: 8.0;
                        transform: Transform::rotate(0.2);
                    },
                )),
        )
}

// ============================================================================
// OPACITY SECTION
// ============================================================================

fn opacity_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let checkerboard = theme.color(ColorToken::SurfaceElevated);

    section_container()
        .child(section_title("Opacity"))
        .child(section_description(
            "Control element transparency with opacity values and presets.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                // Show on checkerboard to demonstrate opacity
                .child(opacity_demo_box(
                    "opacity: 1.0",
                    style! { bg: Color::RED, rounded: 8.0, opacity: 1.0 },
                    checkerboard,
                ))
                .child(opacity_demo_box(
                    "opacity: 0.75",
                    style! { bg: Color::RED, rounded: 8.0, opacity: 0.75 },
                    checkerboard,
                ))
                .child(opacity_demo_box(
                    "opacity: 0.5",
                    style! { bg: Color::RED, rounded: 8.0, opacity: 0.5 },
                    checkerboard,
                ))
                .child(opacity_demo_box(
                    "opacity: 0.25",
                    style! { bg: Color::RED, rounded: 8.0, opacity: 0.25 },
                    checkerboard,
                ))
                .child(opacity_demo_box(
                    "opaque",
                    style! { bg: Color::BLUE, rounded: 8.0, opaque },
                    checkerboard,
                ))
                .child(opacity_demo_box(
                    "translucent",
                    style! { bg: Color::BLUE, rounded: 8.0, translucent },
                    checkerboard,
                ))
                .child(opacity_demo_box(
                    "transparent",
                    style! { bg: Color::BLUE, rounded: 8.0, transparent },
                    checkerboard,
                )),
        )
}

fn opacity_demo_box(label: &str, style: ElementStyle, bg: Color) -> impl ElementBuilder + use<> {
    div().flex_col().gap(8.0).child(code_label(label)).child(
        div()
            .w(80.0)
            .h(80.0)
            .bg(bg)
            .rounded(8.0)
            .items_center()
            .justify_center()
            .child(styled_box_with_element_style(style)),
    )
}

// ============================================================================
// MATERIALS SECTION
// ============================================================================

fn materials_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Materials"))
        .child(section_description(
            "Glass, metallic, chrome, gold, and wood effects (Blinc extensions).",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                // Glass
                .child(labeled_box(
                    "glass",
                    style! { bg: Color::WHITE.with_alpha(0.2), rounded: 16.0, glass },
                ))
                // Metallic
                .child(labeled_box(
                    "metallic",
                    style! { bg: Color::from_hex(0xB4B4BE), rounded: 8.0, metallic },
                ))
                // Chrome
                .child(labeled_box(
                    "chrome",
                    style! { bg: Color::from_hex(0xC8C8D2), rounded: 8.0, chrome },
                ))
                // Gold
                .child(labeled_box(
                    "gold",
                    style! { bg: Color::from_hex(0xD4AF37), rounded: 8.0, gold },
                ))
                // Wood
                .child(labeled_box(
                    "wood",
                    style! { bg: Color::from_hex(0x8B5A2B), rounded: 8.0, wood },
                ))
                // CSS backdrop-filter syntax
                .child(labeled_box(
                    "backdrop-filter: glass",
                    css_style! {
                        background: Color::WHITE.with_alpha(0.15);
                        border-radius: 16.0;
                        backdrop-filter: glass;
                    },
                )),
        )
}

// ============================================================================
// 3D TRANSFORMS SECTION
// ============================================================================

fn transforms_3d_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("3D Transforms"))
        .child(section_description(
            "rotate-x, rotate-y with perspective for 3D element rotation.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                .child(labeled_3d_box(
                    "rotate-x: 30",
                    css_style! {
                        background: Color::from_hex(0x3b82f6);
                        border-radius: 8.0;
                        rotate-x: 30.0;
                        perspective: 800.0;
                    },
                ))
                .child(labeled_3d_box(
                    "rotate-y: 30",
                    css_style! {
                        background: Color::from_hex(0x22c55e);
                        border-radius: 8.0;
                        rotate-y: 30.0;
                        perspective: 800.0;
                    },
                ))
                .child(labeled_3d_box(
                    "rotate-x + rotate-y",
                    style! {
                        bg: Color::from_hex(0xf97316),
                        rounded: 8.0,
                        rotate_x: 20.0,
                        rotate_y: 25.0,
                        perspective: 800.0,
                    },
                ))
                .child(labeled_3d_box(
                    "perspective: 200",
                    style! {
                        bg: Color::from_hex(0xa855f7),
                        rounded: 8.0,
                        rotate_y: 30.0,
                        perspective: 200.0,
                    },
                ))
                .child(labeled_3d_box(
                    "perspective: 2000",
                    style! {
                        bg: Color::from_hex(0xec4899),
                        rounded: 8.0,
                        rotate_y: 30.0,
                        perspective: 2000.0,
                    },
                )),
        )
}

// ============================================================================
// 3D SDF SHAPES SECTION
// ============================================================================

fn sdf_3d_shapes_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("3D SDF Shapes"))
        .child(section_description(
            "Raymarched signed distance field shapes via shape-3d with depth and perspective.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                .child(labeled_3d_box(
                    "box",
                    style! {
                        bg: Color::from_hex(0x3b82f6),
                        shape_3d: "box",
                        depth: 80.0,
                        perspective: 800.0,
                        rotate_x: 15.0,
                        rotate_y: 20.0,
                    },
                ))
                .child(labeled_3d_box(
                    "sphere",
                    style! {
                        bg: Color::from_hex(0x22c55e),
                        shape_3d: "sphere",
                        depth: 120.0,
                        perspective: 800.0,
                        rotate_x: 10.0,
                        rotate_y: 15.0,
                    },
                ))
                .child(labeled_3d_box(
                    "cylinder",
                    style! {
                        bg: Color::from_hex(0xf97316),
                        shape_3d: "cylinder",
                        depth: 120.0,
                        perspective: 800.0,
                        rotate_x: 20.0,
                        rotate_y: 15.0,
                    },
                ))
                .child(labeled_3d_box(
                    "torus",
                    style! {
                        bg: Color::from_hex(0xa855f7),
                        shape_3d: "torus",
                        depth: 120.0,
                        perspective: 800.0,
                        rotate_x: 25.0,
                        rotate_y: 15.0,
                    },
                ))
                .child(labeled_3d_box(
                    "capsule",
                    style! {
                        bg: Color::from_hex(0xec4899),
                        shape_3d: "capsule",
                        depth: 120.0,
                        perspective: 800.0,
                        rotate_x: 15.0,
                        rotate_y: 20.0,
                    },
                )),
        )
}

// ============================================================================
// 3D LIGHTING SECTION
// ============================================================================

fn lighting_3d_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("3D Lighting"))
        .child(section_description(
            "Blinn-Phong shading with configurable light direction, intensity, ambient, and specular.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                .child(labeled_3d_box(
                    "Default lighting",
                    style! {
                        bg: Color::from_hex(0x3b82f6),
                        shape_3d: "sphere",
                        depth: 120.0,
                        perspective: 800.0,
                    },
                ))
                .child(labeled_3d_box(
                    "Top light",
                    style! {
                        bg: Color::from_hex(0x3b82f6),
                        shape_3d: "sphere",
                        depth: 120.0,
                        perspective: 800.0,
                        light_direction: (0.0, -1.0, 0.5),
                    },
                ))
                .child(labeled_3d_box(
                    "Side light",
                    style! {
                        bg: Color::from_hex(0x3b82f6),
                        shape_3d: "sphere",
                        depth: 120.0,
                        perspective: 800.0,
                        light_direction: (1.0, 0.0, 0.5),
                    },
                ))
                .child(labeled_3d_box(
                    "High specular",
                    style! {
                        bg: Color::from_hex(0x3b82f6),
                        shape_3d: "sphere",
                        depth: 120.0,
                        perspective: 800.0,
                        specular: 64.0,
                        light_intensity: 1.5,
                    },
                ))
                .child(labeled_3d_box(
                    "High ambient",
                    style! {
                        bg: Color::from_hex(0x3b82f6),
                        shape_3d: "sphere",
                        depth: 120.0,
                        perspective: 800.0,
                        ambient: 0.8,
                    },
                )),
        )
}

// ============================================================================
// TRANSLATE-Z SECTION
// ============================================================================

fn translate_z_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("translate-z"))
        .child(section_description(
            "Z-axis positioning on 3D shapes. Positive moves toward viewer (appears larger), negative moves away.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                .child(labeled_3d_box(
                    "translate-z: 0",
                    css_style! {
                        background: Color::from_hex(0x3b82f6);
                        shape-3d: "box";
                        depth: 80.0;
                        perspective: 600.0;
                        rotate-y: 15.0;
                        translate-z: 0.0;
                    },
                ))
                .child(labeled_3d_box(
                    "translate-z: 100",
                    css_style! {
                        background: Color::from_hex(0x3b82f6);
                        shape-3d: "box";
                        depth: 80.0;
                        perspective: 600.0;
                        rotate-y: 15.0;
                        translate-z: 100.0;
                    },
                ))
                .child(labeled_3d_box(
                    "translate-z: 200",
                    css_style! {
                        background: Color::from_hex(0x3b82f6);
                        shape-3d: "box";
                        depth: 80.0;
                        perspective: 600.0;
                        rotate-y: 15.0;
                        translate-z: 200.0;
                    },
                ))
                .child(labeled_3d_box(
                    "translate-z: -100",
                    css_style! {
                        background: Color::from_hex(0x3b82f6);
                        shape-3d: "box";
                        depth: 80.0;
                        perspective: 600.0;
                        rotate-y: 15.0;
                        translate-z: -100.0;
                    },
                )),
        )
}

// ============================================================================
// UV MAPPING SECTION
// ============================================================================

fn uv_mapping_3d_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("3D UV Mapping"))
        .child(section_description(
            "Background colors and gradients automatically mapped onto 3D surfaces.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("Box + linear gradient"))
                        .child(div().w(120.0).h(120.0).id("uv-box-gradient")),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("Sphere + linear gradient"))
                        .child(div().w(120.0).h(120.0).id("uv-sphere-gradient")),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("Cylinder + radial gradient"))
                        .child(div().w(120.0).h(120.0).id("uv-cylinder-gradient")),
                )
                .child(labeled_3d_box(
                    "Solid on sphere",
                    style! {
                        bg: Color::from_hex(0xD4AF37),
                        shape_3d: "sphere",
                        depth: 120.0,
                        perspective: 800.0,
                    },
                )),
        )
}

// ============================================================================
// 3D ANIMATION SECTION
// ============================================================================

fn animation_3d_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("3D Animations"))
        .child(section_description(
            "CSS @keyframes animating rotate-x, rotate-y, and translate-z.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("spin-y (4s infinite)"))
                        .child(div().w(120.0).h(120.0).id("anim-spin-3d")),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("wobble-3d (3s infinite)"))
                        .child(div().w(120.0).h(120.0).id("anim-wobble-3d")),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("float-z (2s infinite)"))
                        .child(div().w(120.0).h(120.0).id("anim-float-3d")),
                ),
        )
}

// ============================================================================
// 3D GROUP COMPOSITION SECTION
// ============================================================================

fn group_composition_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("3D Group Composition"))
        .child(section_description(
            "Compound 3D shapes via shape-3d: group. Children contribute to a single SDF with boolean operations.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                // Box with cylinder subtracted (drilled hole)
                // Children use absolute positioning to overlap at same center
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("subtract (hole)"))
                        .child(
                            div()
                                .relative()
                                .w(200.0)
                                .h(200.0)
                                .id("group-subtract")
                                .child(div().absolute().inset(0.0).id("group-subtract-base"))
                                .child(div().absolute().inset(0.0).id("group-subtract-hole")),
                        ),
                )
                // Smooth blob union (two spheres merging, offset to show blob)
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("smooth-union (blob)"))
                        .child(
                            div()
                                .relative()
                                .w(200.0)
                                .h(200.0)
                                .id("group-smooth-union")
                                .child(
                                    div()
                                        .absolute()
                                        .top(0.0)
                                        .left(0.0)
                                        .bottom(70.0)
                                        .right(70.0)
                                        .id("group-smooth-union-a"),
                                )
                                .child(
                                    div()
                                        .absolute()
                                        .top(70.0)
                                        .left(70.0)
                                        .bottom(0.0)
                                        .right(0.0)
                                        .id("group-smooth-union-b"),
                                ),
                        ),
                )
                // Rounded intersection (box ∩ sphere)
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("intersect (rounded)"))
                        .child(
                            div()
                                .relative()
                                .w(200.0)
                                .h(200.0)
                                .id("group-intersect")
                                .child(div().absolute().inset(0.0).id("group-intersect-box"))
                                .child(div().absolute().inset(0.0).id("group-intersect-sphere")),
                        ),
                )
                // Smooth scoop (box with sphere scooped out)
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("smooth-subtract"))
                        .child(
                            div()
                                .relative()
                                .w(200.0)
                                .h(200.0)
                                .id("group-smooth-subtract")
                                .child(div().absolute().inset(0.0).id("group-smooth-subtract-base"))
                                .child(div().absolute().inset(0.0).id("group-smooth-subtract-scoop")),
                        ),
                ),
        )
}

// ============================================================================
// CSS POSITION SECTION
// ============================================================================

fn css_position_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("CSS Position"))
        .child(section_description(
            "position: relative | absolute | fixed | sticky with top, right, bottom, left inset properties.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("relative + absolute children"))
                        .child(
                            div()
                                .id("pos-container")
                                .child(div().id("pos-static-child"))
                                .child(div().id("pos-floating"))
                                .child(div().id("pos-bottom-bar")),
                        ),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("sticky header + fixed button"))
                        .child(
                            scroll()
                                .w(300.0)
                                .h(300.0)
                                .bg(Color::from_hex(0x0f172a))
                                .rounded(8.0)
                                .child(
                                    div()
                                    .w_full()
                                        .flex_col()
                                        .child(
                                            div()
                                                .sticky(0.0)
                                                .h(36.0)
                                                .w_full()
                                                .z_index(10)
                                                .bg(Color::from_hex(0x7c3aed)),
                                        )
                                        .child(
                                            div()
                                                .fixed()
                                                .top(224.0)
                                                .right(12.0)
                                                .w(64.0)
                                                .h(64.0)
                                                .rounded(32.0)
                                                .z_index(10)
                                                .bg(Color::from_hex(0xef4444)),
                                        )
                                        .child(
                                            div().p(5.0).flex_col().gap(5.0).w_full().child(div().h(60.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                            .child(div().h(60.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                            .child(div().h(60.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                            .child(div().h(60.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                            .child(div().h(60.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                            .child(div().h(60.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                            .child(div().h(60.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                            .child(div().h(60.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0)),
                                        ),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("CSS overflow: scroll"))
                        .child(
                            div()
                                .id("css-scroll-container")
                                .child(
                                    div().h(500.0).p(5.0).flex_col().gap(5.0).w_full()
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0)),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("builder overflow_scroll()"))
                        .child(
                            div()
                                .overflow_scroll()
                                .w(300.0)
                                .h(200.0)
                                .bg(Color::from_hex(0x0f172a))
                                .rounded(8.0)
                                .child(
                                    div().h(500.0).p(5.0).flex_col().gap(5.0).w_full()
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0))
                                        .child(div().h(50.0).w_full().bg(Color::from_hex(0x1e293b)).rounded(4.0)),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// CLIP-PATH SECTION
// ============================================================================

fn clip_path_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("CSS clip-path"))
        .child(section_description(
            "Clip elements to shapes: circle, ellipse, inset, polygon. Applied via clip-path CSS property.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("circle(50%)"))
                        .child(div().w(120.0).h(120.0).id("clip-circle")),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("ellipse(50% 35%)"))
                        .child(div().w(120.0).h(120.0).id("clip-ellipse")),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("inset(10% round 12)"))
                        .child(div().w(120.0).h(120.0).id("clip-inset")),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("polygon (hexagon)"))
                        .child(div().w(120.0).h(120.0).id("clip-polygon-hex")),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("polygon (star)"))
                        .child(div().w(120.0).h(120.0).id("clip-polygon-star")),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("polygon (arrow)"))
                        .child(div().w(120.0).h(120.0).id("clip-polygon-arrow")),
                ),
        )
}

// ============================================================================
// ANIMATED CLIP-PATH SECTION (hover-triggered clip-path keyframe animations)
// ============================================================================

fn clip_path_animation_section() -> impl ElementBuilder + use<> {
    let dark_blue = Color::from_hex(0x1e3a5f);
    let dark_green = Color::from_hex(0x0f3d2a);
    let dark_purple = Color::from_hex(0x3d1f5e);

    section_container()
        .child(section_title("Animated clip-path"))
        .child(section_description(
            "Hover to reveal content with animated clip-path transitions. A bright overlay clips over a dark base layer.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("Center reveal"))
                        .child(
                            div()
                                .w(200.0)
                                .h(150.0)
                                .relative()
                                .child(div().absolute().top(0.0).left(0.0).w(200.0).h(150.0).bg(dark_blue).rounded(12.0))
                                .child(div().id("clip-over-center")),
                        ),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("Top-down"))
                        .child(
                            div()
                                .w(200.0)
                                .h(150.0)
                                .relative()
                                .child(div().absolute().top(0.0).left(0.0).w(200.0).h(150.0).bg(dark_green).rounded(12.0))
                                .child(div().id("clip-over-top")),
                        ),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("Left-to-right"))
                        .child(
                            div()
                                .w(200.0)
                                .h(150.0)
                                .relative()
                                .child(div().absolute().top(0.0).left(0.0).w(200.0).h(150.0).bg(dark_purple).rounded(12.0))
                                .child(div().id("clip-over-left")),
                        ),
                ),
        )
}

// ============================================================================
// API COMPARISON SECTION
// ============================================================================

fn api_comparison_section() -> impl ElementBuilder + use<> {
    // Same visual result using all three approaches
    let from_css = css_style! {
        background: Color::from_hex(0x3B82F6);
        border-radius: 12.0;
        box-shadow: md;
        opacity: 0.95;
    };

    let from_style = style! {
        bg: Color::from_hex(0x3B82F6),
        rounded: 12.0,
        shadow_md,
        opacity: 0.95,
    };

    let from_builder = ElementStyle::new()
        .bg(Color::from_hex(0x3B82F6))
        .rounded(12.0)
        .shadow_md()
        .opacity(0.95);

    // CSS parser version
    let css_string = r#"
        .card {
            background: #3b82f6;
            border-radius: 12px;
            box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
            opacity: 0.95;
        }
    "#;
    let stylesheet = Stylesheet::parse(css_string).expect("valid CSS");
    let from_parser = stylesheet.get("card").cloned().unwrap_or_default();

    section_container()
        .child(section_title("API Comparison"))
        .child(section_description(
            "All four approaches produce identical ElementStyle output.",
        ))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("css_style! { ... }"))
                        .child(styled_box_with_element_style(from_css)),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("style! { ... }"))
                        .child(styled_box_with_element_style(from_style)),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("ElementStyle::new()"))
                        .child(styled_box_with_element_style(from_builder)),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(code_label("Stylesheet::parse()"))
                        .child(styled_box_with_element_style(from_parser)),
                ),
        )
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Create a styled box that applies ElementStyle properties
fn styled_box_with_element_style(es: ElementStyle) -> Div {
    div().w(80.0).h(80.0).style(&es)
}

/// Create a labeled demo box
fn labeled_box(label: &str, style: ElementStyle) -> impl ElementBuilder + use<> {
    div()
        .flex_col()
        .gap(8.0)
        .child(code_label(label))
        .child(styled_box_with_element_style(style))
}

/// Create a 120x120 box for 3D demos
fn styled_3d_box(es: ElementStyle) -> Div {
    div().w(120.0).h(120.0).style(&es)
}

/// Create a labeled 3D demo box (120x120)
fn labeled_3d_box(label: &str, style: ElementStyle) -> impl ElementBuilder + use<> {
    div()
        .flex_col()
        .gap(8.0)
        .child(code_label(label))
        .child(styled_3d_box(style))
}
