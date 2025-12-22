//! Layout system tests
//!
//! Tests for the GPUI-style layout builder API powered by Taffy flexbox.

use crate::runner::TestSuite;
use blinc_core::{Color, DrawContext, Rect};
use blinc_layout::prelude::*;

/// Create the layout test suite
pub fn suite() -> TestSuite {
    let mut suite = TestSuite::new("layout");

    // Basic flex row layout - three colored boxes in a row
    suite.add("flex_row_basic", |ctx| {
        let ui = div()
            .w(400.0)
            .h(120.0)
            .flex_row()
            .gap_px(20.0)
            .p_px(10.0)
            .bg(Color::rgba(0.15, 0.15, 0.2, 1.0))
            .child(div().w(100.0).h(100.0).rounded(8.0).bg(Color::rgba(0.9, 0.2, 0.3, 1.0))) // Red
            .child(div().w(100.0).h(100.0).rounded(8.0).bg(Color::rgba(0.2, 0.8, 0.3, 1.0))) // Green
            .child(div().w(100.0).h(100.0).rounded(8.0).bg(Color::rgba(0.2, 0.4, 0.9, 1.0))); // Blue

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(800.0, 600.0);
        tree.render(ctx.ctx());
    });

    // Flex column with gap - vertical stack
    suite.add("flex_col_with_gap", |ctx| {
        let ui = div()
            .w(150.0)
            .h(400.0)
            .flex_col()
            .gap_px(15.0)
            .p_px(15.0)
            .bg(Color::rgba(0.12, 0.12, 0.18, 1.0))
            .child(div().w_full().h(70.0).rounded(8.0).bg(Color::rgba(0.95, 0.3, 0.4, 1.0)))  // Coral
            .child(div().w_full().h(70.0).rounded(8.0).bg(Color::rgba(0.3, 0.85, 0.5, 1.0))) // Mint
            .child(div().w_full().h(70.0).rounded(8.0).bg(Color::rgba(0.3, 0.5, 0.95, 1.0))) // Sky blue
            .child(div().w_full().h(70.0).rounded(8.0).bg(Color::rgba(0.95, 0.85, 0.3, 1.0))); // Gold

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(800.0, 600.0);
        tree.render(ctx.ctx());
    });

    // Flex grow - fixed + flexible + fixed
    suite.add("flex_grow", |ctx| {
        let ui = div()
            .w(500.0)
            .h(100.0)
            .flex_row()
            .p_px(10.0)
            .gap_px(10.0)
            .bg(Color::rgba(0.1, 0.12, 0.18, 1.0))
            .child(div().w(80.0).h(80.0).rounded(8.0).bg(Color::rgba(0.9, 0.3, 0.4, 1.0)))  // Fixed left (coral)
            .child(div().flex_grow().h(80.0).rounded(8.0).bg(Color::rgba(0.3, 0.75, 0.6, 1.0))) // Flexible middle (teal)
            .child(div().w(80.0).h(80.0).rounded(8.0).bg(Color::rgba(0.5, 0.3, 0.9, 1.0))); // Fixed right (purple)

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(800.0, 600.0);
        tree.render(ctx.ctx());
    });

    // Nested layout - grid-like arrangement
    suite.add("nested_layout", |ctx| {
        let ui = div()
            .w(350.0)
            .h(350.0)
            .flex_col()
            .gap_px(12.0)
            .p_px(12.0)
            .rounded(16.0)
            .bg(Color::rgba(0.12, 0.13, 0.18, 1.0))
            // Top row: small square + expanding rectangle
            .child(
                div()
                    .w_full()
                    .h(90.0)
                    .flex_row()
                    .gap_px(12.0)
                    .child(div().w(90.0).h(90.0).rounded(12.0).bg(Color::rgba(0.95, 0.35, 0.45, 1.0))) // Coral
                    .child(div().flex_grow().h(90.0).rounded(12.0).bg(Color::rgba(1.0, 0.65, 0.3, 1.0))), // Orange
            )
            // Middle row: sidebar + main content
            .child(
                div()
                    .w_full()
                    .flex_grow()
                    .flex_row()
                    .gap_px(12.0)
                    .child(div().w(110.0).h_full().rounded(12.0).bg(Color::rgba(0.95, 0.85, 0.35, 1.0))) // Gold
                    .child(div().flex_grow().h_full().rounded(12.0).bg(Color::rgba(0.35, 0.85, 0.55, 1.0))), // Mint
            )
            // Bottom row: full width bar
            .child(div().w_full().h(60.0).rounded(12.0).bg(Color::rgba(0.35, 0.7, 0.95, 1.0))); // Sky blue

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(800.0, 600.0);
        tree.render(ctx.ctx());
    });

    // Justify content variations - four rows showing different alignments
    suite.add("justify_content", |ctx| {
        let c = ctx.ctx();

        // Helper colors for each row
        let bg_color = Color::rgba(0.18, 0.18, 0.24, 1.0);
        let box_colors = [
            Color::rgba(0.95, 0.35, 0.45, 1.0), // Coral
            Color::rgba(0.35, 0.85, 0.55, 1.0), // Mint
            Color::rgba(0.35, 0.55, 0.95, 1.0), // Blue
        ];

        // justify-start (default)
        let row1 = div()
            .w(380.0)
            .h(70.0)
            .flex_row()
            .p_px(5.0)
            .gap_px(10.0)
            .justify_start()
            .rounded(10.0)
            .bg(bg_color)
            .child(div().w(60.0).h(60.0).rounded(8.0).bg(box_colors[0]))
            .child(div().w(60.0).h(60.0).rounded(8.0).bg(box_colors[1]))
            .child(div().w(60.0).h(60.0).rounded(8.0).bg(box_colors[2]));

        let mut tree1 = RenderTree::from_element(&row1);
        tree1.compute_layout(800.0, 600.0);
        c.push_transform(blinc_core::Transform::translate(10.0, 10.0));
        tree1.render(c);
        c.pop_transform();

        // justify-center
        let row2 = div()
            .w(380.0)
            .h(70.0)
            .flex_row()
            .p_px(5.0)
            .gap_px(10.0)
            .justify_center()
            .rounded(10.0)
            .bg(bg_color)
            .child(div().w(60.0).h(60.0).rounded(8.0).bg(box_colors[0]))
            .child(div().w(60.0).h(60.0).rounded(8.0).bg(box_colors[1]))
            .child(div().w(60.0).h(60.0).rounded(8.0).bg(box_colors[2]));

        let mut tree2 = RenderTree::from_element(&row2);
        tree2.compute_layout(800.0, 600.0);
        c.push_transform(blinc_core::Transform::translate(10.0, 95.0));
        tree2.render(c);
        c.pop_transform();

        // justify-end
        let row3 = div()
            .w(380.0)
            .h(70.0)
            .flex_row()
            .p_px(5.0)
            .gap_px(10.0)
            .justify_end()
            .rounded(10.0)
            .bg(bg_color)
            .child(div().w(60.0).h(60.0).rounded(8.0).bg(box_colors[0]))
            .child(div().w(60.0).h(60.0).rounded(8.0).bg(box_colors[1]))
            .child(div().w(60.0).h(60.0).rounded(8.0).bg(box_colors[2]));

        let mut tree3 = RenderTree::from_element(&row3);
        tree3.compute_layout(800.0, 600.0);
        c.push_transform(blinc_core::Transform::translate(10.0, 180.0));
        tree3.render(c);
        c.pop_transform();

        // justify-between
        let row4 = div()
            .w(380.0)
            .h(70.0)
            .flex_row()
            .p_px(5.0)
            .justify_between()
            .rounded(10.0)
            .bg(bg_color)
            .child(div().w(60.0).h(60.0).rounded(8.0).bg(box_colors[0]))
            .child(div().w(60.0).h(60.0).rounded(8.0).bg(box_colors[1]))
            .child(div().w(60.0).h(60.0).rounded(8.0).bg(box_colors[2]));

        let mut tree4 = RenderTree::from_element(&row4);
        tree4.compute_layout(800.0, 600.0);
        c.push_transform(blinc_core::Transform::translate(10.0, 265.0));
        tree4.render(c);
        c.pop_transform();
    });

    // Padding test - outer and inner colors clearly visible
    suite.add("padding", |ctx| {
        let ui = div()
            .w(220.0)
            .h(220.0)
            .p_px(25.0)
            .rounded(16.0)
            .bg(Color::rgba(0.55, 0.25, 0.35, 1.0)) // Deep rose (outer padding area)
            .child(
                div()
                    .w_full()
                    .h_full()
                    .rounded(12.0)
                    .bg(Color::rgba(0.25, 0.55, 0.75, 1.0)), // Teal (inner content)
            );

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(800.0, 600.0);
        tree.render(ctx.ctx());
    });

    // Rounded corners with layout
    suite.add("rounded_layout", |ctx| {
        let ui = div()
            .w(320.0)
            .h(220.0)
            .p_px(18.0)
            .rounded(24.0)
            .bg(Color::rgba(0.14, 0.15, 0.22, 1.0)) // Dark slate
            .flex_col()
            .gap_px(14.0)
            .child(
                div()
                    .w_full()
                    .h(60.0)
                    .rounded(14.0)
                    .bg(Color::rgba(0.95, 0.4, 0.45, 1.0)), // Salmon
            )
            .child(
                div()
                    .w_full()
                    .flex_grow()
                    .rounded(14.0)
                    .bg(Color::rgba(0.3, 0.65, 0.95, 1.0)), // Bright blue
            );

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(800.0, 600.0);
        tree.render(ctx.ctx());
    });

    // Card-like component
    suite.add("card_component", |ctx| {
        let card = div()
            .w(280.0)
            .h(180.0)
            .p_px(16.0)
            .rounded(16.0)
            .bg(Color::rgba(0.95, 0.95, 0.97, 1.0))
            .flex_col()
            .gap_px(12.0)
            // Header row
            .child(
                div()
                    .w_full()
                    .h(40.0)
                    .flex_row()
                    .gap_px(12.0)
                    .items_center()
                    // Avatar placeholder
                    .child(div().square(40.0).rounded(20.0).bg(Color::rgba(0.3, 0.5, 0.9, 1.0)))
                    // Title area
                    .child(
                        div()
                            .flex_grow()
                            .h(40.0)
                            .flex_col()
                            .gap_px(4.0)
                            .child(div().w(120.0).h(14.0).rounded(3.0).bg(Color::rgba(0.2, 0.2, 0.25, 1.0)))
                            .child(div().w(80.0).h(10.0).rounded(2.0).bg(Color::rgba(0.6, 0.6, 0.65, 1.0))),
                    ),
            )
            // Content area
            .child(
                div()
                    .w_full()
                    .flex_grow()
                    .rounded(8.0)
                    .bg(Color::rgba(0.9, 0.9, 0.92, 1.0)),
            )
            // Button row
            .child(
                div()
                    .w_full()
                    .h(36.0)
                    .flex_row()
                    .justify_end()
                    .gap_px(8.0)
                    .child(
                        div()
                            .w(80.0)
                            .h(36.0)
                            .rounded(8.0)
                            .bg(Color::rgba(0.85, 0.85, 0.88, 1.0)),
                    )
                    .child(
                        div()
                            .w(80.0)
                            .h(36.0)
                            .rounded(8.0)
                            .bg(Color::rgba(0.3, 0.5, 0.9, 1.0)),
                    ),
            );

        let mut tree = RenderTree::from_element(&card);
        tree.compute_layout(800.0, 600.0);

        // Center the card
        ctx.ctx()
            .push_transform(blinc_core::Transform::translate(60.0, 60.0));
        tree.render(ctx.ctx());
        ctx.ctx().pop_transform();
    });

    // Glass card with layout - demonstrates glass over layout background
    // Uses the layout API with .effect() for glass and automatic foreground children
    suite.add_glass("glass_card", |ctx| {
        let c = ctx.ctx();

        // Colorful background shapes (will be blurred behind glass)
        c.fill_rect(
            blinc_core::Rect::new(0.0, 0.0, 400.0, 350.0),
            0.0.into(),
            Color::rgba(0.15, 0.2, 0.35, 1.0).into(),
        );

        // Colorful blobs for interesting blur effect
        c.fill_circle(
            blinc_core::Point::new(80.0, 80.0),
            70.0,
            Color::rgba(0.95, 0.35, 0.5, 1.0).into(),
        );
        c.fill_circle(
            blinc_core::Point::new(320.0, 120.0),
            80.0,
            Color::rgba(0.3, 0.85, 0.7, 1.0).into(),
        );
        c.fill_circle(
            blinc_core::Point::new(200.0, 280.0),
            60.0,
            Color::rgba(0.95, 0.75, 0.3, 1.0).into(),
        );
        c.fill_rect(
            blinc_core::Rect::new(30.0, 200.0, 100.0, 100.0),
            16.0.into(),
            Color::rgba(0.4, 0.5, 0.95, 1.0).into(),
        );

        // Build card with glass effect and foreground children (all via layout)
        // Wrap in container to position card at approximately (50, 75)
        let card_ui = div()
            .w(400.0)
            .h(350.0)
            .p_px(50.0)
            .child(
                div()
                    .w(300.0)
                    .h(200.0)
                    .mt(6.0) // Add ~25px top margin (6 * 4 = 24px)
                    .rounded(20.0)
                    .p_px(20.0)
                    .flex_col()
                    .gap_px(8.0)
                    .effect(
                        GlassMaterial::new()
                            .blur(25.0)
                            .tint_rgba(0.95, 0.95, 0.98, 0.6)
                            .saturation(1.1)
                            .border(1.0),
                    )
                    // Header bar - auto foreground
                    .child(
                        div()
                            .w(200.0)
                            .h(24.0)
                            .rounded(6.0)
                            .bg(Color::rgba(1.0, 1.0, 1.0, 0.9)),
                    )
                    // Subtitle - auto foreground
                    .child(
                        div()
                            .w(140.0)
                            .h(14.0)
                            .rounded(4.0)
                            .bg(Color::rgba(1.0, 1.0, 1.0, 0.6)),
                    )
                    // Content area - auto foreground
                    .child(
                        div()
                            .w(260.0)
                            .h(70.0)
                            .rounded(12.0)
                            .bg(Color::rgba(1.0, 1.0, 1.0, 0.15)),
                    )
                    // Button row - auto foreground
                    .child(
                        div()
                            .w_full()
                            .h(28.0)
                            .flex_row()
                            .justify_end()
                            .gap_px(10.0)
                            .child(
                                div()
                                    .w(60.0)
                                    .h(28.0)
                                    .rounded(8.0)
                                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.2)),
                            )
                            .child(
                                div()
                                    .w(80.0)
                                    .h(28.0)
                                    .rounded(8.0)
                                    .bg(Color::rgba(0.3, 0.6, 0.95, 1.0)),
                            ),
                    ),
            );

        let mut tree = RenderTree::from_element(&card_ui);
        tree.compute_layout(400.0, 350.0);

        // Render with layer separation
        tree.render_to_layer(c, RenderLayer::Background);
        tree.render_to_layer(c, RenderLayer::Glass);

        let fg = ctx.foreground();
        tree.render_to_layer(fg, RenderLayer::Foreground);
    });

    // Layout-driven glass panel arrangement
    // Uses the layout API with .effect() for glass and automatic foreground children
    suite.add_glass("glass_layout_panels", |ctx| {
        let c = ctx.ctx();

        // Vibrant gradient-like background
        c.fill_rect(
            blinc_core::Rect::new(0.0, 0.0, 400.0, 350.0),
            0.0.into(),
            Color::rgba(0.2, 0.15, 0.35, 1.0).into(),
        );

        // Colorful shapes for blur effect
        c.fill_circle(
            blinc_core::Point::new(100.0, 100.0),
            90.0,
            Color::rgba(0.9, 0.3, 0.5, 1.0).into(),
        );
        c.fill_circle(
            blinc_core::Point::new(300.0, 250.0),
            100.0,
            Color::rgba(0.3, 0.8, 0.6, 1.0).into(),
        );
        c.fill_circle(
            blinc_core::Point::new(200.0, 50.0),
            70.0,
            Color::rgba(0.95, 0.7, 0.2, 1.0).into(),
        );

        // Build layout with glass panels and their children (all via layout)
        let layout = div()
            .w(360.0)
            .h(310.0)
            .flex_col()
            .gap_px(15.0)
            .p_px(20.0)
            // Top row: two panels side by side
            .child(
                div()
                    .w_full()
                    .h(120.0)
                    .flex_row()
                    .gap_px(15.0)
                    // Left panel with glass and content
                    .child(
                        div()
                            .w(140.0)
                            .h_full()
                            .rounded(16.0)
                            .p_px(15.0)
                            .flex_col()
                            .gap_px(10.0)
                            .effect(GlassMaterial::new().blur(20.0).tint_rgba(0.95, 0.95, 1.0, 0.5).border(0.8))
                            // Icon - auto foreground
                            .child(
                                div()
                                    .w(40.0)
                                    .h(40.0)
                                    .rounded(10.0)
                                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.8)),
                            )
                            // Title - auto foreground
                            .child(
                                div()
                                    .w(110.0)
                                    .h(12.0)
                                    .rounded(4.0)
                                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.6)),
                            )
                            // Subtitle - auto foreground
                            .child(
                                div()
                                    .w(80.0)
                                    .h(10.0)
                                    .rounded(3.0)
                                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.4)),
                            ),
                    )
                    // Right panel with glass and content
                    .child(
                        div()
                            .flex_grow()
                            .h_full()
                            .rounded(16.0)
                            .p_px(15.0)
                            .flex_col()
                            .gap_px(10.0)
                            .effect(GlassMaterial::new().blur(20.0).tint_rgba(1.0, 0.95, 0.95, 0.5).border(0.8))
                            // Content area - auto foreground
                            .child(
                                div()
                                    .w_full()
                                    .h(60.0)
                                    .rounded(10.0)
                                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.15)),
                            )
                            // Label - auto foreground
                            .child(
                                div()
                                    .w(100.0)
                                    .h(12.0)
                                    .rounded(4.0)
                                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.5)),
                            ),
                    ),
            )
            // Bottom: full width panel with progress bar and controls
            .child(
                div()
                    .w_full()
                    .flex_grow()
                    .rounded(16.0)
                    .p_px(20.0)
                    .flex_col()
                    .gap_px(15.0)
                    .items_center()
                    .effect(GlassMaterial::new().blur(20.0).tint_rgba(0.95, 1.0, 0.95, 0.5).border(0.8))
                    // Progress bar container - auto foreground
                    .child(
                        div()
                            .w_full()
                            .h(8.0)
                            .rounded(4.0)
                            .bg(Color::rgba(1.0, 1.0, 1.0, 0.2)),
                    )
                    // Control buttons row - auto foreground
                    .child(
                        div()
                            .flex_row()
                            .gap_px(36.0)
                            .items_center()
                            // Previous button
                            .child(
                                div()
                                    .w(28.0)
                                    .h(28.0)
                                    .rounded(14.0)
                                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.7)),
                            )
                            // Play button (larger)
                            .child(
                                div()
                                    .w(36.0)
                                    .h(36.0)
                                    .rounded(18.0)
                                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.9)),
                            )
                            // Next button
                            .child(
                                div()
                                    .w(28.0)
                                    .h(28.0)
                                    .rounded(14.0)
                                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.7)),
                            ),
                    ),
            );

        let mut tree = RenderTree::from_element(&layout);
        tree.compute_layout(400.0, 350.0);

        // Render with layer separation
        tree.render_to_layer(c, RenderLayer::Background);
        tree.render_to_layer(c, RenderLayer::Glass);

        let fg = ctx.foreground();
        tree.render_to_layer(fg, RenderLayer::Foreground);
    });

    // Material-based glass API test
    // This test demonstrates the new .glass() and .effect() methods on divs
    // Glass panels are automatically collected from the layout tree
    suite.add_glass("glass_material_api", |ctx| {
        let c = ctx.ctx();

        // Colorful background
        c.fill_rect(
            blinc_core::Rect::new(0.0, 0.0, 400.0, 350.0),
            0.0.into(),
            Color::rgba(0.12, 0.15, 0.25, 1.0).into(),
        );

        // Background shapes
        c.fill_circle(
            blinc_core::Point::new(100.0, 100.0),
            80.0,
            Color::rgba(0.9, 0.3, 0.4, 1.0).into(),
        );
        c.fill_circle(
            blinc_core::Point::new(300.0, 200.0),
            90.0,
            Color::rgba(0.3, 0.8, 0.5, 1.0).into(),
        );
        c.fill_circle(
            blinc_core::Point::new(150.0, 280.0),
            70.0,
            Color::rgba(0.3, 0.5, 0.95, 1.0).into(),
        );
        c.fill_circle(
            blinc_core::Point::new(280.0, 80.0),
            60.0,
            Color::rgba(0.95, 0.7, 0.2, 1.0).into(),
        );

        // Build layout with glass materials and children
        // This demonstrates:
        // - Glass materials via .effect()
        // - Children of glass elements are AUTOMATICALLY rendered in foreground
        // - No manual coordinate calculations or .foreground() calls needed!
        let ui = div()
            .w(360.0)
            .h(300.0)
            .p_px(20.0)
            .flex_col()
            .gap_px(15.0)
            // Top panel with glass material and children (auto foreground)
            .child(
                div()
                    .w_full()
                    .h(100.0)
                    .rounded(16.0)
                    .p_px(20.0)
                    .flex_col()
                    .gap_px(10.0)
                    .effect(
                        GlassMaterial::new()
                            .blur(25.0)
                            .tint_rgba(0.95, 0.95, 1.0, 0.5)
                            .border(1.0),
                    )
                    // Title bar - auto foreground as child of glass
                    .child(
                        div()
                            .w(180.0)
                            .h(16.0)
                            .rounded(4.0)
                            .bg(Color::rgba(1.0, 1.0, 1.0, 0.8)),
                    )
                    // Subtitle - auto foreground as child of glass
                    .child(
                        div()
                            .w(120.0)
                            .h(12.0)
                            .rounded(3.0)
                            .bg(Color::rgba(1.0, 1.0, 1.0, 0.5)),
                    ),
            )
            // Bottom row with two glass panels
            .child(
                div()
                    .w_full()
                    .flex_grow()
                    .flex_row()
                    .gap_px(15.0)
                    // Left panel with content
                    .child(
                        div()
                            .w(140.0)
                            .h_full()
                            .rounded(16.0)
                            .p_px(15.0)
                            .flex_col()
                            .gap_px(15.0)
                            .effect(GlassMaterial::thick().tint_rgba(1.0, 0.9, 0.9, 0.4))
                            // Icon placeholder - auto foreground
                            .child(
                                div()
                                    .w(50.0)
                                    .h(50.0)
                                    .rounded(12.0)
                                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.7)),
                            )
                            // Label - auto foreground
                            .child(
                                div()
                                    .w(110.0)
                                    .h(10.0)
                                    .rounded(3.0)
                                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.5)),
                            ),
                    )
                    // Right panel with content
                    .child(
                        div()
                            .flex_grow()
                            .h_full()
                            .rounded(16.0)
                            .p_px(15.0)
                            .flex_col()
                            .gap_px(15.0)
                            .items_center()
                            .effect(GlassMaterial::frosted().tint_rgba(0.9, 1.0, 0.9, 0.4))
                            // Content area - auto foreground
                            .child(
                                div()
                                    .w(140.0)
                                    .h(50.0)
                                    .rounded(10.0)
                                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.15)),
                            )
                            // Button - auto foreground
                            .child(
                                div()
                                    .w(40.0)
                                    .h(40.0)
                                    .rounded(20.0)
                                    .bg(Color::rgba(1.0, 1.0, 1.0, 0.8)),
                            ),
                    ),
            );

        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(400.0, 350.0);

        // Render the layout tree with layer separation:
        // - Background goes to background context
        // - Glass goes to background context (GPU renderer separates glass primitives)
        // - Foreground children of glass go to foreground context
        tree.render_to_layer(c, RenderLayer::Background);
        tree.render_to_layer(c, RenderLayer::Glass);

        // Render foreground to separate context (for proper glass compositing)
        let fg = ctx.foreground();
        tree.render_to_layer(fg, RenderLayer::Foreground);
    });

    // Music player widget using layout API with SVG elements
    // Recreates the iOS Control Center music player with layout-based positioning
    // ALL positioning is handled by Taffy - NO manual coordinate calculations
    suite.add_glass("music_player", |ctx| {
        let c = ctx.ctx();
        let scale = 2.0;

        // SVG icon definitions
        let rewind_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640"><path d="M236.3 107.1C247.9 96 265 92.9 279.7 99.2C294.4 105.5 304 120 304 136L304 272.3L476.3 107.2C487.9 96 505 92.9 519.7 99.2C534.4 105.5 544 120 544 136L544 504C544 520 534.4 534.5 519.7 540.8C505 547.1 487.9 544 476.3 532.9L304 367.7L304 504C304 520 294.4 534.5 279.7 540.8C265 547.1 247.9 544 236.3 532.9L44.3 348.9C36.4 341.4 32 330.9 32 320C32 309.1 36.5 298.7 44.3 291.1L236.3 107.1z" fill="white"/></svg>"#;
        let pause_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640"><path d="M176 96C149.5 96 128 117.5 128 144L128 496C128 522.5 149.5 544 176 544L240 544C266.5 544 288 522.5 288 496L288 144C288 117.5 266.5 96 240 96L176 96zM400 96C373.5 96 352 117.5 352 144L352 496C352 522.5 373.5 544 400 544L464 544C490.5 544 512 522.5 512 496L512 144C512 117.5 490.5 96 464 96L400 96z" fill="white"/></svg>"#;
        let forward_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 640 640"><path d="M403.7 107.1C392.1 96 375 92.9 360.3 99.2C345.6 105.5 336 120 336 136L336 272.3L163.7 107.2C152.1 96 135 92.9 120.3 99.2C105.6 105.5 96 120 96 136L96 504C96 520 105.6 534.5 120.3 540.8C135 547.1 152.1 544 163.7 532.9L336 367.7L336 504C336 520 345.6 534.5 360.3 540.8C375 547.1 392.1 544 403.7 532.9L595.7 348.9C603.6 341.4 608 330.9 608 320C608 309.1 603.5 298.7 595.7 291.1L403.7 107.1z" fill="white"/></svg>"#;
        let airplay_svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 17H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2h-1"/><path d="m12 15 5 6H7Z"/></svg>"##;
        let radio_svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M16.247 7.761a6 6 0 0 1 0 8.478"/><path d="M19.075 4.933a10 10 0 0 1 0 14.134"/><path d="M4.925 19.067a10 10 0 0 1 0-14.134"/><path d="M7.753 16.239a6 6 0 0 1 0-8.478"/><circle cx="12" cy="12" r="2.5" fill="white"/></svg>"##;

        // Import svg function
        use blinc_layout::svg;

        // Dimensions
        let player_radius = 28.0 * scale;
        let bar_h = 7.0 * scale;
        let icon_size = 32.0 * scale;
        let pill_icon_size = 20.0 * scale;
        let pill_padding = 12.0 * scale;

        // Vibrant multicolor background (blurred behind glass)
        c.fill_rect(
            Rect::new(0.0, 0.0, 400.0 * scale, 300.0 * scale),
            0.0.into(),
            Color::rgba(0.4, 0.2, 0.6, 1.0).into(),
        );

        // Colorful shapes for blur effect
        c.fill_circle(
            blinc_core::Point::new(80.0 * scale, 60.0 * scale),
            100.0 * scale,
            Color::rgba(0.95, 0.3, 0.5, 1.0).into(),
        );
        c.fill_circle(
            blinc_core::Point::new(320.0 * scale, 120.0 * scale),
            90.0 * scale,
            Color::rgba(0.2, 0.8, 0.85, 1.0).into(),
        );
        c.fill_circle(
            blinc_core::Point::new(180.0 * scale, 260.0 * scale),
            80.0 * scale,
            Color::rgba(1.0, 0.5, 0.2, 1.0).into(),
        );
        c.fill_circle(
            blinc_core::Point::new(350.0 * scale, 240.0 * scale),
            60.0 * scale,
            Color::rgba(1.0, 0.85, 0.2, 1.0).into(),
        );
        c.fill_circle(
            blinc_core::Point::new(50.0 * scale, 220.0 * scale),
            70.0 * scale,
            Color::rgba(0.3, 0.9, 0.4, 1.0).into(),
        );
        c.fill_rect(
            Rect::new(280.0 * scale, 0.0, 120.0 * scale, 80.0 * scale),
            (20.0 * scale).into(),
            Color::rgba(0.3, 0.4, 0.95, 1.0).into(),
        );

        // SINGLE layout tree - ALL elements positioned by Taffy
        let ui = div()
            .w(400.0 * scale)
            .h(300.0 * scale)
            .p_px(30.0 * scale)
            // Main player card with glass effect
            .child(
                div()
                    .w(340.0 * scale)
                    .h(140.0 * scale)
                    .rounded(player_radius)
                    .flex_col()
                    .p_px(20.0 * scale)
                    .gap_px(8.0 * scale)
                    .effect(
                        GlassMaterial::new()
                            .blur(30.0 * scale)
                            .tint_rgba(0.12, 0.12, 0.14, 0.55)
                            .saturation(0.85)
                            .brightness(1.05)
                            .border(0.6 * scale)
                            .shadow(MaterialShadow::new().blur(20.0 * scale).offset(0.0, 10.0 * scale).opacity(0.35)),
                    )
                    // Title row
                    .child(
                        div()
                            .w_full()
                            .h(20.0 * scale)
                            .flex_row()
                            .justify_center()
                            .items_center()
                            .child(text("Blinc UI 0.1.0").size(14.0 * scale).color(Color::rgba(1.0, 1.0, 1.0, 0.95)))
                    )
                    // Progress bar row: time - slider - time
                    .child(
                        div()
                            .w_full()
                            .h(bar_h + 8.0 * scale)
                            .flex_row()
                            .items_center()
                            .gap_px(8.0 * scale)
                            // Left time label
                            .child(
                                div()
                                    .w(35.0 * scale)
                                    .h(bar_h + 8.0 * scale)
                                    .flex_row()
                                    .justify_end()
                                    .items_center()
                                    .child(text("0:10").size(11.0 * scale).color(Color::rgba(1.0, 1.0, 1.0, 0.85)))
                            )
                            // Slider track (glass)
                            .child(
                                div()
                                    .flex_grow()
                                    .h(bar_h)
                                    .rounded(bar_h / 2.0)
                                    .effect(
                                        GlassMaterial::new()
                                            .blur(25.0 * scale)
                                            .tint_rgba(1.0, 1.0, 1.0, 0.65)
                                            .saturation(0.3)
                                            .brightness(1.3)
                                            .border(0.0),
                                    )
                                    // Progress fill (child of slider, auto foreground)
                                    .child(
                                        div()
                                            .w(20.0 * scale) // ~8% progress
                                            .h_full()
                                            .rounded(bar_h / 2.0)
                                            .bg(Color::rgba(1.0, 1.0, 1.0, 1.0))
                                    )
                            )
                            // Right time label
                            .child(
                                div()
                                    .w(40.0 * scale)
                                    .h(bar_h + 8.0 * scale)
                                    .flex_row()
                                    .justify_start()
                                    .items_center()
                                    .child(text("-3:24").size(11.0 * scale).color(Color::rgba(1.0, 1.0, 1.0, 0.85)))
                            )
                    )
                    // Controls row: pill - icons - pill
                    .child(
                        div()
                            .w_full()
                            .flex_grow()
                            .flex_row()
                            .justify_between()
                            .items_center()
                            // Left pill with airplay icon
                            .child(
                                div()
                                    .w(pill_icon_size + pill_padding * 2.0)
                                    .h(pill_icon_size + pill_padding * 2.0)
                                    .rounded_full()
                                    .flex_row()
                                    .justify_center()
                                    .items_center()
                                    .effect(
                                        GlassMaterial::new()
                                            .blur(20.0 * scale)
                                            .tint_rgba(0.92, 0.92, 0.94, 0.4)
                                            .saturation(0.8)
                                            .brightness(0.95)
                                            .border(1.0 * scale),
                                    )
                                    .child(svg(airplay_svg).square(pill_icon_size))
                            )
                            // Center icons: rewind - pause - forward
                            .child(
                                div()
                                    .flex_row()
                                    .gap_px(38.0 * scale)
                                    .items_center()
                                    .child(svg(rewind_svg).square(icon_size))
                                    .child(svg(pause_svg).square(icon_size))
                                    .child(svg(forward_svg).square(icon_size))
                            )
                            // Right pill with radio icon
                            .child(
                                div()
                                    .w(pill_icon_size + pill_padding * 2.0)
                                    .h(pill_icon_size + pill_padding * 2.0)
                                    .rounded_full()
                                    .flex_row()
                                    .justify_center()
                                    .items_center()
                                    .effect(
                                        GlassMaterial::new()
                                            .blur(20.0 * scale)
                                            .tint_rgba(0.92, 0.92, 0.94, 0.4)
                                            .saturation(0.8)
                                            .brightness(0.95)
                                            .border(1.0 * scale),
                                    )
                                    .child(svg(radio_svg).square(pill_icon_size))
                            )
                    )
            );

        // Build, layout, render - that's it
        let mut tree = RenderTree::from_element(&ui);
        tree.compute_layout(400.0 * scale, 300.0 * scale);
        ctx.render_layout(&tree);
    });

    suite
}


