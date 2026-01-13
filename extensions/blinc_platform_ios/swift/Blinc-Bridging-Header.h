//
//  Blinc-Bridging-Header.h
//  Blinc iOS Bridge
//
//  C declarations for Swift interop with Blinc Rust library.
//  Include this in your Xcode project's bridging header.
//

#ifndef Blinc_Bridging_Header_h
#define Blinc_Bridging_Header_h

#include <stdint.h>
#include <stdbool.h>

// Opaque pointer to the Blinc render context
typedef struct IOSRenderContext IOSRenderContext;

// Opaque pointer to the WindowedContext (for UI building)
typedef struct WindowedContext WindowedContext;

// UI builder function type - Rust app implements this
typedef void (*UIBuilderFn)(WindowedContext* ctx);

// =============================================================================
// Context Management
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
/// @param ctx Render context pointer
void blinc_destroy_context(IOSRenderContext* ctx);

// =============================================================================
// Frame Loop
// =============================================================================

/// Check if a frame needs to be rendered
///
/// @param ctx Render context pointer
/// @return true if render needed (state changed, animations active, or wake requested)
bool blinc_needs_render(IOSRenderContext* ctx);

/// Tick animations - call each frame before building UI
///
/// @param ctx Render context pointer
/// @return true if animations are active (keep rendering)
bool blinc_tick_animations(IOSRenderContext* ctx);

/// Register a UI builder function (call once during init)
///
/// @param builder Function pointer to the Rust UI builder
void blinc_set_ui_builder(UIBuilderFn builder);

/// Build a frame using the registered UI builder
///
/// Call this each frame when blinc_needs_render() returns true.
/// Ticks animations and calls the registered UI builder.
///
/// @param ctx Render context pointer
void blinc_build_frame(IOSRenderContext* ctx);

/// Mark the context as needing a rebuild
///
/// @param ctx Render context pointer
void blinc_mark_dirty(IOSRenderContext* ctx);

/// Clear the dirty flag after processing
///
/// @param ctx Render context pointer
void blinc_clear_dirty(IOSRenderContext* ctx);

// =============================================================================
// Size and Layout
// =============================================================================

/// Update the window size (call when view bounds change)
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

/// Get the display scale factor
double blinc_get_scale_factor(IOSRenderContext* ctx);

/// Get the physical width in pixels
uint32_t blinc_get_physical_width(IOSRenderContext* ctx);

/// Get the physical height in pixels
uint32_t blinc_get_physical_height(IOSRenderContext* ctx);

/// Get the WindowedContext pointer for UI building
WindowedContext* blinc_get_windowed_context(IOSRenderContext* ctx);

// =============================================================================
// Input Handling
// =============================================================================

/// Touch phase values:
/// 0 = Began (finger touched screen)
/// 1 = Moved (finger moved)
/// 2 = Ended (finger lifted)
/// 3 = Cancelled (system cancelled touch)

/// Handle a touch event
///
/// @param ctx Render context pointer
/// @param touch_id Unique touch identifier (use ObjectIdentifier hash)
/// @param x X position in logical points
/// @param y Y position in logical points
/// @param phase Touch phase (0=began, 1=moved, 2=ended, 3=cancelled)
void blinc_handle_touch(IOSRenderContext* ctx, uint64_t touch_id, float x, float y, int32_t phase);

/// Set the focus state (call on viewDidAppear/viewWillDisappear)
///
/// @param ctx Render context pointer
/// @param focused Whether the view is focused
void blinc_set_focused(IOSRenderContext* ctx, bool focused);

// =============================================================================
// Native Bridge (Rust calling Swift)
// =============================================================================

/// Native call function type
/// Called by Rust to execute Swift-registered handlers
/// @param ns Namespace (e.g., "device", "haptics")
/// @param name Function name (e.g., "get_battery_level")
/// @param args_json JSON-encoded arguments array
/// @return JSON-encoded result string (caller must free with blinc_free_string)
typedef char* (*NativeCallFn)(const char* ns, const char* name, const char* args_json);

/// Register the native call function
/// Call this during app initialization to wire up Swift handlers
/// @param call_fn Function pointer to Swift's blinc_ios_native_call
void blinc_set_native_call_fn(NativeCallFn call_fn);

/// Check if native bridge is ready
/// @return true if native call function has been registered
bool blinc_native_bridge_is_ready(void);

/// Free a string allocated by Swift
/// @param ptr String pointer returned from native call
void blinc_free_string(char* ptr);

#endif /* Blinc_Bridging_Header_h */
