//! Blinc Store - Zustand-inspired centralized state management
//!
//! This module provides non-reactive external state that lives outside the
//! reactive graph. Unlike `State<T>`, store values don't trigger UI rebuilds
//! when modified - they're for tracking state that components need to coordinate
//! but that doesn't directly drive the UI tree structure.
//!
//! # Use Cases
//!
//! - **Animation coordination**: Track exiting elements during transitions
//! - **Tab transitions**: Remember which tab is exiting while animation plays
//! - **Modal state**: Track open/closing modals across components
//! - **Form state**: Coordinate form values without rebuilding on every keystroke
//!
//! # Example
//!
//! ```ignore
//! use blinc_core::store::{create_store, Store};
//!
//! // Define a store for tab transitions
//! #[derive(Clone, Default)]
//! struct TabTransitionState {
//!     current_tab: String,
//!     exiting_tab: Option<String>,
//! }
//!
//! // Create a store (returns &'static Store<T>)
//! let store = create_store::<TabTransitionState>("tabs");
//!
//! // Get current state
//! let state = store.get("my-tabs");
//!
//! // Update state
//! store.update("my-tabs", |s| {
//!     s.exiting_tab = Some("tab1".into());
//!     s.current_tab = "tab2".into();
//! });
//!
//! // Subscribe to changes (for components that need notification)
//! let unsub = store.subscribe("my-tabs", |state| {
//!     println!("State changed: {:?}", state);
//! });
//! ```

use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};
use std::sync::{Arc, Mutex, RwLock};

/// A typed store for a specific state type
pub struct Store<T: Clone + Send + Sync + 'static> {
    /// State instances keyed by string ID
    instances: RwLock<FxHashMap<String, T>>,
    /// Subscribers for each instance
    subscribers: RwLock<FxHashMap<String, Vec<Box<dyn Fn(&T) + Send + Sync>>>>,
    /// Factory function for creating default state
    default_factory: Box<dyn Fn() -> T + Send + Sync>,
}

impl<T: Clone + Send + Sync + Default + 'static> Store<T> {
    /// Create a new store with Default as the factory
    pub fn new() -> Self {
        Self {
            instances: RwLock::new(FxHashMap::default()),
            subscribers: RwLock::new(FxHashMap::default()),
            default_factory: Box::new(T::default),
        }
    }
}

impl<T: Clone + Send + Sync + 'static> Store<T> {
    /// Create a new store with a custom factory function
    pub fn with_factory<F>(factory: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            instances: RwLock::new(FxHashMap::default()),
            subscribers: RwLock::new(FxHashMap::default()),
            default_factory: Box::new(factory),
        }
    }

    /// Get the state for a given key, creating it if it doesn't exist
    pub fn get(&self, key: &str) -> T {
        {
            let instances = self.instances.read().unwrap();
            if let Some(state) = instances.get(key) {
                return state.clone();
            }
        }

        // Create new instance
        let state = (self.default_factory)();
        self.instances
            .write()
            .unwrap()
            .insert(key.to_string(), state.clone());
        state
    }

    /// Get the state if it exists, without creating
    pub fn try_get(&self, key: &str) -> Option<T> {
        self.instances.read().unwrap().get(key).cloned()
    }

    /// Set the state for a given key
    pub fn set(&self, key: &str, state: T) {
        {
            let mut instances = self.instances.write().unwrap();
            instances.insert(key.to_string(), state.clone());
        }
        self.notify_subscribers(key, &state);
    }

    /// Update the state using a closure
    pub fn update<F>(&self, key: &str, f: F)
    where
        F: FnOnce(&mut T),
    {
        let state = {
            let mut instances = self.instances.write().unwrap();
            let state = instances
                .entry(key.to_string())
                .or_insert_with(|| (self.default_factory)());
            f(state);
            state.clone()
        };
        self.notify_subscribers(key, &state);
    }

    /// Update state and return a value
    pub fn update_with<F, R>(&self, key: &str, f: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        let (result, state) = {
            let mut instances = self.instances.write().unwrap();
            let state = instances
                .entry(key.to_string())
                .or_insert_with(|| (self.default_factory)());
            let result = f(state);
            (result, state.clone())
        };
        self.notify_subscribers(key, &state);
        result
    }

    /// Delete state for a key
    pub fn delete(&self, key: &str) {
        self.instances.write().unwrap().remove(key);
        self.subscribers.write().unwrap().remove(key);
    }

    /// Subscribe to state changes for a specific key
    ///
    /// Returns an unsubscribe function
    pub fn subscribe<F>(&self, key: &str, callback: F) -> SubscriptionHandle
    where
        F: Fn(&T) + Send + Sync + 'static,
    {
        let mut subscribers = self.subscribers.write().unwrap();
        let subs = subscribers.entry(key.to_string()).or_insert_with(Vec::new);
        let index = subs.len();
        subs.push(Box::new(callback));

        SubscriptionHandle {
            key: key.to_string(),
            index,
        }
    }

    /// Notify all subscribers for a key
    fn notify_subscribers(&self, key: &str, state: &T) {
        let subscribers = self.subscribers.read().unwrap();
        if let Some(subs) = subscribers.get(key) {
            for callback in subs {
                callback(state);
            }
        }
    }

    /// Clear all instances and subscribers
    pub fn clear(&self) {
        self.instances.write().unwrap().clear();
        self.subscribers.write().unwrap().clear();
    }

    /// Get all keys in the store
    pub fn keys(&self) -> Vec<String> {
        self.instances.read().unwrap().keys().cloned().collect()
    }

    /// Check if a key exists
    pub fn contains(&self, key: &str) -> bool {
        self.instances.read().unwrap().contains_key(key)
    }
}

impl<T: Clone + Send + Sync + Default + 'static> Default for Store<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Handle for unsubscribing from store updates
#[derive(Debug)]
pub struct SubscriptionHandle {
    key: String,
    index: usize,
}

// =============================================================================
// GLOBAL STORE REGISTRY
// =============================================================================

/// Type-erased store for the registry
trait AnyStore: Send + Sync {
    fn as_any(&self) -> &dyn Any;
}

impl<T: Clone + Send + Sync + 'static> AnyStore for Store<T> {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Global registry of stores by name and type
static STORE_REGISTRY: std::sync::LazyLock<Mutex<FxHashMap<(TypeId, String), Arc<dyn AnyStore>>>> =
    std::sync::LazyLock::new(|| Mutex::new(FxHashMap::default()));

/// Create or get a store for a specific type and name
///
/// Stores are lazily created and cached globally. Multiple calls with the
/// same type and name return the same store instance.
///
/// # Example
///
/// ```ignore
/// #[derive(Clone, Default)]
/// struct MyState { value: i32 }
///
/// // Get the store (creates if needed)
/// let store = create_store::<MyState>("my-store");
///
/// // Use the store
/// store.set("key", MyState { value: 42 });
/// ```
pub fn create_store<T: Clone + Send + Sync + Default + 'static>(name: &str) -> &'static Store<T> {
    let type_id = TypeId::of::<T>();
    let key = (type_id, name.to_string());

    let mut registry = STORE_REGISTRY.lock().unwrap();

    if let Some(store) = registry.get(&key) {
        // SAFETY: We stored it with this type, and we're returning a 'static reference
        // because the store lives in the global registry for the program's lifetime
        let any = store.as_ref().as_any();
        let store_ref = any.downcast_ref::<Store<T>>().expect("Store type mismatch");
        // Convert to 'static - safe because the Arc in the registry keeps it alive forever
        return unsafe { &*(store_ref as *const Store<T>) };
    }

    let store: Arc<dyn AnyStore> = Arc::new(Store::<T>::new());
    let store_ref = store.as_ref().as_any();
    let typed_ref = store_ref
        .downcast_ref::<Store<T>>()
        .expect("Store type mismatch");
    // Convert to 'static - safe because we're inserting into global registry
    let static_ref: &'static Store<T> = unsafe { &*(typed_ref as *const Store<T>) };
    registry.insert(key, store);
    static_ref
}

/// Create or get a store with a custom factory function
pub fn create_store_with<T, F>(name: &str, factory: F) -> &'static Store<T>
where
    T: Clone + Send + Sync + 'static,
    F: Fn() -> T + Send + Sync + 'static,
{
    let type_id = TypeId::of::<T>();
    let key = (type_id, name.to_string());

    let mut registry = STORE_REGISTRY.lock().unwrap();

    if let Some(store) = registry.get(&key) {
        let any = store.as_ref().as_any();
        let store_ref = any.downcast_ref::<Store<T>>().expect("Store type mismatch");
        return unsafe { &*(store_ref as *const Store<T>) };
    }

    let store: Arc<dyn AnyStore> = Arc::new(Store::<T>::with_factory(factory));
    let store_ref = store.as_ref().as_any();
    let typed_ref = store_ref
        .downcast_ref::<Store<T>>()
        .expect("Store type mismatch");
    let static_ref: &'static Store<T> = unsafe { &*(typed_ref as *const Store<T>) };
    registry.insert(key, store);
    static_ref
}

/// Remove a store from the registry
pub fn remove_store<T: 'static>(name: &str) {
    let type_id = TypeId::of::<T>();
    let key = (type_id, name.to_string());
    STORE_REGISTRY.lock().unwrap().remove(&key);
}

/// Clear all stores from the registry
pub fn clear_all_stores() {
    STORE_REGISTRY.lock().unwrap().clear();
}

// =============================================================================
// CONVENIENCE FUNCTIONS
// =============================================================================

/// Get state from a global store
///
/// This is a convenience function for common use cases where you just
/// need to read/write state without holding a store reference.
///
/// # Example
///
/// ```ignore
/// #[derive(Clone, Default)]
/// struct Counter { value: i32 }
///
/// // Get state
/// let counter = get_store_state::<Counter>("app", "main-counter");
///
/// // Update state
/// update_store_state::<Counter>("app", "main-counter", |c| c.value += 1);
/// ```
pub fn get_store_state<T: Clone + Send + Sync + Default + 'static>(
    store_name: &str,
    key: &str,
) -> T {
    create_store::<T>(store_name).get(key)
}

/// Update state in a global store
pub fn update_store_state<T, F>(store_name: &str, key: &str, f: F)
where
    T: Clone + Send + Sync + Default + 'static,
    F: FnOnce(&mut T),
{
    create_store::<T>(store_name).update(key, f);
}

/// Set state in a global store
pub fn set_store_state<T>(store_name: &str, key: &str, state: T)
where
    T: Clone + Send + Sync + Default + 'static,
{
    create_store::<T>(store_name).set(key, state);
}

// =============================================================================
// SIMPLE KEY-VALUE STORE FOR COMMON PATTERNS
// =============================================================================

/// A simple key-value store for storing any type
///
/// This is useful for heterogeneous state that doesn't fit into typed stores.
/// Each value is stored as a type-erased `Box<dyn Any>`.
pub struct KVStore {
    values: RwLock<FxHashMap<String, Box<dyn Any + Send + Sync>>>,
}

impl KVStore {
    pub fn new() -> Self {
        Self {
            values: RwLock::new(FxHashMap::default()),
        }
    }

    pub fn get<T: Clone + 'static>(&self, key: &str) -> Option<T> {
        self.values
            .read()
            .unwrap()
            .get(key)
            .and_then(|v| v.downcast_ref::<T>().cloned())
    }

    pub fn set<T: Send + Sync + 'static>(&self, key: &str, value: T) {
        self.values
            .write()
            .unwrap()
            .insert(key.to_string(), Box::new(value));
    }

    pub fn delete(&self, key: &str) {
        self.values.write().unwrap().remove(key);
    }

    pub fn contains(&self, key: &str) -> bool {
        self.values.read().unwrap().contains_key(key)
    }

    pub fn clear(&self) {
        self.values.write().unwrap().clear();
    }
}

impl Default for KVStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Global KV store instance
static GLOBAL_KV: std::sync::LazyLock<KVStore> = std::sync::LazyLock::new(KVStore::new);

/// Get a value from the global KV store
pub fn kv_get<T: Clone + 'static>(key: &str) -> Option<T> {
    GLOBAL_KV.get(key)
}

/// Set a value in the global KV store
pub fn kv_set<T: Send + Sync + 'static>(key: &str, value: T) {
    GLOBAL_KV.set(key, value);
}

/// Delete a value from the global KV store
pub fn kv_delete(key: &str) {
    GLOBAL_KV.delete(key);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Default, Debug, PartialEq)]
    struct TestState {
        count: i32,
        name: String,
    }

    #[test]
    fn test_store_basic() {
        let store = Store::<TestState>::new();

        // Get creates default
        let state = store.get("test");
        assert_eq!(state.count, 0);
        assert_eq!(state.name, "");

        // Set works
        store.set(
            "test",
            TestState {
                count: 42,
                name: "hello".into(),
            },
        );

        let state = store.get("test");
        assert_eq!(state.count, 42);
        assert_eq!(state.name, "hello");
    }

    #[test]
    fn test_store_update() {
        let store = Store::<TestState>::new();

        store.update("counter", |s| {
            s.count = 10;
        });

        let state = store.get("counter");
        assert_eq!(state.count, 10);

        store.update("counter", |s| {
            s.count += 5;
        });

        let state = store.get("counter");
        assert_eq!(state.count, 15);
    }

    #[test]
    fn test_store_update_with() {
        let store = Store::<TestState>::new();

        store.set(
            "test",
            TestState {
                count: 10,
                name: "foo".into(),
            },
        );

        let old_count = store.update_with("test", |s| {
            let old = s.count;
            s.count = 20;
            old
        });

        assert_eq!(old_count, 10);
        assert_eq!(store.get("test").count, 20);
    }

    #[test]
    fn test_global_store() {
        // Clear any existing state
        clear_all_stores();

        let store1 = create_store::<TestState>("test-store");
        store1.set(
            "key1",
            TestState {
                count: 100,
                name: "a".into(),
            },
        );

        // Same store retrieved
        let store2 = create_store::<TestState>("test-store");
        let state = store2.get("key1");
        assert_eq!(state.count, 100);
    }

    #[test]
    fn test_convenience_functions() {
        clear_all_stores();

        set_store_state::<TestState>(
            "app",
            "main",
            TestState {
                count: 50,
                name: "test".into(),
            },
        );

        let state = get_store_state::<TestState>("app", "main");
        assert_eq!(state.count, 50);

        update_store_state::<TestState, _>("app", "main", |s| {
            s.count += 10;
        });

        let state = get_store_state::<TestState>("app", "main");
        assert_eq!(state.count, 60);
    }

    #[test]
    fn test_kv_store() {
        kv_set("my-string", "hello".to_string());
        kv_set("my-number", 42i32);

        assert_eq!(kv_get::<String>("my-string"), Some("hello".to_string()));
        assert_eq!(kv_get::<i32>("my-number"), Some(42));
        assert_eq!(kv_get::<String>("nonexistent"), None);

        kv_delete("my-string");
        assert_eq!(kv_get::<String>("my-string"), None);
    }

    #[test]
    fn test_subscriber() {
        use std::sync::atomic::{AtomicI32, Ordering};

        let store = Store::<TestState>::new();
        let call_count = Arc::new(AtomicI32::new(0));
        let call_count_clone = call_count.clone();

        let _handle = store.subscribe("watched", move |state: &TestState| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            assert!(state.count >= 0);
        });

        // Subscriber called on set
        store.set(
            "watched",
            TestState {
                count: 1,
                name: "".into(),
            },
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Subscriber called on update
        store.update("watched", |s| s.count = 2);
        assert_eq!(call_count.load(Ordering::SeqCst), 2);

        // Not called for different key
        store.set(
            "other",
            TestState {
                count: 99,
                name: "".into(),
            },
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }
}
