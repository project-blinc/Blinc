//! Conversions and builder invocation shared by the windowed runner.

use super::*;

/// Convert platform mouse button to layout mouse button
#[cfg(all(feature = "windowed", not(target_os = "android")))]
pub(super) fn convert_mouse_button(button: blinc_platform::MouseButton) -> MouseButton {
    match button {
        blinc_platform::MouseButton::Left => MouseButton::Left,
        blinc_platform::MouseButton::Right => MouseButton::Right,
        blinc_platform::MouseButton::Middle => MouseButton::Middle,
        blinc_platform::MouseButton::Back => MouseButton::Back,
        blinc_platform::MouseButton::Forward => MouseButton::Forward,
        blinc_platform::MouseButton::Other(n) => MouseButton::Other(n),
    }
}

/// Convert layout cursor style to platform cursor
#[cfg(all(feature = "windowed", not(target_os = "android")))]
pub(super) fn convert_cursor_style(cursor: CursorStyle) -> blinc_platform::Cursor {
    match cursor {
        CursorStyle::Default => blinc_platform::Cursor::Default,
        CursorStyle::Pointer => blinc_platform::Cursor::Pointer,
        CursorStyle::Text => blinc_platform::Cursor::Text,
        CursorStyle::Crosshair => blinc_platform::Cursor::Crosshair,
        CursorStyle::Move => blinc_platform::Cursor::Move,
        CursorStyle::NotAllowed => blinc_platform::Cursor::NotAllowed,
        CursorStyle::ResizeNS => blinc_platform::Cursor::ResizeNS,
        CursorStyle::ResizeEW => blinc_platform::Cursor::ResizeEW,
        CursorStyle::ResizeNESW => blinc_platform::Cursor::ResizeNESW,
        CursorStyle::ResizeNWSE => blinc_platform::Cursor::ResizeNWSE,
        CursorStyle::Grab => blinc_platform::Cursor::Grab,
        CursorStyle::Grabbing => blinc_platform::Cursor::Grabbing,
        CursorStyle::Wait => blinc_platform::Cursor::Wait,
        CursorStyle::Progress => blinc_platform::Cursor::Progress,
        CursorStyle::None => blinc_platform::Cursor::None,
    }
}

/// Convenience function to run a windowed ws.app with default configuration
#[cfg(all(feature = "windowed", not(target_os = "android")))]
pub fn run_windowed<F, E>(ui_builder: F) -> Result<()>
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder + 'static,
{
    WindowedApp::run(WindowConfig::default(), ui_builder)
}

/// Convenience function to run a windowed ws.app with a title
#[cfg(all(feature = "windowed", not(target_os = "android")))]
pub fn run_windowed_with_title<F, E>(title: &str, ui_builder: F) -> Result<()>
where
    F: FnMut(&mut WindowedContext) -> E + 'static,
    E: ElementBuilder + 'static,
{
    let config = WindowConfig {
        title: title.to_string(),
        ..Default::default()
    };
    WindowedApp::run(config, ui_builder)
}

/// Invoke the user's primary-window UI builder closure, optionally
/// routing the call through `subsecond::call` so changes to the
/// closure body can be hot-patched in development. Without the
/// `hot-reload` feature this is a direct call and gets inlined away.
///
/// Subsecond itself is gated on `debug_assertions` upstream, so even
/// with the `hot-reload` feature on a release build still pays no
/// runtime cost.
#[cfg(all(feature = "windowed", not(target_os = "android")))]
#[inline]
pub(super) fn invoke_ui_builder<F, E>(ui_builder: &mut F, ctx: &mut WindowedContext) -> E
where
    F: FnMut(&mut WindowedContext) -> E,
    E: ElementBuilder,
{
    #[cfg(feature = "hot-reload")]
    {
        // `subsecond::call` takes a `FnOnce() -> O`. The captured
        // `&mut` borrows live only for this call, so the closure
        // satisfies `FnOnce` regardless of `F`'s `FnMut` bound.
        subsecond::call(move || ui_builder(ctx))
    }
    #[cfg(not(feature = "hot-reload"))]
    {
        ui_builder(ctx)
    }
}

/// Secondary-window variant of [`invoke_ui_builder`]. Secondary
/// windows store their builder as a boxed `WindowBuilder` returning
/// `Div` directly (rather than the generic `E: ElementBuilder` of
/// the primary), so they need a separate helper.
#[cfg(all(feature = "windowed", not(target_os = "android")))]
#[inline]
pub(super) fn invoke_window_builder(builder: &mut WindowBuilder, ctx: &mut WindowedContext) -> Div {
    #[cfg(feature = "hot-reload")]
    {
        subsecond::call(move || builder(ctx))
    }
    #[cfg(not(feature = "hot-reload"))]
    {
        builder(ctx)
    }
}
