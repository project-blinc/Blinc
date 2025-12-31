//! Theme System Demo
//!
//! This example demonstrates the Blinc theming system capabilities:
//! - Light/dark mode switching with smooth transitions
//! - Semantic color tokens (primary, secondary, success, error, etc.)
//! - Typography tokens (font sizes, weights)
//! - Spacing tokens (4px-based scale)
//! - Border radius tokens
//! - Platform-native theme detection
//!
//! Run with: cargo run -p blinc_app --example theme_demo --features windowed

use blinc_app::prelude::*;
use blinc_app::windowed::{WindowedApp, WindowedContext};
use blinc_layout::stateful::ButtonState;
use blinc_theme::{ColorToken, ThemeState};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let config = WindowConfig {
        title: "Blinc Theme Demo".to_string(),
        width: 1000,
        height: 700,
        resizable: true,
        ..Default::default()
    };

    WindowedApp::run(config, |ctx| build_ui(ctx))
}

fn build_ui(ctx: &WindowedContext) -> impl ElementBuilder {
    let theme = ThemeState::get();
    let bg = theme.color(ColorToken::Background);

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
                .p(theme.spacing().space_6)
                .child(
                    div()
                        .w_full()
                        .flex_col()
                        .flex_1()
                        .justify_center()
                        .gap(theme.spacing().space_6)
                        .child(color_palette_section())
                        .child(typography_section())
                        .child(spacing_section())
                        .child(component_showcase(ctx)),
                ),
        )
}

/// Header with theme toggle
fn header(ctx: &WindowedContext) -> impl ElementBuilder {
    let theme = ThemeState::get();
    let surface = theme.color(ColorToken::Surface);
    let text_primary = theme.color(ColorToken::TextPrimary);
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let border = theme.color(ColorToken::Border);
    let scheme = theme.scheme();

    let scheme_label = match scheme {
        ColorScheme::Light => "Light Mode",
        ColorScheme::Dark => "Dark Mode",
    };

    let toggle_label = match scheme {
        ColorScheme::Light => "Switch to Dark",
        ColorScheme::Dark => "Switch to Light",
    };

    div()
        .w_full()
        .h(80.0)
        .bg(surface)
        .border(1.5, border)
        .px(theme.spacing().space_6)
        .flex_row()
        .items_center()
        .justify_between()
        .child(
            div()
                .flex_col()
                .gap(2.0)
                .child(
                    text("Blinc Theme System")
                        .size(theme.typography().text_2xl)
                        .weight(FontWeight::Bold)
                        .color(text_primary),
                )
                .child(
                    text(scheme_label)
                        .size(theme.typography().text_sm)
                        .color(text_secondary),
                ),
        )
        .child(theme_toggle_button(ctx, toggle_label))
}

/// Theme toggle button
fn theme_toggle_button(ctx: &WindowedContext, label: &str) -> impl ElementBuilder {
    let theme = ThemeState::get();
    let radius = theme.radii().radius_lg;

    let handle = ctx.use_state_for("theme_toggle", ButtonState::Idle);

    stateful(handle)
        .w_fit()
        .px(theme.spacing().space_2)
        .py(theme.spacing().space_1)
        .rounded(radius)
        .bg(theme.color(ColorToken::Primary))
        .items_center()
        .border(1.0, theme.color(ColorToken::Border))
        .on_state(|state, container| {
            // Fetch colors inside the callback so they update with theme changes
            let theme = ThemeState::get();
            let scheme = theme.scheme();

            let primary = theme.color(ColorToken::Primary);
            let primary_hover = theme.color(ColorToken::PrimaryHover);

            match state {
                ButtonState::Idle => {
                    container.set_bg(primary);
                }
                ButtonState::Hovered => {
                    container.set_bg(primary_hover);
                    container.set_transform(Transform::scale(1.02, 1.02));
                }
                ButtonState::Pressed => {
                    container.set_bg(primary);
                    container.set_transform(Transform::scale(0.98, 0.98));
                }
                ButtonState::Disabled => {
                    container.set_bg(Color::GRAY);
                }
            }

            let toggle_label = match scheme {
                ColorScheme::Light => "Switch to Dark",
                ColorScheme::Dark => "Switch to Light",
            };

            container.merge(
                div().child(
                    text(toggle_label)
                        .size(theme.typography().text_sm)
                        .weight(FontWeight::Medium)
                        .color(Color::WHITE),
                ),
            );
        })
        .on_click(|_| {
            ThemeState::get().toggle_scheme();
        })
        .child(
            text(label)
                .size(theme.typography().text_sm)
                .weight(FontWeight::Medium)
                .color(Color::WHITE),
        )
}

/// Color palette showcase
fn color_palette_section() -> impl ElementBuilder {
    let theme = ThemeState::get();
    let surface = theme.color(ColorToken::Surface);
    let text_primary = theme.color(ColorToken::TextPrimary);
    let border = theme.color(ColorToken::Border);
    let radius = theme.radii().radius_xl;

    div()
        .bg(surface)
        .rounded(radius)
        .border(1.5, border)
        .p(theme.spacing().space_5)
        .flex_col()
        .gap(theme.spacing().space_4)
        .child(section_title("Color Tokens"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(theme.spacing().space_3)
                // Brand colors
                .child(color_swatch("Primary", ColorToken::Primary))
                .child(color_swatch("Primary Hover", ColorToken::PrimaryHover))
                .child(color_swatch("Secondary", ColorToken::Secondary))
                .child(color_swatch("Accent", ColorToken::Accent)),
        )
        .child(
            text("Semantic Colors")
                .size(theme.typography().text_base)
                .weight(FontWeight::Medium)
                .color(text_primary),
        )
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(theme.spacing().space_3)
                .child(color_swatch("Success", ColorToken::Success))
                .child(color_swatch("Warning", ColorToken::Warning))
                .child(color_swatch("Error", ColorToken::Error))
                .child(color_swatch("Info", ColorToken::Info)),
        )
        .child(
            text("Surface Colors")
                .size(theme.typography().text_base)
                .weight(FontWeight::Medium)
                .color(text_primary),
        )
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(theme.spacing().space_3)
                .child(color_swatch("Background", ColorToken::Background))
                .child(color_swatch("Surface", ColorToken::Surface))
                .child(color_swatch(
                    "Surface Elevated",
                    ColorToken::SurfaceElevated,
                ))
                .child(color_swatch("Border", ColorToken::Border)),
        )
        .child(
            text("Text Colors")
                .size(theme.typography().text_base)
                .weight(FontWeight::Medium)
                .color(text_primary),
        )
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(theme.spacing().space_3)
                .child(color_swatch("Text Primary", ColorToken::TextPrimary))
                .child(color_swatch("Text Secondary", ColorToken::TextSecondary))
                .child(color_swatch("Text Tertiary", ColorToken::TextTertiary))
                .child(color_swatch("Text Link", ColorToken::TextLink)),
        )
}

/// Single color swatch
fn color_swatch(name: &str, token: ColorToken) -> impl ElementBuilder {
    let theme = ThemeState::get();
    let color = theme.color(token);
    let text_primary = theme.color(ColorToken::TextPrimary);
    let border = theme.color(ColorToken::Border);
    let radius = theme.radii().radius_md;

    div()
        .flex_col()
        .gap(theme.spacing().space_1)
        .child(
            div()
                .w(80.0)
                .h(50.0)
                .bg(color)
                .rounded(radius)
                .border(1.0, border),
        )
        .child(
            text(name)
                .size(theme.typography().text_xs)
                .color(text_primary),
        )
}

/// Typography showcase
fn typography_section() -> impl ElementBuilder {
    let theme = ThemeState::get();
    let surface = theme.color(ColorToken::Surface);
    let text_primary = theme.color(ColorToken::TextPrimary);
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let border = theme.color(ColorToken::Border);
    let radius = theme.radii().radius_xl;
    let typo = theme.typography();

    div()
        .bg(surface)
        .rounded(radius)
        .border(1.5, border)
        .p(theme.spacing().space_5)
        .flex_col()
        .gap(theme.spacing().space_4)
        .child(section_title("Typography Scale"))
        .child(
            div()
                .flex_col()
                .gap(theme.spacing().space_2)
                .child(typo_sample("text_5xl", typo.text_5xl, text_primary))
                .child(typo_sample("text_4xl", typo.text_4xl, text_primary))
                .child(typo_sample("text_3xl", typo.text_3xl, text_primary))
                .child(typo_sample("text_2xl", typo.text_2xl, text_primary))
                .child(typo_sample("text_xl", typo.text_xl, text_primary))
                .child(typo_sample("text_lg", typo.text_lg, text_primary))
                .child(typo_sample("text_base", typo.text_base, text_primary))
                .child(typo_sample("text_sm", typo.text_sm, text_secondary))
                .child(typo_sample("text_xs", typo.text_xs, text_secondary)),
        )
}

/// Typography sample row
fn typo_sample(name: &str, size: f32, color: Color) -> impl ElementBuilder {
    let theme = ThemeState::get();
    let text_tertiary = theme.color(ColorToken::TextTertiary);

    div()
        .flex_row()
        .items_center()
        .gap(theme.spacing().space_4)
        .child(
            div().w(80.0).child(
                text(name)
                    .size(theme.typography().text_sm)
                    .color(text_tertiary),
            ),
        )
        .child(
            text(&format!("The quick brown fox ({:.0}px)", size))
                .size(size)
                .color(color),
        )
}

/// Spacing showcase
fn spacing_section() -> impl ElementBuilder {
    let theme = ThemeState::get();
    let surface = theme.color(ColorToken::Surface);
    let border = theme.color(ColorToken::Border);
    let primary = theme.color(ColorToken::Primary);
    let radius = theme.radii().radius_xl;
    let spacing = theme.spacing();

    div()
        .bg(surface)
        .rounded(radius)
        .border(1.5, border)
        .p(theme.spacing().space_5)
        .flex_col()
        .gap(theme.spacing().space_4)
        .child(section_title("Spacing Scale (4px base)"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .items_end()
                .gap(theme.spacing().space_2)
                .child(spacing_block("1", spacing.space_1, primary))
                .child(spacing_block("2", spacing.space_2, primary))
                .child(spacing_block("3", spacing.space_3, primary))
                .child(spacing_block("4", spacing.space_4, primary))
                .child(spacing_block("5", spacing.space_5, primary))
                .child(spacing_block("6", spacing.space_6, primary))
                .child(spacing_block("8", spacing.space_8, primary))
                .child(spacing_block("10", spacing.space_10, primary))
                .child(spacing_block("12", spacing.space_12, primary)),
        )
}

/// Spacing visualization block
fn spacing_block(name: &str, size: f32, color: Color) -> impl ElementBuilder {
    let theme = ThemeState::get();
    let text_primary = theme.color(ColorToken::TextPrimary);
    let radius = theme.radii().radius_sm;

    div()
        .flex_col()
        .items_center()
        .gap(4.0)
        .child(div().w(size).h(size).bg(color).rounded(radius))
        .child(
            text(name)
                .size(theme.typography().text_xs)
                .color(text_primary),
        )
}

/// Component showcase with themed elements
fn component_showcase(ctx: &WindowedContext) -> impl ElementBuilder {
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
        .child(section_title("Component Examples"))
        .child(
            div()
                .flex_row()
                .flex_wrap()
                .gap(theme.spacing().space_3)
                .child(themed_button(ctx, "Primary", ColorToken::Primary))
                .child(themed_button(ctx, "Secondary", ColorToken::Secondary))
                .child(themed_button(ctx, "Success", ColorToken::Success))
                .child(themed_button(ctx, "Error", ColorToken::Error)),
        )
        .child(themed_card())
        .child(themed_input_preview())
}

/// Themed button component
fn themed_button(
    ctx: &WindowedContext,
    label: &str,
    color_token: ColorToken,
) -> impl ElementBuilder {
    let theme = ThemeState::get();
    let color = theme.color(color_token);
    let radius = theme.radii().radius_lg;

    let handle = ctx.use_state_for(format!("btn_{}", label), ButtonState::Idle);

    stateful(handle)
        .px(theme.spacing().space_4)
        .py(theme.spacing().space_2_5)
        .rounded(radius)
        .bg(color)
        .on_state(move |state, div| {
            // Fetch color inside callback for theme reactivity
            let base = ThemeState::get().color(color_token);
            match state {
                ButtonState::Idle => {
                    div.set_bg(base);
                }
                ButtonState::Hovered => {
                    let hover = Color::rgba(
                        (base.r * 1.1).min(1.0),
                        (base.g * 1.1).min(1.0),
                        (base.b * 1.1).min(1.0),
                        base.a,
                    );
                    div.set_bg(hover);
                    div.set_transform(Transform::scale(1.02, 1.02));
                }
                ButtonState::Pressed => {
                    let pressed = Color::rgba(base.r * 0.9, base.g * 0.9, base.b * 0.9, base.a);
                    div.set_bg(pressed);
                    div.set_transform(Transform::scale(0.98, 0.98));
                }
                ButtonState::Disabled => {
                    div.set_bg(Color::GRAY);
                }
            }
        })
        .on_click({
            let label = label.to_string();
            move |_| tracing::info!("{} button clicked", label)
        })
        .child(
            text(label)
                .size(theme.typography().text_sm)
                .weight(FontWeight::Medium)
                .color(Color::WHITE),
        )
}

/// Themed card component
fn themed_card() -> impl ElementBuilder {
    let theme = ThemeState::get();
    let surface_elevated = theme.color(ColorToken::SurfaceElevated);
    let text_primary = theme.color(ColorToken::TextPrimary);
    let text_secondary = theme.color(ColorToken::TextSecondary);
    let border = theme.color(ColorToken::Border);
    let radius = theme.radii().radius_lg;

    div()
        .bg(surface_elevated)
        .rounded(radius)
        .border(1.0, border)
        .p(theme.spacing().space_4)
        .flex_col()
        .gap(theme.spacing().space_2)
        .shadow_md()
        .child(
            text("Themed Card")
                .size(theme.typography().text_lg)
                .weight(FontWeight::SemiBold)
                .color(text_primary),
        )
        .child(
            text("This card uses semantic color tokens from the current theme. It automatically adapts to light and dark modes.")
                .size(theme.typography().text_sm)
                .color(text_secondary)
        )
}

/// Preview of themed input styling
fn themed_input_preview() -> impl ElementBuilder {
    let theme = ThemeState::get();
    let input_bg = theme.color(ColorToken::InputBg);
    let border = theme.color(ColorToken::Border);
    let border_focus = theme.color(ColorToken::BorderFocus);
    let text_primary = theme.color(ColorToken::TextPrimary);
    let text_tertiary = theme.color(ColorToken::TextTertiary);
    let radius = theme.radii().radius_lg;

    div()
        .flex_col()
        .gap(theme.spacing().space_2)
        .child(
            text("Input Preview")
                .size(theme.typography().text_sm)
                .weight(FontWeight::Medium)
                .color(text_primary),
        )
        .child(
            div()
                .flex_row()
                .gap(theme.spacing().space_3)
                .child(
                    // Normal state
                    div()
                        .w(200.0)
                        .h(40.0)
                        .bg(input_bg)
                        .rounded(radius)
                        .border(1.5, border)
                        .px(theme.spacing().space_3)
                        .flex_row()
                        .items_center()
                        .child(
                            text("Normal input...")
                                .size(theme.typography().text_sm)
                                .color(text_tertiary),
                        ),
                )
                .child(
                    // Focused state
                    div()
                        .w(200.0)
                        .h(40.0)
                        .bg(input_bg)
                        .rounded(radius)
                        .border(2.0, border_focus)
                        .px(theme.spacing().space_3)
                        .flex_row()
                        .items_center()
                        .child(
                            text("Focused input")
                                .size(theme.typography().text_sm)
                                .color(text_primary),
                        ),
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
