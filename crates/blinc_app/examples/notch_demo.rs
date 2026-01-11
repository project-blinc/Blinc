//! Notch Menu Bar Demo
//!
//! Demonstrates a macOS-style menu bar with a notched dropdown that slides
//! horizontally between icons. The dropdown maintains a seamless visual
//! connection to the menu bar via concave curves.
//!
//! Run with: cargo run -p blinc_app --example notch_demo --features windowed

use blinc_animation::SpringConfig;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_core::Color;
use blinc_layout::stateful::{ButtonState, NoState};
use blinc_theme::{ColorToken, ThemeState};

// Menu bar height
const MENU_BAR_HEIGHT: f32 = 44.0;
const ICON_SIZE: f32 = 24.0;
const ICON_GAP: f32 = 16.0;
const NOTCH_RADIUS: f32 = 32.0;
const DROPDOWN_HEIGHT: f32 = 20.0;
const DROPDOWN_WIDTH: f32 = 340.0;

/// State for tracking the active menu item and its position
#[derive(Clone, Copy, Debug, Default)]
struct DropdownState {
    item: Option<MenuItem>,
    /// The center X position of the hovered icon (absolute)
    center_x: f32,
}

// Icon SVGs (Lucide-style icons)
const CLOCK_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>"#;

const BATTERY_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="7" width="18" height="10" rx="2" ry="2"/><line x1="22" y1="11" x2="22" y2="13"/></svg>"#;

const WIFI_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.55a11 11 0 0 1 14.08 0"/><path d="M1.42 9a16 16 0 0 1 21.16 0"/><path d="M8.53 16.11a6 6 0 0 1 6.95 0"/><circle cx="12" cy="20" r="1"/></svg>"#;

const WEATHER_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z"/></svg>"#;

const MUSIC_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>"#;

/// Menu item data
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MenuItem {
    Clock,
    Battery,
    Wifi,
    Weather,
    Music,
}

impl MenuItem {
    fn all() -> &'static [MenuItem] {
        &[
            MenuItem::Clock,
            MenuItem::Battery,
            MenuItem::Wifi,
            MenuItem::Weather,
            MenuItem::Music,
        ]
    }

    fn icon_svg(&self) -> &'static str {
        match self {
            MenuItem::Clock => CLOCK_SVG,
            MenuItem::Battery => BATTERY_SVG,
            MenuItem::Wifi => WIFI_SVG,
            MenuItem::Weather => WEATHER_SVG,
            MenuItem::Music => MUSIC_SVG,
        }
    }

    fn dropdown_width(&self) -> f32 {
        match self {
            MenuItem::Clock => 220.0,
            MenuItem::Battery => 200.0,
            MenuItem::Wifi => 220.0,
            MenuItem::Weather => 280.0,
            MenuItem::Music => 260.0,
        }
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let config = WindowConfig {
        title: "Notch Menu Bar Demo".to_string(),
        width: 800,
        height: 600,
        resizable: true,
        fullscreen: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let width = ctx.width;
    let height = ctx.height;

    stateful::<NoState>().on_state(move |ctx| {
        let theme = ThemeState::get();
        let bg = Color::WHITE; //theme.color(ColorToken::Background);

        // Track dropdown state (which item + its position)
        let dropdown_state: blinc_core::State<DropdownState> =
            ctx.use_signal("dropdown_state", || DropdownState {
                item: None,
                center_x: 0.0, // Will be set on first hover
            });

        let state = dropdown_state.get();

        // Track open generation - increments each time we transition from closed to open
        // This creates a new spring on each fresh open, so position appears immediately
        let open_gen = ctx.use_signal("open_gen", || 0u32);
        let prev_item_id = ctx.use_signal("prev_item", || 0u8);

        let is_open = state.item.is_some();
        let was_open = prev_item_id.get() != 0;
        let just_opened = is_open && !was_open;

        // Only update when item actually changes (avoids infinite re-render)
        let current_item_id = state.item.map(|_| 1u8).unwrap_or(0);
        if current_item_id != prev_item_id.get() {
            prev_item_id.set(current_item_id);
            if just_opened {
                open_gen.set(open_gen.get() + 1);
            }
        }

        // Get animated values for dropdown position and size
        let target_center_x = state.center_x;
        let full_height = DROPDOWN_HEIGHT + NOTCH_RADIUS * 2.0;
        let target_height = if state.item.is_some() {
            full_height
        } else {
            0.0 // Collapse to nothing
        };

        // Use dynamic spring key for position - resets on each fresh open
        let pos_spring_key = format!("dropdown_x_{}", open_gen.get());
        let center_x = ctx.use_spring(&pos_spring_key, target_center_x, SpringConfig::gentle());

        // Width: animate between items while open, expand when closing
        let item_width = DROPDOWN_WIDTH;
        let expanded_width = DROPDOWN_WIDTH * 0.3;
        let target_width = if is_open { item_width } else { expanded_width };
        // Dynamic key resets spring on fresh open so it starts at item width, not expanded
        let width_spring_key = format!("dropdown_w_{}", open_gen.get());
        let dropdown_width =
            ctx.use_spring(&width_spring_key, target_width, SpringConfig::snappy());

        let dropdown_height = ctx.use_spring("dropdown_h", target_height, SpringConfig::snappy());

        // Fade out opacity when nearly collapsed (height < 0.5)
        let opacity = if dropdown_height < 0.5 {
            (dropdown_height / 0.5).clamp(0.0, 1.0)
        } else {
            1.0
        };

        // Keep top concave radius at full size so curves stay visible
        let top_radius = NOTCH_RADIUS;

        // Bottom radius: no animation, just use full radius when open
        let bottom_radius = NOTCH_RADIUS;

        let menu_bar_bg = Color::BLACK;

        // Clone state for hover leave handler
        let dropdown_for_leave = dropdown_state.clone();

        // Root container
        div()
            .w(width)
            .h(height)
            .bg(bg)
            .flex_col()
            .child(
                // Menu bar + dropdown container with hover tracking
                stack()
                    .w_full()
                    .h(MENU_BAR_HEIGHT + DROPDOWN_HEIGHT + NOTCH_RADIUS)
                    // Menu bar layer
                    .child(menu_bar(&dropdown_state, menu_bar_bg))
                    // Dropdown layer - render until completely collapsed
                    .when(dropdown_height > 0.6, |s| {
                        s.child(notched_dropdown(
                            state.item,
                            center_x,
                            dropdown_width,
                            opacity,
                            dropdown_height,
                            top_radius,
                            bottom_radius,
                            menu_bar_bg,
                        ))
                    })
                    // Close dropdown when mouse leaves the entire menu + dropdown area
                    .on_hover_leave(move |_| {
                        dropdown_for_leave.set(DropdownState {
                            item: None,
                            center_x: dropdown_for_leave.get().center_x,
                        });
                    }),
            )
            .child(
                // Content area below menu bar
                div()
                    .w_full()
                    .flex_grow()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(
                        text("Hover over the icons in the menu bar above")
                            .size(16.0)
                            .color(theme.color(ColorToken::TextSecondary)),
                    ),
            )
            // Bottom dock bar with center scoop
            .child(bottom_dock_bar(width))
    })
}

/// Menu bar with icon buttons
fn menu_bar(dropdown_state: &blinc_core::State<DropdownState>, bg: Color) -> Div {
    let mut bar = div()
        .w_full()
        .h(MENU_BAR_HEIGHT)
        .bg(bg)
        .flex_row()
        .items_center()
        .justify_center()
        .gap(ICON_GAP);

    for item in MenuItem::all() {
        let item = *item;
        let state = dropdown_state.clone();

        bar = bar.child(stateful_icon_button(item, state));
    }

    bar
}

/// Stateful icon button with hover state tracking
fn stateful_icon_button(
    item: MenuItem,
    dropdown_state: blinc_core::State<DropdownState>,
) -> impl ElementBuilder {
    let dropdown_for_hover = dropdown_state.clone();

    stateful::<ButtonState>()
        .initial(ButtonState::Idle)
        .on_state(move |ctx| {
            let current_state = dropdown_state.get();
            let is_active = current_state.item == Some(item);

            // Background based on state
            let icon_color = match (ctx.state(), is_active) {
                (ButtonState::Hovered, _) | (ButtonState::Pressed, _) | (_, true) => {
                    Color::WHITE
                }
                _ => Color::WHITE.with_alpha(0.8),
            };

            // Scale animation on press
            let scale = ctx.use_spring(
                "scale",
                if matches!(ctx.state(), ButtonState::Hovered) {
                    1.30
                } else {
                    1.0
                },
                SpringConfig::snappy(),
            );

            div()
                .w(32.0)
                .h(32.0)
                // .rounded(8.0)
                // .bg(bg)
                .flex()
                .items_center()
                .justify_center()
                .transform(blinc_core::Transform::scale(scale, scale))
                .child(
                    svg(item.icon_svg())
                        .square(ICON_SIZE)
                        .scale(scale)
                        .color(icon_color),
                )
        })
        .on_hover_enter({
            let dropdown = dropdown_for_hover.clone();
            move |event_ctx| {
                // Get the center X from event bounds
                let center_x = event_ctx.bounds_x + event_ctx.bounds_width / 2.0;
                dropdown.set(DropdownState {
                    item: Some(item),
                    center_x,
                });
            }
        })
}

/// The notched dropdown panel with collapse animation
fn notched_dropdown(
    active_item: Option<MenuItem>,
    center_x: f32,
    width: f32,
    opacity: f32,
    height: f32,
    top_radius: f32,
    bottom_radius: f32,
    menu_bar_bg: Color,
) -> Notch {
    let content = dropdown_content(active_item);

    // Position dropdown so it's centered on the icon position (using animated width)
    let left = center_x - width / 2.0;

    // Calculate height ratio for padding animation (0 when collapsed, 1 when fully open)
    let full_height = DROPDOWN_HEIGHT + NOTCH_RADIUS * 2.0;
    let height_ratio = (height / full_height).clamp(0.0, 1.0);

    // Top concave radius stays full, bottom shrinks with height
    // Position at MENU_BAR_HEIGHT - top_radius so concave curves connect to menu bar
    notch()
        .concave_top(top_radius)
        .rounded_bottom(bottom_radius)
        .bg(menu_bar_bg)
        .opacity(opacity)
        .absolute()
        .top(MENU_BAR_HEIGHT - top_radius)
        .left(left)
        .w(width)
        .h(height) // Animated height for collapse effect
        .overflow_clip()
        .pt(top_radius + 12.0 * height_ratio) // Padding scales with height
        .pb(12.0 * height_ratio) // Animate to 0 when collapsed
        .px(16.0)
        // Always render content - it gets clipped by overflow_clip as height animates
        .child(
            div()
                .px(6.0)
                .w_full()
                .justify_center()
                .overflow_clip()
                .child(content),
        )
}

/// Content displayed in the dropdown based on active item
fn dropdown_content(item: Option<MenuItem>) -> Div {
    let text_primary = Color::WHITE;
    let text_secondary = Color::rgba(1.0, 1.0, 1.0, 0.6);
    let accent_orange = Color::from_hex(0xf59e0b);
    let accent_green = Color::from_hex(0x10b981);
    let accent_blue = Color::from_hex(0x3b82f6);
    let accent_cyan = Color::from_hex(0x06b6d4);
    let accent_purple = Color::from_hex(0xa855f7);

    match item {
        Some(MenuItem::Clock) => div()
            .flex_row()
            .items_center()
            .justify_center()
            .gap(8.0)
            .overflow_clip()
            .child(text("Wed Jan 8").size(14.0).color(accent_orange))
            .child(text("|").size(14.0).color(text_secondary))
            .child(text("10:42 AM").size(14.0).color(text_primary)),

        Some(MenuItem::Battery) => div()
            .flex_row()
            .items_center()
            .gap(8.0)
            .child(text("Battery").size(14.0).color(accent_green))
            .child(text("|").size(14.0).color(text_secondary))
            .child(text("87% Charged").size(14.0).color(text_primary)),

        Some(MenuItem::Wifi) => div()
            .flex_row()
            .items_center()
            .gap(8.0)
            .child(text("Network").size(14.0).color(accent_blue))
            .child(text("|").size(14.0).color(text_secondary))
            .child(text("Home WiFi").size(14.0).color(text_primary)),

        Some(MenuItem::Weather) => div()
            .flex_row()
            .gap_px(8.0)
            .w_full()
            .overflow_clip()
            .justify_center()
            .child(
                div()
                    .flex_row()
                    .items_center()
                    .gap_px(8.0)
                    .child(svg(WEATHER_SVG).square(20.0).color(accent_cyan))
                    .child(text("Cloudy").size(14.0).color(text_primary)),
            )
            .child(text("|").size(14.0).color(text_secondary))
            .child(
                div()
                    .flex_row()
                    .items_center()
                    .gap_px(8.0)
                    .child(text("80°F").size(14.0).color(text_primary))
                    .child(text("•").size(14.0).color(text_secondary))
                    .child(text("San Francisco").size(14.0).color(text_secondary)),
            ),

        Some(MenuItem::Music) => div()
            .flex_col()
            .gap(4.0)
            .child(
                div()
                    .flex_row()
                    .items_center()
                    .gap(8.0)
                    .child(
                        // Equalizer bars animation placeholder
                        div()
                            .flex_row()
                            .items_end()
                            .gap(2.0)
                            .h(16.0)
                            .child(div().w(3.0).h(8.0).bg(accent_purple).rounded(1.0))
                            .child(div().w(3.0).h(14.0).bg(accent_purple).rounded(1.0))
                            .child(div().w(3.0).h(10.0).bg(accent_purple).rounded(1.0))
                            .child(div().w(3.0).h(16.0).bg(accent_purple).rounded(1.0)),
                    )
                    .child(text("Now Playing").size(12.0).color(text_secondary)),
            )
            .child(
                text("Artist Name - Song Title")
                    .size(14.0)
                    .color(text_primary),
            ),

        None => div(),
    }
}

/// Bottom dock bar with Dynamic Island-style center scoop
fn bottom_dock_bar(width: f32) -> impl ElementBuilder {
    let dock_bg = Color::rgba(0.1, 0.1, 0.1, 0.95);
    let icon_color = Color::rgba(1.0, 1.0, 1.0, 0.8);
    let scoop_depth = 30.0;

    // Container with bottom margin
    div().w_full().flex_row().justify_center().child(
        notch()
            .center_scoop_top(scoop_depth * 2.0, scoop_depth)
            .rounded_top(24.0)
            .bg(dock_bg)
            .w_fit()
            .h(50.0 + scoop_depth)
            // .shadow(Shadow { offset_x: 0.0, offset_y: 1.0, blur:3.0, spread: 3.0, color:Color::BLACK.with_alpha(0.5) }) // Needs shadow support
            // Padding for scoop is automatically applied by the notch implementation
            .child(
                div()
                    .w_full()
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .gap(16.0)
                    .p(6.0)
                    .child(svg(CLOCK_SVG).square(ICON_SIZE).color(icon_color))
                    .child(svg(BATTERY_SVG).square(ICON_SIZE).color(icon_color))
                    .child(svg(WIFI_SVG).square(ICON_SIZE).color(icon_color))
                    .child(svg(WEATHER_SVG).square(ICON_SIZE).color(icon_color))
                    .child(svg(MUSIC_SVG).square(ICON_SIZE).color(icon_color)),
            ),
    )
}
