import XCTest

final class AccessFlowUITests: XCTestCase {
    @MainActor
    func testNonGlobalAdministratorCannotOpenAccessAdministration() async throws {
        continueAfterFailure = false
        let session = try await FixtureUITestSession.start(bundle: Bundle(for: Self.self), scenario: "non-admin")
        session.registerTeardown(on: self)
        session.connect()
        let accessItem = session.app.outlines.staticTexts["Access"]
        XCTAssertFalse(accessItem.waitForExistence(timeout: 2))
        let requests = try await session.audit()
        let identity = try XCTUnwrap(requests.first { $0.path == "/v1/auth/me" }, "The fixture must authenticate the project administrator before checking navigation permissions.")
        XCTAssertEqual(identity.globalRole, "reader")
        XCTAssertEqual(identity.projectRole, "admin")
        XCTAssertFalse(requests.contains { $0.path == "/v1/auth/tokens" || $0.path == "/v1/auth/memberships" })
    }

    @MainActor
    func testReadOnlyGlobalAdministratorCanInspectButCannotMutateAccess() async throws {
        continueAfterFailure = false
        let session = try await FixtureUITestSession.start(bundle: Bundle(for: Self.self), scenario: "read-only")
        session.registerTeardown(on: self)
        session.connect()
        session.navigate(to: "Access")
        let app = session.app
        XCTAssertTrue(app.staticTexts["Fixture administrator"].waitForExistence(timeout: 8), "A read-only administrator should still be able to inspect loaded principal records.")
        XCTAssertTrue(app.staticTexts["Read-only connection. Access details are available; changes are disabled."].waitForExistence(timeout: 8))
        let create = app.buttons["access.createToken"]
        XCTAssertTrue(create.waitForExistence(timeout: 5))
        XCTAssertFalse(create.isEnabled)
        XCTAssertFalse(app.buttons["access.grantMembership"].isEnabled)
        let requests = try await session.audit()
        XCTAssertTrue(requests.contains { $0.path == "/v1/auth/me" })
        XCTAssertTrue(requests.contains { $0.method == "GET" && $0.path == "/v1/auth/principals" })
        XCTAssertTrue(requests.contains { $0.method == "GET" && $0.path == "/v1/auth/tokens" })
        XCTAssertTrue(requests.contains { $0.method == "GET" && $0.path == "/v1/auth/memberships" })
        XCTAssertFalse(requests.contains { $0.method == "POST" && ($0.path == "/v1/auth/tokens" || $0.path == "/v1/auth/memberships") })
    }

    @MainActor
    func testGlobalAdministratorCreatesCopiesDismissesAndRevokesServiceTokenAndMembership() async throws {
        continueAfterFailure = false
        let session = try await FixtureUITestSession.start(bundle: Bundle(for: Self.self))
        session.registerTeardown(on: self)
        session.connect()
        session.navigate(to: "Access")
        let app = session.app
        XCTAssertTrue(app.buttons["access.createToken"].waitForExistence(timeout: 10))
        app.buttons["access.createToken"].click()
        let name = app.textFields["access.tokenName"]
        XCTAssertTrue(name.waitForExistence(timeout: 5))
        name.click(); name.typeText("Fixture UI token")
        app.buttons["access.submitToken"].click()

        let secret = app.staticTexts["access.oneTimeSecret"]
        XCTAssertTrue(secret.waitForExistence(timeout: 8))
        let dummySecret = "DUMMY_FIXTURE_TOKEN_NOT_VALID_FOR_ANY_SERVICE"
        XCTAssertEqual(secret.value as? String, dummySecret)
        app.buttons["access.copyToken"].click()
        XCTAssertTrue(app.staticTexts["Copied"].waitForExistence(timeout: 3))
        app.buttons["access.dismissSecret"].click()
        XCTAssertFalse(app.staticTexts["access.oneTimeSecret"].waitForExistence(timeout: 2))
        app.buttons["access.createToken"].click()
        let clipboardCheck = app.textFields["access.tokenName"]
        XCTAssertTrue(clipboardCheck.waitForExistence(timeout: 5))
        clipboardCheck.click(); clipboardCheck.typeKey("v", modifierFlags: .command)
        XCTAssertEqual(clipboardCheck.value as? String, dummySecret, "Pasting the copied token into the fixture form verifies the clipboard payload without reading it from the test runner.")
        try session.clickTopmostSheetButton("Cancel")

        try selectAccessSection("Service tokens", in: app)
        XCTAssertTrue(app.staticTexts["fixture_only"].waitForExistence(timeout: 8))
        XCTAssertFalse(app.staticTexts[dummySecret].exists, "The full token secret must not appear in the reusable token list.")
        let revokeToken = app.buttons["access.revokeToken.fixture-token-1"]
        XCTAssertTrue(revokeToken.waitForExistence(timeout: 5))
        revokeToken.click()
        try session.clickTopmostSheetButton("Cancel")
        var requests = try await session.audit()
        XCTAssertFalse(requests.contains { $0.path == "/v1/auth/tokens/fixture-token-1/revoke" }, "Cancel must not revoke the token.")
        app.buttons["access.revokeToken.fixture-token-1"].click()
        try session.clickTopmostSheetButton("Revoke Fixture UI token")
        XCTAssertTrue(app.staticTexts["Revoked"].waitForExistence(timeout: 8))
        requests = try await session.audit()
        let tokenCreate = try XCTUnwrap(requests.first { $0.method == "POST" && $0.path == "/v1/auth/tokens" })
        XCTAssertEqual(tokenCreate.project, "synthetic-project")
        XCTAssertEqual(tokenCreate.tokenName, "Fixture UI token")
        XCTAssertEqual(tokenCreate.role, "reader")
        XCTAssertTrue(requests.contains { $0.path == "/v1/auth/tokens/fixture-token-1/revoke" })

        try selectAccessSection("Memberships", in: app)
        app.buttons["access.grantMembership"].click()
        let membershipProject = app.textFields["access.membershipProject"]
        XCTAssertTrue(membershipProject.waitForExistence(timeout: 5))
        membershipProject.click(); membershipProject.typeKey("a", modifierFlags: .command)
        membershipProject.typeText("synthetic-project")
        app.buttons["access.submitMembership"].click()
        XCTAssertTrue(app.staticTexts["synthetic-project"].waitForExistence(timeout: 8))
        let revokeMembership = app.buttons["access.revokeMembership.fixture-membership"]
        XCTAssertTrue(revokeMembership.waitForExistence(timeout: 5))
        revokeMembership.click()
        try session.clickTopmostSheetButton("Cancel")
        requests = try await session.audit()
        XCTAssertFalse(requests.contains { $0.method == "DELETE" && $0.path == "/v1/auth/memberships/fixture-membership" }, "Cancel must preserve the fixture membership.")
        app.buttons["access.revokeMembership.fixture-membership"].click()
        try session.clickTopmostSheetButton("Revoke access")
        XCTAssertTrue(app.staticTexts["No explicit memberships"].waitForExistence(timeout: 8))
        requests = try await session.audit()
        let grant = try XCTUnwrap(requests.first { $0.method == "POST" && $0.path == "/v1/auth/memberships" })
        XCTAssertEqual(grant.project, "synthetic-project")
        XCTAssertEqual(grant.principalID, "fixture-admin")
        XCTAssertEqual(grant.role, "reader")
        XCTAssertTrue(requests.contains { $0.method == "DELETE" && $0.path == "/v1/auth/memberships/fixture-membership" })
        XCTAssertFalse(requests.flatMap { [$0.path, $0.tokenName ?? "", $0.editedActionSummary ?? ""] }.contains { $0.contains(dummySecret) })
    }

    @MainActor
    private func selectAccessSection(_ title: String, in app: XCUIApplication) throws {
        let options = app.descendants(matching: .any).matching(
            NSPredicate(format: "label == %@ OR value == %@", title, title)
        )
        XCTAssertTrue(options.firstMatch.waitForExistence(timeout: 5), "Expected the visible Access section option '\(title)'. UI: \(app.debugDescription)")
        let control = try XCTUnwrap(options.allElementsBoundByIndex.first(where: { $0.isEnabled && $0.isHittable }),
                                    "The visible Access section option '\(title)' should be an enabled, hittable control. UI: \(app.debugDescription)")
        control.click()
    }

}
