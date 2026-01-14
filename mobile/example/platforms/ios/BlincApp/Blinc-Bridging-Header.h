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

#endif /* Blinc_Bridging_Header_h */
