import UIKit
import Metal
import QuartzCore

/// A UIView subclass backed by a CAMetalLayer for GPU rendering
class BlincMetalView: UIView {

    // MARK: - Properties

    /// The Metal device for rendering
    private(set) var metalDevice: MTLDevice?

    /// The Metal command queue
    private(set) var commandQueue: MTLCommandQueue?

    /// The Metal layer (type-cast convenience)
    var metalLayer: CAMetalLayer {
        return layer as! CAMetalLayer
    }

    /// Preferred frames per second (used by display link)
    var preferredFramesPerSecond: Int = 60

    // MARK: - Layer Class

    override class var layerClass: AnyClass {
        return CAMetalLayer.self
    }

    // MARK: - Initialization

    override init(frame: CGRect) {
        super.init(frame: frame)
        commonInit()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        commonInit()
    }

    private func commonInit() {
        // Get the default Metal device
        guard let device = MTLCreateSystemDefaultDevice() else {
            print("Error: Metal is not supported on this device")
            return
        }

        metalDevice = device
        commandQueue = device.makeCommandQueue()

        // Configure the Metal layer
        metalLayer.device = device
        metalLayer.pixelFormat = .bgra8Unorm
        metalLayer.framebufferOnly = true
        metalLayer.contentsScale = UIScreen.main.scale

        // Note: displaySyncEnabled is macOS-only; iOS always syncs to display

        // Set opaque for better performance
        isOpaque = true
        backgroundColor = .black

        // Pass touches through to the view controller
        isUserInteractionEnabled = false
    }

    // MARK: - Layout

    override func layoutSubviews() {
        super.layoutSubviews()

        // Update Metal layer drawable size when view size changes
        let scale = UIScreen.main.scale
        let drawableSize = CGSize(
            width: bounds.width * scale,
            height: bounds.height * scale
        )

        if metalLayer.drawableSize != drawableSize {
            metalLayer.drawableSize = drawableSize
        }
    }

    // MARK: - Drawable Access

    /// Get the next drawable for rendering
    /// Returns nil if no drawable is available
    func nextDrawable() -> CAMetalDrawable? {
        return metalLayer.nextDrawable()
    }

    /// Get the current drawable size in pixels
    var drawableSize: CGSize {
        return metalLayer.drawableSize
    }

    /// Get the current pixel format
    var pixelFormat: MTLPixelFormat {
        return metalLayer.pixelFormat
    }
}
