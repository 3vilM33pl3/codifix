import CryptoKit
import Foundation
import XCTest

final class BundleFlowUITests: XCTestCase {
    @MainActor
    func testImportUsesNativeFilePickerPreviewWarningsAndExplicitConfirmation() async throws {
        continueAfterFailure = false
        let session = try await FixtureUITestSession.start(bundle: Bundle(for: Self.self))
        session.registerTeardown(on: self)
        session.connect()
        session.navigate(to: "Bundles")
        let app = session.app
        let fixtureArchive = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "sample", withExtension: "mlbundle.zip"))

        try selectFixtureArchive(app, archiveURL: fixtureArchive)
        app.buttons["bundles.previewImport"].click()
        XCTAssertTrue(app.staticTexts["Review source"].waitForExistence(timeout: 10))
        let counts = app.staticTexts["bundles.importPreviewCounts"]
        XCTAssertTrue(counts.waitForExistence(timeout: 5), "The preview should show import counts.")
        let countsValue = counts.value as? String ?? counts.label
        XCTAssertTrue(countsValue.contains("3 memories") && countsValue.contains("2 new") && countsValue.contains("1 unchanged"), "Unexpected import counts: \(countsValue)")
        XCTAssertTrue(app.buttons["bundles.confirmImport"].waitForExistence(timeout: 5))
        try session.clickTopmostSheetButton("Close")
        let cancelledAudit = try await session.audit()
        XCTAssertFalse(cancelledAudit.contains { $0.method == "POST" && $0.path.hasSuffix("/bundle/import") }, "Closing the preview must not import the archive.")

        try selectFixtureArchive(app, archiveURL: fixtureArchive)
        app.buttons["bundles.previewImport"].click()
        XCTAssertTrue(app.buttons["bundles.confirmImport"].waitForExistence(timeout: 10))
        app.buttons["bundles.confirmImport"].click()
        let status = app.descendants(matching: .any)["bundles.status"]
        XCTAssertTrue(status.waitForExistence(timeout: 10), "The import result should be visible after confirmation.")
        let statusText = status.label.isEmpty ? (status.value as? String ?? "") : status.label
        XCTAssertTrue(statusText.contains("Imported 2 memories into synthetic-project (0 replaced, 1 skipped)"), "Unexpected import result: \(statusText)")

        let requests = try await session.audit()
        let preview = try XCTUnwrap(requests.first { $0.method == "POST" && $0.path.hasSuffix("/bundle/import/preview") })
        XCTAssertEqual(preview.project, "synthetic-project")
        XCTAssertGreaterThan(preview.bodyBytes ?? 0, 0)
        let importRequest = try XCTUnwrap(requests.first { $0.method == "POST" && $0.path.hasSuffix("/bundle/import") })
        XCTAssertEqual(importRequest.project, "synthetic-project")
        XCTAssertEqual(importRequest.bodyBytes, preview.bodyBytes, "Final import must submit the exact archive bytes that were previewed.")
        let selectedArchiveSHA256 = SHA256.hash(data: try Data(contentsOf: fixtureArchive)).map { String(format: "%02x", $0) }.joined()
        XCTAssertEqual(preview.bodySHA256, selectedArchiveSHA256)
        XCTAssertEqual(importRequest.bodySHA256, selectedArchiveSHA256, "The final request must contain the exact file bytes that the user previewed.")
    }

    @MainActor
    func testExportPreviewShowsOptionsAndWarningsBeforeSave() async throws {
        continueAfterFailure = false
        let session = try await FixtureUITestSession.start(bundle: Bundle(for: Self.self))
        session.registerTeardown(on: self)
        session.connect()
        session.navigate(to: "Bundles")
        let app = session.app
        app.buttons["bundles.previewExport"].click()
        XCTAssertTrue(app.buttons["bundles.saveExport"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.staticTexts["One path omitted"].exists)
        let requests = try await session.audit()
        let preview = try XCTUnwrap(requests.first { $0.method == "POST" && $0.path.hasSuffix("/bundle/export/preview") })
        XCTAssertEqual(preview.project, "synthetic-project")
        XCTAssertEqual(preview.options, ["include_archived": false, "include_tags": true, "include_relations": true,
                                          "include_source_file_paths": false, "include_git_commits": false,
                                          "include_source_excerpts": false])
        XCTAssertFalse(requests.contains { $0.method == "GET" && $0.path.hasSuffix("/bundle/export") }, "The export archive is fetched only after the user saves from the review sheet.")
    }

    @MainActor
    private func selectFixtureArchive(_ app: XCUIApplication, archiveURL: URL) throws {
        app.buttons["bundles.chooseFile"].click()
        app.typeKey("g", modifierFlags: [.command, .shift])
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(archiveURL.path, forType: .string)
        app.typeKey("v", modifierFlags: .command)
        app.typeKey("\r", modifierFlags: [])
        app.typeKey("\r", modifierFlags: [])
        XCTAssertTrue(app.buttons["bundles.previewImport"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["bundles.previewImport"].isEnabled, "Import preview is enabled only after a native file selection.")
    }
}
