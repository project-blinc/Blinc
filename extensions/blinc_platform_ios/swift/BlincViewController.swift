//
//  BlincViewController.swift
//  Blinc iOS Integration
//
//  UIViewController that integrates Blinc rendering with CADisplayLink.
//  The UI is built in Rust - this handles platform integration only.
//
//  Usage:
//  1. Build libblinc_app.a with `cargo build --features ios --target aarch64-apple-ios`
//  2. Add the static library and Blinc-Bridging-Header.h to your Xcode project
//  3. Your Rust app must define and export a UI builder function
//  4. Call BlincViewController.registerUIBuilder() with your Rust function
//

import UIKit
import MetalKit

/// View controller that hosts a Blinc UI
///
/// The UI is defined in Rust and built via FFI. This class handles:
/// - CADisplayLink for 60fps frame updates
/// - Touch event forwarding to Blinc
/// - Metal layer setup for GPU rendering
/// - View lifecycle and resize handling
class BlincViewController: UIViewController {

    // MARK: - Static Registration

    /// Register the Rust UI builder function
    ///
    /// Call this once at app startup before creating any BlincViewController.
    /// The builder is a Rust function exported with #[no_mangle].
    ///
    /// Example Rust code:
    /// ```rust
    /// #[no_mangle]
    /// pub extern "C" fn my_app_build_ui(ctx: *mut WindowedContext) {
    ///     // Build UI here
    /// }
    /// ```
    static func registerUIBuilder(_ builder: UIBuilderFn) {
        blinc_set_ui_builder(builder)
    }

    // MARK: - Properties

    /// Opaque pointer to the Blinc render context
    private var blincContext: OpaquePointer?

    /// CADisplayLink for frame updates
    private var displayLink: CADisplayLink?

    /// Metal layer for GPU rendering
    private(set) var metalLayer: CAMetalLayer!

    /// Metal device
    private(set) var metalDevice: MTLDevice!

    /// Metal command queue
    private var commandQueue: MTLCommandQueue!

    // MARK: - Lifecycle

    override func viewDidLoad() {
        super.viewDidLoad()
        setupMetal()
        setupBlincContext()
        startDisplayLink()
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        guard let ctx = blincContext else { return }
        blinc_set_focused(ctx, true)
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        guard let ctx = blincContext else { return }
        blinc_set_focused(ctx, false)
    }

    override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        stopDisplayLink()
    }

    deinit {
        stopDisplayLink()
        if let ctx = blincContext {
            blinc_destroy_context(ctx)
            blincContext = nil
        }
    }

    // MARK: - Metal Setup

    private func setupMetal() {
        guard let device = MTLCreateSystemDefaultDevice() else {
            fatalError("Metal is not supported on this device")
        }
        metalDevice = device
        commandQueue = device.makeCommandQueue()

        let layer = CAMetalLayer()
        layer.device = device
        layer.pixelFormat = .bgra8Unorm
        layer.framebufferOnly = true
        layer.contentsScale = UIScreen.main.scale
        layer.frame = view.bounds

        view.layer.addSublayer(layer)
        metalLayer = layer
    }

    // MARK: - Blinc Context

    private func setupBlincContext() {
        let scale = UIScreen.main.scale
        let width = UInt32(view.bounds.width * scale)
        let height = UInt32(view.bounds.height * scale)

        blincContext = blinc_create_context(width, height, Double(scale))

        guard blincContext != nil else {
            fatalError("Failed to create Blinc render context")
        }
    }

    // MARK: - Display Link

    private func startDisplayLink() {
        guard displayLink == nil else { return }

        displayLink = CADisplayLink(target: self, selector: #selector(displayLinkFired))
        displayLink?.add(to: .main, forMode: .common)
    }

    private func stopDisplayLink() {
        displayLink?.invalidate()
        displayLink = nil
    }

    @objc private func displayLinkFired() {
        guard let ctx = blincContext else { return }

        // Check if rendering is needed
        guard blinc_needs_render(ctx) else { return }

        // Build the frame (ticks animations, calls UI builder)
        blinc_build_frame(ctx)

        // Render to Metal
        renderFrame()
    }

    /// Render the current frame to Metal
    ///
    /// This clears the screen and presents. For actual Blinc UI rendering,
    /// integrate with wgpu's Metal backend or use a Rust GPU renderer.
    func renderFrame() {
        guard let drawable = metalLayer.nextDrawable() else { return }
        guard let commandBuffer = commandQueue.makeCommandBuffer() else { return }

        let passDescriptor = MTLRenderPassDescriptor()
        passDescriptor.colorAttachments[0].texture = drawable.texture
        passDescriptor.colorAttachments[0].loadAction = .clear
        passDescriptor.colorAttachments[0].storeAction = .store
        passDescriptor.colorAttachments[0].clearColor = MTLClearColor(red: 0.1, green: 0.1, blue: 0.15, alpha: 1.0)

        guard let encoder = commandBuffer.makeRenderCommandEncoder(descriptor: passDescriptor) else { return }

        // The actual UI rendering should be done via wgpu Metal interop.
        // See blinc_gpu crate for GPU rendering implementation.
        // This base implementation just clears the screen.

        encoder.endEncoding()
        commandBuffer.present(drawable)
        commandBuffer.commit()
    }

    // MARK: - Resize

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()

        guard let ctx = blincContext else { return }

        let scale = UIScreen.main.scale
        let width = UInt32(view.bounds.width * scale)
        let height = UInt32(view.bounds.height * scale)

        // Update Metal layer
        metalLayer.frame = view.bounds
        metalLayer.drawableSize = CGSize(width: CGFloat(width), height: CGFloat(height))

        // Update Blinc context
        blinc_update_size(ctx, width, height, Double(scale))

        // Mark for rebuild
        blinc_mark_dirty(ctx)
    }

    // MARK: - Touch Handling

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let ctx = blincContext else { return }

        for touch in touches {
            let point = touch.location(in: view)
            let touchId = UInt64(bitPattern: Int64(touch.hash))
            blinc_handle_touch(ctx, touchId, Float(point.x), Float(point.y), 0)
        }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let ctx = blincContext else { return }

        for touch in touches {
            let point = touch.location(in: view)
            let touchId = UInt64(bitPattern: Int64(touch.hash))
            blinc_handle_touch(ctx, touchId, Float(point.x), Float(point.y), 1)
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let ctx = blincContext else { return }

        for touch in touches {
            let point = touch.location(in: view)
            let touchId = UInt64(bitPattern: Int64(touch.hash))
            blinc_handle_touch(ctx, touchId, Float(point.x), Float(point.y), 2)
        }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let ctx = blincContext else { return }

        for touch in touches {
            let point = touch.location(in: view)
            let touchId = UInt64(bitPattern: Int64(touch.hash))
            blinc_handle_touch(ctx, touchId, Float(point.x), Float(point.y), 3)
        }
    }
}
