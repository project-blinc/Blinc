//! Glass/Vibrancy effect tests
//!
//! Tests for Apple-style frosted glass and backdrop blur effects.
//! These tests require multi-pass rendering to capture backdrop content.

use crate::runner::TestSuite;
use blinc_core::{Color, DrawContext, Rect};
use blinc_gpu::GpuGlassPrimitive;

/// Create the glass test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("glass");

    // Basic glass rectangle over solid background
    suite.add_glass("glass_basic", |ctx| {
        let c = ctx.ctx();

        // Colorful background to show blur effect
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.2, 0.4, 0.8, 1.0).into(),
        );

        // Some shapes behind the glass
        c.fill_rect(
            Rect::new(50.0, 50.0, 100.0, 100.0),
            8.0.into(),
            Color::RED.into(),
        );
        c.fill_rect(
            Rect::new(120.0, 80.0, 100.0, 100.0),
            8.0.into(),
            Color::GREEN.into(),
        );
        c.fill_rect(
            Rect::new(80.0, 120.0, 100.0, 100.0),
            8.0.into(),
            Color::YELLOW.into(),
        );

        // Glass overlay
        let glass = GpuGlassPrimitive::new(100.0, 100.0, 200.0, 120.0)
            .with_corner_radius(16.0)
            .with_tint(1.0, 1.0, 1.0, 0.2)
            .with_blur(20.0);
        ctx.add_glass(glass);
    });

    // Glass with different blur radii
    suite.add_glass("glass_blur_levels", |ctx| {
        let c = ctx.ctx();

        // Gradient-like background with shapes
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.1, 0.2, 0.4, 1.0).into(),
        );

        // Pattern of colored squares
        for i in 0..8 {
            for j in 0..6 {
                let color = if (i + j) % 2 == 0 {
                    Color::rgba(0.9, 0.3, 0.3, 1.0)
                } else {
                    Color::rgba(0.3, 0.3, 0.9, 1.0)
                };
                c.fill_rect(
                    Rect::new(i as f32 * 50.0, j as f32 * 50.0, 48.0, 48.0),
                    4.0.into(),
                    color.into(),
                );
            }
        }

        // Three glass panels with different blur amounts
        let glass_small = GpuGlassPrimitive::new(20.0, 100.0, 100.0, 100.0)
            .with_corner_radius(8.0)
            .with_blur(5.0)
            .with_tint(1.0, 1.0, 1.0, 0.15);
        ctx.add_glass(glass_small);

        let glass_medium = GpuGlassPrimitive::new(140.0, 100.0, 100.0, 100.0)
            .with_corner_radius(8.0)
            .with_blur(15.0)
            .with_tint(1.0, 1.0, 1.0, 0.15);
        ctx.add_glass(glass_medium);

        let glass_large = GpuGlassPrimitive::new(260.0, 100.0, 100.0, 100.0)
            .with_corner_radius(8.0)
            .with_blur(30.0)
            .with_tint(1.0, 1.0, 1.0, 0.15);
        ctx.add_glass(glass_large);
    });

    // Glass type presets (UltraThin, Thin, Regular, Thick, Chrome)
    suite.add_glass("glass_types", |ctx| {
        let c = ctx.ctx();

        // Colorful background
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.8, 0.4, 0.2, 1.0).into(),
        );

        // Circles pattern
        for i in 0..10 {
            for j in 0..8 {
                c.fill_circle(
                    blinc_core::Point::new(20.0 + i as f32 * 40.0, 20.0 + j as f32 * 40.0),
                    15.0,
                    Color::rgba(0.2, 0.6, 0.9, 0.8).into(),
                );
            }
        }

        // Five glass types side by side
        let glass_ultra_thin = GpuGlassPrimitive::new(10.0, 100.0, 70.0, 100.0)
            .ultra_thin()
            .with_corner_radius(8.0);
        ctx.add_glass(glass_ultra_thin);

        let glass_thin = GpuGlassPrimitive::new(90.0, 100.0, 70.0, 100.0)
            .thin()
            .with_corner_radius(8.0);
        ctx.add_glass(glass_thin);

        let glass_regular = GpuGlassPrimitive::new(170.0, 100.0, 70.0, 100.0)
            .regular()
            .with_corner_radius(8.0);
        ctx.add_glass(glass_regular);

        let glass_thick = GpuGlassPrimitive::new(250.0, 100.0, 70.0, 100.0)
            .thick()
            .with_corner_radius(8.0);
        ctx.add_glass(glass_thick);

        let glass_chrome = GpuGlassPrimitive::new(330.0, 100.0, 60.0, 100.0)
            .chrome()
            .with_corner_radius(8.0);
        ctx.add_glass(glass_chrome);
    });

    // Glass with colored tints
    suite.add_glass("glass_tinted", |ctx| {
        let c = ctx.ctx();

        // Neutral gray background
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.5, 0.5, 0.5, 1.0).into(),
        );

        // Text-like stripes
        for i in 0..15 {
            c.fill_rect(
                Rect::new(20.0, 20.0 + i as f32 * 18.0, 360.0, 10.0),
                2.0.into(),
                Color::rgba(0.2, 0.2, 0.2, 1.0).into(),
            );
        }

        // Red tinted glass
        let glass_red = GpuGlassPrimitive::new(30.0, 80.0, 100.0, 140.0)
            .with_corner_radius(12.0)
            .with_blur(20.0)
            .with_tint(1.0, 0.3, 0.3, 0.3);
        ctx.add_glass(glass_red);

        // Green tinted glass
        let glass_green = GpuGlassPrimitive::new(150.0, 80.0, 100.0, 140.0)
            .with_corner_radius(12.0)
            .with_blur(20.0)
            .with_tint(0.3, 1.0, 0.3, 0.3);
        ctx.add_glass(glass_green);

        // Blue tinted glass
        let glass_blue = GpuGlassPrimitive::new(270.0, 80.0, 100.0, 140.0)
            .with_corner_radius(12.0)
            .with_blur(20.0)
            .with_tint(0.3, 0.3, 1.0, 0.3);
        ctx.add_glass(glass_blue);
    });

    // Glass with saturation adjustment
    suite.add_glass("glass_saturation", |ctx| {
        let c = ctx.ctx();

        // Very colorful background
        c.fill_rect(
            Rect::new(0.0, 0.0, 200.0, 300.0),
            0.0.into(),
            Color::rgba(1.0, 0.0, 0.5, 1.0).into(),
        );
        c.fill_rect(
            Rect::new(200.0, 0.0, 200.0, 300.0),
            0.0.into(),
            Color::rgba(0.0, 0.8, 1.0, 1.0).into(),
        );

        // Colored circles
        c.fill_circle(
            blinc_core::Point::new(100.0, 150.0),
            60.0,
            Color::YELLOW.into(),
        );
        c.fill_circle(
            blinc_core::Point::new(300.0, 150.0),
            60.0,
            Color::YELLOW.into(),
        );

        // Glass with reduced saturation (more grayscale blur)
        let glass_desat = GpuGlassPrimitive::new(50.0, 100.0, 100.0, 100.0)
            .with_corner_radius(12.0)
            .with_blur(20.0)
            .with_saturation(0.3) // Low saturation
            .with_tint(1.0, 1.0, 1.0, 0.1);
        ctx.add_glass(glass_desat);

        // Glass with enhanced saturation
        let glass_sat = GpuGlassPrimitive::new(250.0, 100.0, 100.0, 100.0)
            .with_corner_radius(12.0)
            .with_blur(20.0)
            .with_saturation(1.5) // High saturation
            .with_tint(1.0, 1.0, 1.0, 0.1);
        ctx.add_glass(glass_sat);
    });

    // Glass with brightness adjustment
    suite.add_glass("glass_brightness", |ctx| {
        let c = ctx.ctx();

        // Medium gray background
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.4, 0.4, 0.4, 1.0).into(),
        );

        // Pattern
        for i in 0..20 {
            c.fill_circle(
                blinc_core::Point::new(20.0 + i as f32 * 20.0, 150.0),
                8.0,
                Color::WHITE.into(),
            );
        }

        // Darker glass (low brightness)
        let glass_dark = GpuGlassPrimitive::new(50.0, 80.0, 130.0, 140.0)
            .with_corner_radius(12.0)
            .with_blur(15.0)
            .with_brightness(0.6)
            .with_tint(0.0, 0.0, 0.0, 0.2);
        ctx.add_glass(glass_dark);

        // Brighter glass (high brightness)
        let glass_bright = GpuGlassPrimitive::new(220.0, 80.0, 130.0, 140.0)
            .with_corner_radius(12.0)
            .with_blur(15.0)
            .with_brightness(1.4)
            .with_tint(1.0, 1.0, 1.0, 0.2);
        ctx.add_glass(glass_bright);
    });

    // Glass with corner radius variations
    suite.add_glass("glass_corners", |ctx| {
        let c = ctx.ctx();

        // Gradient background
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.2, 0.3, 0.5, 1.0).into(),
        );

        // Grid of small shapes
        for i in 0..16 {
            for j in 0..12 {
                c.fill_rect(
                    Rect::new(i as f32 * 25.0, j as f32 * 25.0, 20.0, 20.0),
                    2.0.into(),
                    Color::rgba(0.9, 0.7, 0.3, 0.8).into(),
                );
            }
        }

        // Sharp corners
        let glass_sharp = GpuGlassPrimitive::new(30.0, 100.0, 100.0, 100.0)
            .with_corner_radius(0.0)
            .with_blur(20.0)
            .with_tint(1.0, 1.0, 1.0, 0.2);
        ctx.add_glass(glass_sharp);

        // Medium corners
        let glass_medium = GpuGlassPrimitive::new(150.0, 100.0, 100.0, 100.0)
            .with_corner_radius(16.0)
            .with_blur(20.0)
            .with_tint(1.0, 1.0, 1.0, 0.2);
        ctx.add_glass(glass_medium);

        // Very rounded (pill-like)
        let glass_rounded = GpuGlassPrimitive::new(270.0, 100.0, 100.0, 100.0)
            .with_corner_radius(50.0) // Full pill shape
            .with_blur(20.0)
            .with_tint(1.0, 1.0, 1.0, 0.2);
        ctx.add_glass(glass_rounded);
    });

    // Glass modal dialog pattern
    suite.add_glass("glass_modal_dialog", |ctx| {
        let c = ctx.ctx();

        // App-like background
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.15, 0.15, 0.2, 1.0).into(),
        );

        // Fake app content (cards)
        c.fill_rect(
            Rect::new(20.0, 20.0, 160.0, 80.0),
            8.0.into(),
            Color::rgba(0.25, 0.25, 0.35, 1.0).into(),
        );
        c.fill_rect(
            Rect::new(20.0, 110.0, 160.0, 80.0),
            8.0.into(),
            Color::rgba(0.25, 0.25, 0.35, 1.0).into(),
        );
        c.fill_rect(
            Rect::new(220.0, 20.0, 160.0, 170.0),
            8.0.into(),
            Color::rgba(0.3, 0.25, 0.4, 1.0).into(),
        );

        // Accent elements
        c.fill_circle(
            blinc_core::Point::new(300.0, 100.0),
            40.0,
            Color::rgba(0.4, 0.6, 1.0, 0.8).into(),
        );

        // Dark overlay/scrim
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.0, 0.0, 0.0, 0.4).into(),
        );

        // Glass modal
        let modal = GpuGlassPrimitive::new(80.0, 60.0, 240.0, 180.0)
            .with_corner_radius(20.0)
            .with_blur(25.0)
            .with_tint(0.2, 0.2, 0.25, 0.7)
            .with_saturation(0.8);
        ctx.add_glass(modal);
    });

    // Glass sidebar pattern
    suite.add_glass("glass_sidebar", |ctx| {
        let c = ctx.ctx();

        // Image-like colorful background
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.3, 0.5, 0.7, 1.0).into(),
        );

        // "Photo" content
        c.fill_rect(
            Rect::new(50.0, 30.0, 300.0, 200.0),
            12.0.into(),
            Color::rgba(0.8, 0.6, 0.4, 1.0).into(),
        );
        c.fill_circle(
            blinc_core::Point::new(200.0, 130.0),
            50.0,
            Color::rgba(1.0, 0.8, 0.3, 1.0).into(),
        );

        // Glass sidebar
        let sidebar = GpuGlassPrimitive::new(0.0, 0.0, 80.0, 300.0)
            .with_corner_radius(0.0)
            .thick()
            .with_tint(0.1, 0.1, 0.15, 0.6);
        ctx.add_glass(sidebar);
    });

    // Overlapping glass panels
    suite.add_glass("glass_overlapping", |ctx| {
        let c = ctx.ctx();

        // Colorful background
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.8, 0.2, 0.4, 1.0).into(),
        );

        // Background shapes
        c.fill_rect(
            Rect::new(40.0, 40.0, 120.0, 120.0),
            16.0.into(),
            Color::YELLOW.into(),
        );
        c.fill_rect(
            Rect::new(240.0, 140.0, 120.0, 120.0),
            16.0.into(),
            Color::CYAN.into(),
        );

        // First glass panel (back)
        let glass1 = GpuGlassPrimitive::new(60.0, 60.0, 150.0, 150.0)
            .with_corner_radius(16.0)
            .with_blur(20.0)
            .with_tint(0.0, 0.0, 1.0, 0.2);
        ctx.add_glass(glass1);

        // Second glass panel (overlapping)
        let glass2 = GpuGlassPrimitive::new(140.0, 100.0, 150.0, 150.0)
            .with_corner_radius(16.0)
            .with_blur(20.0)
            .with_tint(0.0, 1.0, 0.0, 0.2);
        ctx.add_glass(glass2);
    });

    // iOS-style notification card
    suite.add_glass("glass_notification", |ctx| {
        let c = ctx.ctx();

        // Lock screen background (gradient-like)
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 150.0),
            0.0.into(),
            Color::rgba(0.1, 0.3, 0.5, 1.0).into(),
        );
        c.fill_rect(
            Rect::new(0.0, 150.0, 400.0, 150.0),
            0.0.into(),
            Color::rgba(0.2, 0.1, 0.4, 1.0).into(),
        );

        // Notification card
        let notification = GpuGlassPrimitive::new(20.0, 80.0, 360.0, 80.0)
            .with_corner_radius(16.0)
            .regular()
            .with_tint(0.95, 0.95, 0.97, 0.7)
            .with_saturation(1.2);
        ctx.add_glass(notification);
    });

    // macOS-style menu bar
    suite.add_glass("glass_menubar", |ctx| {
        let c = ctx.ctx();

        // Desktop wallpaper (colorful)
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.2, 0.5, 0.8, 1.0).into(),
        );

        // Window content
        c.fill_rect(
            Rect::new(50.0, 60.0, 300.0, 200.0),
            8.0.into(),
            Color::WHITE.into(),
        );

        // Menu bar at top
        let menubar = GpuGlassPrimitive::new(0.0, 0.0, 400.0, 28.0)
            .with_corner_radius(0.0)
            .thin()
            .with_tint(0.95, 0.95, 0.97, 0.8)
            .with_saturation(1.0)
            .with_brightness(1.1);
        ctx.add_glass(menubar);

        // Dock at bottom
        let dock = GpuGlassPrimitive::new(80.0, 260.0, 240.0, 35.0)
            .with_corner_radius(10.0)
            .thick()
            .with_tint(0.5, 0.5, 0.5, 0.4);
        ctx.add_glass(dock);
    });

    // Glass with drop shadows
    suite.add_glass("glass_shadows", |ctx| {
        let c = ctx.ctx();

        // Light background to show shadows clearly
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 300.0),
            0.0.into(),
            Color::rgba(0.95, 0.95, 0.97, 1.0).into(),
        );

        // Subtle pattern
        for i in 0..20 {
            for j in 0..15 {
                c.fill_rect(
                    Rect::new(i as f32 * 20.0, j as f32 * 20.0, 18.0, 18.0),
                    2.0.into(),
                    Color::rgba(0.85, 0.85, 0.87, 1.0).into(),
                );
            }
        }

        // Glass with subtle shadow
        let glass_subtle = GpuGlassPrimitive::new(30.0, 80.0, 100.0, 120.0)
            .with_corner_radius(16.0)
            .with_blur(15.0)
            .with_tint(1.0, 1.0, 1.0, 0.3)
            .with_shadow(10.0, 0.15);
        ctx.add_glass(glass_subtle);

        // Glass with medium shadow
        let glass_medium = GpuGlassPrimitive::new(150.0, 80.0, 100.0, 120.0)
            .with_corner_radius(16.0)
            .with_blur(15.0)
            .with_tint(1.0, 1.0, 1.0, 0.3)
            .with_shadow(20.0, 0.3);
        ctx.add_glass(glass_medium);

        // Glass with strong shadow
        let glass_strong = GpuGlassPrimitive::new(270.0, 80.0, 100.0, 120.0)
            .with_corner_radius(16.0)
            .with_blur(15.0)
            .with_tint(1.0, 1.0, 1.0, 0.3)
            .with_shadow(30.0, 0.5);
        ctx.add_glass(glass_strong);
    });

    // Glass with offset shadows (floating card effect)
    suite.add_glass("glass_shadow_offset", |ctx| {
        let c = ctx.ctx();

        // Gradient-like background
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0, 150.0),
            0.0.into(),
            Color::rgba(0.4, 0.5, 0.7, 1.0).into(),
        );
        c.fill_rect(
            Rect::new(0.0, 150.0, 400.0, 150.0),
            0.0.into(),
            Color::rgba(0.5, 0.4, 0.6, 1.0).into(),
        );

        // Some colorful shapes
        c.fill_circle(
            blinc_core::Point::new(80.0, 150.0),
            40.0,
            Color::rgba(1.0, 0.5, 0.3, 0.8).into(),
        );
        c.fill_circle(
            blinc_core::Point::new(320.0, 150.0),
            40.0,
            Color::rgba(0.3, 0.8, 0.5, 0.8).into(),
        );

        // Floating card with bottom-right shadow
        let card1 = GpuGlassPrimitive::new(60.0, 60.0, 120.0, 80.0)
            .with_corner_radius(12.0)
            .with_blur(20.0)
            .with_tint(1.0, 1.0, 1.0, 0.4)
            .with_shadow_offset(15.0, 0.4, 8.0, 8.0);
        ctx.add_glass(card1);

        // Floating card with bottom shadow (iOS style)
        let card2 = GpuGlassPrimitive::new(220.0, 60.0, 120.0, 80.0)
            .with_corner_radius(12.0)
            .with_blur(20.0)
            .with_tint(1.0, 1.0, 1.0, 0.4)
            .with_shadow_offset(20.0, 0.35, 0.0, 12.0);
        ctx.add_glass(card2);

        // Bottom notification with spread shadow
        let notification = GpuGlassPrimitive::new(50.0, 200.0, 300.0, 60.0)
            .with_corner_radius(20.0)
            .with_blur(25.0)
            .with_tint(0.1, 0.1, 0.15, 0.7)
            .with_shadow_offset(25.0, 0.5, 0.0, 15.0);
        ctx.add_glass(notification);
    });

    // iOS 26 Liquid Glass Music Player (based on reference image)
    // This test recreates the Apple Control Center music player widget
    suite.add_glass("music_player", |ctx| {
        // Layout constants
        let player_x = 40.0;
        let player_y = 60.0;
        let player_w = 320.0;
        let player_h = 180.0;
        let bar_x = player_x + 50.0;
        let bar_y = player_y + 55.0;
        let bar_w = player_w - 100.0;
        let bar_h = 4.0;
        let progress = 0.05;
        let knob_x = bar_x + bar_w * progress - 6.0;
        let controls_y = player_y + 100.0;
        let controls_center_x = player_x + player_w / 2.0;
        let rewind_x = controls_center_x - 80.0;
        let pause_x = controls_center_x - 20.0;
        let ff_x = controls_center_x + 50.0;
        let vol_x = player_x + player_w - 45.0;
        let vol_y = player_y + 20.0;
        let airplay_x = player_x + player_w - 45.0;
        let airplay_y = controls_y + 8.0;
        let toolbar_y = 280.0;

        // First, draw all background primitives (will be blurred behind glass)
        {
            let c = ctx.ctx();

            // Background - nature/leaf gradient simulation
            // Top half: olive green
            c.fill_rect(
                Rect::new(0.0, 0.0, 400.0, 200.0),
                0.0.into(),
                Color::rgba(0.35, 0.40, 0.30, 1.0).into(),
            );
            // Bottom half: lighter sage
            c.fill_rect(
                Rect::new(0.0, 200.0, 400.0, 200.0),
                0.0.into(),
                Color::rgba(0.55, 0.60, 0.50, 1.0).into(),
            );
            // Diagonal leaf shape (simplified as overlapping rect)
            c.fill_rect(
                Rect::new(50.0, 100.0, 200.0, 250.0),
                0.0.into(),
                Color::rgba(0.30, 0.35, 0.25, 1.0).into(),
            );
        }

        // Add all glass primitives
        // Main player card
        let player_glass = GpuGlassPrimitive::new(player_x, player_y, player_w, player_h)
            .with_corner_radius(24.0)
            .with_blur(25.0)
            .with_tint(0.15, 0.15, 0.18, 0.6)
            .with_saturation(0.9)
            .with_border_thickness(1.0)
            .with_light_angle_degrees(-45.0);
        ctx.add_glass(player_glass);

        // Flashlight button
        let flash_glass = GpuGlassPrimitive::new(100.0, toolbar_y, 60.0, 60.0)
            .with_corner_radius(30.0)
            .with_blur(20.0)
            .with_tint(0.2, 0.2, 0.25, 0.5)
            .with_border_thickness(0.8)
            .with_light_angle_degrees(-45.0);
        ctx.add_glass(flash_glass);

        // Camera button
        let camera_glass = GpuGlassPrimitive::new(240.0, toolbar_y, 60.0, 60.0)
            .with_corner_radius(30.0)
            .with_blur(20.0)
            .with_tint(0.2, 0.2, 0.25, 0.5)
            .with_border_thickness(0.8)
            .with_light_angle_degrees(-45.0);
        ctx.add_glass(camera_glass);

        // Draw foreground elements ON TOP of glass (not blurred)
        {
            let fg = ctx.foreground();

            // Progress bar track
            fg.fill_rect(
                Rect::new(bar_x, bar_y, bar_w, bar_h),
                2.0.into(),
                Color::rgba(0.3, 0.3, 0.35, 0.6).into(),
            );

            // Progress fill
            fg.fill_rect(
                Rect::new(bar_x, bar_y, bar_w * progress, bar_h),
                2.0.into(),
                Color::rgba(1.0, 1.0, 1.0, 0.9).into(),
            );

            // Scrubber knob
            fg.fill_rect(
                Rect::new(knob_x, bar_y - 4.0, 12.0, 12.0),
                6.0.into(),
                Color::WHITE.into(),
            );

            // Rewind button (two bars representing << icon)
            fg.fill_rect(
                Rect::new(rewind_x - 12.0, controls_y, 16.0, 32.0),
                2.0.into(),
                Color::WHITE.into(),
            );
            fg.fill_rect(
                Rect::new(rewind_x + 4.0, controls_y, 16.0, 32.0),
                2.0.into(),
                Color::WHITE.into(),
            );

            // Pause button (two vertical bars)
            fg.fill_rect(
                Rect::new(pause_x, controls_y, 12.0, 36.0),
                3.0.into(),
                Color::WHITE.into(),
            );
            fg.fill_rect(
                Rect::new(pause_x + 20.0, controls_y, 12.0, 36.0),
                3.0.into(),
                Color::WHITE.into(),
            );

            // Fast-forward button (two bars representing >> icon)
            fg.fill_rect(
                Rect::new(ff_x, controls_y, 16.0, 32.0),
                2.0.into(),
                Color::WHITE.into(),
            );
            fg.fill_rect(
                Rect::new(ff_x + 16.0, controls_y, 16.0, 32.0),
                2.0.into(),
                Color::WHITE.into(),
            );

            // Volume indicator (5 bars increasing in height)
            for i in 0..5 {
                let bar_height = 8.0 + i as f32 * 4.0;
                fg.fill_rect(
                    Rect::new(vol_x + i as f32 * 6.0, vol_y + 20.0 - bar_height, 4.0, bar_height),
                    1.0.into(),
                    Color::WHITE.into(),
                );
            }

            // AirPlay button (circle with inner dot)
            fg.fill_rect(
                Rect::new(airplay_x, airplay_y, 24.0, 24.0),
                12.0.into(),
                Color::rgba(1.0, 1.0, 1.0, 0.3).into(),
            );
            fg.fill_rect(
                Rect::new(airplay_x + 8.0, airplay_y + 8.0, 8.0, 8.0),
                4.0.into(),
                Color::WHITE.into(),
            );

            // Flashlight icon (vertical bar)
            fg.fill_rect(
                Rect::new(122.0, toolbar_y + 15.0, 16.0, 30.0),
                2.0.into(),
                Color::WHITE.into(),
            );

            // Camera icon (rounded rect with lens circle)
            fg.fill_rect(
                Rect::new(252.0, toolbar_y + 18.0, 36.0, 24.0),
                4.0.into(),
                Color::WHITE.into(),
            );
            fg.fill_rect(
                Rect::new(262.0, toolbar_y + 22.0, 16.0, 16.0),
                8.0.into(),
                Color::rgba(0.3, 0.3, 0.35, 1.0).into(),
            );
        }
    });

    suite
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::TestHarness;

    #[test]
    #[ignore] // Requires GPU
    fn run_glass_suite() {
        let harness = TestHarness::new().unwrap();
        let mut suite = suite();

        for case in suite.cases.drain(..) {
            let result = harness.run_glass_test(&case.name, case.test_fn).unwrap();
            assert!(result.is_passed(), "Test {} failed", case.name);
        }
    }
}
