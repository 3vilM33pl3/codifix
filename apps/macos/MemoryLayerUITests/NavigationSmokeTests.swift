import AppKit
import XCTest

final class NavigationSmokeTests: XCTestCase {
    @MainActor
    func testConnectionLoadsSelectedMemoryAndHistoryFromFixture() async throws {
        continueAfterFailure = false
        let fixtureURL = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "server", withExtension: "py"))
        let server = Process()
        let output = Pipe()
        server.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        server.arguments = [fixtureURL.path, "--port", "0"]
        server.standardOutput = output
        try server.run()
        addTeardownBlock { @MainActor in FixtureUITestSession.terminate(server) }
        let announcement = String(decoding: output.fileHandleForReading.availableData, as: UTF8.self)
        let address = try XCTUnwrap(announcement.split(separator: " ").first { $0.hasPrefix("http://127.0.0.1:") })
        let healthURL = try XCTUnwrap(URL(string: "\(address)/healthz"))
        let (_, healthResponse) = try await URLSession.shared.data(from: healthURL)
        XCTAssertEqual((healthResponse as? HTTPURLResponse)?.statusCode, 200)

        let app = XCUIApplication()
        app.launchArguments = ["--uitesting", "--ui-window-size=1440x960"]
        app.launch()
        addTeardownBlock { @MainActor in if app.state != .notRunning { app.terminate() } }
        XCTAssertTrue(app.buttons["Review connections…"].waitForExistence(timeout: 5))
        app.buttons["Review connections…"].click()
        let addressField = app.textFields["connection.serviceURL"]
        let pasteboard = NSPasteboard.general
        let originalPasteboard: [NSPasteboardItem] = pasteboard.pasteboardItems?.map { item in
            let copy = NSPasteboardItem()
            for type in item.types {
                if let data = item.data(forType: type) { copy.setData(data, forType: type) }
            }
            return copy
        } ?? []
        addTeardownBlock { @MainActor in
            pasteboard.clearContents()
            if !originalPasteboard.isEmpty { pasteboard.writeObjects(originalPasteboard) }
        }
        pasteboard.clearContents()
        pasteboard.setString(String(address), forType: .string)
        addressField.click()
        addressField.typeKey("a", modifierFlags: .command)
        addressField.typeKey("v", modifierFlags: .command)
        let projectField = app.textFields["connection.projectSlug"]
        projectField.click()
        projectField.typeText("synthetic-project")
        XCTAssertEqual(addressField.value as? String, String(address))
        XCTAssertEqual(projectField.value as? String, "synthetic-project")
        let connectButton = app.buttons["Connect"]
        XCTAssertTrue(connectButton.isEnabled, "Connect should be enabled after entering a loopback URL and project.")
        connectButton.click()

        let canonical = app.staticTexts["Use the operating system's user cache directory for disposable application data."]
        guard canonical.waitForExistence(timeout: 10) else {
            let requestsURL = try XCTUnwrap(URL(string: "\(address)/__requests"))
            let (data, _) = try await URLSession.shared.data(from: requestsURL)
            let requests = String(decoding: data, as: UTF8.self)
            let state = app.staticTexts.allElementsBoundByIndex.map(\.label).joined(separator: " | ")
            XCTFail("The first fixture memory did not load (\(state)); service URL=\(addressField.value ?? "nil"), project=\(projectField.value ?? "nil"), buttons=\(app.buttons.allElementsBoundByIndex.map(\.label)), fixture requests: \(requests)")
            return
        }
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
        XCTAssertTrue(app.textFields["connection.serviceURL"].exists)
        XCTAssertTrue(app.textFields["connection.projectSlug"].exists)
    }
}
