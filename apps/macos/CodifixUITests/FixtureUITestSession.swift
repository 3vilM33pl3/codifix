import AppKit
import Darwin
import Foundation
import XCTest

struct FixtureAuditEntry: Decodable {
    let method: String
    let path: String
    let project: String?
    let query: String?
    let dryRun: Bool?
    let decision: String?
    let editedActionSummary: String?
    let tokenName: String?
    let role: String?
    let tokenID: String?
    let principalID: String?
    let membershipID: String?
    let bodyBytes: Int?
    let bodySHA256: String?
    let globalRole: String?
    let projectRole: String?
    let options: [String: Bool]?

    private enum CodingKeys: String, CodingKey {
        case method, path, project, query, role, decision, options
        case dryRun = "dry_run"
        case editedActionSummary = "edited_action_summary"
        case tokenName = "token_name"
        case tokenID = "token_id"
        case principalID = "principal_id"
        case membershipID = "membership_id"
        case bodyBytes = "body_bytes"
        case bodySHA256 = "body_sha256"
        case globalRole = "global_role"
        case projectRole = "project_role"
    }
}

@MainActor
final class FixtureUITestSession: NSObject {
    let app = XCUIApplication()
    let address: String
    private let server: Process
    private let pasteboard = NSPasteboard.general
    private let originalPasteboard: [NSPasteboardItem]
    private var stopped = false

    private init(server: Process, address: String) {
        self.server = server
        self.address = address
        originalPasteboard = pasteboard.pasteboardItems?.map { item in
            let copy = NSPasteboardItem()
            for type in item.types {
                if let data = item.data(forType: type) { copy.setData(data, forType: type) }
            }
            return copy
        } ?? []
        super.init()
    }

    static func start(bundle: Bundle, scenario: String = "normal") async throws -> FixtureUITestSession {
        let fixtureURL = try XCTUnwrap(bundle.url(forResource: "server", withExtension: "py"))
        let server = Process()
        let output = Pipe()
        server.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        server.arguments = [fixtureURL.path, "--port", "0"]
        server.standardOutput = output
        try server.run()
        let announcement = String(decoding: output.fileHandleForReading.availableData, as: UTF8.self)
        guard let address = announcement.split(separator: " ").first(where: { $0.hasPrefix("http://127.0.0.1:") }) else {
            terminate(server)
            throw FixtureUITestError.serverDidNotAnnounceAddress(announcement)
        }
        let session = FixtureUITestSession(server: server, address: String(address))
        do {
            try await session.setScenario(scenario)
            try await session.expectHealthyFixture()
            return session
        } catch {
            session.stop()
            throw error
        }
    }

    func connect() {
        app.launchArguments = ["--uitesting", "--ui-window-size=1440x960"]
        app.launch()
        XCTAssertTrue(app.buttons["Review connections…"].waitForExistence(timeout: 8))
        app.buttons["Review connections…"].click()

        pasteboard.clearContents()
        pasteboard.setString(address, forType: .string)
        let addressField = app.textFields["connection.serviceURL"]
        addressField.click()
        addressField.typeKey("a", modifierFlags: .command)
        addressField.typeKey("v", modifierFlags: .command)
        let projectField = app.textFields["connection.projectSlug"]
        projectField.click()
        projectField.typeText("synthetic-project")
        XCTAssertEqual(addressField.value as? String, address)
        XCTAssertEqual(projectField.value as? String, "synthetic-project")
        app.buttons["Connect"].click()
    }

    func navigate(to destination: String) {
        let item = app.outlines.staticTexts[destination]
        XCTAssertTrue(item.waitForExistence(timeout: 8), "Expected (destination) in the feature sidebar.")
        item.click()
    }

    func registerTeardown(on testCase: XCTestCase) {
        testCase.addTeardownBlock { @MainActor in self.stop() }
    }

    func clickTopmostSheetButton(_ title: String) throws {
        let sheet = app.sheets.element(boundBy: max(0, app.sheets.count - 1))
        let matches = sheet.buttons.matching(
            NSPredicate(format: "label == %@ OR value == %@ OR identifier == %@", title, title, title)
        )
        XCTAssertEqual(matches.count, 1, "Expected one '\(title)' button in the active sheet. UI: \(app.debugDescription)")
        let button = try XCTUnwrap(matches.firstMatch.isEnabled && matches.firstMatch.isHittable ? matches.firstMatch : nil,
                                   "The active sheet's '\(title)' button should be enabled and hittable.")
        button.click()
    }

    func audit() async throws -> [FixtureAuditEntry] {
        let endpoint = try XCTUnwrap(URL(string: "\(address)/__requests"))
        let (data, response) = try await URLSession.shared.data(from: endpoint)
        guard (response as? HTTPURLResponse)?.statusCode == 200 else { throw FixtureUITestError.auditUnavailable }
        return try JSONDecoder().decode([FixtureAuditEntry].self, from: data)
    }

    func setScenario(_ name: String, reset: Bool = false) async throws {
        let endpoint = try XCTUnwrap(URL(string: "\(address)/__scenario"))
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: ["name": name, "reset": reset])
        let (_, response) = try await URLSession.shared.data(for: request)
        guard (response as? HTTPURLResponse)?.statusCode == 200 else { throw FixtureUITestError.scenarioUnavailable }
    }

    func stop() {
        guard !stopped else { return }
        stopped = true
        if app.state != .notRunning { app.terminate() }
        Self.terminate(server)
        pasteboard.clearContents()
        if !originalPasteboard.isEmpty { pasteboard.writeObjects(originalPasteboard) }
    }

    static func terminate(_ process: Process) {
        guard process.isRunning else { return }
        let processID = process.processIdentifier
        process.terminate()
        waitForExit(process, until: Date().addingTimeInterval(2))
        if process.isRunning {
            _ = kill(processID, SIGKILL)
            waitForExit(process, until: Date().addingTimeInterval(2))
        }
    }

    private func expectHealthyFixture() async throws {
        let endpoint = try XCTUnwrap(URL(string: "\(address)/healthz"))
        let (_, response) = try await URLSession.shared.data(from: endpoint)
        guard (response as? HTTPURLResponse)?.statusCode == 200 else { throw FixtureUITestError.fixtureUnhealthy }
    }

    private static func waitForExit(_ process: Process, until deadline: Date) {
        while process.isRunning && Date() < deadline {
            RunLoop.current.run(until: Date().addingTimeInterval(0.05))
        }
    }
}

private enum FixtureUITestError: LocalizedError {
    case serverDidNotAnnounceAddress(String)
    case fixtureUnhealthy
    case scenarioUnavailable
    case auditUnavailable

    var errorDescription: String? {
        switch self {
        case .serverDidNotAnnounceAddress(let output): "Fixture server did not announce its address: \(output)"
        case .fixtureUnhealthy: "The isolated fixture server did not pass its health check."
        case .scenarioUnavailable: "The isolated fixture server did not accept the scenario request."
        case .auditUnavailable: "The isolated fixture server request audit was unavailable."
        }
    }
}
