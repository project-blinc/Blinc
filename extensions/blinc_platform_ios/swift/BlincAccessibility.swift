import UIKit

/// Scales the logical viewport with the user's Dynamic Type preference.
enum BlincContentSize {
    static func factor(in view: UIView) -> CGFloat {
        UIFontMetrics.default.scaledValue(for: 17, compatibleWith: view.traitCollection) / 17
    }

    static func renderScale(in view: UIView) -> CGFloat {
        UIScreen.main.scale * factor(in: view)
    }

    static func logicalPoint(_ point: CGPoint, in view: UIView) -> CGPoint {
        let scale = factor(in: view)
        return CGPoint(x: point.x / scale, y: point.y / scale)
    }
}

private struct BlincSemanticNode: Decodable {
    let id: String
    let identifier: String?
    let role, label, value, hint, placeholder: String
    let x, y, width, height: CGFloat
    let disabled, selected, secure, focused: Bool
    let checked, expanded: Bool?
    let selection: [Int]?
    let foreground: [CGFloat]?
    let fontSize: CGFloat?

    var frame: CGRect { CGRect(x: x, y: y, width: width, height: height) }
}

/// A native accessibility container for the GPU-rendered controls.
final class BlincAccessibility: UIView {
    private var context: OpaquePointer?
    private var elements: [String: NSObject] = [:]
    private var previousSnapshot: Data?
    static weak var editingField: UITextField?
    private static weak var current: BlincAccessibility?
    private weak var focusedField: BlincAccessibleTextField?

    static func focusCurrentInput() -> Bool {
        if editingField?.isFirstResponder == true { return true }
        current?.synchronize()
        return current?.focusedField?.becomeFirstResponder() ?? false
    }

    init(host: UIView, context: OpaquePointer) {
        self.context = context
        super.init(frame: host.bounds)
        autoresizingMask = [.flexibleWidth, .flexibleHeight]
        isAccessibilityElement = false
        backgroundColor = .clear
        host.addSubview(self)
        Self.current = self
    }

    required init?(coder: NSCoder) { fatalError("Use init(host:context:)") }

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        nil
    }

    func disconnect() {
        context = nil
        accessibilityElements = []
        for item in elements.values { (item as? UIView)?.removeFromSuperview() }
        elements.removeAll()
    }

    @discardableResult
    fileprivate func perform(_ id: String, action: UInt32, value: String = "") -> Bool {
        guard let context, let id = UInt64(id) else { return false }
        return value.withCString { blinc_accessibility_action(context, id, action, $0) }
    }

    @discardableResult
    fileprivate func submit(_ id: String) -> Bool {
        guard perform(id, action: 1), let context else { return false }
        blinc_ios_handle_key_down(context, 13)
        return true
    }

    func synchronize() {
        guard let context, let raw = blinc_accessibility_snapshot(context) else { return }
        defer { blinc_accessibility_free(raw) }
        let data = Data(bytes: raw, count: strlen(raw))
        guard data != previousSnapshot,
              let nodes = try? JSONDecoder().decode([BlincSemanticNode].self, from: data) else { return }
        previousSnapshot = data
        let scale = CGFloat(blinc_get_scale_factor(context)) / UIScreen.main.scale
        var next: [String: NSObject] = [:]
        var ordered: [Any] = []
        focusedField = nil
        for node in nodes {
            if node.role == "textfield" {
                let field = (elements[node.id] as? BlincAccessibleTextField)
                    ?? BlincAccessibleTextField(owner: self, id: node.id)
                if field.superview == nil { addSubview(field) }
                field.update(node, scale: scale)
                if node.focused { focusedField = field }
                next[node.id] = field
                ordered.append(field)
            } else {
                let element = (elements[node.id] as? BlincAccessibleElement)
                    ?? BlincAccessibleElement(owner: self, id: node.id)
                element.update(node, scale: scale)
                next[node.id] = element
                ordered.append(element)
            }
        }
        let changed = Set(next.keys) != Set(elements.keys)
        for (id, old) in elements where next[id] !== old {
            (old as? UIView)?.resignFirstResponder()
            (old as? UIView)?.removeFromSuperview()
        }
        elements = next
        accessibilityElements = ordered
        if let focusedField, !focusedField.isFirstResponder {
            focusedField.becomeFirstResponder()
        } else if focusedField == nil, let editing = Self.editingField, editing.superview === self {
            editing.resignFirstResponder()
        }
        if changed && UIAccessibility.isVoiceOverRunning {
            UIAccessibility.post(notification: .layoutChanged, argument: nil)
        }
    }
}

private final class BlincAccessibleElement: UIAccessibilityElement {
    private weak var owner: BlincAccessibility?
    private let id: String

    init(owner: BlincAccessibility, id: String) {
        self.owner = owner
        self.id = id
        super.init(accessibilityContainer: owner)
    }

    func update(_ node: BlincSemanticNode, scale: CGFloat) {
        accessibilityIdentifier = node.identifier ?? node.id
        accessibilityLabel = node.label
        accessibilityHint = node.hint.isEmpty ? nil : node.hint
        accessibilityFrameInContainerSpace = node.frame.applying(CGAffineTransform(scaleX: scale, y: scale))
        accessibilityValue = node.checked.map { $0 ? "1" : "0" }
            ?? (node.value.isEmpty ? nil : node.value)
        var traits: UIAccessibilityTraits = []
        switch node.role {
        case "text": traits.insert(.staticText)
        case "heading": traits.formUnion([.staticText, .header])
        case "link": traits.insert(.link)
        case "image": traits.insert(.image)
        case "slider": traits.insert(.adjustable)
        default: traits.insert(.button)
        }
        if node.disabled { traits.insert(.notEnabled) }
        if node.selected || node.checked == true { traits.insert(.selected) }
        accessibilityTraits = traits
        isAccessibilityElement = true
    }

    override func accessibilityActivate() -> Bool { owner?.perform(id, action: 0) ?? false }
    override func accessibilityIncrement() { owner?.perform(id, action: 3) }
    override func accessibilityDecrement() { owner?.perform(id, action: 4) }
}

private final class BlincAccessibleTextField: UITextField, UITextFieldDelegate {
    private weak var owner: BlincAccessibility?
    private let id: String

    init(owner: BlincAccessibility, id: String) {
        self.owner = owner
        self.id = id
        super.init(frame: .zero)
        delegate = self
        textColor = .label
        tintColor = .clear
        backgroundColor = .clear
        autocorrectionType = .no
        autocapitalizationType = .none
        isAccessibilityElement = true
    }

    required init?(coder: NSCoder) { fatalError("Use init(owner:id:)") }

    func update(_ node: BlincSemanticNode, scale: CGFloat) {
        frame = node.frame.applying(CGAffineTransform(scaleX: scale, y: scale))
        if let owner { accessibilityFrame = UIAccessibility.convertToScreenCoordinates(frame, in: owner) }
        font = UIFontMetrics.default.scaledFont(for: UIFont.systemFont(ofSize: node.fontSize ?? 14))
        adjustsFontForContentSizeCategory = true
        accessibilityIdentifier = node.identifier ?? node.id
        accessibilityLabel = node.label
        accessibilityHint = node.hint.isEmpty ? nil : node.hint
        accessibilityValue = node.value.isEmpty ? nil : node.value
        // UIKit draws no text: the GPU remains the visual source.
        if let rgba = node.foreground, rgba.count == 4 {
            textColor = UIColor(red: rgba[0], green: rgba[1], blue: rgba[2], alpha: rgba[3])
        }
        attributedPlaceholder = NSAttributedString(string: node.placeholder, attributes: [.foregroundColor: textColor ?? UIColor.label])
        if text != node.value { text = node.value }
        isEnabled = !node.disabled
        isSecureTextEntry = node.secure
        if node.focused, let selection = node.selection, selection.count == 2,
           let start = position(from: beginningOfDocument, offset: selection[0]),
           let end = position(from: beginningOfDocument, offset: selection[1]) {
            selectedTextRange = textRange(from: start, to: end)
        }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        // Keep UIKit's editing geometry while the GPU draws the content.
        layer.sublayers?.forEach { $0.opacity = 0 }
    }

    override func drawPlaceholder(in rect: CGRect) {}

    override func accessibilityActivate() -> Bool {
        becomeFirstResponder()
    }

    override func becomeFirstResponder() -> Bool {
        guard owner?.perform(id, action: 1) == true else { return false }
        BlincAccessibility.editingField = self
        return super.becomeFirstResponder()
    }

    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        owner?.submit(id)
        return false
    }

    func textField(_ textField: UITextField, shouldChangeCharactersIn range: NSRange, replacementString string: String) -> Bool {
        let current = textField.text ?? ""
        guard let range = Range(range, in: current) else { return false }
        let start = range.lowerBound.utf16Offset(in: current)
        let end = range.upperBound.utf16Offset(in: current)
        let edit: [String: Any] = ["start": start, "end": end, "text": string]
        guard let data = try? JSONSerialization.data(withJSONObject: edit),
              let json = String(data: data, encoding: .utf8),
              owner?.perform(id, action: 5, value: json) == true else { return false }
        owner?.synchronize()
        return false
    }
}
