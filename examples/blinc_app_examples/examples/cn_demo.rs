//! blinc_cn Components Demo
//!
//! Showcases all available blinc_cn components in a scrollable grid layout.
//!
//! Run with: cargo run -p blinc_app_examples --example cn_demo --features cn

use blinc_animation::SpringConfig;
use blinc_app::prelude::*;
use blinc_app::windowed::WindowedApp;
use blinc_app::windowed::WindowedContext;
use blinc_cn::prelude::*;
use blinc_core::Color;
use blinc_layout::widgets::text_input::text_input_data;
use blinc_theme::{ColorToken, ThemeBundle, ThemeState};

/// Theme bundle shared by the desktop entry point and the wasm
/// wrapper. The `build-web-examples` codegen detects this `pub fn`
/// and hands the returned bundle to `ThemeState::init` before
/// `WebApp::run`, so the cn `with_css(CN_STYLES)` payload + the
/// `#css-overrides` rules land on both targets identically.
pub fn theme_bundle() -> ThemeBundle {
    HybridTheme::bundle()
        .with_css(blinc_cn::cn_styles::CN_STYLES)
        .with_css(
            r#"
                #css-overrides .cn-button--primary { border-radius: 0; }
                #css-overrides .cn-button--destructive:hover { background: var(--primary); }
                #css-overrides .cn-badge--success { background: #00cc66; }
                #css-demo-card { border-width: 2px; border-color: var(--primary); }
            "#,
        )
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        // .with_env_filter("blinc_layout::event_dispatch=debug")
        .init();

    let config = WindowConfig {
        title: "blinc_cn Components Demo".to_string(),
        width: 900,
        height: 900,
        resizable: true,
        fullscreen: false,
        animation_fps_cap: None,
        max_frame_latency: 2,
        ..Default::default()
    };

    WindowedApp::run_with_theme(
        config,
        theme_bundle(),
        // blinc_theme::detect_system_color_scheme(),
        ColorScheme::Dark,
        // Closure wrapper rather than passing `build_ui` directly: on
        // edition 2024, `fn build_ui(ctx) -> impl ElementBuilder`
        // captures `ctx`'s lifetime in the return type (RPIT capture
        // rules), which breaks the higher-ranked `FnMut` bound. The
        // closure infers per-lifetime so the bound is satisfied.
        build_ui,
    )
}

pub fn build_ui(ctx: &mut WindowedContext) -> impl ElementBuilder + use<> {
    eprintln!("build_ui called");
    let theme = ThemeState::get();

    eprintln!(
        "Current theme platform: {:?}",
        blinc_theme::platform::Platform::current()
    );
    eprintln!("Theme color scheme: {:?}", theme.scheme());
    let bg = theme.color(ColorToken::Background);

    // Create scroll ref to track scroll position
    let scroll_ref = ctx.use_scroll_ref();

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(bg)
        .flex_col()
        .child(header(ctx))
        .child(
            scroll()
                .w_full()
                .h(ctx.height - 80.0)
                .viewport_cull(true)
                .bind(&scroll_ref)
                .child(
                    div()
                        .w_full()
                        .p(theme.spacing().space_3)
                        .flex_col()
                        .gap(theme.spacing().space_8)
                        // Accordion at top for layout animation testing
                        .child(accordion_section())
                        .child(menubar_demo())
                        // Test layout animation
                        // .child(layout_animation_test())
                        // Component sections
                        .child(progress_section(ctx))
                        .child(buttons_section(ctx))
                        .child(css_overrides_section())
                        .child(badges_section())
                        .child(cards_section())
                        .child(alerts_section())
                        .child(form_inputs_section(ctx))
                        .child(toggles_section(ctx))
                        .child(slider_section(ctx))
                        .child(radio_section(ctx))
                        .child(select_section(ctx))
                        .child(combobox_section(ctx))
                        .child(context_menu_section())
                        .child(dropdown_menu_section())
                        .child(hover_card_section())
                        .child(popover_section())
                        .child(tooltip_section())
                        .child(dialog_section(ctx))
                        .child(sheet_section(ctx))
                        .child(drawer_section(ctx))
                        .child(tabs_section(ctx))
                        .child(breadcrumb_section())
                        .child(pagination_section(ctx))
                        .child(navigation_menu_section())
                        .child(sidebar_section(ctx))
                        .child(resizable_section())
                        .child(scroll_area_section())
                        .child(aspect_ratio_section())
                        .child(avatar_section())
                        .child(toast_section(ctx))
                        .child(loading_section(ctx))
                        .child(kbd_section())
                        .child(icon_gallery_section())
                        .child(misc_section())
                        .child(tree_view_section())
                        .child(table_section())
                        .child(charts_section()),
                ),
        )
}

// /// Test for layout animation - toggles div height on click
// fn layout_animation_test() -> impl ElementBuilder {
//     use blinc_core::context_state::BlincContextState;
//     use blinc_layout::prelude::use_shared_state_with;
//     use blinc_layout::stateful::Stateful;

//     let theme = ThemeState::get();
//     let surface = theme.color(ColorToken::Surface);
//     let border = theme.color(ColorToken::Border);
//     let text_primary = theme.color(ColorToken::TextPrimary);
//     let text_secondary = theme.color(ColorToken::TextSecondary);

//     // State for toggling - use State<bool> for reactivity
//     let is_expanded: blinc_core::State<bool> =
//         BlincContextState::get().use_state_keyed("layout_anim_test_expanded", || false);

//     let signal_id = is_expanded.signal_id();
//     let is_expanded_for_click = is_expanded.clone();
//     let is_expanded_for_state = is_expanded.clone();

//     // Use shared state with unit type, deps on the signal
//     let state_handle = use_shared_state_with("layout_anim_test_state", ());

//     section_container()
//         .child(section_title("Layout Animation Test"))
//         .child(
//             text("Click the box to add/remove children. The container should animate its height.")
//                 .size(t_sm())
//                 .color(text_secondary),
//         )
//         .child(
//             Stateful::with_shared_state(state_handle)
//                 .deps(&[signal_id])
//                 .on_state(move |_: &(), container: &mut Div| {
//                     let expanded = is_expanded_for_state.get();
//                     let is_expanded_click = is_expanded_for_click.clone();

//                     // The container has layout animation with STABLE KEY
//                     // This key persists across rebuilds so animation can track bounds changes
//                     let mut animated_container = div()
//                         .w(300.0)
//                         .bg(surface)
//                         .border(2.0, border)
//                         .rounded(r_default())
//                         .overflow_clip()
//                         .flex_col()
//                         .gap(8.0)
//                         .p(12.0)
//                         .animate_layout(
//                             LayoutAnimationConfig::height()
//                                 .with_key("layout-test-container")
//                                 .snappy(),
//                         )
//                         .cursor_pointer()
//                         .on_click(move |_| {
//                             let current = is_expanded_click.get();
//                             is_expanded_click.set(!current);
//                             tracing::info!(
//                                 "Layout animation test: toggled to {}",
//                                 if !current { "expanded" } else { "collapsed" }
//                             );
//                         });

//                     // Always show header
//                     animated_container = animated_container.child(
//                         text("Click me to toggle content")
//                             .size(t_sm())
//                             .weight(FontWeight::Medium)
//                             .color(text_primary),
//                     );

//                     // Conditionally add more children when expanded
//                     if expanded {
//                         animated_container = animated_container
//                             .child(text("Item 1").size(t_sm()).color(text_secondary))
//                             .child(text("Item 2").size(t_sm()).color(text_secondary))
//                             .child(text("Item 3").size(t_sm()).color(text_secondary))
//                             .child(text("Item 4").size(t_sm()).color(text_secondary));
//                     }

//                     let status = text(format!(
//                         "State: {} ({} children)",
//                         if expanded { "expanded" } else { "collapsed" },
//                         if expanded { 5 } else { 1 }
//                     ))
//                     .size(t_xs())
//                     .color(text_secondary);

//                     container.merge(
//                         div()
//                             .flex_col()
//                             .gap(12.0)
//                             .child(animated_container)
//                             .child(status),
//                     );
//                 }),
//         )
// }

// ============================================================================
// MENUBAR DEMO
// ============================================================================

fn menubar_demo() -> impl ElementBuilder + use<> {
    section_container().child(section_title("Menubar")).child(
        div().flex_row().flex_wrap().child(
            cn::menubar()
                .trigger_mode(cn::MenuTriggerMode::Hover) // Open menus on hover
                .menu("File", |m| {
                    m.item("New", || tracing::info!("New clicked"))
                        .item_with_shortcut("Open", "Ctrl+O", || tracing::info!("Open clicked"))
                        .item_with_shortcut("Save", "Ctrl+S", || tracing::info!("Save clicked"))
                        .item_with_shortcut("Save As...", "Ctrl+Shift+S", || {
                            tracing::info!("Save As clicked")
                        })
                        .separator()
                        .submenu("Recent Files", |sub| {
                            sub.item("document1.txt", || tracing::info!("Recent: document1.txt"))
                                .item("project.rs", || tracing::info!("Recent: project.rs"))
                                .item("config.toml", || tracing::info!("Recent: config.toml"))
                        })
                        .separator()
                        .item_with_shortcut("Exit", "Alt+F4", || tracing::info!("Exit clicked"))
                })
                .menu("Edit", |m| {
                    m.item_with_shortcut("Undo", "Ctrl+Z", || tracing::info!("Undo clicked"))
                        .item_with_shortcut("Redo", "Ctrl+Y", || tracing::info!("Redo clicked"))
                        .separator()
                        .item_with_shortcut("Cut", "Ctrl+X", || tracing::info!("Cut clicked"))
                        .item_with_shortcut("Copy", "Ctrl+C", || tracing::info!("Copy clicked"))
                        .item_with_shortcut("Paste", "Ctrl+V", || tracing::info!("Paste clicked"))
                        .separator()
                        .item_with_shortcut("Select All", "Ctrl+A", || {
                            tracing::info!("Select All clicked")
                        })
                })
                .menu("View", |m| {
                    m.item("Zoom In", || tracing::info!("Zoom In clicked"))
                        .item("Zoom Out", || tracing::info!("Zoom Out clicked"))
                        .item("Reset Zoom", || tracing::info!("Reset Zoom clicked"))
                        .separator()
                        .item("Toggle Sidebar", || {
                            tracing::info!("Toggle Sidebar clicked")
                        })
                        .item("Toggle Fullscreen", || {
                            tracing::info!("Toggle Fullscreen clicked")
                        })
                })
                .menu("Help", |m| {
                    m.item("Documentation", || tracing::info!("Documentation clicked"))
                        .item("Keyboard Shortcuts", || {
                            tracing::info!("Keyboard Shortcuts clicked")
                        })
                        .separator()
                        .item("About", || tracing::info!("About clicked"))
                })
                // Custom trigger — chevron SVG + label. Leave padding
                // to the outer `.cn-menubar-trigger` so this trigger
                // sizes identically to the labelled ones (File / Edit
                // / …) and stays inline with them.
                .menu_custom(
                    |is_open| {
                        let theme = ThemeState::get();
                        let text_color = theme.color(ColorToken::TextPrimary);
                        const CHEVRON_DOWN: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg>"#;
                        const CHEVRON_RIGHT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m9 18 6-6-6-6"/></svg>"#;
                        let icon_svg = if is_open { CHEVRON_DOWN } else { CHEVRON_RIGHT };
                        div()
                            .flex_row()
                            .items_center()
                            .gap(4.0)
                            .child(
                                text("Actions")
                                    .size(theme.typography().text_sm)
                                    .color(text_color),
                            )
                            .child(svg(icon_svg).size(10.0, 10.0).color(text_color))
                    },
                    |m| {
                        m.item("Run Task", || tracing::info!("Run Task clicked"))
                            .item("Build Project", || tracing::info!("Build Project clicked"))
                            .separator()
                            .item("Clear Cache", || tracing::info!("Clear Cache clicked"))
                    },
                ),
        ),
    )
}

/// Header with title and theme toggle
fn header(ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let surface = theme.color(ColorToken::Surface);
    let text_primary = theme.color(ColorToken::TextPrimary);
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let border = theme.color(ColorToken::Border);

    // Theme toggle switch state
    // let is_dark = ctx.use_state_keyed("theme_is_dark", || {
    //     ThemeState::get().scheme() == ColorScheme::Dark
    // });
    let _scheduler = ctx.animation_handle();

    div()
        .w_full()
        .h(80.0)
        .bg(surface)
        .border_bottom(1.5, border)
        .px(theme.spacing().space_3)
        .flex_row()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex_col()
                .gap(2.0)
                .child(
                    text("blinc_cn Components")
                        .size(theme.typography().text_2xl)
                        .weight(FontWeight::Bold)
                        .color(text_primary),
                )
                .child(
                    text("shadcn-inspired component library for Blinc")
                        .size(theme.typography().text_sm)
                        .color(text_secondary),
                ),
        )
    // .child(
    //     // cn::switch(&is_dark, scheduler)
    //     //     .label("Dark Mode")
    //     //     .on_change(|_| {
    //     //         ThemeState::get().toggle_scheme();
    //     //     }),
    // )
}

/// Section title helper
fn section_title(title: &str) -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_primary = theme.color(ColorToken::TextPrimary);

    text(title)
        .size(theme.typography().text_xl)
        .weight(FontWeight::Bold)
        .color(text_primary)
}

// ----- Token shorthand helpers (used by hand-built demo elements) --------
//
// These keep the demo's hardcoded magic numbers in one place so a theme
// swap (Restrained / Hybrid / Expressive) actually shows up in the demo
// content rather than freezing every card at "8px radius / 14px text".

fn r_default() -> f32 {
    ThemeState::get().radius(RadiusToken::Default)
}
fn t_xs() -> f32 {
    ThemeState::get().typography().text_xs
}
fn t_sm() -> f32 {
    ThemeState::get().typography().text_sm
}
fn t_base() -> f32 {
    ThemeState::get().typography().text_base
}
fn t_lg() -> f32 {
    ThemeState::get().typography().text_lg
}

/// Section container helper
fn section_container() -> Div {
    let theme = ThemeState::get();
    let surface = theme.color(ColorToken::Surface);
    let border = theme.color(ColorToken::Border);
    let radius = theme.radii().radius_xl;

    div()
        .w_full()
        .h_fit()
        .bg(surface)
        .rounded(radius)
        .border(1.0, border)
        .p(theme.spacing().space_1)
        .flex_col()
        .gap(theme.spacing().space_3)
}

// ============================================================================
// CSS OVERRIDE DEMO SECTION
// ============================================================================

fn css_overrides_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    section_container()
        .id("css-overrides")
        .child(section_title("CSS Overrides"))
        .child(
            text(
                "Components below demonstrate CSS overrides applied after default styles. \
                  Primary buttons have square corners, destructive hover turns primary, \
                  and cards have a primary-colored border.",
            )
            .size(t_sm())
            .color(text_secondary),
        )
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(12.0)
                .child(cn::button("Square Primary (CSS override)"))
                .child(cn::button("Hover me (destructive)").variant(ButtonVariant::Destructive))
                .child(cn::button("Outline (default)").variant(ButtonVariant::Outline))
                .child(cn::button("Ghost (default)").variant(ButtonVariant::Ghost)),
        )
        .child(
            cn::card()
                .id("css-demo-card")
                .w(350.0)
                .child(cn::card_header().title("Card with CSS border override"))
                .child(
                    cn::card_content().child(
                        text("This card has a 2px primary-colored border via CSS override")
                            .size(t_sm())
                            .color(text_secondary),
                    ),
                ),
        )
}

// ============================================================================
// BUTTON SECTION
// ============================================================================

fn buttons_section(_ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    section_container()
        .child(section_title("Buttons"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(12.0)
                .child(cn::button("Primary"))
                .child(cn::button("Secondary").variant(ButtonVariant::Secondary))
                .child(cn::button("Destructive").variant(ButtonVariant::Destructive))
                .child(cn::button("Outline").variant(ButtonVariant::Outline))
                .child(cn::button("Ghost").variant(ButtonVariant::Ghost))
                .child(cn::button("Link").variant(ButtonVariant::Link)),
        )
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(12.0)
                .child(cn::button("Small").size(ButtonSize::Small))
                .child(cn::button("Medium").size(ButtonSize::Medium))
                .child(cn::button("Large").size(ButtonSize::Large))
                .child(cn::button("Disabled").disabled(true)),
        )
        // Icon-only buttons at various sizes
        .child(
            text("Icon-only buttons (centering test)")
                .size(t_xs())
                .color(text_secondary),
        )
        .child(
            div()
                .flex_row()
                .items_center()
                .gap(12.0)
                // Icon-only at default size (font_size + 2 = 16)
                .child(cn::button("").icon(icons::ARROW_RIGHT))
                // Icon-only at 20px
                .child(cn::button("").icon(icons::ARROW_RIGHT).icon_size(20.0))
                // Icon-only at 24px
                .child(cn::button("").icon(icons::ARROW_RIGHT).icon_size(24.0))
                // Icon-only at 12px
                .child(cn::button("").icon(icons::ARROW_RIGHT).icon_size(12.0))
                // Icon-only with different variant
                .child(
                    cn::button("")
                        .icon(icons::PLUS)
                        .variant(ButtonVariant::Outline),
                )
                .child(
                    cn::button("")
                        .icon(icons::PLUS)
                        .variant(ButtonVariant::Outline)
                        .icon_size(20.0),
                )
                // Button with icon + label
                .child(cn::button("With Icon").icon(icons::STAR)),
        )
}

// ============================================================================
// BADGES SECTION
// ============================================================================

fn badges_section() -> impl ElementBuilder + use<> {
    section_container().child(section_title("Badges")).child(
        div()
            .flex_col()
            .gap(12.0)
            // Soft (default) — pale tint + same-hue text.
            .child(
                div()
                    .flex_row()
                    .flex_wrap()
                    .gap(12.0)
                    .child(cn::badge("In review"))
                    .child(cn::badge("Pending").variant(BadgeVariant::Warning))
                    .child(
                        cn::badge("Shipped")
                            .variant(BadgeVariant::Success)
                            // Raw `svg()` (no `.color(...)` inline) so
                            // the badge's CSS rule can tint the path's
                            // stroke. `cn::icon` would set inline
                            // `.color(TextPrimary)` which wins via
                            // specificity and pins the glyph at dark
                            // text colour.
                            .icon(
                                svg(to_svg_with_stroke(icons::CHECK, 12.0, 2.0)).size(12.0, 12.0),
                            ),
                    )
                    .child(cn::badge("Blocked").variant(BadgeVariant::Destructive))
                    .child(cn::badge("Draft").variant(BadgeVariant::Secondary)),
            )
            // Solid (legacy fill).
            .child(
                div()
                    .flex_row()
                    .flex_wrap()
                    .gap(12.0)
                    .child(cn::badge("Default").style(BadgeStyle::Solid))
                    .child(
                        cn::badge("Secondary")
                            .style(BadgeStyle::Solid)
                            .variant(BadgeVariant::Secondary),
                    )
                    .child(
                        cn::badge("Success")
                            .style(BadgeStyle::Solid)
                            .variant(BadgeVariant::Success),
                    )
                    .child(
                        cn::badge("Warning")
                            .style(BadgeStyle::Solid)
                            .variant(BadgeVariant::Warning),
                    )
                    .child(
                        cn::badge("Destructive")
                            .style(BadgeStyle::Solid)
                            .variant(BadgeVariant::Destructive),
                    ),
            )
            // Outline.
            .child(
                div()
                    .flex_row()
                    .flex_wrap()
                    .gap(12.0)
                    .child(cn::badge("Default").style(BadgeStyle::Outline))
                    .child(
                        cn::badge("Secondary")
                            .style(BadgeStyle::Outline)
                            .variant(BadgeVariant::Secondary),
                    )
                    .child(
                        cn::badge("Success")
                            .style(BadgeStyle::Outline)
                            .variant(BadgeVariant::Success),
                    )
                    .child(
                        cn::badge("Warning")
                            .style(BadgeStyle::Outline)
                            .variant(BadgeVariant::Warning),
                    )
                    .child(
                        cn::badge("Destructive")
                            .style(BadgeStyle::Outline)
                            .variant(BadgeVariant::Destructive),
                    ),
            ),
    )
}

// ============================================================================
// CARDS SECTION
// ============================================================================

fn cards_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();

    section_container()
        .child(section_title("Cards"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                .child(
                    cn::card()
                        .bg(theme.color(ColorToken::SurfaceElevated))
                        .w(300.0)
                        .child(cn::card_header().title("Card Title").description("Card description"))
                        .child(cn::card_content().child(
                            text("This is the card content. Cards are great for grouping related information.")
                                .size(theme.typography().text_sm)
                                .color(theme.color(ColorToken::TextSecondary)),
                        ))
                        .child(cn::card_footer().child(cn::button("Action"))),
                )
                .child(
                    cn::card()
                        .bg(theme.color(ColorToken::SurfaceElevated))
                        .w(300.0)
                        .child(cn::card_header().title("Simple Card"))
                        .child(cn::card_content().child(
                            text("A simpler card without footer.")
                                .size(theme.typography().text_sm)
                                .color(theme.color(ColorToken::TextSecondary)),
                        )),
                ),
        )
}

// ============================================================================
// ALERTS SECTION
// ============================================================================

fn alerts_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Alerts"))
        .child(
            div()
                .flex_col()
                .gap(12.0)
                .child(cn::alert("This is a default informational alert."))
                .child(
                    cn::alert("Operation completed successfully!").variant(AlertVariant::Success),
                )
                .child(cn::alert("Please review before proceeding.").variant(AlertVariant::Warning))
                .child(
                    cn::alert("An error occurred. Please try again.")
                        .variant(AlertVariant::Destructive),
                ),
        )
        .child(
            cn::alert_box()
                .variant(AlertVariant::Warning)
                .title("Heads up!")
                .description("This is an alert box with both title and description."),
        )
}

// ============================================================================
// FORM INPUTS SECTION
// ============================================================================

fn form_inputs_section(ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    let username_data = text_input_data();
    let email_data = text_input_data();
    let password_data = text_input_data();
    let bio_state = blinc_layout::widgets::text_area::text_area_state();

    // Number input states for the new cn::number_input row.
    let qty = ctx.use_state_keyed("number_qty", || 1.0);
    let temperature = ctx.use_state_keyed("number_temp", || 22.5);

    let otp_code = ctx.use_state_keyed("otp_code", || String::new());

    section_container()
        .child(section_title("Form Inputs"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .items_start()
                .w_full()
                .gap_px(24.0)
                .h_fit()
                // Column 1: Text inputs
                .child(
                    div()
                        .flex_col()
                        .flex_wrap()
                        .w(300.0)
                        .h_fit()
                        .gap_px(16.0)
                        .child(
                            cn::input(&username_data)
                                .label("Username")
                                .placeholder("Enter username"),
                        )
                        .child(
                            cn::input(&email_data)
                                .label("Email")
                                .placeholder("you@example.com")
                                .required(),
                        )
                        .child(
                            cn::input(&password_data)
                                .label("Password")
                                .placeholder("Enter password")
                                .password(),
                        ),
                )
                // Column 2: Textarea
                .child(
                    div()
                        .flex_col()
                        .flex_wrap()
                        .h_fit()
                        .gap_px(4.0)
                        .child(
                            cn::textarea(&bio_state)
                                .label("Bio")
                                .placeholder("Tell us about yourself...")
                                .rows(4)
                                .w(300.0),
                        )
                        .child(cn::label("Labels can be standalone")) // Column 3: Number inputs. Each label+field pair is a
                        // tight vertical group (gap 4) — matches the rhythm
                        // `cn::input(...).label(…)` produces internally — and
                        // pairs are spaced 16 px apart to match the inter-row
                        // spacing of the text-input column.
                        .child(
                            div()
                                .my(6.0)
                                .flex_col()
                                .h_fit()
                                .gap_px(8.0)
                                .child(
                                    div()
                                        .flex_col()
                                        .gap_px(4.0)
                                        .child(cn::label("Quantity"))
                                        .child(
                                            cn::number_input(&qty)
                                                .min(0.0)
                                                .max(99.0)
                                                .step(1.0)
                                                .precision(0),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap_px(4.0)
                                        .child(cn::label("Temperature °C"))
                                        .child(
                                            cn::number_input(&temperature)
                                                .min(-50.0)
                                                .max(60.0)
                                                .step(0.5)
                                                .precision(1),
                                        ),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex_col()
                        .w_fit()
                        .h_fit()
                        .gap_px(4.0)
                        .child(cn::label("Verification code"))
                        .child(
                            cn::input_otp(&otp_code, 6)
                                .numeric_only(true)
                                .on_complete(|code| println!("otp complete: {code}")),
                        ),
                ),
        )
}

// ============================================================================
// TOGGLES SECTION (Checkbox, Switch)
// ============================================================================

fn toggles_section(ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    let checkbox1 = ctx.use_state_keyed("checkbox1", || false);
    let checkbox2 = ctx.use_state_keyed("checkbox2", || true);
    let checkbox3 = ctx.use_state_keyed("checkbox3", || false);

    let switch1 = ctx.use_state_keyed("switch1", || false);
    let switch2 = ctx.use_state_keyed("switch2", || true);
    let switch3 = ctx.use_state_keyed("switch3", || false);

    // Toggle widget states. Each toggle gets its own keyed state so the
    // visual differences across variants / sizes can be observed
    // independently — the previous version had two cells share
    // `large_toggle`, making it look like Small ≠ Medium / Large
    // visually because they were actually pointing at different state
    // values.
    let bold_on = ctx.use_state_keyed("toggle_bold", || true);
    let italic_on = ctx.use_state_keyed("toggle_italic", || false);
    let underline_on = ctx.use_state_keyed("toggle_underline", || false);
    let outline_off = ctx.use_state_keyed("toggle_outline_off", || false);
    let outline_on = ctx.use_state_keyed("toggle_outline_on", || true);
    let small_toggle = ctx.use_state_keyed("toggle_small", || false);
    let medium_toggle = ctx.use_state_keyed("toggle_medium", || true);
    let large_toggle = ctx.use_state_keyed("toggle_large", || false);
    let disabled_on = ctx.use_state_keyed("toggle_disabled_on", || true);
    let disabled_off = ctx.use_state_keyed("toggle_disabled_off", || false);

    // Toggle group: single-select text-alignment picker. The shared
    // `State<String>` carries the current choice; each item flips it.
    let text_align = ctx.use_state_keyed("toggle_group_align", || "left".to_string());
    // Same group, Outline variant — same state, second instance to
    // exercise the variant + shared-state pattern.
    let text_align_outline =
        ctx.use_state_keyed("toggle_group_align_outline", || "center".to_string());

    // Canonical Lucide icons re-exported through `blinc_cn::prelude`
    // (`icons` + `to_svg`). The helper wraps each path in a proper
    // `<svg width=… height=… stroke="currentColor">` so the SVG
    // renderer's `color()` tint picks them up correctly.
    let icon_bold = to_svg(icons::BOLD, 16.0);
    let icon_italic = to_svg(icons::ITALIC, 16.0);
    let icon_underline = to_svg(icons::UNDERLINE, 16.0);
    let icon_align_left = to_svg(icons::TEXT_ALIGN_START, 16.0);
    let icon_align_center = to_svg(icons::TEXT_ALIGN_CENTER, 16.0);
    let icon_align_right = to_svg(icons::TEXT_ALIGN_END, 16.0);
    let icon_align_justify = to_svg(icons::TEXT_ALIGN_JUSTIFY, 16.0);

    let theme = ThemeState::get();
    let caption_color = theme.color(ColorToken::TextTertiary);
    let row_caption = |row_label: &str| text(row_label).size(11.0).color(caption_color);

    section_container().child(section_title("Toggles")).child(
        div()
            .flex_row()
            .flex_wrap()
            .gap(24.0)
            // Checkboxes
            .child(
                div()
                    .flex_col()
                    .gap(12.0)
                    .child(cn::checkbox(&checkbox1).label("Accept terms"))
                    .child(cn::checkbox(&checkbox2).label("Checked by default"))
                    .child(cn::checkbox(&checkbox3).label("Disabled").disabled(true)),
            )
            // Switches
            .child(
                div()
                    .flex_col()
                    .gap(12.0)
                    .child(cn::switch(&switch1).label("Notifications"))
                    .child(cn::switch(&switch2).label("Dark mode"))
                    .child(cn::switch(&switch3).label("Disabled").disabled(true)),
            )
            // Toggle component: icon-only toolbar + variant / size matrix.
            .child(
                div()
                    .flex_col()
                    .gap(16.0)
                    // Icon-only toolbar — canonical bold / italic / underline.
                    // `bold_on` starts on so the bg-accent overlay is visible
                    // at first paint; italic / underline start off.
                    .child(
                        div()
                            .flex_col()
                            .gap(4.0)
                            .child(row_caption("Toolbar (default variant)"))
                            .child(
                                div()
                                    .flex_row()
                                    .gap(4.0)
                                    .child(
                                        cn::toggle(&bold_on)
                                            .icon(&icon_bold)
                                            .aria_label("Toggle bold"),
                                    )
                                    .child(
                                        cn::toggle(&italic_on)
                                            .icon(&icon_italic)
                                            .aria_label("Toggle italic"),
                                    )
                                    .child(
                                        cn::toggle(&underline_on)
                                            .icon(&icon_underline)
                                            .aria_label("Toggle underline"),
                                    ),
                            ),
                    )
                    // Variants side-by-side — Outline draws a border in
                    // both states; Default only shows the bg-accent on on.
                    .child(
                        div()
                            .flex_col()
                            .gap(4.0)
                            .child(row_caption("Default vs Outline (off · on)"))
                            .child(
                                div()
                                    .flex_row()
                                    .gap(8.0)
                                    .child(cn::toggle(&small_toggle).label("Default"))
                                    .child(cn::toggle(&medium_toggle).label("Default"))
                                    .child(
                                        cn::toggle(&outline_off)
                                            .variant(cn::ToggleVariant::Outline)
                                            .label("Outline"),
                                    )
                                    .child(
                                        cn::toggle(&outline_on)
                                            .variant(cn::ToggleVariant::Outline)
                                            .label("Outline"),
                                    ),
                            ),
                    )
                    // Size ladder — independent state per cell so heights
                    // are the only difference between Small / Medium / Large.
                    .child(
                        div()
                            .flex_col()
                            .gap(4.0)
                            .child(row_caption("Sizes (Small · Medium · Large)"))
                            .child(
                                div()
                                    .flex_row()
                                    .gap(8.0)
                                    .items_center()
                                    .child(
                                        cn::toggle(&small_toggle)
                                            .size(cn::ToggleSize::Small)
                                            .label("Small"),
                                    )
                                    .child(cn::toggle(&medium_toggle).label("Medium"))
                                    .child(
                                        cn::toggle(&large_toggle)
                                            .size(cn::ToggleSize::Large)
                                            .label("Large"),
                                    ),
                            ),
                    )
                    // Disabled — both states so the dim treatment is visible.
                    .child(
                        div()
                            .flex_col()
                            .gap(4.0)
                            .child(row_caption("Disabled (on · off)"))
                            .child(
                                div()
                                    .flex_row()
                                    .gap(8.0)
                                    .child(
                                        cn::toggle(&disabled_on)
                                            .label("On (disabled)")
                                            .disabled(true),
                                    )
                                    .child(
                                        cn::toggle(&disabled_off)
                                            .variant(cn::ToggleVariant::Outline)
                                            .label("Off (disabled)")
                                            .disabled(true),
                                    ),
                            ),
                    )
                    // ToggleGroup: single-select text-alignment picker.
                    // First row uses Default variant + Small size; second
                    // row uses Outline variant for the boxed look.
                    .child(
                        div()
                            .flex_col()
                            .gap(4.0)
                            .child(row_caption("Group (single-select)"))
                            .child(
                                cn::toggle_group(&text_align)
                                    .size(cn::ToggleSize::Small)
                                    .item(cn::toggle_item("left").icon(&icon_align_left))
                                    .item(cn::toggle_item("center").icon(&icon_align_center))
                                    .item(cn::toggle_item("right").icon(&icon_align_right))
                                    .item(cn::toggle_item("justify").icon(&icon_align_justify)),
                            )
                            .child(
                                cn::toggle_group(&text_align_outline)
                                    .variant(cn::ToggleVariant::Outline)
                                    .size(cn::ToggleSize::Small)
                                    .item(cn::toggle_item("left").icon(&icon_align_left))
                                    .item(cn::toggle_item("center").icon(&icon_align_center))
                                    .item(cn::toggle_item("right").icon(&icon_align_right))
                                    .item(cn::toggle_item("justify").icon(&icon_align_justify)),
                            ),
                    ),
            ),
    )
}

// ============================================================================
// SLIDER SECTION
// ============================================================================

fn slider_section(ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    let volume = ctx.use_state_keyed("volume", || 0.5);
    let brightness = ctx.use_state_keyed("brightness", || 75.0);
    let disabled_slider = ctx.use_state_keyed("disabled_slider", || 0.3);

    section_container().child(section_title("Sliders")).child(
        div()
            .flex_col()
            .items_start() // Prevent width stretching
            .h_fit()
            .gap(4.0)
            .child(
                div()
                    .h_fit()
                    .w(300.0)
                    .child(cn::slider(&volume).label("Volume").show_value()),
            )
            .child(
                div().h_fit().w(300.0).child(
                    cn::slider(&brightness)
                        .label("Brightness")
                        .min(0.0)
                        .max(100.0)
                        .step(5.0)
                        .show_value(),
                ),
            )
            .child(
                div().h_fit().w(300.0).child(
                    cn::slider(&disabled_slider)
                        .label("Disabled")
                        .disabled(true),
                ),
            ),
    )
}

// ============================================================================
// RADIO GROUP SECTION
// ============================================================================

fn radio_section(ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    let size_choice = ctx.use_state_keyed("size_choice", || "medium".to_string());
    let color_choice = ctx.use_state_keyed("color_choice", || "blue".to_string());

    section_container()
        .child(section_title("Radio Groups"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(48.0)
                // Vertical layout
                .child(
                    cn::radio_group(&size_choice)
                        .label("Select Size")
                        .option("small", "Small")
                        .option("medium", "Medium")
                        .option("large", "Large"),
                )
                // Horizontal layout
                .child(
                    cn::radio_group(&color_choice)
                        .label("Select Color")
                        .horizontal()
                        .option("red", "Red")
                        .option("green", "Green")
                        .option("blue", "Blue"),
                ),
        )
}

// ============================================================================
// SELECT SECTION
// ============================================================================

fn select_section(ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    let fruit = ctx.use_state_keyed("fruit_select", || "".to_string());
    let size = ctx.use_state_keyed("size_select", || "medium".to_string());
    let disabled_select = ctx.use_state_keyed("disabled_select", || "option1".to_string());

    section_container().child(section_title("Select")).child(
        div()
            .flex_col()
            .w_full()
            .flex_wrap()
            .h_fit() // Prevent height stretching
            .gap(4.0)
            // Basic select with placeholder
            .child(
                cn::select(&fruit)
                    .label("Favorite Fruit")
                    .placeholder("Choose a fruit...")
                    .option("apple", "Apple")
                    .option("banana", "Banana")
                    .option("cherry", "Cherry")
                    .option("date", "Date")
                    .option("elderberry", "Elderberry")
                    .on_change(|v| tracing::info!("Selected fruit: {}", v)),
            )
            // Select with pre-selected value
            .child(
                cn::select(&size)
                    .label("Size")
                    .option("small", "Small")
                    .option("medium", "Medium")
                    .option("large", "Large")
                    .option("xl", "Extra Large"),
            )
            // Disabled select
            .child(
                cn::select(&disabled_select)
                    .label("Disabled")
                    .option("option1", "Option 1")
                    .option("option2", "Option 2")
                    .disabled(true),
            ),
    )
}

// ============================================================================
// COMBOBOX SECTION
// ============================================================================

fn combobox_section(ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    let country = ctx.use_state_keyed("country_combobox", || "".to_string());
    let framework = ctx.use_state_keyed("framework_combobox", || "".to_string());
    let custom_value = ctx.use_state_keyed("custom_combobox", || "".to_string());

    section_container().child(section_title("Combobox")).child(
        div()
            .flex_row()
            .overflow_visible()
            .items_start() // Prevent height stretching
            .gap(4.0)
            .h_fit()
            .w_full()
            // Basic searchable combobox
            .child(
                cn::combobox(&country)
                    .label("Country")
                    .placeholder("Search countries...")
                    .option("us", "United States")
                    .option("uk", "United Kingdom")
                    .option("de", "Germany")
                    .option("fr", "France")
                    .option("jp", "Japan")
                    .option("au", "Australia")
                    .option("ca", "Canada")
                    .option("br", "Brazil")
                    .on_change(|v| tracing::info!("Selected country: {}", v)),
            )
            // Combobox with more options
            .child(
                cn::combobox(&framework)
                    .label("Framework")
                    .placeholder("Search frameworks...")
                    .option("react", "React")
                    .option("vue", "Vue.js")
                    .option("angular", "Angular")
                    .option("svelte", "Svelte")
                    .option("solid", "SolidJS")
                    .option("qwik", "Qwik")
                    .option("astro", "Astro")
                    .on_change(|v| tracing::info!("Selected framework: {}", v)),
            )
            // Combobox with custom values allowed
            .child(
                cn::combobox(&custom_value)
                    .label("Custom Allowed")
                    .placeholder("Type anything...")
                    .option("preset1", "Preset Option 1")
                    .option("preset2", "Preset Option 2")
                    .option("preset3", "Preset Option 3")
                    .allow_custom(true)
                    .on_change(|v| tracing::info!("Custom value: {}", v)),
            ),
    )
}

// ============================================================================
// CONTEXT MENU SECTION
// ============================================================================

fn context_menu_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let surface = theme.color(ColorToken::Surface);
    let border = theme.color(ColorToken::Border);
    let text_secondary = theme.color(ColorToken::TextSecondary);

    // Common icon SVGs for menu items
    let scissors_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="3"/><path d="M8.12 8.12 12 12"/><path d="M20 4 8.12 15.88"/><circle cx="6" cy="18" r="3"/><path d="M14.8 14.8 20 20"/></svg>"#;
    let copy_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>"#;
    let clipboard_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect width="8" height="4" x="8" y="2" rx="1" ry="1"/><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/></svg>"#;
    let trash_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>"#;

    section_container()
        .child(section_title("Context Menu"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                // Basic context menu trigger
                .child(
                    div()
                        .w(200.0)
                        .h(120.0)
                        .bg(surface)
                        .border(1.0, border)
                        .rounded(r_default())
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .child(text("Click me!").size(t_sm()).color(text_secondary))
                        .child(
                            text("(opens context menu)")
                                .size(t_xs())
                                .color(text_secondary),
                        )
                        .on_click(move |ctx| {
                            cn::context_menu()
                                .at(ctx.mouse_x, ctx.mouse_y)
                                .item("Cut", || tracing::info!("Cut clicked"))
                                .item("Copy", || tracing::info!("Copy clicked"))
                                .item("Paste", || tracing::info!("Paste clicked"))
                                .separator()
                                .item("Delete", || tracing::info!("Delete clicked"))
                                .show();
                        }),
                )
                // Context menu with shortcuts
                .child({
                    div()
                        .w(200.0)
                        .h(120.0)
                        .bg(surface)
                        .border(1.0, border)
                        .rounded(r_default())
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .child(text("With Shortcuts").size(t_sm()).color(text_secondary))
                        .on_click(move |ctx| {
                            cn::context_menu()
                                .at(ctx.mouse_x, ctx.mouse_y)
                                .item_with_shortcut("Undo", "Ctrl+Z", || tracing::info!("Undo"))
                                .item_with_shortcut("Redo", "Ctrl+Y", || tracing::info!("Redo"))
                                .separator()
                                .item_with_shortcut("Cut", "Ctrl+X", || tracing::info!("Cut"))
                                .item_with_shortcut("Copy", "Ctrl+C", || tracing::info!("Copy"))
                                .item_with_shortcut("Paste", "Ctrl+V", || tracing::info!("Paste"))
                                .separator()
                                .item_with_shortcut("Select All", "Ctrl+A", || {
                                    tracing::info!("Select All")
                                })
                                .show();
                        })
                })
                // Context menu with icons
                .child({
                    let scissors = scissors_icon.to_string();
                    let copy = copy_icon.to_string();
                    let paste = clipboard_icon.to_string();
                    let trash = trash_icon.to_string();

                    div()
                        .w(200.0)
                        .h(120.0)
                        .bg(surface)
                        .border(1.0, border)
                        .rounded(r_default())
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .child(text("With Icons").size(t_sm()).color(text_secondary))
                        .on_click(move |ctx| {
                            cn::context_menu()
                                .at(ctx.mouse_x, ctx.mouse_y)
                                .item_with_icon("Cut", scissors.clone(), || tracing::info!("Cut"))
                                .item_with_icon("Copy", copy.clone(), || tracing::info!("Copy"))
                                .item_with_icon("Paste", paste.clone(), || tracing::info!("Paste"))
                                .separator()
                                .item_with_icon("Delete", trash.clone(), || {
                                    tracing::info!("Delete")
                                })
                                .show();
                        })
                })
                // Context menu with disabled items
                .child(
                    div()
                        .w(200.0)
                        .h(120.0)
                        .bg(surface)
                        .border(1.0, border)
                        .rounded(r_default())
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .child(
                            text("With Disabled Items")
                                .size(t_sm())
                                .color(text_secondary),
                        )
                        .on_click(move |ctx| {
                            cn::context_menu()
                                .at(ctx.mouse_x, ctx.mouse_y)
                                .item_disabled("Undo (nothing to undo)")
                                .item_disabled("Redo (nothing to redo)")
                                .separator()
                                .item("Cut", || tracing::info!("Cut"))
                                .item("Copy", || tracing::info!("Copy"))
                                .item("Paste", || tracing::info!("Paste"))
                                .show();
                        }),
                ),
        )
}

// ============================================================================
// DROPDOWN MENU SECTION
// ============================================================================

fn dropdown_menu_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Dropdown Menu"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                // Basic dropdown
                .child(
                    cn::dropdown_menu("Options")
                        .item("Edit", || tracing::info!("Edit clicked"))
                        .item("Duplicate", || tracing::info!("Duplicate clicked"))
                        .separator()
                        .item("Archive", || tracing::info!("Archive clicked"))
                        .item("Delete", || tracing::info!("Delete clicked")),
                )
                // Dropdown with shortcuts
                .child(
                    cn::dropdown_menu("File")
                        .item_with_shortcut("New", "Ctrl+N", || tracing::info!("New"))
                        .item_with_shortcut("Open", "Ctrl+O", || tracing::info!("Open"))
                        .item_with_shortcut("Save", "Ctrl+S", || tracing::info!("Save"))
                        .separator()
                        .item_with_shortcut("Export", "Ctrl+E", || tracing::info!("Export")),
                )
                // Dropdown with custom trigger
                .child(
                    cn::dropdown_menu_custom(|is_open| {
                        // No pinned width: the labels measure 140px and
                        // 109px, so a 100px wrapper clipped both.
                        div().child(
                            cn::button(if is_open {
                                "Close Menu"
                            } else {
                                "Custom Trigger"
                            })
                            .variant(ButtonVariant::Secondary),
                        )
                    })
                    .item("Profile", || tracing::info!("Profile"))
                    .item("Settings", || tracing::info!("Settings"))
                    .separator()
                    .item("Logout", || tracing::info!("Logout")),
                )
                // Dropdown with disabled items
                .child(
                    cn::dropdown_menu("Actions")
                        .item("Available Action", || tracing::info!("Action"))
                        .item_disabled("Disabled Action")
                        .separator()
                        .item("Another Action", || tracing::info!("Another")),
                ),
        )
}

// ============================================================================
// DIALOG SECTION
// ============================================================================

fn dialog_section(_ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let _text_secondary = theme.color(ColorToken::TextSecondary);

    section_container()
        .child(section_title("Dialogs"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(16.0)
                // Basic dialog trigger - imperative API like context menu
                .child(
                    cn::button("Open Basic Dialog")
                        .variant(ButtonVariant::Outline)
                        .on_click(move |_| {
                            tracing::info!("Opening basic dialog...");
                            cn::dialog()
                                .title("Edit Profile")
                                .description("Make changes to your profile here. Click save when you're done.")
                                .content(|| {
                                    let theme = ThemeState::get();
                                    div()
                                        .flex_col()
                                        .gap(2.0)
                                        .child(
                                            text("This is a basic dialog with custom content.")
                                                .size(t_sm())
                                                .color(theme.color(ColorToken::TextSecondary)),
                                        )
                                        .child(
                                            text("You can put any content here - forms, lists, images, etc.")
                                                .size(t_sm())
                                                .color(theme.color(ColorToken::TextSecondary)),
                                        )
                                })
                                .on_confirm(|| {
                                    tracing::info!("Saving changes...");
                                })
                                .show();
                        }),
                )
                // Alert dialog trigger
                .child(
                    cn::button("Open Alert")
                        .variant(ButtonVariant::Secondary)
                        .on_click(move |_| {
                            tracing::info!("Opening alert dialog...");
                            cn::alert_dialog()
                                .title("Information")
                                .description("This is an alert dialog. Click OK to dismiss.")
                                .confirm_text("OK")
                                .on_confirm(|| {
                                    tracing::info!("Alert acknowledged");
                                })
                                .show();
                        }),
                )
                // Destructive dialog trigger
                .child(
                    cn::button("Delete Item")
                        .variant(ButtonVariant::Destructive)
                        .on_click(move |_| {
                            tracing::info!("Opening destructive dialog...");
                            cn::dialog()
                                .title("Delete Item")
                                .description("Are you sure you want to delete this item? This action cannot be undone.")
                                .confirm_text("Delete")
                                .confirm_destructive(true)
                                .on_confirm(|| {
                                    tracing::info!("Item deleted!");
                                })
                                .show();
                        }),
                ),
        )
}

// ============================================================================
// SHEET SECTION
// ============================================================================

fn sheet_section(_ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    section_container().child(section_title("Sheets")).child(
        div()
            .flex_row()
            .flex_wrap()
            .gap(16.0)
            // Right sheet (default)
            .child(
                cn::button("Open Right Sheet")
                    .variant(ButtonVariant::Outline)
                    .on_click(move |_| {
                        cn::sheet()
                            .side(SheetSide::Right)
                            .title("Settings")
                            .description("Configure your preferences.")
                            .content(|| {
                                let theme = ThemeState::get();
                                div()
                                    .flex_col()
                                    .gap(16.0)
                                    .child(
                                        div().flex_col().gap(8.0).child(cn::label("Name")).child(
                                            div()
                                                .w_full()
                                                .h(36.0)
                                                .bg(theme.color(ColorToken::SurfaceElevated))
                                                .border(1.0, theme.color(ColorToken::Border))
                                                .rounded(r_default()),
                                        ),
                                    )
                                    .child(
                                        div().flex_col().gap(8.0).child(cn::label("Email")).child(
                                            div()
                                                .w_full()
                                                .h(36.0)
                                                .bg(theme.color(ColorToken::SurfaceElevated))
                                                .border(1.0, theme.color(ColorToken::Border))
                                                .rounded(r_default()),
                                        ),
                                    )
                                    .child(
                                        text("Sheet content can contain any elements.")
                                            .size(theme.typography().text_sm)
                                            .color(theme.color(ColorToken::TextSecondary)),
                                    )
                            })
                            .footer(|| {
                                div()
                                    .flex_row()
                                    .gap(8.0)
                                    .justify_end()
                                    .child(cn::button("Cancel").variant(ButtonVariant::Outline))
                                    .child(cn::button("Save").variant(ButtonVariant::Primary))
                            })
                            .show();
                    }),
            )
            // Left sheet
            .child(
                cn::button("Open Left Sheet")
                    .variant(ButtonVariant::Secondary)
                    .on_click(move |_| {
                        cn::sheet_left()
                            .title("Navigation")
                            .description("Main menu options")
                            .content(|| {
                                div()
                                    .flex_col()
                                    .gap(4.0)
                                    .child(
                                        div().w_full().child(
                                            cn::button("Home").variant(ButtonVariant::Ghost),
                                        ),
                                    )
                                    .child(
                                        div().w_full().child(
                                            cn::button("Profile").variant(ButtonVariant::Ghost),
                                        ),
                                    )
                                    .child(div().w_full().child(
                                        cn::button("Settings").variant(ButtonVariant::Ghost),
                                    ))
                                    .child(
                                        div().w_full().child(
                                            cn::button("Help").variant(ButtonVariant::Ghost),
                                        ),
                                    )
                            })
                            .show();
                    }),
            )
            // Bottom sheet
            .child(
                cn::button("Open Bottom Sheet")
                    .variant(ButtonVariant::Secondary)
                    .on_click(move |_| {
                        cn::sheet_bottom()
                            .size(SheetSize::Medium)
                            .title("Share")
                            .description("Choose how to share this item")
                            .content(|| {
                                div()
                                    .flex_row()
                                    .gap(16.0)
                                    .justify_center()
                                    .child(
                                        div()
                                            .flex_col()
                                            .items_center()
                                            .gap(4.0)
                                            .child(
                                                div()
                                                    .w(48.0)
                                                    .h(48.0)
                                                    .rounded_full()
                                                    .bg(Color::rgb(0.2, 0.6, 1.0)),
                                            )
                                            .child(text("Twitter").size(t_xs())),
                                    )
                                    .child(
                                        div()
                                            .flex_col()
                                            .items_center()
                                            .gap(4.0)
                                            .child(
                                                div()
                                                    .w(48.0)
                                                    .h(48.0)
                                                    .rounded_full()
                                                    .bg(Color::rgb(0.0, 0.5, 0.0)),
                                            )
                                            .child(text("WhatsApp").size(t_xs())),
                                    )
                                    .child(
                                        div()
                                            .flex_col()
                                            .items_center()
                                            .gap(4.0)
                                            .child(
                                                div()
                                                    .w(48.0)
                                                    .h(48.0)
                                                    .rounded_full()
                                                    .bg(Color::rgb(0.9, 0.3, 0.3)),
                                            )
                                            .child(text("Email").size(t_xs())),
                                    )
                            })
                            .show();
                    }),
            )
            // Large sheet
            .child(
                cn::button("Open Large Sheet")
                    .variant(ButtonVariant::Outline)
                    .on_click(move |_| {
                        cn::sheet()
                            .size(SheetSize::Large)
                            .title("Large Panel")
                            .description("A wider sheet for more content")
                            .content(|| {
                                let theme = ThemeState::get();
                                div()
                                    .flex_col()
                                    .gap(16.0)
                                    .child(
                                        text("This is a larger sheet that can hold more content.")
                                            .size(theme.typography().text_base)
                                            .color(theme.color(ColorToken::TextPrimary)),
                                    )
                                    .child(
                                        div()
                                            .w_full()
                                            .h(200.0)
                                            .bg(theme.color(ColorToken::SurfaceElevated))
                                            .rounded(r_default())
                                            .items_center()
                                            .child(
                                                text("Content Area")
                                                    .color(theme.color(ColorToken::TextSecondary)),
                                            ),
                                    )
                            })
                            .show();
                    }),
            ),
    )
}

// ============================================================================
// DRAWER SECTION
// ============================================================================

fn drawer_section(_ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    section_container().child(section_title("Drawers")).child(
        div()
            .flex_row()
            .flex_wrap()
            .gap(16.0)
            // Left drawer (navigation)
            .child(
                cn::button("Open Nav Drawer")
                    .variant(ButtonVariant::Outline)
                    .on_click(move |_| {
                        cn::drawer()
                            .side(DrawerSide::Left)
                            .title("Menu")
                            .child(|| {
                                div()
                                    .w_full()
                                    .child(cn::button("Dashboard").variant(ButtonVariant::Ghost))
                            })
                            .child(|| {
                                div()
                                    .w_full()
                                    .child(cn::button("Projects").variant(ButtonVariant::Ghost))
                            })
                            .child(|| {
                                div()
                                    .w_full()
                                    .child(cn::button("Team").variant(ButtonVariant::Ghost))
                            })
                            .child(|| {
                                div()
                                    .w_full()
                                    .child(cn::button("Reports").variant(ButtonVariant::Ghost))
                            })
                            .child(|| div().w_full().child(cn::separator()))
                            .child(|| {
                                div()
                                    .w_full()
                                    .child(cn::button("Settings").variant(ButtonVariant::Ghost))
                            })
                            .footer(|| {
                                div()
                                    .w_full()
                                    .child(cn::button("Logout").variant(ButtonVariant::Destructive))
                            })
                            .show();
                    }),
            )
            // Right drawer
            .child(
                cn::button("Open Right Drawer")
                    .variant(ButtonVariant::Secondary)
                    .on_click(move |_| {
                        cn::drawer_right()
                            .title("Notifications")
                            .size(DrawerSize::Wide)
                            .child(|| {
                                let theme = ThemeState::get();
                                div()
                                    .flex_row()
                                    .items_center()
                                    .gap(12.0)
                                    .p(8.0)
                                    .rounded(r_default())
                                    .bg(theme.color(ColorToken::SurfaceElevated))
                                    .child(
                                        div()
                                            .w(32.0)
                                            .h(32.0)
                                            .rounded_full()
                                            .bg(theme.color(ColorToken::Primary)),
                                    )
                                    .child(
                                        div()
                                            .flex_col()
                                            .child(text("New message").size(t_sm()).medium())
                                            .child(
                                                text("John sent you a message")
                                                    .size(t_xs())
                                                    .color(theme.color(ColorToken::TextSecondary)),
                                            ),
                                    )
                            })
                            .child(|| {
                                let theme = ThemeState::get();
                                div()
                                    .flex_row()
                                    .items_center()
                                    .gap(12.0)
                                    .p(8.0)
                                    .rounded(r_default())
                                    .child(
                                        div()
                                            .w(32.0)
                                            .h(32.0)
                                            .rounded_full()
                                            .bg(theme.color(ColorToken::SuccessBg)),
                                    )
                                    .child(
                                        div()
                                            .flex_col()
                                            .child(text("Task completed").size(t_sm()).medium())
                                            .child(
                                                text("Project X was finished")
                                                    .size(t_xs())
                                                    .color(theme.color(ColorToken::TextSecondary)),
                                            ),
                                    )
                            })
                            .show();
                    }),
            )
            // Narrow drawer
            .child(
                cn::button("Open Narrow Drawer")
                    .variant(ButtonVariant::Outline)
                    .on_click(move |_| {
                        cn::drawer()
                            .size(DrawerSize::Narrow)
                            .title("Quick Actions")
                            .child(|| {
                                div()
                                    .w_full()
                                    .child(cn::button("New").variant(ButtonVariant::Ghost))
                            })
                            .child(|| {
                                div()
                                    .w_full()
                                    .child(cn::button("Open").variant(ButtonVariant::Ghost))
                            })
                            .child(|| {
                                div()
                                    .w_full()
                                    .child(cn::button("Save").variant(ButtonVariant::Ghost))
                            })
                            .child(|| {
                                div()
                                    .w_full()
                                    .child(cn::button("Export").variant(ButtonVariant::Ghost))
                            })
                            .show();
                    }),
            ),
    )
}

// ============================================================================
// LOADING SECTION (Skeleton, Spinner)
// ============================================================================

fn loading_section(_ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Loading States"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(32.0)
                .items_center()
                // Skeletons
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(cn::skeleton().h(20.0).w(200.0))
                        .child(cn::skeleton().h(16.0).w(150.0))
                        .child(cn::skeleton().h(16.0).w(180.0)),
                )
                // Avatar skeleton
                .child(cn::skeleton_circle(48.0))
                // Spinners — timeline is constructed internally now.
                .child(
                    div()
                        .flex_row()
                        .gap(16.0)
                        .items_center()
                        .child(cn::spinner().size(SpinnerSize::Small))
                        .child(cn::spinner().size(SpinnerSize::Medium))
                        .child(cn::spinner().size(SpinnerSize::Large)),
                ),
        )
}

// ============================================================================
// PROGRESS SECTION
// ============================================================================

fn progress_section(ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    const PROGRESS_WIDTH: f32 = 300.0;

    // Create animated progress - start at 0. Critically damped (damping ==
    // 2*sqrt(stiffness*mass) for stiffness=400, mass=1) so the bar settles
    // on target without bouncing back over the rest position.
    let animated_progress = ctx.use_animated_value_for(
        "animated_progress_v10",
        0.0,
        SpringConfig::new(400.0, 40.0, 1.0),
    );

    let progress_for_ready = animated_progress.clone();

    // Clone for replay button
    let progress_for_replay = animated_progress.clone();

    let section = section_container().child(section_title("Progress")).child(
        div()
            .flex_col()
            .gap(20.0)
            // Static progress bars
            .child(
                div()
                    .flex_col()
                    .gap(12.0)
                    .child(cn::label("Static Progress"))
                    .child(
                        div()
                            .flex_row()
                            .gap(16.0)
                            .items_center()
                            .child(cn::progress(25.0).w(200.0))
                            .child(cn::label("25%").size(LabelSize::Small)),
                    )
                    .child(
                        div()
                            .flex_row()
                            .gap(16.0)
                            .items_center()
                            .child(cn::progress(50.0).w(200.0).size(ProgressSize::Small))
                            .child(cn::label("50% (small)").size(LabelSize::Small)),
                    )
                    .child(
                        div()
                            .flex_row()
                            .gap(16.0)
                            .items_center()
                            .child(cn::progress(75.0).w(200.0).size(ProgressSize::Large))
                            .child(cn::label("75% (large)").size(LabelSize::Small)),
                    ),
            )
            // Animated progress bar - auto-triggers on load
            .child(
                div()
                    .w_fit()
                    .flex_col()
                    .gap(12.0)
                    .child(cn::label("Animated Progress (auto-animates to 75%)"))
                    .child(cn::progress_animated(animated_progress).w(PROGRESS_WIDTH))
                    .child({
                        cn::button("Replay Animation")
                            .size(ButtonSize::Large)
                            .variant(ButtonVariant::Primary)
                            // Click → snap to 0 (no animation) then re-target
                            // 75%, so the bar visibly plays the 0→75% animation
                            // again. set_immediate clears any active spring so
                            // the snap is truly instant; the subsequent
                            // set_target builds a fresh spring from 0 → 75%.
                            .on_click(move |_| {
                                if let Ok(mut value) = progress_for_replay.lock() {
                                    value.set_immediate(0.0);
                                    value.set_target(PROGRESS_WIDTH * 0.75);
                                }
                            })
                    }),
            )
            .id("progress-section"),
    );

    // Register on_ready callback (fires once with stable ID tracking)
    ctx.query("progress-section").on_ready(move |_| {
        if let Ok(mut value) = progress_for_ready.lock() {
            value.set_target(PROGRESS_WIDTH * 0.75);
            tracing::info!("on_ready: animation triggered to 75%");
        }
    });

    section
}

// ============================================================================
// MISC SECTION (Separator, Label)
// ============================================================================

// ============================================================================
// TABS SECTION
// ============================================================================

fn tabs_section(ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    // Simple tabs state
    let simple_tab = ctx.use_state_keyed("simple_tab", || "tab1".to_string());

    section_container().child(section_title("Tabs")).child(
        div()
            .flex_col()
            .gap(24.0)
            // Simple tabs
            .child(
                div()
                    .w(500.0)
                    .h(300.0)
                    .flex_col()
                    .gap(8.0)
                    .child(cn::label("Simple Tabs"))
                    .child(
                        cn::tabs(&simple_tab)
                            .transition(TabsTransition::SlideRight)
                            .tab("tab1", "Account", || {
                                div()
                                    .px(10.0)
                                    .bg_surface_elevated()
                                    .w_full()
                                    .h_full()
                                    .items_center()
                                    .child(
                                        text("Manage your account settings and preferences.")
                                            .size(t_sm())
                                            .color(
                                                ThemeState::get().color(ColorToken::TextSecondary),
                                            ),
                                    )
                            })
                            .tab("tab2", "Password", || {
                                div()
                                    .px(10.0)
                                    .bg_surface_elevated()
                                    .w_full()
                                    .h_full()
                                    .items_center()
                                    .child(
                                        text("Change your password and security settings.")
                                            .size(t_sm())
                                            .color(
                                                ThemeState::get().color(ColorToken::TextSecondary),
                                            ),
                                    )
                            })
                            .tab("tab3", "Notifications", || {
                                div()
                                    .px(10.0)
                                    .bg_surface_elevated()
                                    .w_full()
                                    .h_full()
                                    .items_center()
                                    .child(
                                        text("Configure your notification preferences.")
                                            .size(t_sm())
                                            .color(
                                                ThemeState::get().color(ColorToken::TextSecondary),
                                            ),
                                    )
                            }),
                    ),
            )
            // Tabs with different sizes
            .child(
                div()
                    .flex_row()
                    .gap(24.0)
                    .child(
                        div()
                            .flex_col()
                            .gap(8.0)
                            .child(cn::label("Small Tabs"))
                            .child({
                                let small_tab =
                                    ctx.use_state_keyed("small_tab", || "a".to_string());
                                cn::tabs(&small_tab)
                                    .size(cn::TabsSize::Small)
                                    .tab("a", "First", div)
                                    .tab("b", "Second", div)
                            }),
                    )
                    .child(
                        div()
                            .flex_col()
                            .gap(8.0)
                            .child(cn::label("Large Tabs"))
                            .child({
                                let large_tab =
                                    ctx.use_state_keyed("large_tab", || "x".to_string());
                                cn::tabs(&large_tab)
                                    .size(cn::TabsSize::Large)
                                    .tab("x", "Overview", div)
                                    .tab("y", "Details", div)
                            }),
                    ),
            ),
    )
}

// ============================================================================
// BREADCRUMB SECTION
// ============================================================================

fn breadcrumb_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    // Home icon for breadcrumb
    let home_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>"#;

    section_container()
        .child(section_title("Breadcrumb"))
        .child(
            div()
                .flex_col()
                .gap(20.0)
                // Basic breadcrumb
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(text("Basic Breadcrumb").size(t_sm()).color(text_secondary))
                        .child(
                            cn::breadcrumb()
                                .item("Home", || tracing::info!("Home clicked"))
                                .item("Products", || tracing::info!("Products clicked"))
                                .item("Electronics", || tracing::info!("Electronics clicked"))
                                .current("Laptop"),
                        ),
                )
                // With icon
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(text("With Home Icon").size(t_sm()).color(text_secondary))
                        .child(
                            cn::breadcrumb()
                                .item_with_icon("Home", home_icon, || {
                                    tracing::info!("Home clicked")
                                })
                                .item("Settings", || tracing::info!("Settings clicked"))
                                .current("Profile"),
                        ),
                )
                // Slash separator
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(text("Slash Separator").size(t_sm()).color(text_secondary))
                        .child(
                            cn::breadcrumb()
                                .slash_separator()
                                .item("Home", || {})
                                .item("Documents", || {})
                                .item("Projects", || {})
                                .current("Current Project"),
                        ),
                )
                // Custom text separator
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(text("Custom Separator").size(t_sm()).color(text_secondary))
                        .child(
                            cn::breadcrumb()
                                .text_separator("→")
                                .item("Start", || {})
                                .item("Middle", || {})
                                .current("End"),
                        ),
                )
                // Different sizes
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(text("Sizes").size(t_sm()).color(text_secondary))
                        .child(
                            div()
                                .flex_col()
                                .gap(12.0)
                                .child(
                                    cn::breadcrumb()
                                        .small()
                                        .item("Home", || {})
                                        .current("Small"),
                                )
                                .child(
                                    cn::breadcrumb()
                                        .item("Home", || {})
                                        .current("Medium (default)"),
                                )
                                .child(
                                    cn::breadcrumb()
                                        .large()
                                        .item("Home", || {})
                                        .current("Large"),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// PAGINATION SECTION
// ============================================================================

fn pagination_section(ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    // State for each pagination demo
    let page1 = ctx.use_state_keyed("pagination_page1", || 1usize);
    let page2 = ctx.use_state_keyed("pagination_page2", || 5usize);
    let page3 = ctx.use_state_keyed("pagination_page3", || 1usize);

    section_container()
        .child(section_title("Pagination"))
        .child(
            div()
                .flex_col()
                .gap(24.0)
                // Basic pagination
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(
                            text("Basic Pagination (10 pages)")
                                .size(t_sm())
                                .color(text_secondary),
                        )
                        .child(
                            cn::pagination(10, page1.clone())
                                .on_page_change(|page| tracing::info!("Page changed to: {}", page)),
                        ),
                )
                // With first/last buttons
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(
                            text("With First/Last Buttons (50 pages)")
                                .size(t_sm())
                                .color(text_secondary),
                        )
                        .child(
                            cn::pagination(50, page2.clone())
                                .visible_pages(7)
                                .show_first_last(true)
                                .on_page_change(|page| tracing::info!("Page changed to: {}", page)),
                        ),
                )
                // Size variants
                .child(
                    div()
                        .flex_col()
                        .gap(12.0)
                        .child(text("Size Variants").size(t_sm()).color(text_secondary))
                        .child(
                            div()
                                .flex_row()
                                .flex_wrap()
                                .gap(24.0)
                                .items_center()
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .child(text("Small").size(t_xs()).color(text_secondary))
                                        .child(cn::pagination(5, page3.clone()).small()),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .gap(4.0)
                                        .child(text("Large").size(t_xs()).color(text_secondary))
                                        .child(cn::pagination(5, page3.clone()).large()),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// NAVIGATION MENU SECTION
// ============================================================================

fn navigation_menu_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    section_container()
        .child(section_title("Navigation Menu"))
        .child(
            div()
                .flex_col()
                .gap(20.0)
                .child(
                    text("Hover over triggers to see dropdown menus")
                        .size(t_sm())
                        .color(text_secondary),
                )
                .child(
                    cn::navigation_menu()
                        .item("Home", || tracing::info!("Home clicked"))
                        .trigger("Products", || {
                            div()
                                .flex_col()
                                .gap(4.0)
                                .child(
                                    cn::navigation_link("Electronics")
                                        .description("Browse our electronic devices")
                                        .on_click(|| tracing::info!("Electronics clicked")),
                                )
                                .child(
                                    cn::navigation_link("Clothing")
                                        .description("Fashion and apparel")
                                        .on_click(|| tracing::info!("Clothing clicked")),
                                )
                                .child(
                                    cn::navigation_link("Home & Garden")
                                        .description("Everything for your home")
                                        .on_click(|| tracing::info!("Home & Garden clicked")),
                                )
                        })
                        .trigger("Services", || {
                            div()
                                .flex_col()
                                .gap(4.0)
                                .child(
                                    cn::navigation_link("Consulting")
                                        .description("Expert advice for your business")
                                        .on_click(|| tracing::info!("Consulting clicked")),
                                )
                                .child(
                                    cn::navigation_link("Development")
                                        .description("Custom software solutions")
                                        .on_click(|| tracing::info!("Development clicked")),
                                )
                                .child(
                                    cn::navigation_link("Support")
                                        .description("24/7 customer support")
                                        .on_click(|| tracing::info!("Support clicked")),
                                )
                        })
                        .item("About", || tracing::info!("About clicked"))
                        .item("Contact", || tracing::info!("Contact clicked")),
                ),
        )
}

// ============================================================================
// SIDEBAR SECTION
// ============================================================================

fn sidebar_section(ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let border = theme.color(ColorToken::Border);

    // State for sidebar collapse
    let sidebar_collapsed = ctx.use_state_keyed("sidebar_collapsed", || false);

    // Icon SVGs
    let home_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m3 9 9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><polyline points="9 22 9 12 15 12 15 22"/></svg>"#;
    let search_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/></svg>"#;
    let inbox_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="22 12 16 12 14 15 10 15 8 12 2 12"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/></svg>"#;
    let settings_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/></svg>"#;
    let user_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/></svg>"#;
    let help_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3"/><path d="M12 17h.01"/></svg>"#;

    section_container().child(section_title("Sidebar")).child(
        div()
            .flex_col()
            .gap(12.0)
            .child(
                text("Click the toggle button to collapse/expand the sidebar")
                    .size(t_sm())
                    .color(text_secondary),
            )
            .child(
                div()
                    .h(400.0)
                    .border(1.0, border)
                    .rounded(r_default())
                    .overflow_clip()
                    .child(
                        cn::sidebar(&sidebar_collapsed)
                            .item_active("Dashboard", home_icon, || {
                                tracing::info!("Dashboard clicked")
                            })
                            .item("Search", search_icon, || tracing::info!("Search clicked"))
                            .item("Inbox", inbox_icon, || tracing::info!("Inbox clicked"))
                            .section("Account")
                            .item("Profile", user_icon, || tracing::info!("Profile clicked"))
                            .item("Settings", settings_icon, || {
                                tracing::info!("Settings clicked")
                            })
                            .section("Help")
                            .item("Support", help_icon, || tracing::info!("Support clicked"))
                            .content(|_active_item| {
                                let theme = ThemeState::get();

                                // Large icons for anti-aliasing comparison
                                let large_search = r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/></svg>"#;
                                let large_settings = r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/></svg>"#;
                                let small_search = r#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/></svg>"#;

                                div()
                                    .bg(theme.color(ColorToken::Background))
                                    .p(24.0)
                                    .flex_col()
                                    .gap(24.0)
                                    .child(
                                        text("Icon Size Comparison")
                                            .size(t_lg())
                                            .weight(FontWeight::SemiBold)
                                            .color(theme.color(ColorToken::TextPrimary)),
                                    )
                                    .child(
                                        div()
                                            .flex_row()
                                            .justify_between()
                                            .gap(2.0)
                                            .items_end()
                                            // Large 64x64 icons
                                            .child(
                                                div()
                                                    .flex_col()
                                                    .items_center()
                                                    .gap(8.0)
                                                    .child(svg(large_search).tint(theme.color(ColorToken::TextPrimary)))
                                                    .child(text("64×64 Search").size(t_xs()).color(theme.color(ColorToken::TextSecondary)))
                                            )
                                            .child(
                                                div()
                                                    .flex_col()
                                                    .items_center()
                                                    .gap(8.0)
                                                    .child(svg(large_settings).tint(theme.color(ColorToken::TextPrimary)))
                                                    .child(text("64×64 Settings").size(t_xs()).color(theme.color(ColorToken::TextSecondary)))
                                            )
                                            // Small 20x20 icons for comparison
                                            .child(
                                                div()
                                                    .flex_col()
                                                    .items_center()
                                                    .gap(8.0)
                                                    .child(svg(small_search).tint(theme.color(ColorToken::TextPrimary)))
                                                    .child(text("20×20 Search").size(t_xs()).color(theme.color(ColorToken::TextSecondary)))
                                            ),
                                    )
                            }),
                    ),
            ),
    )
}

// ============================================================================
// RESIZABLE SECTION
// ============================================================================

fn resizable_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let border = theme.color(ColorToken::Border);
    let surface = theme.color(ColorToken::Surface);
    let surface_elevated = theme.color(ColorToken::SurfaceElevated);

    section_container()
        .child(section_title("Resizable Panels"))
        .child(
            div()
                .flex_col()
                .gap(24.0)
                // Horizontal resizable example
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(cn::label("Horizontal Resizable"))
                        .child(
                            text("Drag the handles between panels to resize them")
                                .size(t_sm())
                                .color(text_secondary),
                        )
                        .child(
                            div()
                                .h(300.0)
                                .border(1.0, border)
                                .rounded(r_default())
                                .overflow_clip()
                                .child(
                                    cn::resizable_group()
                                        .horizontal()
                                        .key("demo_horizontal")
                                        .panel(
                                            cn::resizable_panel()
                                                .id("left")
                                                .default_size(200.0)
                                                .min_size(100.0)
                                                .max_size(400.0)
                                                .child(
                                                    div()
                                                        .w_full()
                                                        .h_full()
                                                        .bg(surface)
                                                        .p(16.0)
                                                        .flex_col()
                                                        .gap(8.0)
                                                        .child(
                                                            text("Left Panel")
                                                                .size(t_sm())
                                                                .weight(FontWeight::SemiBold)
                                                                .color(theme.color(
                                                                    ColorToken::TextPrimary,
                                                                )),
                                                        )
                                                        .child(
                                                            text("Min: 100px, Max: 400px")
                                                                .size(t_xs())
                                                                .color(text_secondary),
                                                        ),
                                                ),
                                        )
                                        .panel(
                                            cn::resizable_panel()
                                                .id("center")
                                                .flex_grow()
                                                .min_size(150.0)
                                                .child(
                                                    div()
                                                        .w_full()
                                                        .h_full()
                                                        .bg(surface_elevated)
                                                        .p(16.0)
                                                        .flex_col()
                                                        .items_center()
                                                        .justify_center()
                                                        .gap(8.0)
                                                        .child(
                                                            text("Center Panel (Flex)")
                                                                .size(t_sm())
                                                                .weight(FontWeight::SemiBold)
                                                                .color(theme.color(
                                                                    ColorToken::TextPrimary,
                                                                )),
                                                        )
                                                        .child(
                                                            text("Grows to fill available space")
                                                                .size(t_xs())
                                                                .color(text_secondary),
                                                        ),
                                                ),
                                        )
                                        .panel(
                                            cn::resizable_panel()
                                                .id("right")
                                                .default_size(180.0)
                                                .min_size(100.0)
                                                .child(
                                                    div()
                                                        .w_full()
                                                        .h_full()
                                                        .bg(surface)
                                                        .p(16.0)
                                                        .flex_col()
                                                        .gap(8.0)
                                                        .child(
                                                            text("Right Panel")
                                                                .size(t_sm())
                                                                .weight(FontWeight::SemiBold)
                                                                .color(theme.color(
                                                                    ColorToken::TextPrimary,
                                                                )),
                                                        )
                                                        .child(
                                                            text("Min: 100px")
                                                                .size(t_xs())
                                                                .color(text_secondary),
                                                        ),
                                                ),
                                        )
                                        .build(),
                                ),
                        ),
                )
                // Vertical resizable example
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(cn::label("Vertical Resizable"))
                        .child(
                            text("Panels can also resize vertically")
                                .size(t_sm())
                                .color(text_secondary),
                        )
                        .child(
                            div()
                                .h(350.0)
                                .border(1.0, border)
                                .rounded(r_default())
                                .overflow_clip()
                                .child(
                                    cn::resizable_group()
                                        .vertical()
                                        .key("demo_vertical")
                                        .panel(
                                            cn::resizable_panel()
                                                .id("top")
                                                .flex_grow()
                                                .min_size(80.0)
                                                .child(
                                                    div()
                                                        .w_full()
                                                        .h_full()
                                                        .bg(surface_elevated)
                                                        .p(16.0)
                                                        .flex_col()
                                                        .items_center()
                                                        .justify_center()
                                                        .child(
                                                            text("Main Content Area")
                                                                .size(t_sm())
                                                                .weight(FontWeight::SemiBold)
                                                                .color(theme.color(
                                                                    ColorToken::TextPrimary,
                                                                )),
                                                        ),
                                                ),
                                        )
                                        .panel(
                                            cn::resizable_panel()
                                                .id("bottom")
                                                .default_size(120.0)
                                                .min_size(60.0)
                                                .max_size(200.0)
                                                .child(
                                                    div()
                                                        .w_full()
                                                        .h_full()
                                                        .bg(surface)
                                                        .p(16.0)
                                                        .flex_col()
                                                        .gap(4.0)
                                                        .child(
                                                            text("Bottom Panel")
                                                                .size(t_sm())
                                                                .weight(FontWeight::SemiBold)
                                                                .color(theme.color(
                                                                    ColorToken::TextPrimary,
                                                                )),
                                                        )
                                                        .child(
                                                            text("Min: 60px, Max: 200px")
                                                                .size(t_xs())
                                                                .color(text_secondary),
                                                        ),
                                                ),
                                        )
                                        .build(),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// ACCORDION SECTION
// ============================================================================

fn accordion_section() -> impl ElementBuilder + use<> {
    section_container()
        .child(section_title("Accordion"))
        .child(
            div()
                .flex_col()
                .flex_wrap()
                .w_full()
                .h_fit()
                .gap(24.0)
                // Single-open accordion (default)
                .child(
                    div()
                        .w_full()
                        .h_fit()
                        .flex_col()
                        .gap(8.0)
                        .child(cn::label("Single Open (default)"))
                        .child(
                            cn::accordion()
                                .default_open("faq-1")
                                .item("faq-1", "What is Blinc?", || {
                                    div().w_full().p(4.0).items_center().child(
                                        text("Blinc is a Rust UI framework for building beautiful, performant user interfaces with a declarative, GPUI-inspired API.")
                                            .size(t_sm())
                                            .color(ThemeState::get().color(ColorToken::TextSecondary)),
                                    )
                                })
                                .item("faq-2", "How do animations work?", || {
                                    div().w_full().p(4.0).items_center().child(
                                        text("Blinc uses spring physics animations via the blinc_animation crate. Animations are scheduled through a global scheduler for smooth performance.")
                                            .size(t_sm())
                                            .color(ThemeState::get().color(ColorToken::TextSecondary)),
                                    )
                                })
                                .item("faq-3", "Is it production ready?", || {
                                    div().w_full().p(4.0).items_center().child(
                                        text("Blinc is under active development. It's suitable for experimentation and side projects, with a growing component library.")
                                            .size(t_sm())
                                            .color(ThemeState::get().color(ColorToken::TextSecondary)),
                                    )
                                })
                                ,
                        ),
                )
                // Multi-open accordion
                .child(
                    div()
                        .w_full()
                         .h_fit()
                        .flex_col()
                        .gap(8.0)
                        .child(cn::label("Multi Open"))
                        .child(
                            cn::accordion()
                                .multi_open()
                                .item("settings-1", "Appearance", || {
                                    div().w_full().h(60.0).p(4.0).items_center().child(
                                        text("Customize the look and feel of your application including themes, colors, and fonts.")
                                            .size(t_sm())
                                            .color(ThemeState::get().color(ColorToken::TextSecondary)),
                                    )
                                })
                                .item("settings-2", "Notifications", || {
                                    div().w_full().h(60.0).p(4.0).items_center().child(
                                        text("Configure how and when you receive notifications, including email and push notifications.")
                                            .size(t_sm())
                                            .color(ThemeState::get().color(ColorToken::TextSecondary)),
                                    )
                                })
                                .item("settings-3", "Privacy", || {
                                    div().w_full().h(60.0).p(4.0).items_center().child(
                                        text("Control your privacy settings, data sharing preferences, and account visibility.")
                                            .size(t_sm())
                                            .color(ThemeState::get().color(ColorToken::TextSecondary)),
                                    )
                                })
                                .build_component(),
                        ),
                ),
        )
}

// ============================================================================
// TOAST SECTION
// ============================================================================

fn toast_section(_ctx: &WindowedContext) -> impl ElementBuilder + use<> {
    section_container().child(section_title("Toasts")).child(
        div()
            .flex_row()
            .flex_wrap()
            .gap(16.0)
            // Default toast
            .child(
                cn::button("Show Toast")
                    .variant(ButtonVariant::Outline)
                    .on_click(|_| {
                        cn::toast("Event Created")
                            .description("Your event has been scheduled.")
                            .show();
                    }),
            )
            // Success toast
            .child(
                cn::button("Success Toast")
                    .variant(ButtonVariant::Secondary)
                    .on_click(|_| {
                        cn::toast_success("Success!")
                            .description("Your changes have been saved.")
                            .show();
                    }),
            )
            // Warning toast
            .child(
                cn::button("Warning Toast")
                    .variant(ButtonVariant::Secondary)
                    .on_click(|_| {
                        cn::toast_warning("Warning")
                            .description("Your session is about to expire.")
                            .show();
                    }),
            )
            // Error toast
            .child(
                cn::button("Error Toast")
                    .variant(ButtonVariant::Destructive)
                    .on_click(|_| {
                        cn::toast_error("Error")
                            .description("Something went wrong. Please try again.")
                            .show();
                    }),
            )
            // Toast with action
            .child(
                cn::button("Toast with Action")
                    .variant(ButtonVariant::Outline)
                    .on_click(|_| {
                        cn::toast("File Deleted")
                            .description("The file has been moved to trash.")
                            .action("Undo", || {
                                tracing::info!("Undo clicked!");
                            })
                            .show();
                    }),
            )
            // Multiple toasts at once (for stacking test)
            .child(
                cn::button("Show 3 Toasts")
                    .variant(ButtonVariant::Primary)
                    .on_click(|_| {
                        cn::toast("First Toast")
                            .description("This is the first toast.")
                            .show();
                        cn::toast_success("Second Toast")
                            .description("This is the second toast.")
                            .show();
                        cn::toast_warning("Third Toast")
                            .description("This is the third toast.")
                            .show();
                    }),
            ),
    )
}

// ============================================================================
// Hover Card Section
// ============================================================================

fn hover_card_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_primary = theme.color(ColorToken::TextPrimary);
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let text_tertiary = theme.color(ColorToken::TextTertiary);
    let accent = theme.color(ColorToken::Primary);

    section_container()
        .child(section_title("Hover Card"))
        .child(
            div()
                .flex_col()
                .gap(24.0)
                // Basic hover card
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(
                            text("Basic Hover Card")
                                .size(t_sm())
                                .medium()
                                .color(text_primary),
                        )
                        .child(
                            cn::hover_card(move || {
                                div().w_fit()
                                    .cursor_pointer()
                                    .child(text("@johndoe").size(t_sm()).color(accent).no_wrap())
                            })
                            .content(move || {
                                div()
                                    .flex_col()
                                    .gap(6.0)
                                    .child(
                                        div()
                                            .flex_row()
                                            .gap(6.0)
                                            .items_center()
                                            .child(
                                                div()
                                                    .w(48.0)
                                                    .h(48.0)
                                                    .rounded_full()
                                                    .bg(accent.with_alpha(0.2)),
                                            )
                                            .child(
                                                div()
                                                    .flex_col()
                                                    .gap(1.0)
                                                    .child(
                                                        text("John Doe")
                                                            .size(t_base())
                                                            .medium()
                                                            .color(text_primary),
                                                    )
                                                    .child(
                                                        text("@johndoe")
                                                            .size(t_sm())
                                                            .color(text_secondary),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        text("Software Engineer at Acme Corp. Building great things with Rust and TypeScript.")
                                            .size(t_sm())
                                            .color(text_secondary),
                                    )
                                    .child(
                                        div()
                                            .flex_row()
                                            .gap(16.0)
                                            .child(
                                                div()
                                                    .flex_row()
                                                    .gap(4.0)
                                                    .child(text("128").size(t_sm()).medium().color(text_primary))
                                                    .child(text("Following").size(t_sm()).color(text_tertiary)),
                                            )
                                            .child(
                                                div()
                                                    .flex_row()
                                                    .gap(4.0)
                                                    .child(text("2.4k").size(t_sm()).medium().color(text_primary))
                                                    .child(text("Followers").size(t_sm()).color(text_tertiary)),
                                            ),
                                    )
                            }),
                        ),
                )
                // Hover card with side positioning
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(
                            text("Side Positions")
                                .size(t_sm())
                                .medium()
                                .color(text_primary),
                        )
                        .child(
                            div()
                                .flex_row()
                                .gap(24.0)
                                .child(
                                    cn::hover_card(move || {
                                        div().child(cn::button("Bottom (Default)").variant(ButtonVariant::Outline))
                                    })
                                    .side(HoverCardSide::Bottom)
                                    .content(move || {
                                        div().child(
                                            text("This card appears below the trigger.")
                                                .size(t_sm())
                                                .color(text_secondary),
                                        )
                                    }),
                                )
                                .child(
                                    cn::hover_card(move || {
                                        div().child(cn::button("Right").variant(ButtonVariant::Outline))
                                    })
                                    .side(HoverCardSide::Right)
                                    .content(move || {
                                        div().child(
                                            text("This card appears to the right.")
                                                .size(t_sm())
                                                .color(text_secondary),
                                        )
                                    }),
                                )
                                .child(
                                    cn::hover_card(move || {
                                        div().child(cn::button("Top").variant(ButtonVariant::Outline))
                                    })
                                    .side(HoverCardSide::Top)
                                    .content(move || {
                                        div().child(
                                            text("This card appears above the trigger.")
                                                .size(t_sm())
                                                .color(text_secondary),
                                        )
                                    }),
                                ),
                        ),
                ),
        )
}

// ============================================================================
// Popover Section
// ============================================================================

fn popover_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_primary = theme.color(ColorToken::TextPrimary);
    let _text_secondary = theme.color(ColorToken::TextSecondary);

    section_container()
        .child(section_title("Popover"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                // Basic popover
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(
                            text("Basic Popover")
                                .size(t_sm())
                                .medium()
                                .color(text_primary),
                        )
                        .child(
                            cn::popover(|is_open| {
                                div().w_fit().child(
                                    cn::button(if is_open { "Close" } else { "Open Popover" })
                                        .variant(ButtonVariant::Outline),
                                )
                            })
                            .content(move || {
                                let theme = ThemeState::get();
                                div()
                                    .flex_col()
                                    .gap(8.0)
                                    .child(
                                        text("Popover Content")
                                            .size(t_sm())
                                            .medium()
                                            .color(theme.color(ColorToken::TextPrimary)),
                                    )
                                    .child(
                                        text("This is some content inside the popover. Click outside or press Escape to close.")
                                            .size(t_sm())
                                            .color(theme.color(ColorToken::TextSecondary)),
                                    )
                            }),
                        ),
                )
                // Popover with form content
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(
                            text("With Form Content")
                                .size(t_sm())
                                .medium()
                                .color(text_primary),
                        )
                        .child(
                            cn::popover(|_is_open| {
                                div().w_fit().child(
                                    cn::button("Edit Settings")
                                        .variant(ButtonVariant::Secondary),
                                )
                            })
                            .content(move || {
                                let theme = ThemeState::get();
                                div()
                                    .flex_col()
                                    .gap(12.0)
                                    .w(240.0)
                                    .child(
                                        text("Settings")
                                            .size(t_sm())
                                            .medium()
                                            .color(theme.color(ColorToken::TextPrimary)),
                                    )
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(4.0)
                                            .child(cn::label("Width"))
                                            .child(
                                                div()
                                                    .w_full()
                                                    .h(32.0)
                                                    .bg(theme.color(ColorToken::SurfaceElevated))
                                                    .border(1.0, theme.color(ColorToken::Border))
                                                    .rounded(r_default()),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex_col()
                                            .gap(4.0)
                                            .child(cn::label("Height"))
                                            .child(
                                                div()
                                                    .w_full()
                                                    .h(32.0)
                                                    .bg(theme.color(ColorToken::SurfaceElevated))
                                                    .border(1.0, theme.color(ColorToken::Border))
                                                    .rounded(r_default()),
                                            ),
                                    )
                            }),
                        ),
                )
                // Positioned to the right
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(
                            text("Positioned Right")
                                .size(t_sm())
                                .medium()
                                .color(text_primary),
                        )
                        .child(
                            cn::popover(|_is_open| {
                                div().w_fit().child(
                                    cn::button("Open Right")
                                        .variant(ButtonVariant::Ghost),
                                )
                            })
                            .side(cn::PopoverSide::Right)
                            .content(move || {
                                let theme = ThemeState::get();
                                div()
                                    .flex_col()
                                    .gap(4.0)
                                    .child(
                                        text("Right-positioned popover")
                                            .size(t_sm())
                                            .color(theme.color(ColorToken::TextSecondary)),
                                    )
                            }),
                        ),
                )
                // Positioned to the top
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(
                            text("Positioned Top")
                                .size(t_sm())
                                .medium()
                                .color(text_primary),
                        )
                        .child(
                            cn::popover(|_is_open| {
                                div().w_fit().child(
                                    cn::button("Open Top")
                                        .variant(ButtonVariant::Ghost),
                                )
                            })
                            .side(cn::PopoverSide::Top)
                            .content(move || {
                                let theme = ThemeState::get();
                                div()
                                    .flex_col()
                                    .gap(4.0)
                                    .child(
                                        text("Top-positioned popover")
                                            .size(t_sm())
                                            .color(theme.color(ColorToken::TextSecondary)),
                                    )
                            }),
                        ),
                ),
        )
}

// ============================================================================
// Tooltip Section
// ============================================================================

fn tooltip_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_primary = theme.color(ColorToken::TextPrimary);

    section_container().child(section_title("Tooltip")).child(
        div()
            .flex_col()
            .gap(24.0)
            // Basic tooltip
            .child(
                div()
                    .flex_col()
                    .gap(8.0)
                    .child(
                        text("Basic Tooltip")
                            .size(t_sm())
                            .medium()
                            .color(text_primary),
                    )
                    .child(
                        cn::tooltip(|| {
                            div().child(cn::button("Hover me").variant(ButtonVariant::Outline))
                        })
                        .text("This is a tooltip"),
                    ),
            )
            // Side positions
            .child(
                div()
                    .flex_col()
                    .gap(8.0)
                    .child(
                        text("Side Positions")
                            .size(t_sm())
                            .medium()
                            .color(text_primary),
                    )
                    .child(
                        div()
                            .flex_row()
                            .gap(16.0)
                            .child(
                                cn::tooltip(|| {
                                    div().child(cn::button("Top").variant(ButtonVariant::Outline))
                                })
                                .text("Appears above")
                                .side(TooltipSide::Top),
                            )
                            .child(
                                cn::tooltip(|| {
                                    div()
                                        .child(cn::button("Bottom").variant(ButtonVariant::Outline))
                                })
                                .text("Appears below")
                                .side(TooltipSide::Bottom),
                            )
                            .child(
                                cn::tooltip(|| {
                                    div().child(cn::button("Left").variant(ButtonVariant::Outline))
                                })
                                .text("Appears left")
                                .side(TooltipSide::Left),
                            )
                            .child(
                                cn::tooltip(|| {
                                    div().child(cn::button("Right").variant(ButtonVariant::Outline))
                                })
                                .text("Appears right")
                                .side(TooltipSide::Right),
                            ),
                    ),
            )
            // Custom delay
            .child(
                div()
                    .flex_col()
                    .gap(8.0)
                    .child(
                        text("Custom Delay")
                            .size(t_sm())
                            .medium()
                            .color(text_primary),
                    )
                    .child(
                        div()
                            .flex_row()
                            .gap(16.0)
                            .child(
                                cn::tooltip(|| {
                                    div().child(
                                        cn::button("Instant").variant(ButtonVariant::Secondary),
                                    )
                                })
                                .text("No delay!")
                                .open_delay_ms(0),
                            )
                            .child(
                                cn::tooltip(|| {
                                    div().child(
                                        cn::button("Slow (1s)").variant(ButtonVariant::Secondary),
                                    )
                                })
                                .text("Waited for it...")
                                .open_delay_ms(1000),
                            ),
                    ),
            ),
    )
}

fn kbd_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    section_container()
        .child(section_title("Keyboard Shortcuts"))
        .child(
            div()
                .flex_col()
                .gap(16.0)
                // Basic keyboard shortcut example
                .child(
                    div()
                        .flex_row()
                        .items_center()
                        .gap_px(8.0)
                        .child(text("Press").size(t_sm()).color(text_secondary))
                        .child(cn::kbd("⌘"))
                        .child(text("+").size(t_sm()).color(text_secondary))
                        .child(cn::kbd("K"))
                        .child(
                            text("to open command palette")
                                .size(t_sm())
                                .color(text_secondary),
                        ),
                )
                // Size variants
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(
                            div()
                                .flex_row()
                                .items_center()
                                .gap_px(8.0)
                                .child(text("Small:").size(t_sm()).color(text_secondary))
                                .child(cn::kbd("Ctrl").size(KbdSize::Small))
                                .child(cn::kbd("S").size(KbdSize::Small)),
                        )
                        .child(
                            div()
                                .flex_row()
                                .items_center()
                                .gap_px(8.0)
                                .child(text("Medium:").size(t_sm()).color(text_secondary))
                                .child(cn::kbd("Ctrl"))
                                .child(cn::kbd("S")),
                        )
                        .child(
                            div()
                                .flex_row()
                                .items_center()
                                .gap_px(8.0)
                                .child(text("Large:").size(t_sm()).color(text_secondary))
                                .child(cn::kbd("Ctrl").size(KbdSize::Large))
                                .child(cn::kbd("S").size(KbdSize::Large)),
                        ),
                )
                // Common shortcuts
                .child(
                    div()
                        .flex_row()
                        .flex_wrap()
                        .gap(16.0)
                        .child(
                            div()
                                .flex_row()
                                .items_center()
                                .gap_px(4.0)
                                .child(cn::kbd("⌘"))
                                .child(cn::kbd("C"))
                                .child(text(" - Copy").size(t_xs()).color(text_secondary)),
                        )
                        .child(
                            div()
                                .flex_row()
                                .items_center()
                                .gap_px(4.0)
                                .child(cn::kbd("⌘"))
                                .child(cn::kbd("V"))
                                .child(text(" - Paste").size(t_xs()).color(text_secondary)),
                        )
                        .child(
                            div()
                                .flex_row()
                                .items_center()
                                .gap_px(4.0)
                                .child(cn::kbd("⌘"))
                                .child(cn::kbd("Z"))
                                .child(text(" - Undo").size(t_xs()).color(text_secondary)),
                        )
                        .child(
                            div()
                                .flex_row()
                                .items_center()
                                .gap_px(4.0)
                                .child(cn::kbd("⇧"))
                                .child(cn::kbd("⌘"))
                                .child(cn::kbd("Z"))
                                .child(text(" - Redo").size(t_xs()).color(text_secondary)),
                        ),
                )
                // Special keys
                .child(
                    div()
                        .flex_row()
                        .flex_wrap()
                        .gap_px(8.0)
                        .child(cn::kbd("Enter"))
                        .child(cn::kbd("Tab"))
                        .child(cn::kbd("Esc"))
                        .child(cn::kbd("Space"))
                        .child(cn::kbd("←"))
                        .child(cn::kbd("→"))
                        .child(cn::kbd("↑"))
                        .child(cn::kbd("↓")),
                ),
        )
}

fn misc_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();

    section_container()
        .child(section_title("Miscellaneous"))
        .child(
            div()
                .flex_col()
                .gap(16.0)
                .child(
                    div()
                        .flex_row()
                        .items_center()
                        .gap(12.0)
                        .child(
                            text("Left content")
                                .size(t_sm())
                                .color(theme.color(ColorToken::TextSecondary)),
                        )
                        .child(cn::separator().w(100.0))
                        .child(
                            text("Right content")
                                .size(t_sm())
                                .color(theme.color(ColorToken::TextSecondary)),
                        ),
                )
                .child(
                    div()
                        .flex_row()
                        .gap(16.0)
                        .child(cn::label("Small Label").size(LabelSize::Small))
                        .child(cn::label("Medium Label").size(LabelSize::Medium))
                        .child(cn::label("Large Label").size(LabelSize::Large)),
                ),
        )
}

// ============================================================================
// Tree View Section
// ============================================================================

fn tree_view_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    section_container().child(section_title("Tree View")).child(
        div()
            .flex_row()
            .gap(24.0)
            .child(
                // File explorer style tree
                div()
                    .flex_col()
                    .gap(8.0)
                    .child(text("File Explorer").size(t_xs()).color(text_secondary))
                    .child(
                        scroll()
                            .h_full()
                            .both_directions()
                            .w(250.0)
                            .p(4.0)
                            .bg(theme.color(ColorToken::Surface))
                            .border(1.0, theme.color(ColorToken::Border))
                            .rounded(r_default())
                            .child(cn::tree_view().node("project", "my-project", |n| {
                                n.expanded()
                                    .child("src", "src/", |n| {
                                        n.expanded()
                                            .child("main", "main.rs", |n| n)
                                            .child("lib", "lib.rs", |n| n)
                                            .child("utils", "utils/", |n| {
                                                n.child("helpers", "helpers.rs", |n| n)
                                            })
                                    })
                                    .child("tests", "tests/", |n| {
                                        n.child("integration", "integration.rs", |n| n)
                                    })
                                    .child("cargo", "Cargo.toml", |n| n)
                                    .child("readme", "README.md", |n| n)
                            })),
                    ),
            )
            .child(
                // Diff tree (for debugger)
                div()
                    .flex_col()
                    .gap(8.0)
                    .child(
                        text("Element Tree with Diff")
                            .size(t_xs())
                            .color(text_secondary),
                    )
                    .child(
                        scroll()
                            .h_full()
                            .both_directions()
                            .w(250.0)
                            .p(4.0)
                            .bg(theme.color(ColorToken::Surface))
                            .border(1.0, theme.color(ColorToken::Border))
                            .rounded(r_default())
                            .child(
                                cn::tree_view()
                                    .node("root", "Window", |n| {
                                        n.expanded().child("container", "Container", |n| {
                                            n.expanded()
                                                .child("header", "Header", |n| n)
                                                .child("content", "Content", |n| {
                                                    n.expanded()
                                                        .child("button", "Button", |n| {
                                                            n.diff(TreeNodeDiff::Modified)
                                                        })
                                                        .child("new_div", "NewDiv", |n| {
                                                            n.diff(TreeNodeDiff::Added)
                                                        })
                                                })
                                                .child("old_footer", "OldFooter", |n| {
                                                    n.diff(TreeNodeDiff::Removed)
                                                })
                                        })
                                    })
                                    .with_guides(),
                            ),
                    ),
            ),
    )
}

// ============================================================================
// Table Section
// ============================================================================

fn table_section() -> impl ElementBuilder + use<> {
    section_container().child(section_title("Table")).child(
        cn::table()
            .w_full()
            .child(
                cn::table_header().child(
                    cn::table_row()
                        .child(cn::table_head("Invoice"))
                        .child(cn::table_head("Status"))
                        .child(cn::table_head("Method"))
                        .child(cn::table_head("Amount")),
                ),
            )
            .child(
                cn::table_body()
                    .child(
                        cn::table_row()
                            .child(cn::table_cell().child(text("INV001")))
                            .child(cn::table_cell().child(text("Paid")))
                            .child(cn::table_cell().child(text("Credit Card")))
                            .child(cn::table_cell().child(text("$250.00"))),
                    )
                    .child(
                        cn::table_row()
                            .selected(true)
                            .child(cn::table_cell().child(text("INV002")))
                            .child(cn::table_cell().child(text("Pending")))
                            .child(cn::table_cell().child(text("Wire")))
                            .child(cn::table_cell().child(text("$1,200.00"))),
                    )
                    .child(
                        cn::table_row()
                            .child(cn::table_cell().child(text("INV003")))
                            .child(cn::table_cell().child(text("Unpaid")))
                            .child(cn::table_cell().child(text("ACH")))
                            .child(cn::table_cell().child(text("$450.00"))),
                    )
                    .child(
                        cn::table_row()
                            .child(cn::table_cell().child(text("INV004")))
                            .child(cn::table_cell().child(text("Paid")))
                            .child(cn::table_cell().child(text("Credit Card")))
                            .child(cn::table_cell().child(text("$80.00"))),
                    ),
            )
            .child(
                cn::table_footer().child(
                    cn::table_row()
                        .child(cn::table_cell().child(text("Total")))
                        .child(cn::table_cell())
                        .child(cn::table_cell())
                        .child(cn::table_cell().child(text("$1,980.00"))),
                ),
            )
            .child(cn::table_caption("A list of your recent invoices")),
    )
}

// ============================================================================
// Charts Section
// ============================================================================

fn charts_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    section_container().child(section_title("Charts")).child(
        div()
            .flex_col()
            .gap(24.0)
            // Line charts row
            .child(
                div()
                    .flex_col()
                    .gap(8.0)
                    .child(
                        text("Line Chart - Multi-series")
                            .size(t_xs())
                            .color(text_secondary),
                    )
                    .child(
                        cn::line_chart()
                            .width(400.0)
                            .height(180.0)
                            .series(
                                "CPU",
                                &[0.3, 0.45, 0.4, 0.6, 0.55, 0.7, 0.65, 0.8, 0.75, 0.9],
                            )
                            .series(
                                "Memory",
                                &[0.2, 0.25, 0.3, 0.35, 0.4, 0.42, 0.45, 0.48, 0.5, 0.52],
                            )
                            .with_dots()
                            .build(),
                    ),
            )
            // Bar charts row
            .child(
                div()
                    .flex_row()
                    .gap(24.0)
                    .child(
                        div()
                            .flex_col()
                            .gap(8.0)
                            .child(
                                text("Bar Chart - Vertical")
                                    .size(t_xs())
                                    .color(text_secondary),
                            )
                            .child(
                                cn::bar_chart()
                                    .width(200.0)
                                    .height(150.0)
                                    .data(&[
                                        ("Jan", 120.0),
                                        ("Feb", 180.0),
                                        ("Mar", 150.0),
                                        ("Apr", 210.0),
                                        ("May", 190.0),
                                    ])
                                    .build(),
                            ),
                    )
                    .child(
                        div()
                            .flex_col()
                            .gap(8.0)
                            .child(
                                text("Bar Chart - Horizontal")
                                    .size(t_xs())
                                    .color(text_secondary),
                            )
                            .child(
                                cn::bar_chart()
                                    .width(250.0)
                                    .height(150.0)
                                    .data(&[
                                        ("React", 85.0),
                                        ("Vue", 65.0),
                                        ("Svelte", 45.0),
                                        ("Angular", 40.0),
                                    ])
                                    .horizontal()
                                    .color(theme.color(ColorToken::Secondary))
                                    .build(),
                            ),
                    ),
            )
            // Sparklines row
            .child(
                div()
                    .flex_col()
                    .gap(8.0)
                    .child(
                        text("Sparklines - Inline trends")
                            .size(t_xs())
                            .color(text_secondary),
                    )
                    .child(
                        div()
                            .flex_col()
                            .items_center()
                            .gap(24.0)
                            .child(
                                div()
                                    .flex_row()
                                    .items_center()
                                    .gap(8.0)
                                    .child(text("Sales").size(t_sm()).color(text_secondary))
                                    .child(
                                        cn::spark_line(&[1.0, 2.5, 2.0, 3.5, 3.0, 4.5, 4.0, 5.0])
                                            .width(100.0)
                                            .height(24.0)
                                            .color(theme.color(ColorToken::Success))
                                            .build(),
                                    )
                                    .child(
                                        text("+25%")
                                            .size(t_xs())
                                            .color(theme.color(ColorToken::Success)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_row()
                                    .items_center()
                                    .gap(8.0)
                                    .child(text("Errors").size(t_sm()).color(text_secondary))
                                    .child(
                                        cn::spark_line(&[5.0, 4.0, 4.5, 3.0, 3.5, 2.0, 2.5, 1.0])
                                            .width(100.0)
                                            .height(24.0)
                                            .color(theme.color(ColorToken::Error))
                                            .filled()
                                            .build(),
                                    )
                                    .child(
                                        text("-60%")
                                            .size(t_xs())
                                            .color(theme.color(ColorToken::Error)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_row()
                                    .items_center()
                                    .gap(8.0)
                                    .child(text("Latency").size(t_sm()).color(text_secondary))
                                    .child(
                                        cn::spark_line(&[
                                            45.0, 48.0, 42.0, 50.0, 47.0, 45.0, 43.0, 46.0,
                                        ])
                                        .width(100.0)
                                        .height(24.0)
                                        .build(),
                                    )
                                    .child(text("46ms").size(t_xs()).color(text_secondary)),
                            ),
                    ),
            )
            // Regression Detection Charts
            .child(
                div()
                    .flex_col()
                    .gap(8.0)
                    .child(
                        text("Threshold Line Chart - Regression Detection")
                            .size(t_xs())
                            .color(text_secondary),
                    )
                    .child(
                        cn::threshold_line_chart()
                            .width(450.0)
                            .height(160.0)
                            .data(&[
                                12.5, 13.2, 14.8, 15.1, 14.5, 16.2, 15.8, 17.4, 18.2, 19.5, 18.8,
                                20.1, 22.5, 24.8, 28.2, 25.5,
                            ])
                            .regression_bands(16.67, 33.33) // 60fps and 30fps budgets
                            .baseline(16.67)
                            .build(),
                    ),
            )
            // Histogram
            .child(
                div()
                    .flex_col()
                    .gap(8.0)
                    .child(
                        text("Histogram - Pixel Diff Distribution")
                            .size(t_xs())
                            .color(text_secondary),
                    )
                    .child(
                        cn::histogram(&generate_diff_data())
                            .width(400.0)
                            .height(120.0)
                            .bins(40)
                            .threshold_line(5.0, "noise floor")
                            .build(),
                    ),
            )
            // Comparison Bar Chart
            .child(
                div()
                    .flex_col()
                    .gap(8.0)
                    .child(
                        text("Comparison Bar Chart - Baseline vs Current")
                            .size(t_xs())
                            .color(text_secondary),
                    )
                    .child(
                        cn::comparison_bar_chart()
                            .width(450.0)
                            .height(180.0)
                            .item("Render time", 12.5, 14.2)
                            .item("Layout time", 3.2, 3.0)
                            .item("Paint time", 8.4, 11.8)
                            .item("Composite", 2.1, 2.3)
                            .threshold(10.0)
                            .build(),
                    ),
            ),
    )
}

/// Generate sample diff data for histogram demo
fn generate_diff_data() -> Vec<f64> {
    // Simulate pixel differences - most near 0, long tail
    let mut data = Vec::with_capacity(500);
    for i in 0..500 {
        let val = if i < 350 {
            (i as f64 * 0.01).sin().abs() * 3.0 // Low values
        } else if i < 450 {
            3.0 + (i as f64 * 0.05).cos().abs() * 8.0 // Medium values
        } else {
            10.0 + (i as f64 * 0.1).sin().abs() * 20.0 // High values (regressions)
        };
        data.push(val);
    }
    data
}

fn icon_gallery_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);

    section_container()
        .child(section_title("Icons (Lucide)"))
        .child(
            div()
                .flex_col()
                .gap_px(24.0)
                // Size variants
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(text("Size Variants").size(t_xs()).color(text_secondary))
                        .child(
                            div()
                                .flex_row()
                                .items_center()
                                .gap(16.0)
                                .child(
                                    div()
                                        .flex_col()
                                        .items_center()
                                        .gap(4.0)
                                        .child(cn::icon(icons::CHECK).size(IconSize::ExtraSmall))
                                        .child(text("XS").size(10.0).color(text_secondary)),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .items_center()
                                        .gap(4.0)
                                        .child(cn::icon(icons::CHECK).size(IconSize::Small))
                                        .child(text("SM").size(10.0).color(text_secondary)),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .items_center()
                                        .gap(4.0)
                                        .child(cn::icon(icons::CHECK).size(IconSize::Medium))
                                        .child(text("MD").size(10.0).color(text_secondary)),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .items_center()
                                        .gap(4.0)
                                        .child(cn::icon(icons::CHECK).size(IconSize::Large))
                                        .child(text("LG").size(10.0).color(text_secondary)),
                                )
                                .child(
                                    div()
                                        .flex_col()
                                        .items_center()
                                        .gap(4.0)
                                        .child(cn::icon(icons::CHECK).size(IconSize::ExtraLarge))
                                        .child(text("XL").size(10.0).color(text_secondary)),
                                ),
                        ),
                )
                // Color variants
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(text("Color Variants").size(t_xs()).color(text_secondary))
                        .child(
                            div()
                                .flex_row()
                                .items_center()
                                .gap(12.0)
                                .child(cn::icon(icons::HEART).size(IconSize::Large))
                                .child(
                                    cn::icon(icons::HEART)
                                        .size(IconSize::Large)
                                        .color(ColorToken::Primary),
                                )
                                .child(
                                    cn::icon(icons::HEART)
                                        .size(IconSize::Large)
                                        .color(ColorToken::Success),
                                )
                                .child(
                                    cn::icon(icons::HEART)
                                        .size(IconSize::Large)
                                        .color(ColorToken::Warning),
                                )
                                .child(
                                    cn::icon(icons::HEART)
                                        .size(IconSize::Large)
                                        .color(ColorToken::Error),
                                ),
                        ),
                )
                // Common icons grid
                .child(
                    div()
                        .flex_col()
                        .gap(8.0)
                        .child(text("Common Icons").size(t_xs()).color(text_secondary))
                        .child(
                            div()
                                .flex_row()
                                .flex_wrap()
                                .gap(2.0)
                                .child(icon_tile(icons::ARROW_RIGHT, "arrow-right"))
                                .child(icon_tile(icons::ARROW_LEFT, "arrow-left"))
                                .child(icon_tile(icons::ARROW_UP, "arrow-up"))
                                .child(icon_tile(icons::ARROW_DOWN, "arrow-down"))
                                .child(icon_tile(icons::CHECK, "check"))
                                .child(icon_tile(icons::X, "x"))
                                .child(icon_tile(icons::PLUS, "plus"))
                                .child(icon_tile(icons::MINUS, "minus"))
                                .child(icon_tile(icons::SEARCH, "search"))
                                .child(icon_tile(icons::SETTINGS, "settings"))
                                .child(icon_tile(icons::USER, "user"))
                                .child(icon_tile(icons::USERS, "users"))
                                .child(icon_tile(icons::HOUSE, "house"))
                                .child(icon_tile(icons::MENU, "menu"))
                                .child(icon_tile(icons::BELL, "bell"))
                                .child(icon_tile(icons::MAIL, "mail"))
                                .child(icon_tile(icons::CALENDAR, "calendar"))
                                .child(icon_tile(icons::CLOCK, "clock"))
                                .child(icon_tile(icons::STAR, "star"))
                                .child(icon_tile(icons::HEART, "heart"))
                                .child(icon_tile(icons::TRASH_2, "trash-2"))
                                .child(icon_tile(icons::PENCIL, "pencil"))
                                .child(icon_tile(icons::COPY, "copy"))
                                .child(icon_tile(icons::DOWNLOAD, "download"))
                                .child(icon_tile(icons::UPLOAD, "upload"))
                                .child(icon_tile(icons::FILE, "file"))
                                .child(icon_tile(icons::FOLDER, "folder"))
                                .child(icon_tile(icons::IMAGE, "image"))
                                .child(icon_tile(icons::VIDEO, "video"))
                                .child(icon_tile(icons::MUSIC, "music"))
                                .child(icon_tile(icons::PLAY, "play"))
                                .child(icon_tile(icons::PAUSE, "pause")),
                        ),
                ),
        )
}

fn icon_tile(icon_data: &'static str, name: &str) -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_tertiary = theme.color(ColorToken::TextTertiary);
    let border = theme.color(ColorToken::Border);

    div()
        .flex_col()
        .items_center()
        .gap(2.0)
        .p(2.0)
        .w(72.0)
        .border(1.0, border)
        .rounded(r_default())
        .child(cn::icon(icon_data).size(IconSize::Large))
        .child(text(name).size(9.0).color(text_tertiary))
}

// ============================================================================
// SCROLL AREA SECTION
// ============================================================================

fn scroll_area_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let text_primary = theme.color(ColorToken::TextPrimary);
    let panel_bg = theme.color(ColorToken::SurfaceElevated);
    let border = theme.color(ColorToken::Border);

    // Build the inner scrollable list with explicit theme tokens — bg lifts
    // the panel one tier above the section card, text uses TextPrimary so
    // it reads correctly across light + dark schemes. Without this, the
    // text widget defaulted to `Color::BLACK` which only worked in light.
    let scroll_panel = |lines: &[&str]| -> Div {
        let mut inner = div()
            .flex_col()
            .gap_px(8.0)
            .p(8.0)
            .bg(panel_bg)
            .border(1.0, border)
            .rounded(r_default());
        for line in lines {
            inner = inner.child(text(*line).size(t_sm()).color(text_primary));
        }
        inner
    };

    section_container()
        .child(section_title("Scroll Area"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap_px(24.0)
                // Auto visibility (default) - shows on scroll/hover, auto-dismisses
                .child(
                    div()
                        .flex_col()
                        .gap_px(8.0)
                        .child(text("Auto (default)").size(t_xs()).color(text_secondary))
                        .child(
                            cn::scroll_area()
                                .scrollbar(cn::ScrollbarVisibility::Auto)
                                .w(200.0)
                                .h(150.0)
                                .child(scroll_panel(&[
                                    "Scroll content 1",
                                    "Scroll content 2",
                                    "Scroll content 3",
                                    "Scroll content 4",
                                    "Scroll content 5",
                                    "Scroll content 6",
                                    "Scroll content 7",
                                    "Scroll content 8",
                                    "Scroll content 9",
                                    "Scroll content 10",
                                ])),
                        ),
                )
                // Always visible scrollbar
                .child(
                    div()
                        .flex_col()
                        .gap_px(8.0)
                        .child(text("Always visible").size(t_xs()).color(text_secondary))
                        .child(
                            cn::scroll_area()
                                .scrollbar(cn::ScrollbarVisibility::Always)
                                .w(200.0)
                                .h(150.0)
                                .child(scroll_panel(&[
                                    "Item A", "Item B", "Item C", "Item D", "Item E", "Item F",
                                    "Item G", "Item H",
                                ])),
                        ),
                )
                // Hover visibility
                .child(
                    div()
                        .flex_col()
                        .gap_px(8.0)
                        .child(text("Show on hover").size(t_xs()).color(text_secondary))
                        .child(
                            cn::scroll_area()
                                .scrollbar(cn::ScrollbarVisibility::Hover)
                                .w(200.0)
                                .h(150.0)
                                .child(scroll_panel(&[
                                    "Hover to see scrollbar",
                                    "Line 2",
                                    "Line 3",
                                    "Line 4",
                                    "Line 5",
                                    "Line 6",
                                    "Line 7",
                                    "Line 8",
                                ])),
                        ),
                )
                // Never show scrollbar
                .child(
                    div()
                        .flex_col()
                        .gap_px(8.0)
                        .child(text("Hidden scrollbar").size(t_xs()).color(text_secondary))
                        .child(
                            cn::scroll_area()
                                .scrollbar(cn::ScrollbarVisibility::Never)
                                .w(200.0)
                                .h(150.0)
                                .child(scroll_panel(&[
                                    "No visible scrollbar",
                                    "But still scrollable",
                                    "Line 3",
                                    "Line 4",
                                    "Line 5",
                                    "Line 6",
                                    "Line 7",
                                    "Line 8",
                                ])),
                        ),
                ),
        )
}

// ============================================================================
// ASPECT RATIO SECTION
// ============================================================================

fn aspect_ratio_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let text_primary = theme.color(ColorToken::TextPrimary);
    let _surface = theme.color(ColorToken::Surface);
    let primary = theme.color(ColorToken::Primary);

    section_container()
        .child(section_title("Aspect Ratio"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap_px(24.0)
                .items_end()
                // Square (1:1)
                .child(
                    div()
                        .flex_col()
                        .gap_px(8.0)
                        .child(text("1:1 Square").size(t_xs()).color(text_secondary))
                        .child(
                            cn::aspect_ratio_square()
                                .w(100.0)
                                .bg(primary.with_alpha(0.25))
                                .rounded(r_default())
                                .child(
                                    div()
                                        .w_full()
                                        .h_full()
                                        .items_center()
                                        .justify_center()
                                        .child(text("1:1").size(t_sm()).color(text_primary)),
                                ),
                        ),
                )
                // 16:9 Widescreen
                .child(
                    div()
                        .flex_col()
                        .gap_px(8.0)
                        .child(text("16:9 Widescreen").size(t_xs()).color(text_secondary))
                        .child(
                            cn::aspect_ratio_16_9()
                                .w(160.0)
                                .bg(primary.with_alpha(0.2))
                                .rounded(r_default())
                                .child(
                                    div()
                                        .w_full()
                                        .h_full()
                                        .items_center()
                                        .justify_center()
                                        .child(text("16:9").size(t_sm()).color(text_primary)),
                                ),
                        ),
                )
                // 4:3 Traditional
                .child(
                    div()
                        .flex_col()
                        .gap_px(8.0)
                        .child(text("4:3 Traditional").size(t_xs()).color(text_secondary))
                        .child(
                            cn::aspect_ratio_4_3()
                                .w(120.0)
                                .bg(primary.with_alpha(0.1))
                                .rounded(r_default())
                                .child(
                                    div()
                                        .w_full()
                                        .h_full()
                                        .items_center()
                                        .justify_center()
                                        .child(text("4:3").size(t_sm()).color(text_primary)),
                                ),
                        ),
                )
                // 21:9 Ultrawide
                .child(
                    div()
                        .flex_col()
                        .gap_px(8.0)
                        .child(text("21:9 Ultrawide").size(t_xs()).color(text_secondary))
                        .child(
                            cn::aspect_ratio_21_9()
                                .w(210.0)
                                .bg(primary.with_alpha(0.3))
                                .rounded(r_default())
                                .child(
                                    div()
                                        .w_full()
                                        .h_full()
                                        .items_center()
                                        .justify_center()
                                        .child(text("21:9").size(t_sm()).color(text_primary)),
                                ),
                        ),
                )
                // 9:16 Vertical (portrait)
                .child(
                    div()
                        .flex_col()
                        .gap_px(8.0)
                        .child(text("9:16 Vertical").size(t_xs()).color(text_secondary))
                        .child(
                            cn::aspect_ratio_9_16()
                                .w(56.0)
                                .bg(primary.with_alpha(0.35))
                                .rounded(r_default())
                                .child(
                                    div()
                                        .w_full()
                                        .h_full()
                                        .items_center()
                                        .justify_center()
                                        .child(text("9:16").size(t_xs()).color(text_primary)),
                                ),
                        ),
                )
                // Custom ratio
                .child(
                    div()
                        .flex_col()
                        .gap_px(8.0)
                        .child(text("Custom 3:2").size(t_xs()).color(text_secondary))
                        .child(
                            cn::aspect_ratio(3.0 / 2.0)
                                .w(120.0)
                                .bg(primary.with_alpha(0.15))
                                .rounded(r_default())
                                .child(
                                    div()
                                        .w_full()
                                        .h_full()
                                        .items_center()
                                        .justify_center()
                                        .child(text("3:2").size(t_sm()).color(text_primary)),
                                ),
                        ),
                ),
        )
}

fn avatar_section() -> impl ElementBuilder + use<> {
    let theme = ThemeState::get();
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let primary = theme.color(ColorToken::Primary);
    let secondary = theme.color(ColorToken::Secondary);
    let tertiary = theme.color(ColorToken::AccentSubtle);

    // Path to the avatar image
    let avatar_path = "examples/blinc_app_examples/examples/assets/avatar.jpg";

    section_container().child(section_title("Avatar")).child(
        div()
            .flex_col()
            .gap_px(24.0)
            // Sizes section
            .child(
                div()
                    .flex_col()
                    .gap_px(8.0)
                    .child(
                        text("Sizes")
                            .size(t_sm())
                            .weight(FontWeight::Medium)
                            .color(text_secondary),
                    )
                    .child(
                        div()
                            .flex_row()
                            .gap_px(16.0)
                            .items_end()
                            .child(
                                div()
                                    .flex_col()
                                    .gap_px(4.0)
                                    .items_center()
                                    .child(cn::avatar().src(avatar_path).xs())
                                    .child(text("xs").size(10.0).color(text_secondary)),
                            )
                            .child(
                                div()
                                    .flex_col()
                                    .gap_px(4.0)
                                    .items_center()
                                    .child(cn::avatar().src(avatar_path).sm())
                                    .child(text("sm").size(10.0).color(text_secondary)),
                            )
                            .child(
                                div()
                                    .flex_col()
                                    .gap_px(4.0)
                                    .items_center()
                                    .child(cn::avatar().src(avatar_path).md())
                                    .child(text("md").size(10.0).color(text_secondary)),
                            )
                            .child(
                                div()
                                    .flex_col()
                                    .gap_px(4.0)
                                    .items_center()
                                    .child(cn::avatar().src(avatar_path).lg())
                                    .child(text("lg").size(10.0).color(text_secondary)),
                            )
                            .child(
                                div()
                                    .flex_col()
                                    .gap_px(4.0)
                                    .items_center()
                                    .child(cn::avatar().src(avatar_path).xl())
                                    .child(text("xl").size(10.0).color(text_secondary)),
                            ),
                    ),
            )
            // Shapes section
            .child(
                div()
                    .flex_col()
                    .gap_px(8.0)
                    .child(
                        text("Shapes")
                            .size(t_sm())
                            .weight(FontWeight::Medium)
                            .color(text_secondary),
                    )
                    .child(
                        div()
                            .flex_row()
                            .gap_px(16.0)
                            .items_center()
                            .child(
                                div()
                                    .flex_col()
                                    .gap_px(4.0)
                                    .items_center()
                                    .child(cn::avatar().src(avatar_path).lg().circle())
                                    .child(text("Circle").size(10.0).color(text_secondary)),
                            )
                            .child(
                                div()
                                    .flex_col()
                                    .gap_px(4.0)
                                    .items_center()
                                    .child(cn::avatar().src(avatar_path).lg().square())
                                    .child(text("Square").size(10.0).color(text_secondary)),
                            ),
                    ),
            )
            // Fallback initials section
            .child(
                div()
                    .flex_col()
                    .gap_px(8.0)
                    .child(
                        text("Fallback Initials")
                            .size(t_sm())
                            .weight(FontWeight::Medium)
                            .color(text_secondary),
                    )
                    .child(
                        div()
                            .flex_row()
                            .gap_px(12.0)
                            .items_center()
                            .child(cn::avatar().fallback("JD").lg())
                            .child(
                                cn::avatar()
                                    .fallback("AB")
                                    .lg()
                                    .fallback_bg(primary.with_alpha(0.2)),
                            )
                            .child(cn::avatar().fallback("CN").lg().square())
                            .child(cn::avatar().lg()), // Empty fallback shows "?"
                    ),
            )
            // Status indicators section
            .child(
                div()
                    .flex_col()
                    .gap_px(8.0)
                    .child(
                        text("Status Indicators")
                            .size(t_sm())
                            .weight(FontWeight::Medium)
                            .color(text_secondary),
                    )
                    .child(
                        div()
                            .flex_row()
                            .gap_px(16.0)
                            .items_center()
                            .child(
                                div()
                                    .flex_col()
                                    .gap_px(4.0)
                                    .items_center()
                                    .child(cn::avatar().src(avatar_path).lg().online())
                                    .child(text("Online").size(10.0).color(text_secondary)),
                            )
                            .child(
                                div()
                                    .flex_col()
                                    .gap_px(4.0)
                                    .items_center()
                                    .child(cn::avatar().src(avatar_path).lg().away())
                                    .child(text("Away").size(10.0).color(text_secondary)),
                            )
                            .child(
                                div()
                                    .flex_col()
                                    .gap_px(4.0)
                                    .items_center()
                                    .child(cn::avatar().src(avatar_path).lg().busy())
                                    .child(text("Busy").size(10.0).color(text_secondary)),
                            )
                            .child(
                                div()
                                    .flex_col()
                                    .gap_px(4.0)
                                    .items_center()
                                    .child(cn::avatar().src(avatar_path).lg().offline())
                                    .child(text("Offline").size(10.0).color(text_secondary)),
                            ),
                    ),
            )
            // Avatar group section
            .child(
                div()
                    .flex_col()
                    .gap_px(8.0)
                    .child(
                        text("Avatar Group")
                            .size(t_sm())
                            .weight(FontWeight::Medium)
                            .color(text_secondary),
                    )
                    .child(
                        div()
                            .flex_col()
                            .gap_px(12.0)
                            .child(
                                cn::avatar_group()
                                    .size(cn::AvatarSize::Medium)
                                    .child(cn::avatar().src(avatar_path))
                                    .child(
                                        cn::avatar()
                                            .fallback("AB")
                                            .fallback_bg(primary.with_alpha(0.2)),
                                    )
                                    .child(
                                        cn::avatar()
                                            .fallback("CD")
                                            .fallback_bg(tertiary.with_alpha(0.2)),
                                    )
                                    .child(
                                        cn::avatar()
                                            .fallback("EF")
                                            .fallback_bg(secondary.with_alpha(0.2)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_row()
                                    .gap_px(4.0)
                                    .child(text("With max:").size(t_xs()).color(text_secondary))
                                    .child(
                                        cn::avatar_group()
                                            .size(cn::AvatarSize::Small)
                                            .max(3)
                                            .child(cn::avatar().src(avatar_path))
                                            .child(cn::avatar().fallback("A"))
                                            .child(cn::avatar().fallback("B"))
                                            .child(cn::avatar().fallback("C"))
                                            .child(cn::avatar().fallback("D"))
                                            .child(cn::avatar().fallback("E")),
                                    ),
                            ),
                    ),
            ),
    )
}
