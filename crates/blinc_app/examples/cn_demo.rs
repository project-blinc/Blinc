//! blinc_cn Components Demo
//!
//! Showcases all available blinc_cn components in a scrollable grid layout.
//!
//! Run with: cargo run -p blinc_app --example cn_demo --features windowed

use blinc_animation::SpringConfig;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_cn::prelude::*;
use blinc_core::Color;
use blinc_layout::selector::ScrollRef;
use blinc_layout::widgets::text_input::text_input_data;
use blinc_theme::{ColorScheme, ColorToken, ThemeState};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "blinc_cn Components Demo".to_string(),
        width: 900,
        height: 900,
        resizable: true,
        fullscreen: false,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    eprintln!("build_ui called");
    let theme = ThemeState::get();
    let bg = theme.color(ColorToken::Background);

    // Create scroll ref to track scroll position
    let scroll_ref = ctx.use_scroll_ref("main_scroll");

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
                .bind(&scroll_ref)
                .child(
                    div()
                        .w_full()
                        .p(theme.spacing().space_6)
                        .flex_col()
                        .gap(theme.spacing().space_8)
                        // Component sections
                        .child(progress_section(ctx, &scroll_ref))
                        .child(buttons_section(ctx))
                        .child(badges_section())
                        .child(cards_section())
                        .child(alerts_section())
                        .child(form_inputs_section(ctx))
                        .child(toggles_section(ctx))
                        .child(slider_section(ctx))
                        .child(radio_section(ctx))
                        .child(select_section(ctx))
                        .child(context_menu_section())
                        .child(dropdown_menu_section())
                        .child(dialog_section(ctx))
                        .child(tabs_section(ctx))
                        .child(toast_section(ctx))
                        .child(loading_section(ctx))
                        .child(misc_section()),
                ),
        )
}

/// Header with title and theme toggle
fn header(ctx: &WindowedContext) -> impl ElementBuilder {
    let theme = ThemeState::get();
    let surface = theme.color(ColorToken::Surface);
    let text_primary = theme.color(ColorToken::TextPrimary);
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let border = theme.color(ColorToken::Border);

    // Theme toggle switch state
    let is_dark = ctx.use_state_keyed("theme_is_dark", || {
        ThemeState::get().scheme() == ColorScheme::Dark
    });
    let scheduler = ctx.animation_handle();

    div()
        .w_full()
        .h(80.0)
        .bg(surface)
        .border_bottom(1.5, border)
        .px(theme.spacing().space_6)
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
        .child(
            cn::switch(&is_dark, scheduler)
                .label("Dark Mode")
                .on_change(|_| {
                    ThemeState::get().toggle_scheme();
                }),
        )
}

/// Section title helper
fn section_title(title: &str) -> impl ElementBuilder {
    let theme = ThemeState::get();
    let text_primary = theme.color(ColorToken::TextPrimary);

    text(title)
        .size(theme.typography().text_xl)
        .weight(FontWeight::Bold)
        .color(text_primary)
}

/// Section container helper
fn section_container() -> Div {
    let theme = ThemeState::get();
    let surface = theme.color(ColorToken::Background).with_alpha(0.6);
    let border = theme.color(ColorToken::Border);
    let radius = theme.radii().radius_xl;

    div()
        .bg(surface)
        .rounded(radius)
        .border(1.5, border)
        .p(theme.spacing().space_5)
        .flex_col()
        .gap(theme.spacing().space_4)
}

// ============================================================================
// BUTTON SECTION
// ============================================================================

fn buttons_section(_ctx: &WindowedContext) -> impl ElementBuilder {
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
}

// ============================================================================
// BADGES SECTION
// ============================================================================

fn badges_section() -> impl ElementBuilder {
    section_container().child(section_title("Badges")).child(
        div()
            .flex_row()
            .flex_wrap()
            .gap(12.0)
            .child(cn::badge("Default"))
            .child(cn::badge("Secondary").variant(BadgeVariant::Secondary))
            .child(cn::badge("Success").variant(BadgeVariant::Success))
            .child(cn::badge("Warning").variant(BadgeVariant::Warning))
            .child(cn::badge("Destructive").variant(BadgeVariant::Destructive))
            .child(cn::badge("Outline").variant(BadgeVariant::Outline)),
    )
}

// ============================================================================
// CARDS SECTION
// ============================================================================

fn cards_section() -> impl ElementBuilder {
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

fn alerts_section() -> impl ElementBuilder {
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

fn form_inputs_section(_ctx: &WindowedContext) -> impl ElementBuilder {
    let username_data = text_input_data();
    let email_data = text_input_data();
    let password_data = text_input_data();
    let bio_state = blinc_layout::widgets::text_area::text_area_state();

    section_container()
        .child(section_title("Form Inputs"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                // Column 1: Text inputs
                .child(
                    div()
                        .w(280.0)
                        .flex_col()
                        .gap(16.0)
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
                        .w(300.0)
                        .flex_col()
                        .gap(16.0)
                        .child(
                            cn::textarea(&bio_state)
                                .label("Bio")
                                .placeholder("Tell us about yourself...")
                                .rows(4),
                        )
                        .child(cn::label("Labels can be standalone")),
                ),
        )
}

// ============================================================================
// TOGGLES SECTION (Checkbox, Switch)
// ============================================================================

fn toggles_section(ctx: &WindowedContext) -> impl ElementBuilder {
    let checkbox1 = ctx.use_state_keyed("checkbox1", || false);
    let checkbox2 = ctx.use_state_keyed("checkbox2", || true);
    let checkbox3 = ctx.use_state_keyed("checkbox3", || false);

    let switch1 = ctx.use_state_keyed("switch1", || false);
    let switch2 = ctx.use_state_keyed("switch2", || true);
    let switch3 = ctx.use_state_keyed("switch3", || false);

    let scheduler = ctx.animation_handle();

    section_container().child(section_title("Toggles")).child(
        div()
            .flex_row()
            .flex_wrap()
            .gap(48.0)
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
                    .child(cn::switch(&switch1, scheduler.clone()).label("Notifications"))
                    .child(cn::switch(&switch2, scheduler.clone()).label("Dark mode"))
                    .child(
                        cn::switch(&switch3, scheduler.clone())
                            .label("Disabled")
                            .disabled(true),
                    ),
            ),
    )
}

// ============================================================================
// SLIDER SECTION
// ============================================================================

fn slider_section(ctx: &WindowedContext) -> impl ElementBuilder {
    let volume = ctx.use_state_keyed("volume", || 0.5);
    let brightness = ctx.use_state_keyed("brightness", || 75.0);
    let disabled_slider = ctx.use_state_keyed("disabled_slider", || 0.3);

    section_container().child(section_title("Sliders")).child(
        div()
            .flex_col()
            .gap(24.0)
            .child(
                div().w(300.0).child(
                    cn::slider(&volume)
                        .label("Volume")
                        .show_value()
                        .build_final(ctx),
                ),
            )
            .child(
                div().w(300.0).child(
                    cn::slider(&brightness)
                        .label("Brightness")
                        .min(0.0)
                        .max(100.0)
                        .step(5.0)
                        .show_value()
                        .build_final(ctx),
                ),
            )
            .child(
                div().w(300.0).child(
                    cn::slider(&disabled_slider)
                        .label("Disabled")
                        .disabled(true)
                        .build_final(ctx),
                ),
            ),
    )
}

// ============================================================================
// RADIO GROUP SECTION
// ============================================================================

fn radio_section(ctx: &WindowedContext) -> impl ElementBuilder {
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

fn select_section(ctx: &WindowedContext) -> impl ElementBuilder {
    let fruit = ctx.use_state_keyed("fruit_select", || "".to_string());
    let size = ctx.use_state_keyed("size_select", || "medium".to_string());
    let disabled_select = ctx.use_state_keyed("disabled_select", || "option1".to_string());

    section_container().child(section_title("Select")).child(
        div()
            .flex_row()
            .flex_wrap()
            .gap(24.0)
            // Basic select with placeholder
            .child(
                div().w(200.0).child(
                    cn::select(&fruit)
                        .label("Favorite Fruit")
                        .placeholder("Choose a fruit...")
                        .option("apple", "Apple")
                        .option("banana", "Banana")
                        .option("cherry", "Cherry")
                        .option("date", "Date")
                        .option("elderberry", "Elderberry")
                        .on_change(|v| tracing::info!("Selected fruit: {}", v)),
                ),
            )
            // Select with pre-selected value
            .child(
                div().w(200.0).child(
                    cn::select(&size)
                        .label("Size")
                        .option("small", "Small")
                        .option("medium", "Medium")
                        .option("large", "Large")
                        .option("xl", "Extra Large"),
                ),
            )
            // Disabled select
            .child(
                div().w(200.0).child(
                    cn::select(&disabled_select)
                        .label("Disabled")
                        .option("option1", "Option 1")
                        .option("option2", "Option 2")
                        .disabled(true),
                ),
            ),
    )
}

// ============================================================================
// CONTEXT MENU SECTION
// ============================================================================

fn context_menu_section() -> impl ElementBuilder {
    let theme = ThemeState::get();
    let surface = theme.color(ColorToken::Surface);
    let border = theme.color(ColorToken::Border);
    let text_secondary = theme.color(ColorToken::TextSecondary);

    // Common icon SVGs for menu items
    let scissors_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="3"/><path d="M8.12 8.12 12 12"/><path d="M20 4 8.12 15.88"/><circle cx="6" cy="18" r="3"/><path d="M14.8 14.8 20 20"/></svg>"#;
    let copy_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="14" height="14" x="8" y="8" rx="2" ry="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/></svg>"#;
    let clipboard_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="8" height="4" x="8" y="2" rx="1" ry="1"/><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/></svg>"#;
    let trash_icon = r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>"#;

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
                        .rounded(8.0)
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .child(text("Click me!").size(14.0).color(text_secondary))
                        .child(
                            text("(opens context menu)")
                                .size(12.0)
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
                        .rounded(8.0)
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .child(text("With Shortcuts").size(14.0).color(text_secondary))
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
                        .rounded(8.0)
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .child(text("With Icons").size(14.0).color(text_secondary))
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
                        .rounded(8.0)
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .child(text("With Disabled Items").size(14.0).color(text_secondary))
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

fn dropdown_menu_section() -> impl ElementBuilder {
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
                        div().child(
                            cn::button(if is_open { "Close Menu" } else { "Custom Trigger" })
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

fn dialog_section(_ctx: &WindowedContext) -> impl ElementBuilder {
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
                                                .size(14.0)
                                                .color(theme.color(ColorToken::TextSecondary)),
                                        )
                                        .child(
                                            text("You can put any content here - forms, lists, images, etc.")
                                                .size(14.0)
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
// LOADING SECTION (Skeleton, Spinner)
// ============================================================================

fn loading_section(ctx: &WindowedContext) -> impl ElementBuilder {
    let timeline1 = ctx.use_animated_timeline_for("spinner1");
    let timeline2 = ctx.use_animated_timeline_for("spinner2");
    let timeline3 = ctx.use_animated_timeline_for("spinner3");

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
                // Spinners
                .child(
                    div()
                        .flex_row()
                        .gap(16.0)
                        .items_center()
                        .child(cn::spinner(timeline1).size(SpinnerSize::Small))
                        .child(cn::spinner(timeline2).size(SpinnerSize::Medium))
                        .child(cn::spinner(timeline3).size(SpinnerSize::Large)),
                ),
        )
}

// ============================================================================
// PROGRESS SECTION
// ============================================================================

fn progress_section(ctx: &WindowedContext, _scroll_ref: &ScrollRef) -> impl ElementBuilder {
    const PROGRESS_WIDTH: f32 = 300.0;

    // Create animated progress - start at 0
    // Using gentle() for visible spring animation
    let animated_progress = ctx.use_animated_value_for(
        "animated_progress_v9", // Fresh key to reset persisted state
        0.0,
        SpringConfig::gentle(),
    );

    let progress_for_ready = animated_progress.clone();

    // Debug: log current animated value on each rebuild
    if let Ok(value) = animated_progress.lock() {
        tracing::info!(
            "build: animated_progress current={:.1}, target={:.1}, animating={}",
            value.get(),
            value.target(),
            value.is_animating()
        );
    }

    let is_reset = ctx.use_state_keyed("is_reset", || false);

    // Clone for reset button
    let progress_for_reset = animated_progress.clone();

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
                    .child(
                        div()
                            .w(PROGRESS_WIDTH)
                            .justify_center()
                            .p(8.0)
                            .px(16.0)
                            .bg(ThemeState::get().color(ColorToken::Primary))
                            .rounded(6.0)
                            .cursor_pointer()
                            .child(text("Reset Animation").size(14.0).color(Color::WHITE))
                            .on_click(move |_| {
                                if let Ok(mut value) = progress_for_reset.lock() {
                                    let reset_flag = !is_reset.get();

                                    if reset_flag {
                                        value.set_target(0.0);
                                        is_reset.set(true);
                                        tracing::info!("Progress animation reset to 0");
                                    } else {
                                        value.set_target(PROGRESS_WIDTH * 0.75);
                                        is_reset.set(false);
                                        tracing::info!("Progress animation reset to 75%");
                                    }
                                }
                            }),
                    ),
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

fn tabs_section(ctx: &WindowedContext) -> impl ElementBuilder {
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
                                div().px(10.0).bg_surface_elevated().w_full().h_full().items_center().child(
                                    text("Manage your account settings and preferences.")
                                        .size(14.0)
                                        .color(ThemeState::get().color(ColorToken::TextSecondary)),
                                )
                            })
                            .tab("tab2", "Password", || {
                                div().px(10.0).bg_surface_elevated().w_full().h_full().items_center().child(
                                    text("Change your password and security settings.")
                                        .size(14.0)
                                        .color(ThemeState::get().color(ColorToken::TextSecondary)),
                                )
                            })
                            .tab("tab3", "Notifications", || {
                                div().px(10.0).bg_surface_elevated().w_full().h_full().items_center().child(
                                    text("Configure your notification preferences.")
                                        .size(14.0)
                                        .color(ThemeState::get().color(ColorToken::TextSecondary)),
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
                                    .tab("a", "First", || div())
                                    .tab("b", "Second", || div())
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
                                    .tab("x", "Overview", || div())
                                    .tab("y", "Details", || div())
                            }),
                    ),
            ),
    )
}

// ============================================================================
// TOAST SECTION
// ============================================================================

fn toast_section(_ctx: &WindowedContext) -> impl ElementBuilder {
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
            ),
    )
}

fn misc_section() -> impl ElementBuilder {
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
                                .size(14.0)
                                .color(theme.color(ColorToken::TextSecondary)),
                        )
                        .child(cn::separator().w(100.0))
                        .child(
                            text("Right content")
                                .size(14.0)
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
