import XCTest
@testable import BlincApp

@_silgen_name("blinc_test_ime_changes")
private func imeChanges() -> UInt

@_silgen_name("blinc_test_emoji_covered")
private func emojiCovered() -> UInt8

@_silgen_name("blinc_test_cjk_covered")
private func cjkCovered() -> UInt8

@MainActor
final class BlincAccessibilityActions: XCTestCase {
    private func eventually(_ check: @escaping () -> Bool, file: StaticString = #filePath, line: UInt = #line) {
        let ready = expectation(description: "Semantic update")
        let timer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { timer in
            if check() { timer.invalidate(); ready.fulfill() }
        }
        wait(for: [ready], timeout: 10)
        timer.invalidate()
        XCTAssertTrue(check(), file: file, line: line)
    }

    func testNativeAssistiveActionsUpdateTheWidgetsAndRetainIdentity() {
        var bridge: BlincAccessibility?
        eventually {
            bridge = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
                .flatMap { $0.windows }.compactMap { $0.rootViewController?.view }
                .flatMap { $0.subviews }.compactMap { $0 as? BlincAccessibility }.first
            return !(bridge?.accessibilityElements ?? []).isEmpty
        }
        guard let bridge else { return XCTFail("Missing accessibility container") }
        func element(_ label: String) -> NSObject? {
            (bridge.accessibilityElements ?? []).compactMap { $0 as? NSObject }
                .first { $0.accessibilityLabel == label }
        }
        guard let increment = element("Increment"), let checkbox = element("Enable notifications"),
              let input = (bridge.accessibilityElements ?? []).compactMap({ $0 as? UITextField }).first(where: { $0.accessibilityLabel == "Name" }),
              let unavailable = element("Unavailable") else {
            return XCTFail("Missing native semantic control")
        }
        XCTAssertTrue(increment.accessibilityActivate())
        eventually { element("Count: 1") != nil }
        XCTAssertTrue(element("Increment") === increment)
        XCTAssertTrue(checkbox.accessibilityActivate())
        eventually { checkbox.accessibilityValue == "1" }
        XCTAssertFalse(unavailable.accessibilityActivate())
        XCTAssertTrue(unavailable.accessibilityTraits.contains(.notEnabled))
        guard let volume = element("Volume") else { return XCTFail("Missing adjustable slider") }
        XCTAssertTrue(volume.accessibilityTraits.contains(.adjustable))
        XCTAssertEqual(volume.accessibilityValue, "50")
        volume.accessibilityIncrement()
        eventually { volume.accessibilityValue == "55" }
        XCTAssertTrue(element("Volume") === volume)
        volume.accessibilityDecrement()
        eventually { volume.accessibilityValue == "50" }
        XCTAssertTrue(input.accessibilityActivate())
        XCTAssertEqual(emojiCovered(), 1, "The system emoji font must render a colour fox")
        XCTAssertEqual(cjkCovered(), 1, "Candidate characters need visible fallback glyphs")
        input.insertText("A🦊")
        let changes = imeChanges()
        input.setMarkedText("ni", selectedRange: NSRange(location: 2, length: 0))
        XCTAssertNotNil(input.markedTextRange)
        eventually { input.accessibilityValue == "A🦊ni" }
        XCTAssertEqual(imeChanges(), changes)
        input.setMarkedText("nihao", selectedRange: NSRange(location: 5, length: 0))
        XCTAssertNotNil(input.markedTextRange)
        eventually { input.accessibilityValue == "A🦊nihao" }
        XCTAssertEqual(imeChanges(), changes)
        input.insertText("你好")
        XCTAssertNil(input.markedTextRange)
        eventually { input.accessibilityValue == "A🦊你好" }
        XCTAssertEqual(imeChanges(), changes + 1)
        if let window = bridge.window {
            let image = UIGraphicsImageRenderer(bounds: window.bounds).image { _ in
                window.drawHierarchy(in: window.bounds, afterScreenUpdates: true)
            }
            let composed = XCTAttachment(image: image)
            composed.name = "Composed input with colour emoji"
            composed.lifetime = .keepAlways
            add(composed)
        }
        input.setMarkedText("ma", selectedRange: NSRange(location: 2, length: 0))
        XCTAssertEqual(input.delegate?.textFieldShouldReturn?(input), false)
        XCTAssertNil(input.markedTextRange)
        eventually { input.accessibilityValue == "A🦊你好ma" }
        XCTAssertEqual(imeChanges(), changes + 2)
        XCTAssertNotNil(element("Count: 1"))
        input.selectedTextRange = input.textRange(from: input.beginningOfDocument, to: input.endOfDocument)
        input.insertText("")
        input.insertText("Ada")
        eventually { input.accessibilityValue == "Ada" }
        XCTAssertEqual(input.delegate?.textFieldShouldReturn?(input), false)
        eventually { element("Count: 11") != nil }
        XCTAssertGreaterThan(input.accessibilityFrame.width, 0)
        XCTAssertGreaterThanOrEqual(input.accessibilityFrame.height, 44)
    }
}
