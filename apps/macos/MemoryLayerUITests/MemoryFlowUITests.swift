import XCTest

final class MemoryFlowUITests: XCTestCase {
    @MainActor
    func testFilterSwitchesStatusAndSelectedDetailShowsProvenanceAndHistory() async throws {
        continueAfterFailure = false
        let session = try await FixtureUITestSession.start(bundle: Bundle(for: Self.self))
        session.registerTeardown(on: self)
        session.connect()
        session.navigate(to: "Memories")
        let app = session.app
        let active = app.buttons["memories.row.10000000-0000-4000-8000-000000000001"]
        let archived = app.buttons["memories.row.10000000-0000-4000-8000-000000000002"]
        XCTAssertTrue(active.waitForExistence(timeout: 8))
        let status = app.popUpButtons["memories.status"]
        XCTAssertTrue(status.exists)
        status.click(); app.menuItems["Archived"].click()
        XCTAssertTrue(archived.waitForExistence(timeout: 5))
        XCTAssertFalse(active.exists)
        status.click(); app.menuItems["Active"].click()
        XCTAssertTrue(active.waitForExistence(timeout: 5))
        active.click()
        XCTAssertTrue(app.staticTexts["Use the operating system's user cache directory for disposable application data."].waitForExistence(timeout: 5))
        app.radioButtons["Provenance"].click()
        XCTAssertTrue(app.staticTexts["Sources/CacheStore.swift"].waitForExistence(timeout: 5))
        app.radioButtons["History"].click()
        XCTAssertTrue(app.staticTexts["Use a cache directory."].waitForExistence(timeout: 5))

        let requests = try await session.audit()
        XCTAssertTrue(requests.contains { $0.path == "/v1/projects/synthetic-project/memories" })
        XCTAssertTrue(requests.contains { $0.path == "/v1/memory/10000000-0000-4000-8000-000000000001/history" })
    }

    @MainActor
    func testEmptyServiceProjectStateIdentifiesConnectionAndOffersReview() async throws {
        continueAfterFailure = false
        let session = try await FixtureUITestSession.start(bundle: Bundle(for: Self.self), scenario: "empty")
        session.registerTeardown(on: self)
        session.connect()
        session.navigate(to: "Memories")
        let app = session.app
        XCTAssertTrue(app.staticTexts["No memories in this project"].waitForExistence(timeout: 8))
        let projectContext = app.staticTexts.matching(
            NSPredicate(format: "value CONTAINS %@ OR label CONTAINS %@", "synthetic-project", "synthetic-project")
        ).firstMatch
        XCTAssertTrue(projectContext.exists, "The empty state should identify the selected service project. UI: \(app.debugDescription)")
        let reviewConnection = app.buttons["Review connection settings…"]
        XCTAssertTrue(reviewConnection.waitForExistence(timeout: 5))
        reviewConnection.click()
        XCTAssertTrue(app.textFields["connection.serviceURL"].waitForExistence(timeout: 5),
                      "The empty-state action should open the selected service's connection settings.")
    }
}
