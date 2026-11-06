/**
 * Blinc Native Bridge for iOS
 *
 * Swift implementation for handling native calls from Rust.
 * Register handlers for each namespace/function, then Rust can call
 * them via native_call("namespace", "function", args).
 *
 * Usage:
 * ```swift
 * // In AppDelegate.application(_:didFinishLaunchingWithOptions:)
 * BlincNativeBridge.shared.registerDefaults()
 * BlincNativeBridge.shared.connectToRust()
 *
 * // Or register custom handlers
 * BlincNativeBridge.shared.register(namespace: "myapi", name: "my_function") { args in
 *     // args is [Any]
 *     return "result"
 * }
 * ```
 */

import Foundation
import UIKit
import AudioToolbox
import AVFoundation

public final class BlincNativeBridge {

    public static let shared = BlincNativeBridge()

    // Handler type: (args: [Any]) throws -> Any?
    private var handlers: [String: [String: ([Any]) throws -> Any?]] = [:]

    private init() {}

    // MARK: - Registration

    /// Register a native function handler
    ///
    /// - Parameters:
    ///   - namespace: The namespace (e.g., "device", "haptics")
    ///   - name: The function name
    ///   - handler: Handler that receives args array and returns a result
    public func register(namespace: String, name: String, handler: @escaping ([Any]) throws -> Any?) {
        if handlers[namespace] == nil {
            handlers[namespace] = [:]
        }
        handlers[namespace]![name] = handler
    }

    /// Convenience: Register a no-arg function returning String
    public func registerString(namespace: String, name: String, handler: @escaping () -> String) {
        register(namespace: namespace, name: name) { _ in handler() }
    }

    /// Convenience: Register a no-arg void function
    public func registerVoid(namespace: String, name: String, handler: @escaping () -> Void) {
        register(namespace: namespace, name: name) { _ in handler(); return nil }
    }

    // MARK: - Native Call Handler

    /// Called from Rust via C FFI to execute a registered function
    ///
    /// - Parameters:
    ///   - namespace: The namespace
    ///   - name: The function name
    ///   - argsJson: JSON-encoded arguments array
    /// - Returns: JSON-encoded result or error
    func callNative(namespace: String, name: String, argsJson: String) -> String {
        do {
            guard let nsHandlers = handlers[namespace] else {
                return errorJson(type: "NotRegistered", message: "Namespace '\(namespace)' not found")
            }

            guard let handler = nsHandlers[name] else {
                return errorJson(type: "NotRegistered", message: "Function '\(namespace).\(name)' not found")
            }

            // Parse args from JSON
            let args = parseArgs(argsJson)

            // Call handler
            let result = try handler(args)

            return successJson(value: result)
        } catch {
            return errorJson(type: "PlatformError", message: error.localizedDescription)
        }
    }

    /// Connect to Rust by registering our native call function
    public func connectToRust() {
        blinc_set_native_call_fn(blinc_ios_native_call)
    }

    // MARK: - Default Handlers

    /// Register default handlers for common functionality
    public func registerDefaults() {
        // =====================================================================
        // Device namespace
        // =====================================================================

        registerString(namespace: "device", name: "get_battery_level") {
            UIDevice.current.isBatteryMonitoringEnabled = true
            let level = UIDevice.current.batteryLevel
            UIDevice.current.isBatteryMonitoringEnabled = false
            return level >= 0 ? String(Int(level * 100)) : "0"
        }

        registerString(namespace: "device", name: "get_model") {
            UIDevice.current.model
        }

        registerString(namespace: "device", name: "get_os_version") {
            UIDevice.current.systemVersion
        }

        register(namespace: "device", name: "is_low_power_mode") { _ in
            ProcessInfo.processInfo.isLowPowerModeEnabled
        }

        register(namespace: "device", name: "has_notch") { _ in
            if #available(iOS 13.0, *) {
                let window = UIApplication.shared.connectedScenes
                    .compactMap { $0 as? UIWindowScene }
                    .flatMap { $0.windows }
                    .first { $0.isKeyWindow }
                    ?? UIApplication.shared.connectedScenes
                        .compactMap { $0 as? UIWindowScene }
                        .flatMap { $0.windows }
                        .first
                return (window?.safeAreaInsets.top ?? 0) > 20
            }
            return false
        }

        registerString(namespace: "device", name: "get_locale") {
            Locale.current.identifier
        }

        registerString(namespace: "device", name: "get_timezone") {
            TimeZone.current.identifier
        }

        // =====================================================================
        // Haptics namespace
        // =====================================================================

        register(namespace: "haptics", name: "vibrate") { args in
            // iOS doesn't support custom duration vibration via public API
            AudioServicesPlaySystemSound(kSystemSoundID_Vibrate)
            return nil
        }

        register(namespace: "haptics", name: "impact") { args in
            if #available(iOS 10.0, *) {
                let style: Int = args.first as? Int ?? 1
                let feedbackStyle: UIImpactFeedbackGenerator.FeedbackStyle
                switch style {
                case 0: feedbackStyle = .light
                case 2: feedbackStyle = .heavy
                default: feedbackStyle = .medium
                }
                let generator = UIImpactFeedbackGenerator(style: feedbackStyle)
                generator.prepare()
                generator.impactOccurred()
            }
            return nil
        }

        registerVoid(namespace: "haptics", name: "selection") {
            if #available(iOS 10.0, *) {
                let generator = UISelectionFeedbackGenerator()
                generator.prepare()
                generator.selectionChanged()
            }
        }

        registerVoid(namespace: "haptics", name: "success") {
            if #available(iOS 10.0, *) {
                let generator = UINotificationFeedbackGenerator()
                generator.prepare()
                generator.notificationOccurred(.success)
            }
        }

        registerVoid(namespace: "haptics", name: "warning") {
            if #available(iOS 10.0, *) {
                let generator = UINotificationFeedbackGenerator()
                generator.prepare()
                generator.notificationOccurred(.warning)
            }
        }

        registerVoid(namespace: "haptics", name: "error") {
            if #available(iOS 10.0, *) {
                let generator = UINotificationFeedbackGenerator()
                generator.prepare()
                generator.notificationOccurred(.error)
            }
        }

        // =====================================================================
        // Clipboard namespace
        // =====================================================================

        register(namespace: "clipboard", name: "copy") { args in
            let text = args.first as? String ?? ""
            UIPasteboard.general.string = text
            return nil
        }

        registerString(namespace: "clipboard", name: "paste") {
            UIPasteboard.general.string ?? ""
        }

        register(namespace: "clipboard", name: "has_content") { _ in
            UIPasteboard.general.hasStrings
        }

        registerVoid(namespace: "clipboard", name: "clear") {
            UIPasteboard.general.items = []
        }

        // =====================================================================
        // Text-edit context menu namespace
        // =====================================================================
        //
        // The Rust text-editable widgets (text_input, text_area,
        // code_editor, rich_text_editor) call into this namespace
        // from their double-tap handlers to show a native iOS edit
        // menu (Cut / Copy / Paste / Select All) over the focused
        // selection.
        //
        // This implementation uses the legacy `UIMenuController` API
        // because it's available back to iOS 13 (the modern
        // `UIEditMenuInteraction` requires iOS 16+ and a
        // UITextInteraction host view, which our hidden text field
        // doesn't have). The action callbacks are routed back into
        // Rust by re-using the existing `blinc_ios_handle_key_down`
        // FFI export with synthesized Cmd+key codes:
        //
        //   Cut        → Cmd+X (key code 88)
        //   Copy       → Cmd+C (key code 67)
        //   Paste      → Cmd+V (key code 86)
        //   Select All → Cmd+A (key code 65)
        //
        // Each text-editable widget already handles those Cmd-shortcut
        // key codes in its `on_key_down` handler, so the menu plugs
        // straight into the existing copy/cut/paste/select-all paths
        // without needing a new dispatch route.
        //
        // The bitmask layout matches `text_edit::edit_menu_actions`:
        //   bit 0 (0x01) = CUT
        //   bit 1 (0x02) = COPY
        //   bit 2 (0x04) = PASTE
        //   bit 3 (0x08) = SELECT_ALL

        register(namespace: "edit_menu", name: "show") { args in
            // Coerce via NSNumber so any numeric type (Double, Float,
            // Int, NSNumber) ends up as a CGFloat. A direct
            // `as? Double` cast fails silently when the JSON parser
            // produces an NSNumber that doesn't bridge as Double —
            // which is exactly what happens for `NativeValue::Float32`
            // values from the Rust side.
            func coerceCGFloat(_ value: Any?) -> CGFloat? {
                if let n = value as? NSNumber {
                    return CGFloat(truncating: n)
                }
                return nil
            }
            func coerceInt(_ value: Any?) -> Int? {
                if let n = value as? NSNumber {
                    return n.intValue
                }
                return nil
            }
            let anchorX = coerceCGFloat(args[safe: 0]) ?? 0
            let anchorY = coerceCGFloat(args[safe: 1]) ?? 0
            let selX = coerceCGFloat(args[safe: 2]) ?? anchorX
            let selY = coerceCGFloat(args[safe: 3]) ?? anchorY
            let selW = coerceCGFloat(args[safe: 4]) ?? 0
            let selH = coerceCGFloat(args[safe: 5]) ?? 24
            let actions = coerceInt(args[safe: 6]) ?? 0

            DispatchQueue.main.async {
                BlincEditMenuHelper.shared.show(
                    anchor: CGPoint(x: anchorX, y: anchorY),
                    selectionRect: CGRect(x: selX, y: selY, width: selW, height: selH),
                    actions: actions
                )
            }
            return nil
        }

        registerVoid(namespace: "edit_menu", name: "hide") {
            DispatchQueue.main.async {
                BlincEditMenuHelper.shared.hide()
            }
        }

        // =====================================================================
        // Camera namespace
        // =====================================================================

        register(namespace: "camera", name: "preview_start") { args in
            let width = args[safe: 0] as? Int ?? 640
            let height = args[safe: 1] as? Int ?? 480
            let fps = args[safe: 2] as? Int ?? 30
            let facing = args[safe: 3] as? Int ?? 0  // 0=front, 1=back
            let streamId = args[safe: 4] as? Int64 ?? 0

            BlincCameraHelper.shared.startPreview(
                width: width, height: height, fps: fps,
                facing: facing == 0 ? .front : .back,
                streamId: UInt64(streamId)
            )
            return nil
        }

        registerVoid(namespace: "camera", name: "preview_stop") {
            BlincCameraHelper.shared.stopPreview()
        }

        // =====================================================================
        // Audio recording namespace
        // =====================================================================

        register(namespace: "audio", name: "record_start") { args in
            let sampleRate = args[safe: 0] as? Int ?? 44100
            let channels = args[safe: 1] as? Int ?? 1
            let streamId = args[safe: 2] as? Int64 ?? 0

            BlincAudioRecorderHelper.shared.startRecording(
                sampleRate: sampleRate, channels: channels,
                streamId: UInt64(streamId)
            )
            return nil
        }

        registerVoid(namespace: "audio", name: "record_stop") {
            BlincAudioRecorderHelper.shared.stopRecording()
        }

        // =====================================================================
        // Keyboard namespace
        // =====================================================================

        register(namespace: "keyboard", name: "show") { _ in
            DispatchQueue.main.async {
                BlincKeyboardHelper.shared.showKeyboard()
            }
            return nil
        }

        register(namespace: "keyboard", name: "hide") { _ in
            DispatchQueue.main.async {
                BlincKeyboardHelper.shared.hideKeyboard()
            }
            return nil
        }

        // =====================================================================
        // App namespace
        // =====================================================================

        registerString(namespace: "app", name: "get_version") {
            Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0"
        }

        registerString(namespace: "app", name: "get_build_number") {
            Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "1"
        }

        registerString(namespace: "app", name: "get_bundle_id") {
            Bundle.main.bundleIdentifier ?? ""
        }

        register(namespace: "app", name: "open_url") { args in
            guard let urlString = args.first as? String,
                  let url = URL(string: urlString) else {
                return false
            }

            if #available(iOS 10.0, *) {
                UIApplication.shared.open(url, options: [:], completionHandler: nil)
                return true
            } else {
                return UIApplication.shared.openURL(url)
            }
        }

        register(namespace: "app", name: "share_text") { args in
            let text = args.first as? String ?? ""
            DispatchQueue.main.async {
                let activityVC = UIActivityViewController(activityItems: [text], applicationActivities: nil)
                if let windowScene = UIApplication.shared.connectedScenes.first as? UIWindowScene,
                   let rootVC = windowScene.windows.first?.rootViewController {
                    rootVC.present(activityVC, animated: true)
                }
            }
            return nil
        }
    }

    // MARK: - Helper Functions

    private func parseArgs(_ json: String) -> [Any] {
        guard let data = json.data(using: .utf8),
              let array = try? JSONSerialization.jsonObject(with: data) as? [Any] else {
            return []
        }
        return array
    }

    private func successJson(value: Any?) -> String {
        var result: [String: Any] = ["success": true]

        switch value {
        case nil:
            result["value"] = NSNull()
        case let bool as Bool:
            result["value"] = bool
        case let int as Int:
            result["value"] = int
        case let int64 as Int64:
            result["value"] = int64
        case let float as Float:
            result["value"] = float
        case let double as Double:
            result["value"] = double
        case let string as String:
            result["value"] = string
        case let data as Data:
            result["value"] = data.base64EncodedString()
        default:
            result["value"] = String(describing: value)
        }

        if let data = try? JSONSerialization.data(withJSONObject: result),
           let json = String(data: data, encoding: .utf8) {
            return json
        }
        return "{\"success\":true,\"value\":null}"
    }

    private func errorJson(type: String, message: String) -> String {
        let result: [String: Any] = [
            "success": false,
            "errorType": type,
            "errorMessage": message
        ]

        if let data = try? JSONSerialization.data(withJSONObject: result),
           let json = String(data: data, encoding: .utf8) {
            return json
        }
        return "{\"success\":false,\"errorType\":\"\(type)\",\"errorMessage\":\"\(message)\"}"
    }
}

// MARK: - Safe Array Access

private extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}

// MARK: - Camera Helper

/// Captures camera frames and sends RGBA data to Rust.
///
/// Uses AVCaptureSession + AVCaptureVideoDataOutput.
/// Each frame is converted to RGBA and sent via blinc_dispatch_stream_data.
class BlincCameraHelper: NSObject, AVCaptureVideoDataOutputSampleBufferDelegate {
    static let shared = BlincCameraHelper()

    private var session: AVCaptureSession?
    private var streamId: UInt64 = 0
    private let queue = DispatchQueue(label: "blinc.camera")

    func startPreview(width: Int, height: Int, fps: Int, facing: AVCaptureDevice.Position, streamId: UInt64) {
        self.streamId = streamId

        let session = AVCaptureSession()
        session.sessionPreset = .medium

        guard let device = AVCaptureDevice.default(
            .builtInWideAngleCamera, for: .video, position: facing
        ) else { return }

        guard let input = try? AVCaptureDeviceInput(device: device) else { return }
        if session.canAddInput(input) { session.addInput(input) }

        let output = AVCaptureVideoDataOutput()
        output.videoSettings = [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA
        ]
        output.setSampleBufferDelegate(self, queue: queue)
        if session.canAddOutput(output) { session.addOutput(output) }

        session.startRunning()
        self.session = session
    }

    func stopPreview() {
        session?.stopRunning()
        session = nil
    }

    func captureOutput(_ output: AVCaptureOutput,
                       didOutput sampleBuffer: CMSampleBuffer,
                       from connection: AVCaptureConnection) {
        guard let pixelBuffer = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }

        CVPixelBufferLockBaseAddress(pixelBuffer, .readOnly)
        defer { CVPixelBufferUnlockBaseAddress(pixelBuffer, .readOnly) }

        let width = CVPixelBufferGetWidth(pixelBuffer)
        let height = CVPixelBufferGetHeight(pixelBuffer)
        let bytesPerRow = CVPixelBufferGetBytesPerRow(pixelBuffer)

        guard let baseAddress = CVPixelBufferGetBaseAddress(pixelBuffer) else { return }
        let ptr = baseAddress.assumingMemoryBound(to: UInt8.self)

        // Convert BGRA → RGBA
        var rgba = [UInt8](repeating: 0, count: width * height * 4)
        for y in 0..<height {
            for x in 0..<width {
                let srcIdx = y * bytesPerRow + x * 4
                let dstIdx = (y * width + x) * 4
                rgba[dstIdx + 0] = ptr[srcIdx + 2]  // R ← B
                rgba[dstIdx + 1] = ptr[srcIdx + 1]  // G
                rgba[dstIdx + 2] = ptr[srcIdx + 0]  // B ← R
                rgba[dstIdx + 3] = ptr[srcIdx + 3]  // A
            }
        }

        // Send to Rust
        rgba.withUnsafeBufferPointer { buf in
            blinc_dispatch_stream_data(streamId, buf.baseAddress!, UInt64(rgba.count))
        }
    }
}

// MARK: - Audio Recording Helper

/// Records audio from the microphone and sends PCM float samples to Rust.
class BlincAudioRecorderHelper {
    static let shared = BlincAudioRecorderHelper()

    private var audioEngine: AVAudioEngine?
    private var streamId: UInt64 = 0

    func startRecording(sampleRate: Int, channels: Int, streamId: UInt64) {
        self.streamId = streamId

        let engine = AVAudioEngine()
        let inputNode = engine.inputNode
        let format = AVAudioFormat(
            commonFormat: .pcmFormatFloat32,
            sampleRate: Double(sampleRate),
            channels: AVAudioChannelCount(channels),
            interleaved: true
        )!

        inputNode.installTap(onBus: 0, bufferSize: 4096, format: format) { [weak self] buffer, _ in
            guard let self = self else { return }
            guard let floatData = buffer.floatChannelData else { return }

            let frameCount = Int(buffer.frameLength)
            let channelCount = Int(buffer.format.channelCount)

            // Convert float samples to bytes (little-endian)
            var bytes = [UInt8](repeating: 0, count: frameCount * channelCount * 4)
            for i in 0..<(frameCount * channelCount) {
                let ch = i % channelCount
                let frame = i / channelCount
                let value = floatData[ch][frame]
                let valueBytes = withUnsafeBytes(of: value.bitPattern.littleEndian) { Array($0) }
                bytes[i * 4 + 0] = valueBytes[0]
                bytes[i * 4 + 1] = valueBytes[1]
                bytes[i * 4 + 2] = valueBytes[2]
                bytes[i * 4 + 3] = valueBytes[3]
            }

            bytes.withUnsafeBufferPointer { buf in
                blinc_dispatch_stream_data(self.streamId, buf.baseAddress!, UInt64(bytes.count))
            }
        }

        do {
            try engine.start()
            self.audioEngine = engine
        } catch {
            print("BlincAudioRecorder: failed to start: \(error)")
        }
    }

    func stopRecording() {
        audioEngine?.inputNode.removeTap(onBus: 0)
        audioEngine?.stop()
        audioEngine = nil
    }
}

// MARK: - Keyboard Helper

/// `UITextField` subclass that overrides `deleteBackward()` so
/// the delegate is informed of backspace presses *even when the
/// field is empty*.
///
/// ## Why this is necessary
///
/// `BlincKeyboardHelper` clears the hidden text field on every
/// change (`textField.text = ""`) so that the field's own buffer
/// never accumulates — the source of truth is the Rust
/// `text_input` widget. But that means the field is *always*
/// empty, and iOS does NOT call
/// `shouldChangeCharactersIn:replacementString:` when the user
/// presses backspace on an empty field. Backspace presses get
/// silently dropped.
///
/// The standard iOS workaround: subclass `UITextField` and
/// override `deleteBackward()`. This method is called for every
/// backspace press regardless of buffer state, and we forward
/// the event to the delegate via a custom protocol so
/// `BlincKeyboardHelper` can dispatch `blinc_ios_handle_key_down(ctx, 8)`.
class BlincHiddenTextField: UITextField {
    weak var blincDelegate: BlincKeyboardHelper?

    /// Bitmask of edit-menu actions the field should report as
    /// available the next time `UIMenuController` queries
    /// `canPerformAction(_:withSender:)`. Set by
    /// `BlincEditMenuHelper.show(...)` right before the menu pops up.
    /// Defaults to all four actions enabled when the user just
    /// double-tapped a word.
    ///
    /// Bits match `text_edit::edit_menu_actions`:
    ///   bit 0 = Cut
    ///   bit 1 = Copy
    ///   bit 2 = Paste
    ///   bit 3 = Select All
    var blincEditMenuActions: Int = 0

    override func deleteBackward() {
        // Forward to the Blinc helper *first*, then call super
        // so the field's own (empty) buffer behavior is
        // preserved. Calling super on an empty field is a no-op,
        // so the order is mostly cosmetic.
        blincDelegate?.didPressBackspace()
        super.deleteBackward()
    }

    /// Tell `UIMenuController` which standard menu items to show.
    ///
    /// The hidden text field has no text content of its own (Blinc
    /// owns the buffer), so `UITextField`'s default
    /// `canPerformAction` would return false for cut/copy and
    /// inconsistent values for paste/selectAll. We override it to
    /// return true exclusively for the four selectors corresponding
    /// to bits set in `blincEditMenuActions`, and false for
    /// everything else (including the system selectors that would
    /// otherwise show up like Look Up, Translate, Share, etc.).
    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        if action == #selector(UIResponderStandardEditActions.cut(_:)) {
            return blincEditMenuActions & 0x01 != 0
        }
        if action == #selector(UIResponderStandardEditActions.copy(_:)) {
            return blincEditMenuActions & 0x02 != 0
        }
        if action == #selector(UIResponderStandardEditActions.paste(_:)) {
            return blincEditMenuActions & 0x04 != 0
        }
        if action == #selector(UIResponderStandardEditActions.selectAll(_:)) {
            return blincEditMenuActions & 0x08 != 0
        }
        return false
    }

    /// Intercept the system Cut action and dispatch a synthesized
    /// `Cmd+X` key-down event into Rust. Each Blinc text-editable
    /// widget already handles `Cmd+X` in its `on_key_down` handler
    /// (writing the selection to the clipboard and deleting it), so
    /// this routes the menu choice through the same code path the
    /// hardware-keyboard shortcut uses on every platform.
    override func cut(_ sender: Any?) {
        forwardEditMenuKey(keyCode: 88) // X
    }

    /// Cmd+C
    override func copy(_ sender: Any?) {
        forwardEditMenuKey(keyCode: 67) // C
    }

    /// Cmd+V
    override func paste(_ sender: Any?) {
        forwardEditMenuKey(keyCode: 86) // V
    }

    /// Cmd+A
    override func selectAll(_ sender: Any?) {
        forwardEditMenuKey(keyCode: 65) // A
    }

    /// Helper: dispatch the given key code into Rust with the meta
    /// (Cmd) modifier set. The bit layout matches
    /// `IOSRenderContext::handle_key_down_with_modifiers`:
    /// shift=0x01, ctrl=0x02, alt=0x04, meta=0x08.
    private func forwardEditMenuKey(keyCode: UInt32) {
        guard let ctx = BlincKeyboardHelper.blincContext else { return }
        blinc_ios_handle_key_down_with_modifiers(ctx, keyCode, 0x08)
    }

    // MARK: - UITextInput geometry overrides
    //
    // The hidden text field has no real text content (Blinc owns
    // the buffer), so the default `UITextField` implementations of
    // these geometry queries return `CGRect.zero` or NaN-filled
    // rects for any range/position UIKit asks about. UIKit's
    // `UIEditMenuInteraction` chrome layout queries the first
    // responder for selection geometry during menu presentation
    // and unions the result with the delegate-supplied target
    // rect — when the responder returns NaN, the union goes NaN
    // and the chrome positions the menu at NaN coordinates,
    // producing dozens of CoreGraphics warnings followed by an
    // invisible menu.
    //
    // We override each geometry method to return a finite, 1pt
    // rect at the field's origin. The actual menu position comes
    // from `BlincEditMenuInteractionDelegate.targetRectFor`, so
    // these values only need to be finite — not accurate.

    override func caretRect(for position: UITextPosition) -> CGRect {
        return CGRect(x: 0, y: 0, width: 1, height: 1)
    }

    override func firstRect(for range: UITextRange) -> CGRect {
        return CGRect(x: 0, y: 0, width: 1, height: 1)
    }

    override func selectionRects(for range: UITextRange) -> [UITextSelectionRect] {
        return []
    }
}

/// Helper class that uses a hidden `BlincHiddenTextField` to
/// trigger the iOS soft keyboard and forward keystrokes back
/// into the Rust runtime.
///
/// iOS requires a `UITextInput` responder to show the keyboard —
/// there's no standalone API like Android's `InputMethodManager`.
///
/// ## Wiring text input back to Rust
///
/// `BlincViewController` (or any code that owns the
/// `IOSRenderContext`) MUST set `BlincKeyboardHelper.blincContext`
/// after creating the context, e.g.:
///
/// ```swift
/// // After: let ctx = blinc_create_context(...)
/// BlincKeyboardHelper.blincContext = ctx
/// ```
///
/// Without that, the keyboard pops up but every typed character
/// is silently dropped — the delegate has no context pointer to
/// forward into.
class BlincKeyboardHelper: NSObject, UITextFieldDelegate {
    static let shared = BlincKeyboardHelper()

    /// Active Blinc render context. Set by `BlincViewController`
    /// (or whatever owns the context) after `blinc_create_context`
    /// returns. Read by the delegate methods to forward typed
    /// characters into the Rust runtime.
    static var blincContext: OpaquePointer? = nil

    /// The hidden text field that hosts the soft keyboard.
    /// `BlincEditMenuHelper` reads this to anchor `UIMenuController`
    /// against it (so the menu's `canPerformAction` queries hit the
    /// hidden field's overrides) and to set the `blincEditMenuActions`
    /// bitmask before showing the menu.
    fileprivate(set) var hiddenTextField: BlincHiddenTextField?

    private override init() {
        super.init()

        // Subscribe to keyboard frame notifications. We use
        // `WillChangeFrame` rather than `WillShow` / `WillHide`
        // because the former fires for every transition, including
        // hardware-keyboard attach (which collapses the soft
        // keyboard to a small inline accessory bar — height drops
        // but isn't zero), interactive dismissal (the user dragging
        // the keyboard down), and split-keyboard / floating-keyboard
        // mode changes on iPad. `WillShow`/`WillHide` miss those.
        //
        // The notification's `userInfo` contains:
        //   * `UIKeyboardFrameEndUserInfoKey`   — final frame in
        //     SCREEN coordinates as `NSValue<CGRect>`. We compute the
        //     intersection with the current key window's bounds to
        //     get the actually-obscured area (the keyboard frame can
        //     extend below the bottom of the screen during the
        //     animation), then take the height in POINTS (which is
        //     UIKit's logical-pixel unit — same coordinate space the
        //     Rust runner stores in `WindowedContext.width/height`).
        //   * `UIKeyboardAnimationDurationUserInfoKey` /
        //     `UIKeyboardAnimationCurveUserInfoKey` — duration and
        //     timing curve of the system animation. We don't push
        //     these into Rust right now (the runner just snaps to
        //     the new inset on the next frame), but they're worth
        //     a future hook for matching the system curve when
        //     we animate the scroll-into-view.
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleKeyboardFrameChange(_:)),
            name: UIResponder.keyboardWillChangeFrameNotification,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleKeyboardFrameChange(_:)),
            name: UIResponder.keyboardWillHideNotification,
            object: nil
        )
    }

    deinit {
        NotificationCenter.default.removeObserver(self)
    }

    /// Compute the inset (height of the screen the keyboard is
    /// covering) and forward to Rust via the new FFI export.
    /// `keyboardWillHide` is wired to the same handler — UIKit
    /// posts a final frame at the bottom of the screen for the
    /// hide path, so the intersection-with-window-bounds math
    /// naturally produces zero in that case.
    @objc private func handleKeyboardFrameChange(_ notification: Notification) {
        guard let userInfo = notification.userInfo else { return }
        guard let endFrameValue = userInfo[UIResponder.keyboardFrameEndUserInfoKey] as? NSValue else { return }
        let keyboardFrameInScreen = endFrameValue.cgRectValue

        // Find the active key window so we can convert from screen
        // coordinates and compute the intersection with the visible
        // area. On iOS 13+ we go through `connectedScenes`; the
        // top-most foreground active scene's first key window is the
        // one our `BlincViewController` lives in.
        let keyWindow: UIWindow? = UIApplication.shared
            .connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .filter { $0.activationState == .foregroundActive }
            .flatMap { $0.windows }
            .first { $0.isKeyWindow }

        guard let window = keyWindow else { return }

        let keyboardFrameInWindow = window.convert(keyboardFrameInScreen, from: nil)
        let intersection = keyboardFrameInWindow.intersection(window.bounds)

        // `intersection.height` is in points (UIKit logical units).
        // The Rust runner already stores logical pixels in
        // `WindowedContext.width/height`, so this maps 1:1 with no
        // DPI conversion needed.
        let insetPoints = intersection.isNull ? 0.0 : Double(intersection.height)

        if let ctx = BlincKeyboardHelper.blincContext {
            blinc_ios_set_keyboard_inset(ctx, Float(insetPoints * Double(UIScreen.main.scale) / blinc_get_scale_factor(ctx)))
        }
    }

    func showKeyboard() {
        if BlincAccessibility.focusCurrentInput() { return }
        if hiddenTextField == nil {
            // Position the hidden text field off-screen at
            // (-1000, -1000). It exists only to host the soft
            // keyboard and dispatch keystrokes back into Rust —
            // it must never be visible, never receive touches,
            // and never participate in layout. The off-screen
            // frame is the simplest way to satisfy all three.
            //
            // The UIKit edit-menu chrome's NaN-on-empty-text
            // problem is handled separately by the
            // `caretRect`/`firstRect`/`selectionRects` overrides
            // on `BlincHiddenTextField`, which return finite
            // 1pt rects regardless of the field's frame.
            let tf = BlincHiddenTextField(frame: CGRect(x: -1000, y: -1000, width: 1, height: 1))
            tf.autocorrectionType = .no
            tf.autocapitalizationType = .none
            tf.spellCheckingType = .no
            tf.delegate = self
            tf.blincDelegate = self
            if let windowScene = UIApplication.shared.connectedScenes.first as? UIWindowScene,
               let window = windowScene.windows.first {
                window.addSubview(tf)
            }
            hiddenTextField = tf
        }
        hiddenTextField?.becomeFirstResponder()
    }

    func hideKeyboard() {
        BlincAccessibility.editingField?.resignFirstResponder()
        hiddenTextField?.resignFirstResponder()
    }

    /// Called by `BlincHiddenTextField.deleteBackward` when the
    /// user presses backspace. Forwards as virtual key code 8 so
    /// the Rust `text_input` widget's `on_key_down` handler runs
    /// `delete_backward()` (matches the desktop runner's table).
    func didPressBackspace() {
        if let ctx = BlincKeyboardHelper.blincContext {
            blinc_ios_handle_key_down(ctx, 8)
        }
    }

    /// Forward typed characters to the Rust text-input widget.
    ///
    /// The hidden `UITextField` is purely a keyboard host — its
    /// own buffer is irrelevant and we clear it on every change
    /// to prevent accumulation. The actual character dispatch
    /// happens via `blinc_ios_handle_text_input`, which
    /// broadcasts the event through the render tree to whichever
    /// Blinc text-input widget is currently focused.
    ///
    /// Backspace on an EMPTY field is NOT delivered through this
    /// delegate — see `BlincHiddenTextField.deleteBackward`.
    /// Backspace WHILE the field has content (rare, since we
    /// keep it empty) lands here with `range.length > 0,
    /// string.isEmpty` and we forward it via the same
    /// `didPressBackspace` path.
    ///
    /// Returning `false` tells UITextField NOT to apply the
    /// replacement to its buffer (we don't want it accumulating
    /// state). The `textField.text = ""` clear is belt-and-
    /// suspenders for autocorrect / dictation paths that might
    /// stuff text in despite the `false` return.
    func textField(_ textField: UITextField, shouldChangeCharactersIn range: NSRange, replacementString string: String) -> Bool {
        if let ctx = BlincKeyboardHelper.blincContext {
            if range.length > 0 && string.isEmpty {
                // Backspace path with non-empty field. Same
                // forwarding as `deleteBackward`.
                blinc_ios_handle_key_down(ctx, 8)
            } else if !string.isEmpty {
                // Normal character insert (or autocorrect /
                // dictation multi-char insert).
                //
                // The bridging header declares both FFI functions
                // as `IOSRenderContext* _Nonnull ctx`, which
                // Swift bridges as `OpaquePointer` (the opaque-
                // struct C type maps to `OpaquePointer`, NOT
                // `UnsafeMutablePointer<T>`). Pass `ctx` directly
                // without wrapping.
                string.withCString { ptr in
                    blinc_ios_handle_text_input(ctx, ptr)
                }
            }
        }

        // Always clear the hidden field — Blinc owns the text
        // buffer, the UITextField is just the keyboard host.
        textField.text = ""
        return false
    }

    /// Return-key handler. Forwards as virtual key code 13
    /// (matches the desktop runner's table for Enter).
    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        if let ctx = BlincKeyboardHelper.blincContext {
            blinc_ios_handle_key_down(ctx, 13)
        }
        return false
    }
}

// MARK: - Edit Menu Helper

/// Native iOS edit menu (Cut / Copy / Paste / Select All) shown over
/// the focused Blinc text-editable widget on double-tap or long press.
///
/// On iOS 16+ this uses `UIEditMenuInteraction` (the modern,
/// recommended API). On iOS 13–15 it falls back to the legacy
/// `UIMenuController` API. Both routes anchor against
/// `BlincHiddenTextField`, the same hidden first-responder view
/// `BlincKeyboardHelper` uses to host the soft keyboard.
///
/// The menu's actions are routed back to Rust via
/// `blinc_ios_handle_key_down_with_modifiers(ctx, key_code, 0x08)`,
/// synthesizing the same Cmd+key codes Blinc's text-editable widgets
/// already handle in their `on_key_down` paths:
///
///   - Cut        → key code 88 (Cmd+X)
///   - Copy       → key code 67 (Cmd+C)
///   - Paste      → key code 86 (Cmd+V)
///   - Select All → key code 65 (Cmd+A)
///
/// The dispatch lives in the four `UIResponderStandardEditActions`
/// overrides (`cut(_:)`, `copy(_:)`, `paste(_:)`, `selectAll(_:)`)
/// on `BlincHiddenTextField`. Both the modern and legacy menu APIs
/// query the first responder's `canPerformAction(_:withSender:)`
/// before showing items, so the bitmask the Rust side passes
/// through (`blincEditMenuActions`) controls which items render.
class BlincEditMenuHelper: NSObject {
    static let shared = BlincEditMenuHelper()

    /// iOS 16+ modern menu interaction. Lazily created on first
    /// `show()` because the type isn't available pre-16. Stored as
    /// `Any?` so the file still compiles on iOS 13 SDKs (it would
    /// otherwise need an `@available` on the property itself).
    private var editMenuInteraction: Any? = nil

    private override init() {
        super.init()
    }

    func show(anchor: CGPoint, selectionRect: CGRect, actions: Int) {
        // Sanitize coordinates: replace NaN/inf with finite fallbacks
        // and ensure the selection rect has a meaningful non-zero
        // area. UIKit's edit-menu chrome layout divides by source-
        // rect-derived metrics in places, and tiny (1pt-wide) rects
        // trigger the same NaN cascade as zero-area rects. The
        // minimum 32x44 here is the typical hit-test target for an
        // iOS text-selection caret — small enough not to obscure
        // anything, large enough that UIKit's chrome math always
        // resolves to finite values.
        let MIN_W: CGFloat = 32
        let MIN_H: CGFloat = 44
        let safeAnchorX = anchor.x.isFinite ? anchor.x : 0
        let safeAnchorY = anchor.y.isFinite ? anchor.y : 0
        let safeAnchor = CGPoint(x: safeAnchorX, y: safeAnchorY)
        let rawX = selectionRect.origin.x.isFinite ? selectionRect.origin.x : safeAnchorX
        let rawY = selectionRect.origin.y.isFinite ? selectionRect.origin.y : safeAnchorY
        let rawW = (selectionRect.width.isFinite && selectionRect.width >= MIN_W) ? selectionRect.width : MIN_W
        let rawH = (selectionRect.height.isFinite && selectionRect.height >= MIN_H) ? selectionRect.height : MIN_H
        // Center the minimum rect on the anchor when the original
        // selection rect was a zero-width caret.
        let safeRectX = (selectionRect.width >= MIN_W) ? rawX : safeAnchorX - rawW / 2
        let safeRectY = (selectionRect.height >= MIN_H) ? rawY : safeAnchorY - rawH / 2
        let safeRect = CGRect(x: safeRectX, y: safeRectY, width: rawW, height: rawH)

        // Make sure the hidden text field exists and is the first
        // responder. `showKeyboard()` is idempotent — if the keyboard
        // is already up (the common case, since the user just
        // double-tapped a focused input) it just calls
        // `becomeFirstResponder()` on the existing field.
        BlincKeyboardHelper.shared.showKeyboard()

        guard let hidden = BlincKeyboardHelper.shared.hiddenTextField else {
            return
        }
        guard let window = currentKeyWindow() else {
            return
        }
        // Use the root view controller's view as the host for the
        // interaction, NOT the window itself. UIEditMenuInteraction
        // is designed to live on a regular UIView; hosting on a
        // UIWindow confuses UIKit's chrome layout pipeline (the
        // chrome's coordinate-space resolution against a UIWindow
        // produces NaN values during the presentation animation
        // setup with no calls to the responder geometry overrides
        // — i.e. it's not the responder's fault, it's the host
        // view's class).
        let hostView = window.rootViewController?.view ?? window

        // Tell the hidden text field which standard menu items it
        // should report as available the next time
        // `canPerformAction(_:withSender:)` is queried. The override
        // on `BlincHiddenTextField` reads this bitmask to decide.
        // We still set this even though the iOS 16+ path returns a
        // fully custom menu — the legacy `UIMenuController` fallback
        // (iOS 13-15) reads it from `canPerformAction`.
        hidden.blincEditMenuActions = actions

        // iOS 16+: use UIEditMenuInteraction. UIMenuController is
        // deprecated in 16 and on a UITextField first responder
        // `showMenu(from:rect:)` is mostly silently ignored.
        //
        // Critical: the interaction must be added to a view that
        // is **on screen** because `presentEditMenu` positions the
        // menu in that view's coordinate space. We host on the
        // root view controller's view — the Rust side passes the
        // anchor in window coords, which equals root view coords
        // for any normal app where the root VC's view fills the
        // window.
        if #available(iOS 16.0, *) {
            let interaction: UIEditMenuInteraction
            if let existing = editMenuInteraction as? UIEditMenuInteraction,
               existing.view === hostView {
                interaction = existing
            } else {
                // If we previously installed the interaction on a
                // different view (e.g. an old window after a scene
                // change), tear it down and re-add to the current
                // host view.
                if let old = editMenuInteraction as? UIEditMenuInteraction,
                   let oldView = old.view {
                    oldView.removeInteraction(old)
                }
                let new = UIEditMenuInteraction(delegate: BlincEditMenuInteractionDelegate.shared)
                hostView.addInteraction(new)
                editMenuInteraction = new
                interaction = new
            }
            // Stash the current actions and target rect on the
            // delegate so its `menuFor:` and `targetRectFor:`
            // callbacks know which items to build and where to
            // anchor. We also pre-compute and stash the target rect
            // because UIKit's default "small rect at source point"
            // implementation produces NaN values during chrome
            // layout when the source rect has zero area, so we
            // explicitly hand UIKit a finite, non-zero rect.
            BlincEditMenuInteractionDelegate.shared.currentActions = actions
            BlincEditMenuInteractionDelegate.shared.currentTargetRect = safeRect
            let config = UIEditMenuConfiguration(
                identifier: "blinc.editMenu" as NSString,
                sourcePoint: safeAnchor
            )
            interaction.presentEditMenu(with: config)
            return
        }

        // iOS 13-15 fallback: legacy UIMenuController. This API
        // takes the rect in the *anchor view's* coordinate space.
        // We use the host view so the rect is in root-view coords
        // (matching what the Rust side passes in).
        let menu = UIMenuController.shared
        if #available(iOS 13.0, *) {
            menu.showMenu(from: hostView, rect: safeRect)
        }
    }

    func hide() {
        if #available(iOS 16.0, *) {
            if let interaction = editMenuInteraction as? UIEditMenuInteraction {
                interaction.dismissMenu()
            }
        } else if #available(iOS 13.0, *) {
            UIMenuController.shared.hideMenu()
        }
        // Clear the actions bitmask so a stale double-tap doesn't
        // leave the field reporting actions as available the next
        // time something else queries `canPerformAction`.
        BlincKeyboardHelper.shared.hiddenTextField?.blincEditMenuActions = 0
    }

    fileprivate func currentKeyWindow() -> UIWindow? {
        return UIApplication.shared
            .connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .filter { $0.activationState == .foregroundActive }
            .flatMap { $0.windows }
            .first { $0.isKeyWindow }
    }
}

/// iOS 16+ delegate that builds the `UIMenu` for the modern
/// `UIEditMenuInteraction`. Stored as a singleton because
/// `UIEditMenuInteraction` only holds a weak reference to its
/// delegate — making it a property of `BlincEditMenuHelper` would
/// risk the delegate being deallocated mid-presentation.
@available(iOS 16.0, *)
class BlincEditMenuInteractionDelegate: NSObject, UIEditMenuInteractionDelegate {
    static let shared = BlincEditMenuInteractionDelegate()

    /// Bitmask of actions to expose, captured from the Rust
    /// side at `BlincEditMenuHelper.show(...)` time.
    var currentActions: Int = 0

    /// Target rect (in the host view's coordinate space) the
    /// edit menu should anchor against. Set by
    /// `BlincEditMenuHelper.show(...)` immediately before calling
    /// `presentEditMenu`. The rect is sanitized to have non-zero
    /// width and height — UIKit's chrome layout for the edit
    /// menu performs CG arithmetic that produces NaN values when
    /// it's handed a zero-area rect, which manifests as dozens of
    /// CoreGraphics warnings and an invisible menu.
    var currentTargetRect: CGRect = CGRect(x: 0, y: 0, width: 1, height: 24)

    /// Tell UIKit exactly which rect the menu should point at.
    /// Without this delegate method UIKit defaults to "small rect
    /// at the source point" which, in practice, causes a
    /// CoreGraphics divide-by-zero somewhere inside the chrome
    /// layout pipeline (see `currentTargetRect`).
    func editMenuInteraction(
        _ interaction: UIEditMenuInteraction,
        targetRectFor configuration: UIEditMenuConfiguration
    ) -> CGRect {
        return currentTargetRect
    }

    func editMenuInteraction(
        _ interaction: UIEditMenuInteraction,
        menuFor configuration: UIEditMenuConfiguration,
        suggestedActions: [UIMenuElement]
    ) -> UIMenu? {
        // Build a menu from the Rust-supplied actions bitmask.
        // Bit layout matches `text_edit::edit_menu_actions`:
        //   bit 0 = Cut, bit 1 = Copy, bit 2 = Paste, bit 3 = Select All.
        //
        // Each action dispatches a synthesized Cmd+key event into
        // Rust through `BlincHiddenTextField`'s standard edit-action
        // overrides. We call them via the responder chain
        // (`hidden.cut(_:)` etc.) instead of via `UIApplication.sendAction`
        // so the dispatch is unconditional — the responder chain
        // would otherwise consult `canPerformAction` again and
        // could short-circuit.
        guard let hidden = BlincKeyboardHelper.shared.hiddenTextField else {
            return nil
        }

        var children: [UIMenuElement] = []
        if currentActions & 0x01 != 0 {
            children.append(UIAction(title: "Cut") { _ in
                hidden.cut(nil)
            })
        }
        if currentActions & 0x02 != 0 {
            children.append(UIAction(title: "Copy") { _ in
                hidden.copy(nil)
            })
        }
        if currentActions & 0x04 != 0 {
            children.append(UIAction(title: "Paste") { _ in
                hidden.paste(nil)
            })
        }
        if currentActions & 0x08 != 0 {
            children.append(UIAction(title: "Select All") { _ in
                hidden.selectAll(nil)
            })
        }

        if children.isEmpty {
            return nil
        }
        return UIMenu(title: "", children: children)
    }
}

// MARK: - C FFI Entry Point

/// C function called by Rust to execute native handlers
/// Returns a malloc'd string that Rust must free with blinc_free_string
@_cdecl("blinc_ios_native_call")
public func blinc_ios_native_call(
    ns: UnsafePointer<CChar>,
    name: UnsafePointer<CChar>,
    argsJson: UnsafePointer<CChar>
) -> UnsafeMutablePointer<CChar>? {
    let namespace = String(cString: ns)
    let funcName = String(cString: name)
    let args = String(cString: argsJson)

    let result = BlincNativeBridge.shared.callNative(
        namespace: namespace,
        name: funcName,
        argsJson: args
    )

    return strdup(result)
}

/// Free a string allocated by blinc_ios_native_call
@_cdecl("blinc_free_string")
public func blinc_free_string(ptr: UnsafeMutablePointer<CChar>?) {
    if let ptr = ptr {
        free(ptr)
    }
}

/// Show the soft keyboard (called from Rust frame loop)
@_cdecl("blinc_ios_show_keyboard")
public func blinc_ios_show_keyboard() {
    DispatchQueue.main.async {
        BlincKeyboardHelper.shared.showKeyboard()
    }
}

/// Hide the soft keyboard (called from Rust frame loop)
@_cdecl("blinc_ios_hide_keyboard")
public func blinc_ios_hide_keyboard() {
    DispatchQueue.main.async {
        BlincKeyboardHelper.shared.hideKeyboard()
    }
}
