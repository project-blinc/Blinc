//! blinc_cn Components Demo
//!
//! Showcases all available blinc_cn components in a scrollable grid layout.
//!
//! Run with: cargo run -p blinc_app --example cn_demo --features windowed

use std::os::unix::thread;
use std::thread::{sleep, Thread};
use std::time::Duration;

use blinc_animation::SpringConfig;
use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_cn::prelude::*;
use blinc_core::Color;
use blinc_layout::selector::ScrollRef;
use blinc_layout::widgets::text_input::text_input_data;
use blinc_theme::{ColorToken, ThemeState};

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
    let theme = ThemeState::get();
    let bg = theme.color(ColorToken::Background);

    // Create scroll ref to track scroll position
    let scroll_ref = ctx.use_scroll_ref("main_scroll");

    div()
        .w(ctx.width)
        .h(ctx.height)
        .bg(bg)
        .flex_col()
        .child(header())
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
                        .child(loading_section(ctx))
                        .child(misc_section()),
                ),
        )
}

/// Header with title
fn header() -> impl ElementBuilder {
    let theme = ThemeState::get();
    let surface = theme.color(ColorToken::Surface);
    let text_primary = theme.color(ColorToken::TextPrimary);
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let border = theme.color(ColorToken::Border);

    div()
        .w_full()
        .h(80.0)
        .bg(surface)
        .border_bottom(1.5, border)
        .px(theme.spacing().space_6)
        .flex_row()
        .items_center()
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
    let surface = theme.color(ColorToken::Surface);
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
    let fruit_open = ctx.use_state_keyed("fruit_open", || false);
    let size = ctx.use_state_keyed("size_select", || "medium".to_string());
    let size_open = ctx.use_state_keyed("size_open", || false);
    let disabled_select = ctx.use_state_keyed("disabled_select", || "option1".to_string());
    let disabled_open = ctx.use_state_keyed("disabled_open", || false);

    section_container()
        .child(section_title("Select"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(24.0)
                // Basic select with placeholder
                .child(
                    div().w(200.0).child(
                        cn::select(&fruit, &fruit_open)
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
                        cn::select(&size, &size_open)
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
                        cn::select(&disabled_select, &disabled_open)
                            .label("Disabled")
                            .option("option1", "Option 1")
                            .option("option2", "Option 2")
                            .disabled(true),
                    ),
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

    section_container().child(section_title("Progress")).child(
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
            // on_ready fires once after element is laid out (with built-in 500ms delay)
            .on_ready(move |_| {
                if let Ok(mut value) = progress_for_ready.lock() {
                    value.set_target(PROGRESS_WIDTH * 0.75);
                    tracing::info!("on_ready: animation triggered to 75%");
                }
            }),
    )
}

// ============================================================================
// MISC SECTION (Separator, Label)
// ============================================================================

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
