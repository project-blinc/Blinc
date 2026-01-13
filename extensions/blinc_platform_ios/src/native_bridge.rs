//! iOS Native Bridge Adapter
//!
//! C FFI-based implementation of `PlatformAdapter` for calling Swift-registered
//! native functions from Rust.
//!
//! # Architecture
//!
//! ```text
//! Rust: native_call("device", "get_battery")
//!              │
//!              ▼
//! IOSNativeBridgeAdapter.call()
//!              │
//!              ▼ (C FFI)
//! Swift: blinc_ios_native_call(ns, name, argsJson)
//!              │
//!              ▼
//! BlincNativeBridge.shared.callNative(...)
//!              │
//!              ▼
//! Swift handler executes, returns JSON result
//! ```
//!
//! # Swift Side
//!
//! ```swift
//! public final class BlincNativeBridge {
//!     public static let shared = BlincNativeBridge()
//!     private var handlers: [String: [String: ([Any]) throws -> Any?]] = [:]
//!
//!     public func register(namespace: String, name: String, handler: @escaping ([Any]) throws -> Any?) {
//!         handlers[namespace, default: [:]][name] = handler
//!     }
//!
//!     func callNative(namespace: String, name: String, argsJson: String) -> String {
//!         guard let handler = handlers[namespace]?[name] else {
//!             return errorJson(type: "NotRegistered", message: "\(namespace).\(name) not found")
//!         }
//!         let args = parseJson(argsJson)
//!         let result = try? handler(args)
//!         return successJson(value: result)
//!     }
//! }
//!
//! @_cdecl("blinc_ios_native_call")
//! public func blincIOSNativeCall(
//!     ns: UnsafePointer<CChar>,
//!     name: UnsafePointer<CChar>,
//!     args: UnsafePointer<CChar>
//! ) -> UnsafeMutablePointer<CChar> {
//!     let result = BlincNativeBridge.shared.callNative(
//!         namespace: String(cString: ns),
//!         name: String(cString: name),
//!         argsJson: String(cString: args)
//!     )
//!     return strdup(result)!
//! }
//!
//! @_cdecl("blinc_free_string")
//! public func blincFreeString(ptr: UnsafeMutablePointer<CChar>) {
//!     free(ptr)
//! }
//! ```

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::Arc;

use blinc_core::native_bridge::{
    parse_native_result_json, NativeBridgeError, NativeBridgeState, NativeResult, NativeValue,
    PlatformAdapter,
};

/// Function pointer type for iOS native call
///
/// Takes namespace, name, and args JSON as C strings.
/// Returns a C string containing JSON result (caller must free with `blinc_free_string`).
pub type IOSNativeCallFn =
    extern "C" fn(ns: *const c_char, name: *const c_char, args_json: *const c_char) -> *mut c_char;

/// External declaration for freeing strings allocated by Swift
extern "C" {
    fn blinc_free_string(ptr: *mut c_char);
}

/// iOS platform adapter using C FFI to call Swift handlers
pub struct IOSNativeBridgeAdapter {
    /// Function pointer to Swift's native call handler
    call_fn: IOSNativeCallFn,
}

impl IOSNativeBridgeAdapter {
    /// Create a new iOS native bridge adapter
    ///
    /// # Arguments
    /// * `call_fn` - Function pointer to `blinc_ios_native_call` from Swift
    pub fn new(call_fn: IOSNativeCallFn) -> Self {
        Self { call_fn }
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
                NativeValue::String(s) => {
                    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
                }
                NativeValue::Bytes(b) => {
                    // Base64 encode bytes - simple implementation
                    let encoded = base64_encode(b);
                    format!("\"{}\"", encoded)
                }
                NativeValue::Json(j) => j.clone(),
            };
            parts.push(json);
        }
        format!("[{}]", parts.join(","))
    }
}

impl PlatformAdapter for IOSNativeBridgeAdapter {
    fn call(
        &self,
        namespace: &str,
        name: &str,
        args: Vec<NativeValue>,
    ) -> NativeResult<NativeValue> {
        // Create C strings for arguments
        let ns_cstr = CString::new(namespace)
            .map_err(|e| NativeBridgeError::PlatformError(format!("Invalid namespace: {}", e)))?;

        let name_cstr = CString::new(name)
            .map_err(|e| NativeBridgeError::PlatformError(format!("Invalid name: {}", e)))?;

        // Serialize args to JSON
        let args_json = Self::args_to_json(&args);
        let args_cstr = CString::new(args_json)
            .map_err(|e| NativeBridgeError::PlatformError(format!("Invalid args: {}", e)))?;

        // Call Swift function
        let result_ptr = (self.call_fn)(ns_cstr.as_ptr(), name_cstr.as_ptr(), args_cstr.as_ptr());

        if result_ptr.is_null() {
            return Err(NativeBridgeError::PlatformError(
                "Null result from iOS native call".to_string(),
            ));
        }

        // Extract result string
        let result_cstr = unsafe { CStr::from_ptr(result_ptr) };
        let result_str = result_cstr.to_string_lossy().into_owned();

        // Free the Swift-allocated string
        unsafe { blinc_free_string(result_ptr) };

        // Parse the JSON result
        parse_native_result_json(&result_str)
    }
}

// ============================================================================
// FFI Exports for Swift
// ============================================================================

/// Static storage for the call function pointer
static mut IOS_NATIVE_CALL_FN: Option<IOSNativeCallFn> = None;

/// Register the iOS native call function
///
/// Called from Swift during app initialization to wire up the native bridge.
///
/// # Swift Usage
///
/// ```swift
/// // In AppDelegate.application(_:didFinishLaunchingWithOptions:)
/// blinc_set_native_call_fn(blinc_ios_native_call)
/// ```
///
/// # Safety
///
/// Must be called from the main thread before any native calls are made.
#[no_mangle]
pub extern "C" fn blinc_set_native_call_fn(call_fn: IOSNativeCallFn) {
    unsafe {
        IOS_NATIVE_CALL_FN = Some(call_fn);
    }

    // Initialize native bridge if not already done
    if !NativeBridgeState::is_initialized() {
        NativeBridgeState::init();
    }

    // Create and register the adapter
    let adapter = IOSNativeBridgeAdapter::new(call_fn);
    NativeBridgeState::get().set_platform_adapter(Arc::new(adapter));
}

/// Check if the iOS native bridge is initialized
#[no_mangle]
pub extern "C" fn blinc_native_bridge_is_ready() -> bool {
    (unsafe { IOS_NATIVE_CALL_FN.is_some() }) && NativeBridgeState::is_initialized()
}

// ============================================================================
// Helpers
// ============================================================================

/// Simple base64 encoding for bytes
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;

        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);

        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }
    }

    result
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_to_json_empty() {
        let json = IOSNativeBridgeAdapter::args_to_json(&[]);
        assert_eq!(json, "[]");
    }

    #[test]
    fn test_args_to_json_primitives() {
        let args = vec![
            NativeValue::Int32(42),
            NativeValue::Bool(true),
            NativeValue::String("hello".to_string()),
        ];
        let json = IOSNativeBridgeAdapter::args_to_json(&args);
        assert_eq!(json, r#"[42,true,"hello"]"#);
    }

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b"hello"), "aGVsbG8=");
        assert_eq!(base64_encode(b"a"), "YQ==");
        assert_eq!(base64_encode(b"ab"), "YWI=");
        assert_eq!(base64_encode(b"abc"), "YWJj");
    }
}
