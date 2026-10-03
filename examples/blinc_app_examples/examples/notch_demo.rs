//! Notch Menu Bar Demo
//!
//! Demonstrates a macOS-style menu bar with a notched dropdown that
//! slides horizontally between icons, plus four other notch shapes:
//! a nav bar with a top bulge, two V-shape cuts, a V-peak, and a
//! Dynamic-Island-style bottom dock with a center scoop.
//!
//! Run with: cargo run -p blinc_app_examples --example notch_demo

use blinc_animation::SpringConfig;
use blinc_app::prelude::*;
use blinc_app::windowed::WindowedContext;
use blinc_core::{Color, Shadow, State, Transform};
use blinc_layout::stateful::{ButtonState, NoState};
use blinc_theme::{ColorToken, ThemeState};

// =============================================================================
// Layout constants
// =============================================================================

const MENU_BAR_HEIGHT: f32 = 44.0;
const ICON_SIZE: f32 = 24.0;
const ICON_GAP: f32 = 16.0;
const NOTCH_RADIUS: f32 = 32.0;
const DROPDOWN_BODY_HEIGHT: f32 = 20.0;
const DROPDOWN_WIDTH: f32 = 340.0;

/// Total height of the dropdown when fully open: the visible body
/// plus the concave radius slots top and bottom.
const DROPDOWN_FULL_HEIGHT: f32 = DROPDOWN_BODY_HEIGHT + NOTCH_RADIUS * 2.0;

/// Stack height that wraps the menu bar + dropdown. Includes a
/// 10 px safety margin past the dropdown's drop-shadow bleed
/// (offset_y + blur), otherwise the shadow gets clipped by the
/// stack's default overflow-hidden.
const MENU_STACK_HEIGHT: f32 = MENU_BAR_HEIGHT + DROPDOWN_BODY_HEIGHT + NOTCH_RADIUS + 10.0;

// =============================================================================
// Icon SVGs (Lucide-style)
// =============================================================================

const CLOCK_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>"#;

const BATTERY_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="2" y="7" width="18" height="10" rx="2" ry="2"/><line x1="22" y1="11" x2="22" y2="13"/></svg>"#;

const WIFI_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12.55a11 11 0 0 1 14.08 0"/><path d="M1.42 9a16 16 0 0 1 21.16 0"/><path d="M8.53 16.11a6 6 0 0 1 6.95 0"/><circle cx="12" cy="20" r="1"/></svg>"#;

const WEATHER_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z"/></svg>"#;

const MUSIC_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/></svg>"#;

const PLUS_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></svg>"#;

const HOME_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>"#;

const SEARCH_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/></svg>"#;

const USER_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>"#;

const SETTINGS_SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>"#;

// =============================================================================
// Menu items
// =============================================================================

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum MenuItem {
    Clock,
    Battery,
    Wifi,
    Weather,
    Music,
}

impl MenuItem {
    const ALL: [MenuItem; 5] = [
        Self::Clock,
        Self::Battery,
        Self::Wifi,
        Self::Weather,
        Self::Music,
    ];

    fn icon_svg(self) -> &'static str {
        match self {
            Self::Clock => CLOCK_SVG,
            Self::Battery => BATTERY_SVG,
            Self::Wifi => WIFI_SVG,
            Self::Weather => WEATHER_SVG,
            Self::Music => MUSIC_SVG,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct DropdownState {
    item: Option<MenuItem>,
    /// Absolute screen X of the hovered icon's centre — drives the
    /// dropdown's slide position.
    center_x: f32,
}

// =============================================================================
// Entry point
// =============================================================================

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    use blinc_platform::AnimationFps;

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
        fullscreen: false,
        animation_fps: AnimationFps::Fixed(30),
        max_frame_latency: 2,
        ..Default::default()
    };

    blinc_app::windowed::WindowedApp::run(config, build_ui)
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    // The Stateful is intentionally scoped to JUST `menu_bar_section`
    // — the dropdown's springs trigger one rebuild per animation tick,
    // and we don't want the static demos below paying for that. With
    // the Stateful narrowed, only the menu bar + dropdown subtree
    // runs through the layout-prop fast path; the nav bar, V-shapes,
    // and bottom dock stay cached.
    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(Color::WHITE)
        .flex_col()
        .child(menu_bar_section())
        .child(
            div()
                .w_full()
                .flex_grow()
                .flex_col()
                .items_center()
                .justify_start()
                .gap(6.0)
                .child(
                    text("Hover over the icons in the menu bar above")
                        .size(16.0)
                        .color(text_secondary),
                )
                .child(labeled_section(
                    "Navigation Bar with Bulge Active Indicator",
                    text_secondary,
                    active_nav_bar(),
                ))
                .child(labeled_section(
                    "Sharp Angle Cuts & Peaks (V-shapes)",
                    text_secondary,
                    sharp_angle_demo(),
                )),
        )
        .child(bottom_dock_bar())
}

/// Section header + content stacked vertically. Used for the two
/// static demos below the menu bar.
fn labeled_section<C: ElementBuilder + 'static>(
    label: &'static str,
    label_color: Color,
    content: C,
) -> impl ElementBuilder + use<C> {
    div()
        .flex_col()
        .items_center()
        .gap(4.0)
        .child(text(label).size(12.0).color(label_color))
        .child(content)
}

// =============================================================================
// Menu bar + dropdown (the only animated section)
// =============================================================================

fn menu_bar_section() -> impl ElementBuilder + use<> {
    stateful::<NoState>().on_state(|ctx| {
        let state = ctx.use_signal::<DropdownState, _>("dropdown_state", DropdownState::default);
        let open_gen = ctx.use_signal("open_gen", || 0u32);
        let was_open = ctx.use_signal("was_open", || false);

        let snapshot = state.get();
        let is_open = snapshot.item.is_some();

        // Bump the generation on every closed → open edge so the
        // position and width springs that the dropdown subscribes to
        // get fresh keys — meaning a fresh open snaps to the target
        // instantly instead of sliding from the previous-close
        // position.
        if is_open != was_open.get() {
            was_open.set(is_open);
            if is_open {
                open_gen.set(open_gen.get() + 1);
            }
        }
        let open_gen_id = open_gen.get();

        let target_width = if is_open {
            DROPDOWN_WIDTH
        } else {
            DROPDOWN_WIDTH * 0.3
        };
        let target_height = if is_open { DROPDOWN_FULL_HEIGHT } else { 0.0 };

        let center_x = ctx.use_spring(
            &format!("dropdown_x_{open_gen_id}"),
            snapshot.center_x,
            SpringConfig::gentle(),
        );
        let dropdown_width = ctx.use_spring(
            &format!("dropdown_w_{open_gen_id}"),
            target_width,
            SpringConfig::snappy(),
        );
        let dropdown_height = ctx.use_spring("dropdown_h", target_height, SpringConfig::snappy());

        // Fade out as height approaches zero so the dropdown doesn't
        // suddenly vanish at the `dropdown_height > 0.6` cutoff
        // below.
        let opacity = (dropdown_height / 0.5).clamp(0.0, 1.0);

        let menu_bar_bg = Color::BLACK;
        let state_for_leave = state.clone();

        // The Stateful callback signature returns `Div`, so wrap
        // the stack in a Div. Cheap — adds one passthrough node.
        div().w_full().child(
            stack()
                .w_full()
                .h(MENU_STACK_HEIGHT)
                .child(menu_bar(&state, menu_bar_bg))
                .when(dropdown_height > 0.6, |s| {
                    s.child(notched_dropdown(NotchedDropdownArgs {
                        item: snapshot.item,
                        center_x,
                        width: dropdown_width,
                        height: dropdown_height,
                        opacity,
                        bg: menu_bar_bg,
                    }))
                })
                .on_hover_leave(move |_| {
                    let mut s = state_for_leave.get();
                    s.item = None;
                    state_for_leave.set(s);
                }),
        )
    })
}

/// Top menu bar with five icon buttons.
///
/// Shadow strategy: ONLY the menu bar carries a shadow. Draw order
/// is menu bar FIRST, dropdown SECOND on top. The menu bar's
/// downward shadow renders beneath the menu bar everywhere; where
/// the dropdown sits centred below the active icon, the dropdown's
/// opaque fill — drawn on top — covers the shadow, so the shadow
/// flows around the dropdown body without double-shadowing the
/// merge region.
fn menu_bar(state: &State<DropdownState>, bg: Color) -> Div {
    let mut bar = div()
        .w_full()
        .h(MENU_BAR_HEIGHT)
        .bg(bg)
        .shadow(Shadow {
            offset_x: 0.0,
            offset_y: 4.0,
            blur: 12.0,
            spread: 0.0,
            color: Color::BLACK.with_alpha(0.35),
        })
        .flex_row()
        .items_center()
        .justify_center()
        .gap(ICON_GAP);

    for item in MenuItem::ALL {
        bar = bar.child(menu_icon_button(item, state.clone()));
    }
    bar
}

/// Single menu-bar icon with hover-scale spring + dropdown trigger.
fn menu_icon_button(item: MenuItem, state: State<DropdownState>) -> impl ElementBuilder + use<> {
    let state_for_hover = state.clone();
    stateful::<ButtonState>()
        .initial(ButtonState::Idle)
        .on_state(move |ctx| {
            let is_active = state.get().item == Some(item);
            let icon_color = match (ctx.state(), is_active) {
                (ButtonState::Hovered, _) | (ButtonState::Pressed, _) | (_, true) => Color::WHITE,
                _ => Color::WHITE.with_alpha(0.8),
            };
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
                .flex()
                .items_center()
                .justify_center()
                .transform(Transform::scale(scale, scale))
                .child(
                    svg(item.icon_svg())
                        .square(ICON_SIZE)
                        .scale(scale)
                        .color(icon_color),
                )
        })
        .on_hover_enter(move |evt| {
            state_for_hover.set(DropdownState {
                item: Some(item),
                center_x: evt.bounds_x + evt.bounds_width / 2.0,
            });
        })
}

/// Arguments for `notched_dropdown` — grouped to keep the
/// constructor signature manageable and avoid
/// `clippy::too_many_arguments`.
struct NotchedDropdownArgs {
    item: Option<MenuItem>,
    center_x: f32,
    width: f32,
    height: f32,
    opacity: f32,
    bg: Color,
}

/// The notched panel that slides under the menu bar. Concave top
/// corners merge visually with the bar; bottom corners are convex.
fn notched_dropdown(args: NotchedDropdownArgs) -> Notch {
    let top_radius = NOTCH_RADIUS;
    let bottom_radius = NOTCH_RADIUS - 16.0;
    let height_ratio = (args.height / DROPDOWN_FULL_HEIGHT).clamp(0.0, 1.0);
    let left = args.center_x - args.width / 2.0;

    // Shadow trick: `offset_y >= blur` keeps the Gaussian shadow
    // entirely below the shape's top edge so it doesn't bleed
    // upward into the menu bar at the concave merge. Trade-off is
    // a slightly bottom-biased shadow, but that's preferable to
    // a double-shadow at the seam.
    notch()
        .concave_top(top_radius)
        .rounded_bottom(bottom_radius)
        .bg(args.bg)
        .shadow(Shadow {
            offset_x: 0.0,
            offset_y: 6.0,
            blur: 8.0,
            spread: 0.0,
            color: Color::BLACK.with_alpha(0.12),
        })
        .opacity(args.opacity)
        // The dropdown's bbox overlaps neighbouring menu icons —
        // without pass-through hit-test, hovering across the bar
        // would only trigger every other icon (the dropdown
        // intercepts the events for the icons it covers).
        .pointer_events_none()
        .absolute()
        .top(MENU_BAR_HEIGHT - top_radius)
        .bottom(12.0)
        .left(left)
        .w(args.width)
        .h(args.height)
        .overflow_clip()
        // Inner padding scales with the height ratio so content
        // looks crisp when fully open and tucks away cleanly while
        // collapsing.
        .pt(top_radius + 12.0 * height_ratio)
        .pb(12.0 * height_ratio)
        .px(16.0)
        .child(
            div()
                .px(6.0)
                .w_full()
                .justify_center()
                .overflow_clip()
                .child(dropdown_content(args.item)),
        )
}

/// Per-item dropdown contents. Empty `Div` when no item is hovered.
fn dropdown_content(item: Option<MenuItem>) -> Div {
    const TEXT_PRIMARY: Color = Color::WHITE;
    let text_secondary = Color::rgba(1.0, 1.0, 1.0, 0.6);

    match item {
        Some(MenuItem::Clock) => labeled_row(
            "Wed Jan 8",
            Color::from_hex(0xf59e0b), // amber-500
            "10:42 AM",
            TEXT_PRIMARY,
            text_secondary,
        ),
        Some(MenuItem::Battery) => labeled_row(
            "Battery",
            Color::from_hex(0x10b981), // emerald-500
            "87% Charged",
            TEXT_PRIMARY,
            text_secondary,
        ),
        Some(MenuItem::Wifi) => labeled_row(
            "Network",
            Color::from_hex(0x3b82f6), // blue-500
            "Home WiFi",
            TEXT_PRIMARY,
            text_secondary,
        ),
        Some(MenuItem::Weather) => weather_row(text_secondary),
        Some(MenuItem::Music) => music_row(text_secondary),
        None => div(),
    }
}

/// `<label> | <value>` row in the dropdown — used for Clock,
/// Battery, Wifi.
fn labeled_row(
    label: &str,
    label_color: Color,
    value: &str,
    value_color: Color,
    sep_color: Color,
) -> Div {
    div()
        .flex_row()
        .items_center()
        .justify_center()
        .gap(8.0)
        .overflow_clip()
        .child(text(label).size(14.0).color(label_color))
        .child(text("|").size(14.0).color(sep_color))
        .child(text(value).size(14.0).color(value_color))
}

fn weather_row(text_secondary: Color) -> Div {
    let cyan = Color::from_hex(0x06b6d4);
    div()
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
                .child(svg(WEATHER_SVG).square(20.0).color(cyan))
                .child(text("Cloudy").size(14.0).color(Color::WHITE)),
        )
        .child(text("|").size(14.0).color(text_secondary))
        .child(
            div()
                .flex_row()
                .items_center()
                .gap_px(8.0)
                .child(text("80°F").size(14.0).color(Color::WHITE))
                .child(text("•").size(14.0).color(text_secondary))
                .child(text("San Francisco").size(14.0).color(text_secondary)),
        )
}

fn music_row(text_secondary: Color) -> Div {
    let purple = Color::from_hex(0xa855f7);
    let bar = |h: f32| div().w(3.0).h(h).bg(purple).rounded(1.0);
    div()
        .flex_col()
        .gap(4.0)
        .child(
            div()
                .flex_row()
                .items_center()
                .gap(8.0)
                .child(
                    div()
                        .flex_row()
                        .items_end()
                        .gap(2.0)
                        .h(16.0)
                        .child(bar(8.0))
                        .child(bar(14.0))
                        .child(bar(10.0))
                        .child(bar(16.0)),
                )
                .child(text("Now Playing").size(12.0).color(text_secondary)),
        )
        .child(
            text("Artist Name - Song Title")
                .size(14.0)
                .color(Color::WHITE),
        )
}

// =============================================================================
// Navigation bar with center bulge
// =============================================================================

fn active_nav_bar() -> impl ElementBuilder + use<> {
    let nav_bg = Color::from_hex(0x1e293b); // slate-800
    let active_bg = Color::from_hex(0x3b82f6); // blue-500
    let icon_color = Color::WHITE.with_alpha(0.7);

    // Bulge geometry sized so its apex curvature matches the 44 px
    // active button radius: r ≈ (half_w² + h²) / (2·h) = (20² + 8.5²) / 17 ≈ 27.8.
    // `bulge_corner_radius` is the ear fillet where the arc joins the baseline.
    let bulge_height = 8.5;
    let bulge_width = 40.0;
    let bulge_corner_radius = 6.0;

    div().flex_row().justify_center().child(
        notch()
            .center_bulge_top_rounded(bulge_width, bulge_height, bulge_corner_radius)
            .rounded(16.0)
            .bg(nav_bg)
            .h(56.0 + bulge_height)
            .px(8.0)
            .flex_row()
            .items_center()
            .gap(4.0)
            .child(nav_icon_cell(HOME_SVG, icon_color))
            .child(nav_icon_cell(SEARCH_SVG, icon_color))
            .child(
                // Centre item (active) — round button positioned in the bulge.
                div()
                    .w(44.0)
                    .h(44.0)
                    .rounded_full()
                    .bg(active_bg)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(svg(PLUS_SVG).square(24.0).color(Color::WHITE)),
            )
            .child(nav_icon_cell(USER_SVG, icon_color))
            .child(nav_icon_cell(SETTINGS_SVG, icon_color)),
    )
}

fn nav_icon_cell(svg_src: &'static str, color: Color) -> Div {
    div()
        .w(56.0)
        .h(44.0)
        .flex()
        .items_center()
        .justify_center()
        .child(svg(svg_src).square(22.0).color(color))
}

// =============================================================================
// Sharp angle demo (V-cuts and V-peaks)
// =============================================================================

fn sharp_angle_demo() -> impl ElementBuilder + use<> {
    let bar_bg = Color::from_hex(0x374151); // gray-700
    let peak_bg = Color::from_hex(0x059669); // emerald-600
    let cut_bg = Color::from_hex(0xdc2626); // red-600
    let icon_color = Color::WHITE.with_alpha(0.9);

    div()
        .flex_row()
        .items_end()
        .gap(24.0)
        .child(labeled_demo(
            "V-Cut (60° angle)",
            // Width 40, depth 16 → ~60° angle
            notch()
                .center_cut_top(40.0, 16.0)
                .rounded(12.0)
                .bg(cut_bg)
                .w(160.0)
                .h(44.0)
                .flex_row()
                .items_center()
                .justify_center()
                .gap(32.0)
                .child(svg(HOME_SVG).square(20.0).color(icon_color))
                .child(svg(SETTINGS_SVG).square(20.0).color(icon_color)),
        ))
        .child(labeled_demo(
            "V-Peak (pointing up)",
            // Extra wrapper for the peak's overshoot above the layout box.
            div().h(20.0).child(
                notch()
                    .center_peak_top(50.0, 18.0) // width 50, height 18
                    .rounded(12.0)
                    .bg(peak_bg)
                    .w(180.0)
                    .h(52.0 + 18.0) // base height + peak height
                    .flex_row()
                    .items_end()
                    .justify_center()
                    .pb(8.0)
                    .gap(16.0)
                    .child(svg(HOME_SVG).square(20.0).color(icon_color))
                    .child(
                        // Active indicator at the peak.
                        div()
                            .w(36.0)
                            .h(36.0)
                            .rounded_full()
                            .bg(Color::WHITE)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(svg(PLUS_SVG).square(20.0).color(peak_bg)),
                    )
                    .child(svg(SETTINGS_SVG).square(20.0).color(icon_color)),
            ),
        ))
        .child(labeled_demo(
            "Steep Cut (30° angle)",
            // Width 20, depth 20 → ~30° angle
            notch()
                .center_cut_top(20.0, 20.0)
                .rounded(12.0)
                .bg(bar_bg)
                .w(140.0)
                .h(44.0)
                .flex_row()
                .items_center()
                .justify_center()
                .gap(40.0)
                .child(svg(SEARCH_SVG).square(20.0).color(icon_color))
                .child(svg(USER_SVG).square(20.0).color(icon_color)),
        ))
}

fn labeled_demo<D: ElementBuilder + 'static>(
    label: &'static str,
    demo: D,
) -> impl ElementBuilder + use<D> {
    div()
        .flex_col()
        .items_center()
        .gap(4.0)
        .child(text(label).size(10.0).color(Color::GRAY))
        .child(demo)
}

// =============================================================================
// Bottom dock with Dynamic-Island-style centre scoop
// =============================================================================

fn bottom_dock_bar() -> impl ElementBuilder + use<> {
    let dock_bg = Color::rgba(0.1, 0.1, 0.1, 0.95);
    let icon_color = Color::rgba(1.0, 1.0, 1.0, 0.8);

    // Scoop geometry tuned for the 64×64 FAB sitting in the
    // hollow. The scoop carves a stadium pouch leaving ~6 px of
    // visible padding between the button and the dock fill on all
    // sides:
    //   width 80   →  8 px horizontal padding per side of the 64 px button
    //   depth 42   →  6 px below the button bottom (button y=36 → floor y=42)
    //   cr 8       → small ears at the top corners of the hollow
    let scoop_width: f32 = 80.0;
    let scoop_depth: f32 = 42.0;
    let scoop_corner_radius = 8.0;

    div().w_full().flex_row().justify_center().child(
        notch()
            .center_scoop_top_rounded(scoop_width, scoop_depth, scoop_corner_radius)
            .rounded_top(24.0)
            .bg(dock_bg)
            .w_fit()
            .h(50.0 + scoop_depth)
            .child(dock_icon_row(icon_color))
            .child(dock_fab()),
    )
}

/// The row of dock icons inside the notch's reserved padding.
fn dock_icon_row(icon_color: Color) -> Div {
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
        .child(svg(MUSIC_SVG).square(ICON_SIZE).color(icon_color))
}

/// Floating action button that sits in the scoop. Position is
/// hard-coded against the dock's `w_fit()` size — works because the
/// icon row has fixed gaps + icon sizes so the dock width is
/// predictable.
fn dock_fab() -> Div {
    div()
        .absolute()
        .top(-28.0)
        .left(180.0)
        .rounded_full()
        .w(64.0)
        .h(64.0)
        .bg(Color::from_hex(0x39FF14)) // neon green
        .shadow_lg()
        .flex()
        .items_center()
        .justify_center()
        .child(svg(PLUS_SVG).square(28.0).color(Color::BLACK))
}
