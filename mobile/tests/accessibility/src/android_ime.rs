use super::*;
use jni::{
    JNIEnv,
    objects::JClass,
    sys::{jboolean, jint, jlong, jstring},
};
use std::sync::OnceLock;

static INPUT: OnceLock<SharedTextInputData> = OnceLock::new();

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_blinc_NativeImeTests_nativeEmojiCovered(
    _env: JNIEnv,
    _class: JClass,
) -> jint {
    blinc_test_emoji_covered() as jint
}

#[unsafe(no_mangle)]
fn android_main(app: android_activity::AndroidApp) {
    blinc_platform_android::init_android_native_bridge(&app).expect("native bridge");
    ThemeState::init(HybridTheme::bundle(), ColorScheme::Dark);
    let input = INPUT.get_or_init(text_input_data).clone();
    let styles = std::sync::Once::new();
    blinc::app::AndroidApp::run(app, move |ctx| {
        styles.call_once(|| {
            ctx.add_css(blinc::cn::cn_styles::CN_STYLES);
            ctx.set_css("ime-fixture", include_str!("../fixture.css"))
                .expect("fixture CSS");
            ctx.set_css(
                "ime-platform",
                include_str!("../../ime/android/fixture.css"),
            )
            .expect("fixture insets");
        });
        fixture(ctx, &input)
    })
    .expect("Android fixture");
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_blinc_NativeImeTests_nativeFocus(
    _env: JNIEnv,
    _class: JClass,
) -> jboolean {
    let Some(input) = INPUT.get() else { return 0 };
    blinc::layout::widgets::text_input::focus_text_input_deferred(input);
    1
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_blinc_NativeImeTests_nativeText(
    env: JNIEnv,
    _class: JClass,
) -> jstring {
    let value = INPUT
        .get()
        .map(|input| input.lock().unwrap().value.clone())
        .unwrap_or_default();
    env.new_string(value).expect("test string").into_raw()
}

#[unsafe(no_mangle)]
pub extern "system" fn Java_com_blinc_NativeImeTests_nativeChanges(
    _env: JNIEnv,
    _class: JClass,
) -> jlong {
    IME_CHANGES.load(Ordering::Relaxed) as jlong
}
