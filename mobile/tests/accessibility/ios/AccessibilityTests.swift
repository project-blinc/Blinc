import XCTest

final class BlincAccessibilityTests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    private func launchFixture() -> XCUIApplication {
        let app = XCUIApplication(bundleIdentifier: "com.blinc.accessibility.fixture")
        app.launchEnvironment = [
            "BLINC_GPU_MAX_PRIMITIVES": "1024",
            "BLINC_GPU_MAX_GLYPHS": "1024"
        ]
        app.launch()
        XCTAssertTrue(app.wait(for: .runningForeground, timeout: 15))
        let warmup = expectation(description: "Initial GPU frame")
        DispatchQueue.main.asyncAfter(deadline: .now() + 1) { warmup.fulfill() }
        wait(for: [warmup], timeout: 3)
        let screenshot = XCTAttachment(screenshot: app.screenshot())
        screenshot.name = "Rendered fixture"
        screenshot.lifetime = .keepAlways
        add(screenshot)
        let hierarchy = XCTAttachment(string: app.debugDescription)
        hierarchy.name = "Native accessibility hierarchy"
        hierarchy.lifetime = .keepAlways
        add(hierarchy)
        return app
    }

    func testControlsExposeSemanticsAndActions() throws {
        let app = launchFixture()
        let increment = app.buttons["Increment"]
        XCTAssertTrue(increment.waitForExistence(timeout: 5),
                      "The rendered Blinc button must be exposed as an accessible button.")
        increment.tap()
        XCTAssertTrue(app.staticTexts["Count: 1"].waitForExistence(timeout: 3))

        let name = app.textFields["Name"]
        XCTAssertTrue(name.exists, "The input needs its visible label and editable role.")
        name.tap()
        name.typeText("Ada")
        XCTAssertEqual(name.value as? String, "Ada")

        let checkbox = app.descendants(matching: .any)
            .matching(NSPredicate(format: "label == %@", "Enable notifications")).firstMatch
        XCTAssertTrue(checkbox.exists)
        let previous = checkbox.value as? String
        XCTAssertNotNil(previous, "The selected state must be exposed.")
        checkbox.tap()
        XCTAssertNotEqual(checkbox.value as? String, previous)
        let unavailable = app.buttons["Unavailable"]
        XCTAssertTrue(unavailable.exists)
        XCTAssertFalse(unavailable.isEnabled)
    }

    func testSystemAccessibilityAudit() throws {
        let app = launchFixture()
        try app.performAccessibilityAudit()
    }
}
