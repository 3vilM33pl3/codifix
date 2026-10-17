import XCTest

final class KnowledgeFlowUITests: XCTestCase {
    @MainActor
    func testQueryShowsCitedAnswerAndOpensSourceMemory() async throws {
        continueAfterFailure = false
        let session = try await FixtureUITestSession.start(bundle: Bundle(for: Self.self))
        session.registerTeardown(on: self)
        session.connect()
        session.navigate(to: "Query")

        let app = session.app
        let question = app.textFields["query.question"]
        XCTAssertTrue(question.waitForExistence(timeout: 10))
        question.click()
        question.typeText("Where should disposable cache data go?")
        app.buttons["query.submit"].click()

        XCTAssertTrue(app.staticTexts["Use the user cache directory [1]."].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["query.citation.1"].waitForExistence(timeout: 5))
        app.buttons["query.citation.1"].click()
        XCTAssertTrue(app.staticTexts["Sources/CacheStore.swift"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Use the operating system's user cache directory for disposable application data."].waitForExistence(timeout: 5))

        let requests = try await session.audit()
        let query = try XCTUnwrap(requests.first { $0.path == "/v1/query" })
        XCTAssertEqual(query.method, "POST")
        XCTAssertEqual(query.project, "synthetic-project")
        XCTAssertEqual(query.query, "Where should disposable cache data go?")
    }

    @MainActor
    func testReviewApprovalRequiresConfirmationAndRecordsApprovedResult() async throws {
        try await resolveReviewProposal(approve: true)
    }

    @MainActor
    func testReviewRejectionRequiresConfirmationAndRecordsRejectedResult() async throws {
        try await resolveReviewProposal(approve: false)
    }

    @MainActor
    private func resolveReviewProposal(approve: Bool) async throws {
        continueAfterFailure = false
        let session = try await FixtureUITestSession.start(bundle: Bundle(for: Self.self))
        session.registerTeardown(on: self)
        session.connect()
        session.navigate(to: "Review")
        let app = session.app
        XCTAssertTrue(app.buttons["review.approve"].waitForExistence(timeout: 10))
        let auditBeforeCancel = try await session.audit()

        app.buttons[approve ? "review.approve" : "review.reject"].click()
        let dialogTitle = approve ? "Approve replacement?" : "Reject proposal?"
        let confirmation = app.sheets.element(boundBy: max(0, app.sheets.count - 1))
        XCTAssertTrue(confirmation.buttons[approve ? "Approve replacement" : "Reject proposal"].waitForExistence(timeout: 3))
        try session.clickTopmostSheetButton("Cancel")
        let decisionsBeforeCancel = auditBeforeCancel.filter { $0.method == "POST" && $0.path.contains("/replacement-proposals/") }
        let decisionsAfterCancel = try await session.audit().filter { $0.method == "POST" && $0.path.contains("/replacement-proposals/") }
        XCTAssertEqual(decisionsAfterCancel.count, decisionsBeforeCancel.count,
                       "Cancel must leave the proposal pending without a decision request.")

        app.buttons[approve ? "review.approve" : "review.reject"].click()
        let decision = approve ? "Approve replacement" : "Reject proposal"
        let finalConfirmation = app.sheets.element(boundBy: max(0, app.sheets.count - 1))
        XCTAssertTrue(finalConfirmation.buttons[decision].waitForExistence(timeout: 3))
        try session.clickTopmostSheetButton(decision)
        let expected = approve ? "Proposal approved" : "Proposal rejected"
        let result = app.descendants(matching: .any)["review.result"]
        XCTAssertTrue(result.waitForExistence(timeout: 8), "The Review screen should show the decision result.")
        XCTAssertTrue(result.label.contains(expected) || (result.value as? String)?.contains(expected) == true)
        XCTAssertTrue(app.staticTexts["No pending proposals"].waitForExistence(timeout: 5))

        let requests = try await session.audit()
        let expectedPath = "/v1/projects/synthetic-project/replacement-proposals/40000000-0000-4000-8000-000000000001/" + (approve ? "approve" : "reject")
        let write = try XCTUnwrap(requests.first { $0.path == expectedPath })
        XCTAssertEqual(write.method, "POST")
    }
}
