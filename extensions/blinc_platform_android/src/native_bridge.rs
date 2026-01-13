//! Android Native Bridge Adapter
//!
//! JNI-based implementation of `PlatformAdapter` for calling Kotlin-registered
//! native functions from Rust.
//!
//! # Architecture
//!
//! ```text
//! Rust: native_call("device", "get_battery")
//!              │
//!              ▼
//! AndroidNativeBridgeAdapter.call()
//!              │
//!              ▼ (JNI)
//! Kotlin: BlincNativeBridge.callNative(ns, name, argsJson)
//!              │
//!              ▼
//! Kotlin handler executes, returns JSON result
//! ```
//!
//! # Kotlin Side
//!
//! ```kotlin
//! object BlincNativeBridge {
//!     private val handlers = mutableMapOf<String, MutableMap<String, (JSONArray) -> Any?>>()
//!
//!     fun register(namespace: String, name: String, handler: (JSONArray) -> Any?) {
//!         handlers.getOrPut(namespace) { mutableMapOf() }[name] = handler
//!     }
//!
//!     @JvmStatic
//!     fun callNative(namespace: String, name: String, argsJson: String): String {
//!         val handler = handlers[namespace]?.get(name)
//!             ?: return """{"success":false,"errorType":"NotRegistered","errorMessage":"$namespace.$name not found"}"""
//!         val args = JSONArray(argsJson)
//!         val result = handler(args)
//!         return """{"success":true,"value":${toJson(result)}}"""
//!     }
//! }
//! ```

#[cfg(target_os = "android")]
use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
#[cfg(target_os = "android")]
use jni::sys::jstring;
#[cfg(target_os = "android")]
use jni::{JNIEnv, JavaVM};

#[cfg(target_os = "android")]
use std::sync::Arc;

#[cfg(target_os = "android")]
use blinc_core::native_bridge::{
    parse_native_result_json, NativeBridgeError, NativeBridgeState, NativeResult, NativeValue,
    PlatformAdapter,
};

#[cfg(target_os = "android")]
use tracing::{debug, error, warn};

/// Android platform adapter using JNI to call Kotlin handlers
#[cfg(target_os = "android")]
pub struct AndroidNativeBridgeAdapter {
    /// Cached JavaVM for thread attachment
    vm: JavaVM,
    /// Global reference to BlincNativeBridge class
    bridge_class: GlobalRef,
}

#[cfg(target_os = "android")]
impl AndroidNativeBridgeAdapter {
    /// Create a new Android native bridge adapter
    ///
    /// # Arguments
    /// * `vm` - JavaVM from android-activity or JNI_OnLoad
    /// * `env` - JNI environment for class lookup
    ///
    /// # Returns
    /// * Adapter instance or JNI error
    pub fn new(vm: JavaVM, env: &mut JNIEnv) -> Result<Self, jni::errors::Error> {
        // Find the BlincNativeBridge class
        let class = env.find_class("com/blinc/BlincNativeBridge")?;
        let bridge_class = env.new_global_ref(class)?;

        debug!("AndroidNativeBridgeAdapter initialized");

        Ok(Self { vm, bridge_class })
    }

    /// Create from android-activity's AndroidApp
    ///
    /// # Arguments
    /// * `app` - AndroidApp from android_main
    pub fn from_android_app(
        app: &android_activity::AndroidApp,
    ) -> Result<Self, NativeBridgeError> {
        // Get JavaVM from AndroidApp
        let vm = unsafe { JavaVM::from_raw(app.vm_as_ptr() as *mut _) }
            .map_err(|e| NativeBridgeError::PlatformError(format!("Failed to get JavaVM: {}", e)))?;

        let mut env = vm
            .attach_current_thread()
            .map_err(|e| NativeBridgeError::PlatformError(format!("Failed to attach thread: {}", e)))?;

        Self::new(vm, &mut env)
            .map_err(|e| NativeBridgeError::PlatformError(format!("JNI init failed: {}", e)))
    }

    /// Serialize NativeValue arguments to JSON
    fn args_to_json(args: &[NativeValue]) -> String {
        let mut parts = Vec::with_capacity(args.len());
        for arg in args {
            let json = match arg {
                NativeValue::Void => "null".to_string(),
                NativeValue::Bool(v) => v.to_string(),
                NativeValue::Int32(v) => v.to_string(),
                NativeValue::Int64(v) => v.to_string(),
                NativeValue::Float32(v) => v.to_string(),
                NativeValue::Float64(v) => v.to_string(),
                NativeValue::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
                NativeValue::Bytes(b) => {
                    // Base64 encode bytes
                    use base64::{engine::general_purpose::STANDARD, Engine};
                    format!("\"{}\"", STANDARD.encode(b))
                }
                NativeValue::Json(j) => j.clone(),
            };
            parts.push(json);
        }
        format!("[{}]", parts.join(","))
    }
}

#[cfg(target_os = "android")]
impl PlatformAdapter for AndroidNativeBridgeAdapter {
    fn call(
        &self,
        namespace: &str,
        name: &str,
        args: Vec<NativeValue>,
    ) -> NativeResult<NativeValue> {
        debug!("Android native call: {}.{}", namespace, name);

        // Attach current thread to JVM
        let mut env = self
            .vm
            .attach_current_thread()
            .map_err(|e| NativeBridgeError::PlatformError(format!("JNI attach failed: {}", e)))?;

        // Create Java strings for arguments
        let ns_jstring = env
            .new_string(namespace)
            .map_err(|e| NativeBridgeError::PlatformError(format!("Failed to create namespace string: {}", e)))?;

        let name_jstring = env
            .new_string(name)
            .map_err(|e| NativeBridgeError::PlatformError(format!("Failed to create name string: {}", e)))?;

        // Serialize args to JSON
        let args_json = Self::args_to_json(&args);
        let args_jstring = env
            .new_string(&args_json)
            .map_err(|e| NativeBridgeError::PlatformError(format!("Failed to create args string: {}", e)))?;

        // Call BlincNativeBridge.callNative(namespace, name, argsJson) -> String
        let result = env
            .call_static_method(
                &self.bridge_class,
                "callNative",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;",
                &[
                    JValue::Object(&ns_jstring),
                    JValue::Object(&name_jstring),
                    JValue::Object(&args_jstring),
                ],
            )
            .map_err(|e| {
                error!("JNI call failed: {}", e);
                NativeBridgeError::PlatformError(format!("JNI call failed: {}", e))
            })?;

        // Extract the result string
        let result_obj = result
            .l()
            .map_err(|e| NativeBridgeError::PlatformError(format!("Failed to get result object: {}", e)))?;

        let result_jstring = JString::from(result_obj);
        let result_str: String = env
            .get_string(&result_jstring)
            .map_err(|e| NativeBridgeError::PlatformError(format!("Failed to get result string: {}", e)))?
            .into();

        debug!("Android native result: {}", result_str);

        // Parse the JSON result
        parse_native_result_json(&result_str)
    }
}

// ============================================================================
// Registration helpers
// ============================================================================

/// Initialize the Android native bridge adapter
///
/// Call this during Android app initialization to wire up the JNI bridge.
///
/// # Arguments
/// * `app` - AndroidApp from android_main
///
/// # Example
///
/// ```ignore
/// #[no_mangle]
/// fn android_main(app: AndroidApp) {
///     // Initialize native bridge
///     if let Err(e) = init_android_native_bridge(&app) {
///         error!("Failed to init native bridge: {}", e);
///     }
///
///     // ... rest of app initialization
/// }
/// ```
#[cfg(target_os = "android")]
pub fn init_android_native_bridge(
    app: &android_activity::AndroidApp,
) -> Result<(), NativeBridgeError> {
    // Ensure NativeBridgeState is initialized
    if !NativeBridgeState::is_initialized() {
        NativeBridgeState::init();
    }

    // Create and register the adapter
    let adapter = AndroidNativeBridgeAdapter::from_android_app(app)?;
    NativeBridgeState::get().set_platform_adapter(Arc::new(adapter));

    debug!("Android native bridge initialized");
    Ok(())
}

// ============================================================================
// Non-Android stubs
// ============================================================================

#[cfg(not(target_os = "android"))]
pub struct AndroidNativeBridgeAdapter;

#[cfg(not(target_os = "android"))]
impl AndroidNativeBridgeAdapter {
    pub fn new() -> Self {
        Self
    }
}

/// Placeholder for non-Android builds
#[cfg(not(target_os = "android"))]
pub fn init_android_native_bridge() -> Result<(), String> {
    Err("Android native bridge only available on Android".to_string())
}
