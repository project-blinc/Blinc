import UIKit
import MetalKit
import os.log

private let log = OSLog(subsystem: "com.blinc.example", category: "BlincViewController")

/// Main view controller for Blinc iOS applications
///
/// This controller manages the Metal rendering surface, display link for
/// frame timing, and touch event routing to the Blinc framework.
///
/// Usage:
/// 1. Set as root view controller in AppDelegate
/// 2. Call `registerUIBuilder(_:)` to set up your UI
/// 3. The controller handles rendering and events automatically
class BlincViewController: UIViewController {

    // MARK: - Properties

    /// The Metal view for rendering
    private var metalView: BlincMetalView!

    /// CADisplayLink for frame timing
    private var displayLink: CADisplayLink?

    /// Blinc render context (manages UI state)
    private var renderContext: OpaquePointer?

    /// Blinc GPU renderer (manages Metal rendering)
    private var gpuRenderer: OpaquePointer?

    /// Whether the view is currently visible
    private var isVisible = false

    /// Track touches by their hash for multi-touch support
    private var touchIds: [ObjectIdentifier: UInt64] = [:]
    private var nextTouchId: UInt64 = 1

    // MARK: - Lifecycle

    override func viewDidLoad() {
        super.viewDidLoad()

        // Create Metal view
        metalView = BlincMetalView(frame: view.bounds)
        metalView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.addSubview(metalView)

        // Initialize Blinc context
        initializeBlinc()

        // Set up display link
        setupDisplayLink()
    }

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        isVisible = true
        displayLink?.isPaused = false

        if let ctx = renderContext {
            blinc_set_focused(ctx, true)
        }
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        isVisible = false
        displayLink?.isPaused = true

        if let ctx = renderContext {
            blinc_set_focused(ctx, false)
        }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()

        // Update Blinc with new size
        let scale = UIScreen.main.scale
        let width = UInt32(view.bounds.width * scale)
        let height = UInt32(view.bounds.height * scale)

        if let ctx = renderContext {
            blinc_update_size(ctx, width, height, Double(scale))
        }

        if let gpu = gpuRenderer {
            blinc_gpu_resize(gpu, width, height)
        }
    }

    deinit {
        // Stop display link
        displayLink?.invalidate()
        displayLink = nil

        // Clean up Blinc resources
        if let gpu = gpuRenderer {
            blinc_destroy_gpu(gpu)
        }
        if let ctx = renderContext {
            blinc_destroy_context(ctx)
        }
    }

    // MARK: - Initialization

    private func initializeBlinc() {
        let scale = UIScreen.main.scale
        let width = UInt32(view.bounds.width * scale)
        let height = UInt32(view.bounds.height * scale)

        os_log(.info, log: log, "Starting initialization %dx%d @ %.1fx", width, height, scale)

        // Initialize the Rust app (registers UI builder)
        os_log(.info, log: log, "Calling ios_app_init()")
        ios_app_init()

        // Create render context
        os_log(.info, log: log, "Calling blinc_create_context()")
        guard let ctx = blinc_create_context(width, height, Double(scale)) else {
            os_log(.error, log: log, "Failed to create Blinc render context")
            return
        }
        renderContext = ctx
        os_log(.info, log: log, "Render context created")

        // Initialize GPU with Metal layer
        let metalLayer = metalView.metalLayer
        os_log(.info, log: log, "Metal layer device: %{public}@", String(describing: metalLayer.device))

        let layerPtr = Unmanaged.passUnretained(metalLayer).toOpaque()
        os_log(.info, log: log, "Calling blinc_init_gpu()")

        guard let gpu = blinc_init_gpu(ctx, layerPtr, width, height) else {
            os_log(.error, log: log, "Failed to initialize Blinc GPU renderer")
            return
        }
        gpuRenderer = gpu

        // Load bundled fonts from app bundle
        loadBundledFonts(gpu: gpu)

        os_log(.info, log: log, "Blinc fully initialized: %dx%d @ %.1fx", width, height, scale)
    }

    /// Load bundled fonts from the app bundle
    private func loadBundledFonts(gpu: OpaquePointer) {
        // Get the bundle path for fonts
        let fontNames = ["Arial.ttf"]

        for fontName in fontNames {
            if let fontPath = Bundle.main.path(forResource: fontName.replacingOccurrences(of: ".ttf", with: ""),
                                                ofType: "ttf") {
                os_log(.info, log: log, "Loading bundled font: %{public}@", fontPath)
                let loaded = blinc_load_bundled_font(gpu, fontPath)
                os_log(.info, log: log, "Loaded %d font faces from %{public}@", loaded, fontName)
            } else {
                os_log(.fault, log: log, "Bundled font not found: %{public}@", fontName)
            }
        }
    }

    private func setupDisplayLink() {
        displayLink = CADisplayLink(target: self, selector: #selector(displayLinkFired))

        // Prefer 60fps, but allow system to throttle
        if #available(iOS 15.0, *) {
            displayLink?.preferredFrameRateRange = CAFrameRateRange(minimum: 30, maximum: 120, preferred: 60)
        } else {
            displayLink?.preferredFramesPerSecond = 60
        }

        displayLink?.add(to: .main, forMode: .common)
    }

    // MARK: - Rendering

    private var frameCount = 0

    @objc private func displayLinkFired() {
        guard isVisible,
              let ctx = renderContext,
              let gpu = gpuRenderer else {
            if frameCount == 0 {
                os_log(.error, log: log, "displayLinkFired - missing context or gpu (isVisible: %d, ctx: %d, gpu: %d)",
                       isVisible ? 1 : 0, renderContext != nil ? 1 : 0, gpuRenderer != nil ? 1 : 0)
            }
            return
        }

        // Log first few frames
        if frameCount < 3 {
            os_log(.debug, log: log, "Frame %d - checking if render needed", frameCount)
        }

        // Check if we need to render
        let needsRender = blinc_needs_render(ctx)
        if frameCount < 3 {
            os_log(.debug, log: log, "Frame %d - needs_render: %d", frameCount, needsRender ? 1 : 0)
        }

        if !needsRender && frameCount >= 3 {
            return
        }

        // Build UI (this ticks animations and calls the UI builder)
        if frameCount < 3 {
            os_log(.debug, log: log, "Frame %d - calling blinc_build_frame", frameCount)
        }
        blinc_build_frame(ctx)

        // Render to Metal
        if frameCount < 3 {
            os_log(.debug, log: log, "Frame %d - calling blinc_render_frame", frameCount)
        }
        let result = blinc_render_frame(gpu)
        if frameCount < 3 {
            os_log(.info, log: log, "Frame %d - render result: %d", frameCount, result ? 1 : 0)
        }

        frameCount += 1
    }

    // MARK: - Touch Handling

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        os_log(.info, log: log, "touchesBegan: %d touches, renderContext=%{public}@",
               touches.count, renderContext != nil ? "valid" : "nil")

        guard let ctx = renderContext else {
            os_log(.error, log: log, "touchesBegan: renderContext is nil!")
            return
        }

        for touch in touches {
            let point = touch.location(in: view)
            let touchId = getTouchId(for: touch)
            os_log(.info, log: log, "touchesBegan: calling blinc_handle_touch at (%.1f, %.1f)", point.x, point.y)
            blinc_handle_touch(ctx, touchId, Float(point.x), Float(point.y), 0) // 0 = began
        }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let ctx = renderContext else { return }

        for touch in touches {
            let point = touch.location(in: view)
            let touchId = getTouchId(for: touch)
            blinc_handle_touch(ctx, touchId, Float(point.x), Float(point.y), 1) // 1 = moved
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        os_log(.info, log: log, "touchesEnded: %d touches", touches.count)

        guard let ctx = renderContext else {
            os_log(.error, log: log, "touchesEnded: renderContext is nil!")
            return
        }

        for touch in touches {
            let point = touch.location(in: view)
            let touchId = getTouchId(for: touch)
            os_log(.info, log: log, "touchesEnded: calling blinc_handle_touch at (%.1f, %.1f)", point.x, point.y)
            blinc_handle_touch(ctx, touchId, Float(point.x), Float(point.y), 2) // 2 = ended
            removeTouchId(for: touch)
        }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let ctx = renderContext else { return }

        for touch in touches {
            let point = touch.location(in: view)
            let touchId = getTouchId(for: touch)
            blinc_handle_touch(ctx, touchId, Float(point.x), Float(point.y), 3) // 3 = cancelled
            removeTouchId(for: touch)
        }
    }

    // MARK: - Touch ID Management

    private func getTouchId(for touch: UITouch) -> UInt64 {
        let identifier = ObjectIdentifier(touch)
        if let existingId = touchIds[identifier] {
            return existingId
        }
        let newId = nextTouchId
        nextTouchId += 1
        touchIds[identifier] = newId
        return newId
    }

    private func removeTouchId(for touch: UITouch) {
        let identifier = ObjectIdentifier(touch)
        touchIds.removeValue(forKey: identifier)
    }

    // MARK: - Status Bar

    override var prefersStatusBarHidden: Bool {
        return true
    }

    override var preferredStatusBarStyle: UIStatusBarStyle {
        return .lightContent
    }

    // MARK: - Safe Area

    override var preferredScreenEdgesDeferringSystemGestures: UIRectEdge {
        return .all
    }
}

// MARK: - UI Builder Registration

/// Global UI builder function pointer for FFI
private var globalUIBuilder: UIBuilderFn?

/// Register a UI builder function for the application
///
/// This function should be called from your app's Rust code via FFI
/// before the view controller is created.
///
/// Example Rust:
/// ```rust
/// #[no_mangle]
/// pub extern "C" fn my_ui_builder(ctx: *mut WindowedContext) {
///     // Build UI here
/// }
///
/// fn main() {
///     blinc_set_ui_builder(my_ui_builder);
/// }
/// ```
func registerUIBuilder(_ builder: UIBuilderFn) {
    blinc_set_ui_builder(builder)
}
