//
//  Blinc-Bridging-Header.h
//  BlincApp
//
//  Bridging header for Rust FFI integration
//

#ifndef Blinc_Bridging_Header_h
#define Blinc_Bridging_Header_h

#include <stdint.h>
#include <stdbool.h>

// Opaque type for the Blinc render context
typedef struct IOSRenderContext IOSRenderContext;

// Opaque type for the WindowedContext (used by UI builder)
typedef struct WindowedContext WindowedContext;

// Type for UI builder function pointer
typedef void (*UIBuilderFn)(WindowedContext* ctx);

// =============================================================================
// Application Initialization
// =============================================================================

/// Initialize the iOS application
///
/// This registers the Rust UI builder. Must be called before blinc_create_context.
void ios_app_init(void);

// =============================================================================
// Context Lifecycle
// =============================================================================

/// Create an iOS render context
///
/// @param width Physical width in pixels
/// @param height Physical height in pixels
/// @param scale_factor Display scale factor (UIScreen.scale)
/// @return Pointer to render context, or NULL on failure
IOSRenderContext* blinc_create_context(uint32_t width, uint32_t height, double scale_factor);

/// Destroy the render context and free resources
///
/// @param ctx Render context pointer (can be NULL)
void blinc_destroy_context(IOSRenderContext* ctx);

// =============================================================================
// Rendering
// =============================================================================

/// Check if a frame needs to be rendered
///
/// Returns true if reactive state changed, animations are active,
/// or a wake was requested by the animation thread.
///
/// @param ctx Render context pointer
/// @return true if rendering is needed
bool blinc_needs_render(IOSRenderContext* ctx);

/// Register a UI builder function
///
/// The builder function will be called each frame to build the UI.
/// Call this once during initialization before any rendering.
///
/// @param builder Function pointer to UI builder
void blinc_set_ui_builder(UIBuilderFn builder);

/// Build a frame using the registered UI builder
///
/// This ticks animations, calls the registered UI builder, and prepares
/// the frame for rendering. Call this each frame when blinc_needs_render() is true.
///
/// @param ctx Render context pointer
void blinc_build_frame(IOSRenderContext* ctx);

/// Tick animations
///
/// Call this each frame before building UI.
///
/// @param ctx Render context pointer
/// @return true if any animations are active
bool blinc_tick_animations(IOSRenderContext* ctx);

// =============================================================================
// Window Size
// =============================================================================

/// Update the window size
///
/// Call this when the view's bounds change.
///
/// @param ctx Render context pointer
/// @param width New physical width in pixels
/// @param height New physical height in pixels
/// @param scale_factor Display scale factor
void blinc_update_size(IOSRenderContext* ctx, uint32_t width, uint32_t height, double scale_factor);

/// Get the logical width for UI layout
float blinc_get_width(IOSRenderContext* ctx);

/// Get the logical height for UI layout
float blinc_get_height(IOSRenderContext* ctx);

/// Get the physical width in pixels
uint32_t blinc_get_physical_width(IOSRenderContext* ctx);

/// Get the physical height in pixels
uint32_t blinc_get_physical_height(IOSRenderContext* ctx);

/// Get the scale factor
double blinc_get_scale_factor(IOSRenderContext* ctx);

// =============================================================================
// Input Events
// =============================================================================

/// Handle a touch event
///
/// Touch coordinates should be in logical points (not physical pixels).
///
/// @param ctx Render context pointer
/// @param touch_id Unique touch identifier
/// @param x X position in logical points
/// @param y Y position in logical points
/// @param phase Touch phase: 0=began, 1=moved, 2=ended, 3=cancelled
void blinc_handle_touch(IOSRenderContext* ctx, uint64_t touch_id, float x, float y, int32_t phase);

/// Set the focus state
///
/// @param ctx Render context pointer
/// @param focused Whether the view is focused
void blinc_set_focused(IOSRenderContext* ctx, bool focused);

/// Forward a typed character (or autocorrect insertion) from
/// the iOS soft keyboard to the focused text-input widget.
///
/// `BlincKeyboardHelper`'s hidden `UITextField` reports user
/// keystrokes via the `shouldChangeCharactersIn` delegate; the
/// delegate forwards each replacement string here, which
/// broadcasts a `TEXT_INPUT` event to all focused text input
/// handlers in the tree.
///
/// @param ctx   Render context pointer
/// @param text  UTF-8 NUL-terminated string with the typed
///              character(s) — usually one char, occasionally
///              several (autocorrect / paste / dead-key folding)
void blinc_ios_handle_text_input(IOSRenderContext* _Nonnull ctx, const char* _Nonnull text);

/// Forward a key-down event from the iOS soft keyboard.
///
/// Used for non-character keys (Backspace = 8, Return = 13,
/// Escape = 27). Key codes match the desktop runner's table so
/// the same `text_input` widget handlers fire on every
/// platform. Backspace is detected in `shouldChangeCharactersIn`
/// when `range.length > 0 && replacementString.isEmpty`.
///
/// @param ctx       Render context pointer
/// @param key_code  Virtual key code (8 = Backspace, etc.)
void blinc_ios_handle_key_down(IOSRenderContext* _Nonnull ctx, uint32_t key_code);

/// Forward a key-down event with explicit modifier flags.
///
/// Same as `blinc_ios_handle_key_down` but lets the Swift caller mark
/// the event as Cmd / Ctrl / Alt / Shift held. The native edit menu
/// uses this to dispatch synthesized `Cmd+X / Cmd+C / Cmd+V / Cmd+A`
/// events when the user picks Cut / Copy / Paste / Select All from
/// `UIMenuController` — the meta modifier routes the key-down into
/// the existing Cmd-shortcut branch of every Blinc text-editable
/// widget's `on_key_down` handler.
///
/// @param ctx        Render context pointer
/// @param key_code   Virtual key code (88 = X, 67 = C, 86 = V, 65 = A)
/// @param modifiers  Bitmask: bit 0 = shift, bit 1 = ctrl,
///                   bit 2 = alt, bit 3 = meta (Cmd)
void blinc_ios_handle_key_down_with_modifiers(
    IOSRenderContext* _Nonnull ctx,
    uint32_t key_code,
    uint32_t modifiers
);

/// Update the soft-keyboard inset (height of the screen the
/// keyboard is currently obscuring) in **logical points / pixels**
/// — this is the same coordinate space the Rust runner uses for
/// `WindowedContext.width/height`.
///
/// `BlincKeyboardHelper` subscribes to
/// `UIKeyboardWillChangeFrameNotification` and
/// `UIKeyboardWillHideNotification`, intersects the keyboard's
/// reported screen frame with the active key window, and pushes
/// the height through this FFI export. Pass `0.0` when the
/// keyboard is hidden — the Rust side picks the new value up on
/// the next frame and the layout / scroll-into-focused-input
/// machinery responds.
///
/// @param ctx    Render context pointer
/// @param inset  Keyboard height in logical points (`0.0` = hidden)
void blinc_ios_set_keyboard_inset(IOSRenderContext* _Nonnull ctx, float inset);

// =============================================================================
// State Management
// =============================================================================

/// Mark the context as needing a rebuild
///
/// Call this when external state changes that should trigger a UI update.
void blinc_mark_dirty(IOSRenderContext* ctx);

/// Clear the dirty flag
///
/// Call this after processing a rebuild.
void blinc_clear_dirty(IOSRenderContext* ctx);

/// Get a pointer to the WindowedContext for UI building
///
/// @param ctx Render context pointer
/// @return Pointer to WindowedContext (valid while ctx is valid)
WindowedContext* blinc_get_windowed_context(IOSRenderContext* ctx);

// =============================================================================
// GPU Rendering
// =============================================================================

/// Opaque type for the GPU renderer
typedef struct IOSGpuRenderer IOSGpuRenderer;

/// Initialize the GPU renderer with a CAMetalLayer
///
/// @param ctx Render context pointer from blinc_create_context
/// @param metal_layer Pointer to CAMetalLayer
/// @param width Drawable width in pixels
/// @param height Drawable height in pixels
/// @return Pointer to GPU renderer, or NULL on failure
IOSGpuRenderer* blinc_init_gpu(IOSRenderContext* ctx, void* metal_layer, uint32_t width, uint32_t height);

/// Resize the GPU surface
///
/// Call this when the Metal layer's drawable size changes.
///
/// @param gpu GPU renderer pointer
/// @param width New width in pixels
/// @param height New height in pixels
void blinc_gpu_resize(IOSGpuRenderer* gpu, uint32_t width, uint32_t height);

/// Render a frame
///
/// This renders the current UI to the surface.
/// Call this from your CADisplayLink callback when blinc_needs_render() is true.
///
/// @param gpu GPU renderer pointer
/// @return true if frame was rendered successfully
bool blinc_render_frame(IOSGpuRenderer* gpu);

/// Destroy the GPU renderer
///
/// @param gpu GPU renderer pointer (can be NULL)
void blinc_destroy_gpu(IOSGpuRenderer* gpu);

/// Load a bundled font from the app bundle
///
/// Call this after blinc_init_gpu to load fonts from the app bundle.
/// Returns the number of font faces loaded.
///
/// @param gpu GPU renderer pointer
/// @param path Path to the font file (null-terminated C string)
/// @return Number of font faces loaded (0 on failure)
uint32_t blinc_load_bundled_font(IOSGpuRenderer* gpu, const char* path);

/// Free a string allocated by Rust
void blinc_free_string(char* ptr);

// =============================================================================
// Native Bridge (Rust calling Swift)
// =============================================================================
//
// These declarations match the canonical bridging header at
// `extensions/blinc_platform_ios/swift/Blinc-Bridging-Header.h`.
// The `BlincNativeBridge.swift` file in this directory is a copy
// of `extensions/blinc_platform_ios/templates/BlincNativeBridge.swift`
// and uses these symbols to wire camera / audio / haptics /
// device / clipboard / keyboard handlers from Swift back into the
// Rust runtime.
//
// `BlincNativeBridge.shared.connectToRust()` calls
// `blinc_set_native_call_fn(blinc_ios_native_call)` to register
// Swift's central dispatch function. After that, every
// `native_call("namespace", "function", args)` from Rust ends
// up routed through Swift's `BlincNativeBridge.callNative(...)`.
//
// Camera and audio paths additionally call
// `blinc_dispatch_stream_data` to push captured frames /
// recorded buffers into the Rust side.

/// Native call function type — implemented by Swift, called by Rust.
///
/// All three string arguments are guaranteed non-null by the
/// caller (Rust always passes valid CStrings). The
/// `_Nonnull` annotations are what make Swift bridge this as
/// `(UnsafePointer<CChar>, UnsafePointer<CChar>, UnsafePointer<CChar>)
/// -> UnsafeMutablePointer<CChar>?` instead of the all-optional
/// default. Without them, the Swift `blinc_ios_native_call`
/// implementation in `BlincNativeBridge.swift` (which uses
/// non-optional parameters) won't satisfy the function-pointer
/// type at the `blinc_set_native_call_fn(blinc_ios_native_call)`
/// call site.
///
/// @param ns         Namespace (e.g., "device", "haptics")
/// @param name       Function name (e.g., "get_battery_level")
/// @param args_json  JSON-encoded arguments array
/// @return JSON-encoded result string (caller must `free` it via
///         `blinc_free_string`); may be NULL if the result is
///         the JSON `null` literal
typedef char* _Nullable (*NativeCallFn)(
    const char* _Nonnull ns,
    const char* _Nonnull name,
    const char* _Nonnull args_json
);

/// Register Swift's `blinc_ios_native_call` as the dispatch
/// function the Rust runtime should invoke when a user app
/// makes a `native_call("namespace", "function", args)`.
/// Call this once during app init from Swift, typically via
/// `BlincNativeBridge.shared.connectToRust()`.
void blinc_set_native_call_fn(NativeCallFn call_fn);

/// Returns true if `blinc_set_native_call_fn` has been called and
/// the Rust runtime is ready to dispatch native calls back to
/// Swift.
bool blinc_native_bridge_is_ready(void);

/// Push native-side stream data into the Rust runtime.
///
/// Used by the camera and audio recording paths in
/// `BlincNativeBridge.swift`: each frame / audio buffer is
/// converted to a flat byte array and dispatched here, where
/// the Rust side delivers it to whatever subscriber registered
/// for that `stream_id` via
/// `blinc_core::native_bridge::dispatch_stream_data`.
///
/// @param stream_id  Subscriber ID returned by the Rust side
///                   when the native producer was registered
/// @param data_ptr   Pointer to the byte buffer (caller-owned)
/// @param data_len   Length of the buffer in bytes
void blinc_dispatch_stream_data(uint64_t stream_id, const uint8_t* data_ptr, uint64_t data_len);


// Accessibility snapshots and actions (main thread).
char* blinc_ime_snapshot(const IOSRenderContext* ctx);
void blinc_ime_free(char* snapshot);
bool blinc_ime_edit(IOSRenderContext* ctx, const char* json);

char* blinc_accessibility_snapshot(const IOSRenderContext* ctx);
void blinc_accessibility_free(char* snapshot);
bool blinc_accessibility_action(IOSRenderContext* ctx, uint64_t id, uint32_t action, const char* value);

#endif /* Blinc_Bridging_Header_h */
