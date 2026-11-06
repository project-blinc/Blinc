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
    static weak var editingField: UIResponder?
    private static weak var current: BlincAccessibility?
    private weak var focusedField: BlincAccessibleTextField?
    private var multilineEditor: BlincImeTextView?

    static func focusCurrentInput() -> Bool {
        if editingField?.isFirstResponder == true { return true }
        current?.synchronize()
        if editingField?.isFirstResponder == true { return true }
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
        multilineEditor?.resignFirstResponder()
        multilineEditor?.removeFromSuperview()
        multilineEditor = nil
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

    fileprivate func readIme() -> BlincImeContent? {
        guard let context, let raw = blinc_ime_snapshot(context) else { return nil }
        defer { blinc_ime_free(raw) }
        return try? JSONDecoder().decode(BlincImeContent.self, from: Data(bytes: raw, count: strlen(raw)))
    }

    fileprivate func editIme(_ json: String) -> Bool {
        guard let context else { return false }
        return json.withCString { blinc_ime_edit(context, $0) }
    }

    private func synchronizeIme() {
        guard let value = readIme() else {
            multilineEditor?.resignFirstResponder()
            multilineEditor?.removeFromSuperview()
            multilineEditor = nil
            return
        }
        if let field = elements[value.id] as? BlincAccessibleTextField, !value.multiline {
            multilineEditor?.resignFirstResponder()
            multilineEditor?.removeFromSuperview()
            multilineEditor = nil
            field.connection.receive(value)
            if !field.isFirstResponder { field.becomeFirstResponder() }
        } else {
            if multilineEditor?.connection.id != value.id {
                multilineEditor?.resignFirstResponder()
                multilineEditor?.removeFromSuperview()
                multilineEditor = BlincImeTextView(owner: self, id: value.id)
                addSubview(multilineEditor!)
            }
            multilineEditor?.connection.receive(value)
            if multilineEditor?.isFirstResponder == false { multilineEditor?.becomeFirstResponder() }
        }
    }

    func synchronize() {
        guard let context, let raw = blinc_accessibility_snapshot(context) else { return }
        defer { blinc_accessibility_free(raw) }
        let data = Data(bytes: raw, count: strlen(raw))
        defer { synchronizeIme() }
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
        } else if focusedField == nil, let editing = Self.editingField as? UIView, editing.superview === self, editing !== multilineEditor {
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
    fileprivate lazy var connection = BlincImeConnection(owner: owner, host: self, id: id)

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
        addTarget(self, action: #selector(nativeTextChanged), for: .editingChanged)
    }

    @objc private func nativeTextChanged() { connection.publish() }

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
        if !node.focused && text != node.value { text = node.value }
        isEnabled = !node.disabled
        if isSecureTextEntry != node.secure { isSecureTextEntry = node.secure }
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
        if let value = owner?.readIme() { connection.receive(value) }
        BlincAccessibility.editingField = self
        return super.becomeFirstResponder()
    }

    override func setMarkedText(_ markedText: String?, selectedRange: NSRange) {
        connection.begin(); defer { connection.end() }
        super.setMarkedText(markedText, selectedRange: selectedRange)
    }
    override func unmarkText() {
        connection.begin(); defer { connection.end() }
        super.unmarkText()
    }
    override func insertText(_ text: String) {
        connection.begin(); defer { connection.end() }
        super.insertText(text)
    }
    override func deleteBackward() {
        connection.begin(); defer { connection.end() }
        super.deleteBackward()
    }
    override func replace(_ range: UITextRange, withText text: String) {
        connection.begin(); defer { connection.end() }
        super.replace(range, withText: text)
    }
    func textFieldDidChangeSelection(_ textField: UITextField) { connection.publish() }
    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        if markedTextRange != nil { unmarkText(); return false }
        owner?.submit(id)
        return false
    }
    func textField(_ textField: UITextField, shouldChangeCharactersIn range: NSRange, replacementString string: String) -> Bool { true }
}

private struct BlincImeContent: Decodable {
    let id, session, text: String
    let sequence: UInt64
    let selection: [Int]
    let composing: [Int]?
    let secure, multiline, autocorrect: Bool
    let x, y, width, height, fontSize: CGFloat
}

private final class BlincImeConnection {
    weak var owner: BlincAccessibility?
    weak var host: UIView?
    let id: String
    private var session = ""
    private var sequence: UInt64 = 0
    private var depth = 0
    private var applying = false
    private var lastText: String?
    private var lastSelection: [Int]?
    private var lastComposing: [Int]?
    private(set) var multiline = true

    init(owner: BlincAccessibility?, host: UIView, id: String) {
        self.owner = owner; self.host = host; self.id = id
    }
    func begin() { depth += 1 }
    func end() { depth -= 1; publish() }

    private var input: (any UITextInput)? { host as? any UITextInput }
    private var text: String {
        (host as? UITextField)?.text ?? (host as? UITextView)?.text ?? ""
    }
    private func offsets(_ range: UITextRange?) -> [Int]? {
        guard let input, let range else { return nil }
        return [input.offset(from: input.beginningOfDocument, to: range.start), input.offset(from: input.beginningOfDocument, to: range.end)]
    }

    func receive(_ value: BlincImeContent) {
        guard value.id == id, session != value.session || value.sequence >= sequence,
              let input, let host else { return }
        applying = true; defer { applying = false }
        session = value.session; sequence = value.sequence
        multiline = value.multiline
        let scale = BlincContentSize.factor(in: host)
        host.frame = CGRect(x: value.x * scale, y: value.y * scale, width: value.width * scale, height: value.height * scale)
        let font = UIFont.systemFont(ofSize: value.fontSize * scale)
        if let field = host as? UITextField {
            if field.font?.pointSize != font.pointSize { field.font = font }
            if field.isSecureTextEntry != value.secure { field.isSecureTextEntry = value.secure }
        }
        if let area = host as? UITextView {
            if area.font?.pointSize != font.pointSize { area.font = font }
            area.autocorrectionType = value.autocorrect ? .default : .no
            area.autocapitalizationType = value.autocorrect ? .sentences : .none
            area.smartQuotesType = value.autocorrect ? .default : .no
            area.smartDashesType = value.autocorrect ? .default : .no
        }
        if value.composing == nil && input.markedTextRange != nil { input.unmarkText() }
        if text != value.text {
            (host as? UITextField)?.text = value.text
            (host as? UITextView)?.text = value.text
        }
        if offsets(input.selectedTextRange) != value.selection, value.selection.count == 2,
           let a = input.position(from: input.beginningOfDocument, offset: value.selection[0]),
           let b = input.position(from: input.beginningOfDocument, offset: value.selection[1]) {
            input.selectedTextRange = input.textRange(from: a, to: b)
        }
        lastText = value.text; lastSelection = value.selection; lastComposing = value.composing
    }

    func publish(submit: Bool = false) {
        guard !applying, depth == 0, !session.isEmpty, let input,
              let selection = offsets(input.selectedTextRange) else { return }
        let composing = offsets(input.markedTextRange)
        let current = text
        guard submit || current != lastText || selection != lastSelection || composing != lastComposing else { return }
        sequence &+= 1
        let edit: [String: Any] = ["id": id, "session": session, "sequence": sequence,
            "text": current, "selection": selection, "composing": composing as Any? ?? NSNull(), "submit": submit]
        guard let data = try? JSONSerialization.data(withJSONObject: edit), let json = String(data: data, encoding: .utf8) else { return }
        if owner?.editIme(json) == true {
            lastText = current; lastSelection = selection; lastComposing = composing
        } else { sequence &-= 1 }
        if let value = owner?.readIme() { receive(value) }
    }
}

private final class BlincImeTextView: UITextView, UITextViewDelegate {
    fileprivate lazy var connection = BlincImeConnection(owner: owner, host: self, id: id)
    private weak var owner: BlincAccessibility?
    private let id: String
    init(owner: BlincAccessibility, id: String) {
        self.owner = owner; self.id = id
        super.init(frame: .zero, textContainer: nil)
        delegate = self
        backgroundColor = .clear
        tintColor = .clear
        accessibilityElementsHidden = true
        autocorrectionType = .default
    }
    required init?(coder: NSCoder) { fatalError("Use init(owner:id:)") }
    override func becomeFirstResponder() -> Bool {
        if let value = owner?.readIme() { connection.receive(value) }
        BlincAccessibility.editingField = self
        return super.becomeFirstResponder()
    }
    override func layoutSubviews() {
        super.layoutSubviews()
        layer.sublayers?.forEach { $0.opacity = 0 }
    }
    override func setMarkedText(_ markedText: String?, selectedRange: NSRange) {
        connection.begin(); defer { connection.end() }
        super.setMarkedText(markedText, selectedRange: selectedRange)
    }
    override func unmarkText() {
        connection.begin(); defer { connection.end() }
        super.unmarkText()
    }
    override func insertText(_ text: String) {
        if !connection.multiline && text == "\n" {
            if markedTextRange != nil { unmarkText() } else { connection.publish(submit: true) }
            return
        }
        connection.begin(); defer { connection.end() }
        super.insertText(text)
    }
    override func deleteBackward() {
        connection.begin(); defer { connection.end() }
        super.deleteBackward()
    }
    override func replace(_ range: UITextRange, withText text: String) {
        connection.begin(); defer { connection.end() }
        super.replace(range, withText: text)
    }
    func textViewDidChange(_ textView: UITextView) { connection.publish() }
    func textViewDidChangeSelection(_ textView: UITextView) { connection.publish() }
}
