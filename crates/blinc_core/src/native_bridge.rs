//! Native Bridge for Rust ↔ Platform FFI
//!
//! Enables Rust code to call platform-native functions (Android/iOS) via
//! registered namespace/name pairs with type-safe contracts.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │              Rust App                    │
//! │  native_call("device", "get_battery")   │
//! └────────────────┬────────────────────────┘
//!                  │
//!                  ▼
//! ┌─────────────────────────────────────────┐
//! │          NativeBridgeState              │
//! │  - Rust handlers (fallback/testing)     │
//! │  - Platform adapter (JNI/Swift FFI)     │
//! └────────────────┬────────────────────────┘
//!                  │
//!       ┌──────────┴──────────┐
//!       ▼                     ▼
//! ┌───────────┐        ┌───────────┐
//! │  Android  │        │    iOS    │
//! │  (JNI)    │        │ (C FFI)   │
//! └───────────┘        └───────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use blinc_core::native_bridge::{native_call, native_register, NativeValue};
//!
//! // Register a Rust-side handler (for testing/fallback)
//! native_register("math", "add", |args| {
//!     let a = args.get(0).and_then(|v| v.as_i32()).unwrap_or(0);
//!     let b = args.get(1).and_then(|v| v.as_i32()).unwrap_or(0);
//!     Ok(NativeValue::Int32(a + b))
//! });
//!
//! // Call native function
//! let battery: String = native_call("device", "get_battery_level", ()).unwrap();
//! ```
//!
//! # Platform Registration
//!
//! Native platforms register handlers at app initialization:
//!
//! **Kotlin (Android):**
//! ```kotlin
//! BlincNativeBridge.register("device", "get_battery_level") {
//!     BatteryManager.getBatteryLevel().toString()
//! }
//! ```
//!
//! **Swift (iOS):**
//! ```swift
//! BlincNativeBridge.shared.register(namespace: "device", name: "get_battery_level") {
//!     String(UIDevice.current.batteryLevel)
//! }
//! ```

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, OnceLock, RwLock};

// ============================================================================
// Types
// ============================================================================

/// Global native bridge singleton
static NATIVE_BRIDGE: OnceLock<NativeBridgeState> = OnceLock::new();

/// Result type for native bridge operations
pub type NativeResult<T> = Result<T, NativeBridgeError>;

/// Handler function type for native calls
pub type NativeHandler = Arc<dyn Fn(Vec<NativeValue>) -> NativeResult<NativeValue> + Send + Sync>;

/// Error type for native bridge operations
#[derive(Debug, Clone)]
pub enum NativeBridgeError {
    /// Function not registered in any handler
    NotRegistered {
        namespace: String,
        name: String,
    },
    /// Type mismatch when extracting return value
    TypeMismatch {
        expected: &'static str,
        actual: String,
    },
    /// Platform-specific error (JNI, Swift FFI, etc.)
    PlatformError(String),
    /// JSON serialization/deserialization error
    SerializationError(String),
    /// Bridge not initialized
    NotInitialized,
}

impl fmt::Display for NativeBridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotRegistered { namespace, name } => {
                write!(f, "Native function '{}.{}' not registered", namespace, name)
            }
            Self::TypeMismatch { expected, actual } => {
                write!(f, "Type mismatch: expected {}, got {}", expected, actual)
            }
            Self::PlatformError(msg) => write!(f, "Platform error: {}", msg),
            Self::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            Self::NotInitialized => write!(f, "Native bridge not initialized"),
        }
    }
}

impl std::error::Error for NativeBridgeError {}

/// Value type for cross-FFI transport
///
/// This enum represents values that can be passed between Rust and native code.
/// Complex types should use `Json` variant with serde serialization.
#[derive(Debug, Clone, PartialEq)]
pub enum NativeValue {
    /// No value (void return)
    Void,
    /// Boolean value
    Bool(bool),
    /// 32-bit signed integer
    Int32(i32),
    /// 64-bit signed integer
    Int64(i64),
    /// 32-bit float
    Float32(f32),
    /// 64-bit float
    Float64(f64),
    /// UTF-8 string
    String(String),
    /// Raw bytes
    Bytes(Vec<u8>),
    /// JSON-encoded complex type
    Json(String),
}

impl NativeValue {
    /// Extract as bool
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            NativeValue::Bool(v) => Some(*v),
            _ => None,
        }
    }

    /// Extract as i32
    pub fn as_i32(&self) -> Option<i32> {
        match self {
            NativeValue::Int32(v) => Some(*v),
            _ => None,
        }
    }

    /// Extract as i64
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            NativeValue::Int64(v) => Some(*v),
            NativeValue::Int32(v) => Some(*v as i64),
            _ => None,
        }
    }

    /// Extract as f32
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            NativeValue::Float32(v) => Some(*v),
            _ => None,
        }
    }

    /// Extract as f64
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            NativeValue::Float64(v) => Some(*v),
            NativeValue::Float32(v) => Some(*v as f64),
            _ => None,
        }
    }

    /// Extract as string reference
    pub fn as_str(&self) -> Option<&str> {
        match self {
            NativeValue::String(v) => Some(v),
            _ => None,
        }
    }

    /// Extract as owned string
    pub fn into_string(self) -> Option<String> {
        match self {
            NativeValue::String(v) => Some(v),
            _ => None,
        }
    }

    /// Extract as bytes reference
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            NativeValue::Bytes(v) => Some(v),
            _ => None,
        }
    }

    /// Extract as JSON string reference
    pub fn as_json(&self) -> Option<&str> {
        match self {
            NativeValue::Json(v) => Some(v),
            _ => None,
        }
    }

    /// Get type name for error messages
    pub fn type_name(&self) -> &'static str {
        match self {
            NativeValue::Void => "Void",
            NativeValue::Bool(_) => "Bool",
            NativeValue::Int32(_) => "Int32",
            NativeValue::Int64(_) => "Int64",
            NativeValue::Float32(_) => "Float32",
            NativeValue::Float64(_) => "Float64",
            NativeValue::String(_) => "String",
            NativeValue::Bytes(_) => "Bytes",
            NativeValue::Json(_) => "Json",
        }
    }
}

// ============================================================================
// Conversion Traits
// ============================================================================

/// Trait for converting Rust types to NativeValue arguments
pub trait IntoNativeArgs {
    fn into_native_args(self) -> Vec<NativeValue>;
}

// Unit type (no args)
impl IntoNativeArgs for () {
    fn into_native_args(self) -> Vec<NativeValue> {
        vec![]
    }
}

// Single value tuples
impl IntoNativeArgs for (i32,) {
    fn into_native_args(self) -> Vec<NativeValue> {
        vec![NativeValue::Int32(self.0)]
    }
}

impl IntoNativeArgs for (i64,) {
    fn into_native_args(self) -> Vec<NativeValue> {
        vec![NativeValue::Int64(self.0)]
    }
}

impl IntoNativeArgs for (f32,) {
    fn into_native_args(self) -> Vec<NativeValue> {
        vec![NativeValue::Float32(self.0)]
    }
}

impl IntoNativeArgs for (f64,) {
    fn into_native_args(self) -> Vec<NativeValue> {
        vec![NativeValue::Float64(self.0)]
    }
}

impl IntoNativeArgs for (bool,) {
    fn into_native_args(self) -> Vec<NativeValue> {
        vec![NativeValue::Bool(self.0)]
    }
}

impl IntoNativeArgs for (String,) {
    fn into_native_args(self) -> Vec<NativeValue> {
        vec![NativeValue::String(self.0)]
    }
}

impl IntoNativeArgs for (&str,) {
    fn into_native_args(self) -> Vec<NativeValue> {
        vec![NativeValue::String(self.0.to_string())]
    }
}

// Two value tuples
impl IntoNativeArgs for (i32, i32) {
    fn into_native_args(self) -> Vec<NativeValue> {
        vec![NativeValue::Int32(self.0), NativeValue::Int32(self.1)]
    }
}

impl IntoNativeArgs for (String, String) {
    fn into_native_args(self) -> Vec<NativeValue> {
        vec![NativeValue::String(self.0), NativeValue::String(self.1)]
    }
}

impl IntoNativeArgs for (&str, &str) {
    fn into_native_args(self) -> Vec<NativeValue> {
        vec![
            NativeValue::String(self.0.to_string()),
            NativeValue::String(self.1.to_string()),
        ]
    }
}

// Vec of NativeValue (direct)
impl IntoNativeArgs for Vec<NativeValue> {
    fn into_native_args(self) -> Vec<NativeValue> {
        self
    }
}

/// Trait for extracting return values from NativeValue
pub trait FromNativeValue: Sized {
    fn from_native_value(value: NativeValue) -> NativeResult<Self>;
}

impl FromNativeValue for () {
    fn from_native_value(value: NativeValue) -> NativeResult<Self> {
        match value {
            NativeValue::Void => Ok(()),
            _ => Ok(()), // Accept any value for void return
        }
    }
}

impl FromNativeValue for bool {
    fn from_native_value(value: NativeValue) -> NativeResult<Self> {
        value.as_bool().ok_or_else(|| NativeBridgeError::TypeMismatch {
            expected: "Bool",
            actual: value.type_name().to_string(),
        })
    }
}

impl FromNativeValue for i32 {
    fn from_native_value(value: NativeValue) -> NativeResult<Self> {
        value.as_i32().ok_or_else(|| NativeBridgeError::TypeMismatch {
            expected: "Int32",
            actual: value.type_name().to_string(),
        })
    }
}

impl FromNativeValue for i64 {
    fn from_native_value(value: NativeValue) -> NativeResult<Self> {
        value.as_i64().ok_or_else(|| NativeBridgeError::TypeMismatch {
            expected: "Int64",
            actual: value.type_name().to_string(),
        })
    }
}

impl FromNativeValue for f32 {
    fn from_native_value(value: NativeValue) -> NativeResult<Self> {
        value.as_f32().ok_or_else(|| NativeBridgeError::TypeMismatch {
            expected: "Float32",
            actual: value.type_name().to_string(),
        })
    }
}

impl FromNativeValue for f64 {
    fn from_native_value(value: NativeValue) -> NativeResult<Self> {
        value.as_f64().ok_or_else(|| NativeBridgeError::TypeMismatch {
            expected: "Float64",
            actual: value.type_name().to_string(),
        })
    }
}

impl FromNativeValue for String {
    fn from_native_value(value: NativeValue) -> NativeResult<Self> {
        value.into_string().ok_or_else(|| NativeBridgeError::TypeMismatch {
            expected: "String",
            actual: "non-string".to_string(),
        })
    }
}

impl FromNativeValue for Vec<u8> {
    fn from_native_value(value: NativeValue) -> NativeResult<Self> {
        match value {
            NativeValue::Bytes(v) => Ok(v),
            _ => Err(NativeBridgeError::TypeMismatch {
                expected: "Bytes",
                actual: value.type_name().to_string(),
            }),
        }
    }
}

impl FromNativeValue for NativeValue {
    fn from_native_value(value: NativeValue) -> NativeResult<Self> {
        Ok(value)
    }
}

// ============================================================================
// Platform Adapter Trait
// ============================================================================

/// Trait for platform-specific native call adapters
///
/// Implemented by `AndroidNativeBridgeAdapter` and `IOSNativeBridgeAdapter`.
pub trait PlatformAdapter: Send + Sync {
    /// Call a native function by namespace and name
    fn call(
        &self,
        namespace: &str,
        name: &str,
        args: Vec<NativeValue>,
    ) -> NativeResult<NativeValue>;
}

// ============================================================================
// Native Bridge State
// ============================================================================

/// Global native bridge singleton
///
/// Manages Rust-side handlers and platform adapters for native function calls.
pub struct NativeBridgeState {
    /// Rust-registered handlers: namespace -> (name -> handler)
    handlers: RwLock<HashMap<String, HashMap<String, NativeHandler>>>,
    /// Platform adapter (JNI for Android, C FFI for iOS)
    platform_adapter: RwLock<Option<Arc<dyn PlatformAdapter>>>,
}

impl NativeBridgeState {
    /// Initialize the native bridge singleton
    ///
    /// Call once at app startup (typically in BlincContextState::init).
    ///
    /// # Panics
    ///
    /// Panics if called more than once.
    pub fn init() {
        let state = NativeBridgeState {
            handlers: RwLock::new(HashMap::new()),
            platform_adapter: RwLock::new(None),
        };

        if NATIVE_BRIDGE.set(state).is_err() {
            // Already initialized - this is fine, don't panic
            tracing::debug!("NativeBridgeState already initialized");
        }
    }

    /// Get the singleton instance
    ///
    /// # Panics
    ///
    /// Panics if `init()` has not been called.
    pub fn get() -> &'static NativeBridgeState {
        NATIVE_BRIDGE.get().expect(
            "NativeBridgeState not initialized. Call NativeBridgeState::init() at app startup.",
        )
    }

    /// Try to get the singleton (returns None if not initialized)
    pub fn try_get() -> Option<&'static NativeBridgeState> {
        NATIVE_BRIDGE.get()
    }

    /// Check if the bridge has been initialized
    pub fn is_initialized() -> bool {
        NATIVE_BRIDGE.get().is_some()
    }

    /// Set the platform adapter
    ///
    /// Called during platform initialization to wire up JNI/Swift FFI.
    pub fn set_platform_adapter(&self, adapter: Arc<dyn PlatformAdapter>) {
        *self.platform_adapter.write().unwrap() = Some(adapter);
    }

    /// Clear the platform adapter
    pub fn clear_platform_adapter(&self) {
        *self.platform_adapter.write().unwrap() = None;
    }

    /// Register a Rust-side handler
    ///
    /// Rust handlers are checked before platform adapters, useful for:
    /// - Testing without native code
    /// - Cross-platform fallback implementations
    /// - Rust-only functionality
    pub fn register<F>(&self, namespace: &str, name: &str, handler: F)
    where
        F: Fn(Vec<NativeValue>) -> NativeResult<NativeValue> + Send + Sync + 'static,
    {
        let mut handlers = self.handlers.write().unwrap();
        let ns_handlers = handlers.entry(namespace.to_string()).or_default();
        ns_handlers.insert(name.to_string(), Arc::new(handler));
    }

    /// Unregister a Rust-side handler
    pub fn unregister(&self, namespace: &str, name: &str) -> bool {
        let mut handlers = self.handlers.write().unwrap();
        if let Some(ns_handlers) = handlers.get_mut(namespace) {
            return ns_handlers.remove(name).is_some();
        }
        false
    }

    /// Call a native function
    ///
    /// Resolution order:
    /// 1. Rust-registered handlers
    /// 2. Platform adapter (JNI/Swift FFI)
    pub fn call<R, A>(&self, namespace: &str, name: &str, args: A) -> NativeResult<R>
    where
        R: FromNativeValue,
        A: IntoNativeArgs,
    {
        let native_args = args.into_native_args();

        // First check Rust-registered handlers
        {
            let handlers = self.handlers.read().unwrap();
            if let Some(ns_handlers) = handlers.get(namespace) {
                if let Some(handler) = ns_handlers.get(name) {
                    let result = handler(native_args.clone())?;
                    return R::from_native_value(result);
                }
            }
        }

        // Fall back to platform adapter
        if let Some(adapter) = self.platform_adapter.read().unwrap().as_ref() {
            let result = adapter.call(namespace, name, native_args)?;
            return R::from_native_value(result);
        }

        Err(NativeBridgeError::NotRegistered {
            namespace: namespace.to_string(),
            name: name.to_string(),
        })
    }

    /// Check if a handler is registered (Rust or platform)
    pub fn has_handler(&self, namespace: &str, name: &str) -> bool {
        // Check Rust handlers
        let handlers = self.handlers.read().unwrap();
        if let Some(ns_handlers) = handlers.get(namespace) {
            if ns_handlers.contains_key(name) {
                return true;
            }
        }

        // Platform adapter existence (can't check specific functions)
        self.platform_adapter.read().unwrap().is_some()
    }

    /// List all registered Rust namespaces
    pub fn namespaces(&self) -> Vec<String> {
        self.handlers.read().unwrap().keys().cloned().collect()
    }

    /// List all registered Rust functions in a namespace
    pub fn functions(&self, namespace: &str) -> Vec<String> {
        self.handlers
            .read()
            .unwrap()
            .get(namespace)
            .map(|ns| ns.keys().cloned().collect())
            .unwrap_or_default()
    }
}

// ============================================================================
// Convenience Free Functions
// ============================================================================

/// Call a native function
///
/// Convenience wrapper around `NativeBridgeState::get().call()`.
///
/// # Example
///
/// ```ignore
/// use blinc_core::native_bridge::native_call;
///
/// // Call with no args, String return
/// let battery: String = native_call("device", "get_battery_level", ())?;
///
/// // Call with args, void return
/// native_call::<(), _>("haptics", "vibrate", (100i32,))?;
/// ```
///
/// # Panics
///
/// Panics if `NativeBridgeState::init()` has not been called.
pub fn native_call<R, A>(namespace: &str, name: &str, args: A) -> NativeResult<R>
where
    R: FromNativeValue,
    A: IntoNativeArgs,
{
    NativeBridgeState::get().call(namespace, name, args)
}

/// Register a Rust-side native handler
///
/// Convenience wrapper around `NativeBridgeState::get().register()`.
///
/// # Example
///
/// ```ignore
/// use blinc_core::native_bridge::{native_register, NativeValue, NativeResult};
///
/// native_register("math", "add", |args| {
///     let a = args.get(0).and_then(|v| v.as_i32()).unwrap_or(0);
///     let b = args.get(1).and_then(|v| v.as_i32()).unwrap_or(0);
///     Ok(NativeValue::Int32(a + b))
/// });
/// ```
///
/// # Panics
///
/// Panics if `NativeBridgeState::init()` has not been called.
pub fn native_register<F>(namespace: &str, name: &str, handler: F)
where
    F: Fn(Vec<NativeValue>) -> NativeResult<NativeValue> + Send + Sync + 'static,
{
    NativeBridgeState::get().register(namespace, name, handler)
}

/// Set the platform adapter
///
/// Convenience wrapper for platform initialization code.
///
/// # Panics
///
/// Panics if `NativeBridgeState::init()` has not been called.
pub fn set_platform_adapter(adapter: Arc<dyn PlatformAdapter>) {
    NativeBridgeState::get().set_platform_adapter(adapter)
}

// ============================================================================
// JSON Helpers
// ============================================================================

/// Parse a JSON result string from native code
///
/// Expected format:
/// ```json
/// { "success": true, "value": ... }
/// { "success": false, "errorType": "...", "errorMessage": "..." }
/// ```
pub fn parse_native_result_json(json: &str) -> NativeResult<NativeValue> {
    // Simple JSON parsing without serde dependency in core
    // Platform adapters can use serde_json for full parsing

    if json.contains("\"success\":true") || json.contains("\"success\": true") {
        // Extract value - simplified parsing
        if let Some(value_start) = json.find("\"value\":") {
            let value_part = &json[value_start + 8..];
            let value_str = value_part.trim();

            if value_str.starts_with("null") || value_str.starts_with("\"null\"") {
                return Ok(NativeValue::Void);
            } else if value_str.starts_with("true") {
                return Ok(NativeValue::Bool(true));
            } else if value_str.starts_with("false") {
                return Ok(NativeValue::Bool(false));
            } else if value_str.starts_with('"') {
                // String value - find closing quote
                if let Some(end) = value_str[1..].find('"') {
                    let s = &value_str[1..end + 1];
                    return Ok(NativeValue::String(s.to_string()));
                }
            } else if let Ok(n) = value_str
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '-' || *c == '.')
                .collect::<String>()
                .parse::<i64>()
            {
                if n >= i32::MIN as i64 && n <= i32::MAX as i64 {
                    return Ok(NativeValue::Int32(n as i32));
                } else {
                    return Ok(NativeValue::Int64(n));
                }
            }
        }
        Ok(NativeValue::Void)
    } else {
        // Error response
        let error_type = extract_json_string(json, "errorType").unwrap_or("Unknown");
        let error_msg = extract_json_string(json, "errorMessage").unwrap_or("Unknown error");

        match error_type {
            "NotRegistered" => Err(NativeBridgeError::NotRegistered {
                namespace: "unknown".to_string(),
                name: "unknown".to_string(),
            }),
            _ => Err(NativeBridgeError::PlatformError(error_msg.to_string())),
        }
    }
}

/// Helper to extract a string value from JSON
fn extract_json_string<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let search = format!("\"{}\":\"", key);
    if let Some(start) = json.find(&search) {
        let value_start = start + search.len();
        if let Some(end) = json[value_start..].find('"') {
            return Some(&json[value_start..value_start + end]);
        }
    }
    None
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_native_value_accessors() {
        assert_eq!(NativeValue::Bool(true).as_bool(), Some(true));
        assert_eq!(NativeValue::Int32(42).as_i32(), Some(42));
        assert_eq!(NativeValue::Int64(100).as_i64(), Some(100));
        assert_eq!(NativeValue::Float32(3.14).as_f32(), Some(3.14));
        assert_eq!(
            NativeValue::String("hello".to_string()).as_str(),
            Some("hello")
        );
    }

    #[test]
    fn test_native_value_type_mismatch() {
        assert_eq!(NativeValue::Int32(42).as_bool(), None);
        assert_eq!(NativeValue::Bool(true).as_i32(), None);
    }

    #[test]
    fn test_into_native_args() {
        let args: Vec<NativeValue> = ().into_native_args();
        assert!(args.is_empty());

        let args: Vec<NativeValue> = (42i32,).into_native_args();
        assert_eq!(args.len(), 1);
        assert_eq!(args[0].as_i32(), Some(42));

        let args: Vec<NativeValue> = (1i32, 2i32).into_native_args();
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn test_from_native_value() {
        assert_eq!(i32::from_native_value(NativeValue::Int32(42)).unwrap(), 42);
        assert_eq!(
            String::from_native_value(NativeValue::String("test".to_string())).unwrap(),
            "test"
        );
        assert!(bool::from_native_value(NativeValue::Int32(42)).is_err());
    }

    #[test]
    fn test_parse_native_result_json() {
        let success = r#"{"success":true,"value":"hello"}"#;
        let result = parse_native_result_json(success).unwrap();
        assert_eq!(result.as_str(), Some("hello"));

        let error = r#"{"success":false,"errorType":"NotRegistered","errorMessage":"not found"}"#;
        let result = parse_native_result_json(error);
        assert!(result.is_err());
    }
}
