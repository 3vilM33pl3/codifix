import XCTest

final class AutomationFlowUITests: XCTestCase {
    @MainActor
    func testAutomationPreviewIsDryRunAndInspectable() async throws {
        continueAfterFailure = false
        let session = try await FixtureUITestSession.start(bundle: Bundle(for: Self.self))
        session.registerTeardown(on: self)
        session.connect()
        session.navigate(to: "Automations")
        let app = session.app
        XCTAssertTrue(app.staticTexts["Project digest"].waitForExistence(timeout: 10))
        app.buttons["automation.previewRun"].click()
        XCTAssertTrue(app.staticTexts["Dry-run preview completed for loop / one."].waitForExistence(timeout: 10))
        XCTAssertTrue(app.descendants(matching: .any)["automation.runInspector"].waitForExistence(timeout: 5))

        let requests = try await session.audit()
        let run = try XCTUnwrap(requests.first { $0.path == "/v1/loops/loop / one/run" })
        XCTAssertEqual(run.method, "POST")
        XCTAssertEqual(run.project, "synthetic-project")
        XCTAssertEqual(run.dryRun, true, "The fixture accepts only a dry-run preview.")
    }

    @MainActor
    func testAutomationApprovalEditAndApproveAreAudited() async throws {
        continueAfterFailure = false
        let session = try await FixtureUITestSession.start(bundle: Bundle(for: Self.self))
        session.registerTeardown(on: self)
        session.connect()
        session.navigate(to: "Automations")
        let app = session.app
        XCTAssertTrue(app.buttons["automation.approvalQueue"].waitForExistence(timeout: 10))
        app.buttons["automation.approvalQueue"].click()
        let editor = app.textViews["automation.approvalEditor.approval-1"]
        XCTAssertTrue(editor.waitForExistence(timeout: 8))
        editor.click(); editor.typeKey("a", modifierFlags: .command)
        let editedJSON = "{\"summary\":\"Edited fixture approval\"}"
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(editedJSON, forType: .string)
        editor.typeKey("v", modifierFlags: .command)
        XCTAssertEqual(editor.value as? String, editedJSON,
                       "The visible approval editor should contain the exact replacement JSON before saving.")
        let save = app.buttons["automation.approvalSave.approval-1"]
        XCTAssertTrue(save.isEnabled, "A valid edited action should be saveable.")
        save.click()
        var requests = try await session.audit()
        for _ in 0..<20 where !requests.contains(where: { $0.path == "/v1/loops/approvals/approval-1/edit" }) {
            try await Task.sleep(for: .milliseconds(150))
            requests = try await session.audit()
        }
        XCTAssertTrue(requests.contains { $0.path == "/v1/loops/approvals/approval-1/edit" },
                      "Saving the edited action should send the fixture edit route. Observed paths: \(requests.map(\.path)); visible invalid JSON: \(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "Invalid JSON")).firstMatch.exists)")
        let approveButton = app.buttons["automation.approvalApprove.approval-1"]
        let readyToApprove = XCTNSPredicateExpectation(predicate: NSPredicate(format: "isEnabled == true"), object: approveButton)
        XCTAssertEqual(XCTWaiter.wait(for: [readyToApprove], timeout: 8), .completed)
        approveButton.click()
        XCTAssertTrue(app.staticTexts["No pending automation items"].waitForExistence(timeout: 8))
        try session.clickTopmostSheetButton("Close")
        requests = try await session.audit()
        let edit = try XCTUnwrap(requests.first { $0.path == "/v1/loops/approvals/approval-1/edit" })
        XCTAssertEqual(edit.method, "POST")
        XCTAssertEqual(edit.editedActionSummary, "Edited fixture approval")
        let approve = try XCTUnwrap(requests.first { $0.path == "/v1/loops/approvals/approval-1/approve" })
        XCTAssertEqual(approve.method, "POST")
    }

    @MainActor
    func testAutomationApprovalRejectionCanBeCancelledBeforeWrite() async throws {
        continueAfterFailure = false
        let session = try await FixtureUITestSession.start(bundle: Bundle(for: Self.self))
        session.registerTeardown(on: self)
        session.connect()
        session.navigate(to: "Automations")
        let app = session.app
        XCTAssertTrue(app.buttons["automation.approvalQueue"].waitForExistence(timeout: 10))
        app.buttons["automation.approvalQueue"].click()
        let reject = app.buttons["automation.approvalReject.approval-1"]
        XCTAssertTrue(reject.waitForExistence(timeout: 8))
        let before = try await session.audit()
        reject.click()
        XCTAssertTrue(app.sheets.buttons["Reject item"].waitForExistence(timeout: 3))
        try session.clickTopmostSheetButton("Cancel")
        let afterCancel = try await session.audit()
        XCTAssertEqual(afterCancel.filter { $0.path == "/v1/loops/approvals/approval-1/reject" }.count,
                       before.filter { $0.path == "/v1/loops/approvals/approval-1/reject" }.count)

        reject.click()
        try session.clickTopmostSheetButton("Reject item")
        XCTAssertTrue(app.staticTexts["No pending automation items"].waitForExistence(timeout: 8))
        try session.clickTopmostSheetButton("Close")
        let finalAudit = try await session.audit()
        XCTAssertEqual(finalAudit.filter { $0.path == "/v1/loops/approvals/approval-1/reject" }.count, 1)
    }
}
