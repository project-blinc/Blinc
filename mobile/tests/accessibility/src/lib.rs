#![cfg(any(target_os = "ios", target_os = "android"))]

use blinc::app::windowed::WindowedContext;
use blinc::layout::binding::IntoReactive;
use blinc::layout::widgets::text_input::{SharedTextInputData, text_input_data};
use blinc::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static IME_CHANGES: AtomicUsize = AtomicUsize::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn blinc_test_ime_changes() -> usize {
    IME_CHANGES.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn blinc_test_emoji_covered() -> u8 {
    let registry = blinc::text::global_font_registry();
    let font = match registry
        .lock()
        .unwrap()
        .load_generic(blinc::text::GenericFont::Emoji)
    {
        Ok(font) => font,
        Err(error) => {
            tracing::warn!(?error, "Emoji font lookup failed");
            return 2;
        }
    };
    let Some(glyph) = font.glyph_id('🦊').filter(|glyph| *glyph != 0) else {
        return 3;
    };
    match blinc::text::rasterizer::GlyphRasterizer::new().rasterize_color(&font, glyph, 28.0) {
        Ok(image) => {
            if image.format == blinc::text::rasterizer::GlyphFormat::Rgba
                && image
                    .bitmap
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel[3] > 0 && (pixel[0] != pixel[1] || pixel[1] != pixel[2]))
            {
                1
            } else {
                eprintln!(
                    "Emoji raster probe: family={}, face={}, glyph={}, bitmap={}x{}, format={:?}",
                    font.family_name(),
                    font.face_index(),
                    glyph,
                    image.width,
                    image.height,
                    image.format
                );
                5
            }
        }
        Err(error) => {
            tracing::warn!(?error, "Emoji rasterization failed");
            4
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn blinc_test_cjk_covered() -> u8 {
    use blinc::text::{FallbackFaces, GlyphRasterizer, TextShaper};
    let registry = blinc::text::global_font_registry();
    let mut registry = registry.lock().unwrap();
    let Ok(primary) = registry.load_font("Arial") else {
        return 0;
    };
    let faces = FallbackFaces::resolve(&mut registry, &primary, "你好");
    let mut shaped = TextShaper::new().shape("你好", &primary, 28.0);
    faces.apply(&mut shaped, &primary, 28.0);
    u8::from(
        !shaped.glyphs.is_empty()
            && shaped.glyphs.iter().all(|glyph| {
                let face = faces.face(glyph.face).unwrap_or(&primary);
                glyph.glyph_id != 0
                    && GlyphRasterizer::new()
                        .rasterize(face, glyph.glyph_id, 28.0)
                        .is_ok_and(|image| image.bitmap.iter().any(|&byte| byte != 0))
            }),
    )
}

#[unsafe(no_mangle)]
#[cfg(target_os = "ios")]
pub extern "C" fn ios_app_init() {
    ThemeState::init(HybridTheme::bundle(), ColorScheme::Dark);
    let input = text_input_data();
    let styles = std::sync::Once::new();
    blinc::app::ios::register_rust_ui_builder(move |ctx| {
        styles.call_once(|| {
            ctx.add_css(blinc::cn::cn_styles::CN_STYLES);
            ctx.set_css("accessibility-fixture", include_str!("../fixture.css"))
                .expect("valid fixture CSS");
        });
        fixture(ctx, &input)
    });
}

#[cfg(target_os = "android")]
mod android_ime;

fn fixture(ctx: &mut WindowedContext, input: &SharedTextInputData) -> Div {
    let count = ctx.use_signal_keyed("a11y-count", || 0u32);
    let checked = ctx.use_signal_keyed("a11y-checked", || false);
    let volume = ctx.use_signal_keyed("a11y-volume", || 50.0f32);
    let caption = computed(move |graph| format!("Count: {}", graph.get(count).unwrap_or(0)));
    div()
        .class("a11y-fixture")
        .child(text("Accessibility fixture").class("a11y-title"))
        .child(Text::bound(caption.into_reactive()).class("a11y-counter"))
        .child(
            blinc::cn::button("Increment")
                .on_click(move |_| count.set(count.get() + 1))
                .build_component()
                .id("increment"),
        )
        .child(
            blinc::cn::input(input)
                .label("Name")
                .placeholder("Your name")
                .on_change(|_| {
                    IME_CHANGES.fetch_add(1, Ordering::Relaxed);
                })
                .on_submit(move |_| count.set(count.get() + 10)),
        )
        .child(blinc::cn::checkbox(checked).label("Enable notifications"))
        .child(
            blinc::cn::slider(volume)
                .max(100.0)
                .step(5.0)
                .label("Volume")
                .show_value(),
        )
        .child(blinc::cn::button("Unavailable").disabled(true))
}
