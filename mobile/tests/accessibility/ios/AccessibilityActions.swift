import XCTest
@testable import BlincApp

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
        let accepted = input.delegate?.textField?(input, shouldChangeCharactersIn: NSRange(location: 0, length: 0), replacementString: "Ada")
        XCTAssertEqual(accepted, false)
        eventually { input.accessibilityValue == "Ada" }
        XCTAssertEqual(input.delegate?.textFieldShouldReturn?(input), false)
        eventually { element("Count: 11") != nil }
        XCTAssertGreaterThan(input.accessibilityFrame.width, 0)
        XCTAssertGreaterThanOrEqual(input.accessibilityFrame.height, 44)
    }
}
