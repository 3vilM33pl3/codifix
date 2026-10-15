import XCTest

final class NavigationSmokeTests: XCTestCase {
    func testUnconfiguredAppShowsExistingServiceConnectionPrompt() {
        let app = XCUIApplication()
        app.launchArguments = ["--uitesting"]
        app.launch()

        XCTAssertTrue(app.staticTexts["Choose a Memory Layer connection"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["Review connections…"].exists)
        XCTAssertTrue(app.outlines.staticTexts["Memories"].exists || app.staticTexts["Memories"].exists)

        app.buttons["Review connections…"].click()
        XCTAssertTrue(app.staticTexts["Select an existing service and project."].waitForExistence(timeout: 2))
        XCTAssertTrue(app.textFields["Service URL"].exists)
        XCTAssertTrue(app.textFields["Project slug"].exists)
    }
}
