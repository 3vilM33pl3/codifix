import XCTest

final class NavigationSmokeTests: XCTestCase {
    func testConnectionLoadsSelectedMemoryAndHistoryFromFixture() throws {
        let fixtureURL = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "server", withExtension: "py"))
        let server = Process()
        let output = Pipe()
        server.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        server.arguments = [fixtureURL.path, "--port", "0"]
        server.standardOutput = output
        try server.run()
        defer { if server.isRunning { server.terminate() } }
        let announcement = String(decoding: output.fileHandleForReading.availableData, as: UTF8.self)
        let address = try XCTUnwrap(announcement.split(separator: " ").first { $0.hasPrefix("http://127.0.0.1:") })

        let app = XCUIApplication()
        app.launchArguments = ["--uitesting", "--ui-window-size=1440x960"]
        app.launch()
        defer { app.terminate() }
        XCTAssertTrue(app.buttons["Review connections…"].waitForExistence(timeout: 5))
        app.buttons["Review connections…"].click()
        let addressField = app.textFields["Service URL"]
        addressField.click()
        addressField.typeKey("a", modifierFlags: .command)
        addressField.typeText(String(address))
        app.textFields["Project slug"].click()
        app.textFields["Project slug"].typeText("synthetic-project")
        app.buttons["Connect"].click()

        let canonical = app.staticTexts["Use the operating system's user cache directory for disposable application data."]
        XCTAssertTrue(canonical.waitForExistence(timeout: 10), "The automatically selected first memory must load its detail.")
        app.radioButtons["Provenance"].click()
        XCTAssertTrue(app.staticTexts["Sources/CacheStore.swift"].firstMatch.waitForExistence(timeout: 5))
        app.radioButtons["History"].click()
        XCTAssertTrue(app.staticTexts["Use a cache directory."].waitForExistence(timeout: 5))
    }

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
