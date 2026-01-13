//! JNI utilities for Android platform
//!
//! Provides helper functions for accessing Android APIs via JNI.

#[cfg(target_os = "android")]
use android_activity::AndroidApp;

#[cfg(target_os = "android")]
use jni::{objects::JValue, JNIEnv, JavaVM};

#[cfg(target_os = "android")]
use tracing::{debug, warn};

/// Get the display density (scale factor) from Android DisplayMetrics
///
/// This queries `Resources.getDisplayMetrics().density` via JNI.
/// Returns 1.0 if the query fails.
#[cfg(target_os = "android")]
pub fn get_display_density(app: &AndroidApp) -> f64 {
    // Get the JavaVM from the app
    let vm = match unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut _) } {
        Ok(vm) => vm,
        Err(e) => {
            warn!("Failed to get JavaVM: {:?}", e);
            return 1.0;
        }
    };

    // Attach to the current thread
    let mut env = match vm.attach_current_thread() {
        Ok(env) => env,
        Err(e) => {
            warn!("Failed to attach JNI thread: {:?}", e);
            return 1.0;
        }
    };

    // Get the activity object
    let activity = match app.activity_as_ptr() {
        ptr if !ptr.is_null() => unsafe { jni::objects::JObject::from_raw(ptr as *mut _) },
        _ => {
            warn!("Activity pointer is null");
            return 1.0;
        }
    };

    // Call activity.getResources().getDisplayMetrics().density
    let density = get_density_from_activity(&mut env, &activity).unwrap_or_else(|e| {
        warn!("Failed to get density: {:?}", e);
        1.0
    });

    debug!("Display density: {}", density);
    density
}

/// Internal helper to get density via JNI calls
#[cfg(target_os = "android")]
fn get_density_from_activity(
    env: &mut JNIEnv,
    activity: &jni::objects::JObject,
) -> Result<f64, jni::errors::Error> {
    // Get Resources: activity.getResources()
    let resources = env
        .call_method(
            activity,
            "getResources",
            "()Landroid/content/res/Resources;",
            &[],
        )?
        .l()?;

    // Get DisplayMetrics: resources.getDisplayMetrics()
    let display_metrics = env
        .call_method(
            &resources,
            "getDisplayMetrics",
            "()Landroid/util/DisplayMetrics;",
            &[],
        )?
        .l()?;

    // Get density field: displayMetrics.density
    let density = env.get_field(&display_metrics, "density", "F")?.f()?;

    Ok(density as f64)
}

/// Get the screen DPI (dots per inch) from Android DisplayMetrics
///
/// This queries `Resources.getDisplayMetrics().densityDpi` via JNI.
/// Returns 160 (mdpi baseline) if the query fails.
#[cfg(target_os = "android")]
pub fn get_display_dpi(app: &AndroidApp) -> i32 {
    let vm = match unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut _) } {
        Ok(vm) => vm,
        Err(e) => {
            warn!("Failed to get JavaVM: {:?}", e);
            return 160;
        }
    };

    let mut env = match vm.attach_current_thread() {
        Ok(env) => env,
        Err(e) => {
            warn!("Failed to attach JNI thread: {:?}", e);
            return 160;
        }
    };

    let activity = match app.activity_as_ptr() {
        ptr if !ptr.is_null() => unsafe { jni::objects::JObject::from_raw(ptr as *mut _) },
        _ => {
            warn!("Activity pointer is null");
            return 160;
        }
    };

    get_dpi_from_activity(&mut env, &activity).unwrap_or_else(|e| {
        warn!("Failed to get DPI: {:?}", e);
        160
    })
}

#[cfg(target_os = "android")]
fn get_dpi_from_activity(
    env: &mut JNIEnv,
    activity: &jni::objects::JObject,
) -> Result<i32, jni::errors::Error> {
    let resources = env
        .call_method(
            activity,
            "getResources",
            "()Landroid/content/res/Resources;",
            &[],
        )?
        .l()?;

    let display_metrics = env
        .call_method(
            &resources,
            "getDisplayMetrics",
            "()Landroid/util/DisplayMetrics;",
            &[],
        )?
        .l()?;

    let dpi = env.get_field(&display_metrics, "densityDpi", "I")?.i()?;

    Ok(dpi)
}

/// Check if the system is in dark mode
///
/// This queries the UI mode via `Configuration.uiMode` and checks for `UI_MODE_NIGHT_YES`.
#[cfg(target_os = "android")]
pub fn is_dark_mode(app: &AndroidApp) -> bool {
    let vm = match unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut _) } {
        Ok(vm) => vm,
        Err(e) => {
            warn!("Failed to get JavaVM: {:?}", e);
            return false;
        }
    };

    let mut env = match vm.attach_current_thread() {
        Ok(env) => env,
        Err(e) => {
            warn!("Failed to attach JNI thread: {:?}", e);
            return false;
        }
    };

    let activity = match app.activity_as_ptr() {
        ptr if !ptr.is_null() => unsafe { jni::objects::JObject::from_raw(ptr as *mut _) },
        _ => {
            warn!("Activity pointer is null");
            return false;
        }
    };

    check_dark_mode(&mut env, &activity).unwrap_or_else(|e| {
        warn!("Failed to check dark mode: {:?}", e);
        false
    })
}

#[cfg(target_os = "android")]
fn check_dark_mode(
    env: &mut JNIEnv,
    activity: &jni::objects::JObject,
) -> Result<bool, jni::errors::Error> {
    // Get Resources
    let resources = env
        .call_method(
            activity,
            "getResources",
            "()Landroid/content/res/Resources;",
            &[],
        )?
        .l()?;

    // Get Configuration: resources.getConfiguration()
    let configuration = env
        .call_method(
            &resources,
            "getConfiguration",
            "()Landroid/content/res/Configuration;",
            &[],
        )?
        .l()?;

    // Get uiMode field
    let ui_mode = env.get_field(&configuration, "uiMode", "I")?.i()?;

    // UI_MODE_NIGHT_MASK = 0x30
    // UI_MODE_NIGHT_YES = 0x20
    const UI_MODE_NIGHT_MASK: i32 = 0x30;
    const UI_MODE_NIGHT_YES: i32 = 0x20;

    Ok((ui_mode & UI_MODE_NIGHT_MASK) == UI_MODE_NIGHT_YES)
}

// Placeholder implementations for non-Android builds
#[cfg(not(target_os = "android"))]
pub fn get_display_density(_app: &()) -> f64 {
    1.0
}

#[cfg(not(target_os = "android"))]
pub fn get_display_dpi(_app: &()) -> i32 {
    160
}

#[cfg(not(target_os = "android"))]
pub fn is_dark_mode(_app: &()) -> bool {
    false
}
