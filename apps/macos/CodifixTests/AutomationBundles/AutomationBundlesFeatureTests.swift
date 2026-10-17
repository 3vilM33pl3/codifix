import Foundation
import XCTest
@testable import Codifix

final class AutomationBundlesFeatureTests: XCTestCase {
    func testTransientLoopModesUseActionableConfigurationMode() {
        XCTAssertEqual(LoopMode.configurationValue(effective: .off, default: .draftOutput), .draftOutput)
        XCTAssertEqual(LoopMode.configurationValue(effective: .paused, default: .suggestOnly), .suggestOnly)
        XCTAssertEqual(LoopMode.configurationValue(effective: .snoozed, default: .autonomousSafe), .autonomousSafe)
        XCTAssertEqual(LoopMode.configurationValue(effective: .observe, default: .draftOutput), .observe)
        XCTAssertEqual(LoopMode.configurationValue(effective: nil, default: .off), .observe)
    }

    func testLoopDefinitionsRunsAndDryRunUseServiceContracts() async throws {
        let profile = profile()
        let transport = BundlesRecordingTransport(responses: [
            "/v1/loops": #"{"definitions":[{"id":"10000000-0000-4000-8000-000000000001","loop_id":"loop / one","version":2,"name":"Project digest","description":"Summarize work","risk_level":"low","default_mode":"suggest_only","trigger_spec":{"supported":["manual","project_changed"]},"context_spec":{"limit":12},"policy_spec":{"approvals":true},"output_spec":{"type":"briefing"},"created_at":"2026-10-15T10:00:00Z"}]}"#,
            "/v1/loops/loop%20%2F%20one": #"{"definition":{"id":"10000000-0000-4000-8000-000000000001","loop_id":"loop / one","version":2,"name":"Project digest","description":"Summarize work","risk_level":"low","default_mode":"suggest_only","trigger_spec":{},"context_spec":{},"policy_spec":{},"output_spec":{},"created_at":"2026-10-15T10:00:00Z"},"effective_settings":{"loop_id":"loop / one","enabled":true,"mode":"suggest_only","scope_type":"repo","scope_id":"/repo","global_kill_switch":false,"blocked_reasons":[],"budgets":{"daily_runs":4},"paused_until":null,"snoozed_until":null}}"#,
            "/v1/loops/runs": #"{"total_returned":1,"runs":[{"id":"20000000-0000-4000-8000-000000000001","loop_id":"loop / one","definition_version":2,"project":"project / one","repo_root":"/repo","mode":"suggest_only","status":"succeeded","started_at":"2026-10-15T10:30:00Z","finished_at":"2026-10-15T10:30:02Z","output_summary":"Preview completed","trace_count":2,"blocked_reasons":[]}]}"#,
            "/v1/loops/loop%20%2F%20one/run": runResponse
        ])
        let service = AutomationService(context: context(profile, transport: transport))

        let definitions = try await service.definitions()
        let detail = try await service.definition("loop / one")
        let runs = try await service.recentRuns(loopID: "loop / one")
        let preview = try await service.previewRun("loop / one")

        XCTAssertEqual(definitions.first?.loopID, "loop / one")
        XCTAssertEqual(definitions.first?.triggerSpec["supported"]?.prettyPrinted().contains("project_changed"), true)
        XCTAssertEqual(detail.effectiveSettings?.scopeID, "/repo")
        XCTAssertEqual(runs.first?.outputSummary, "Preview completed")
        XCTAssertEqual(preview.summary.status, "succeeded")
        let requests = await transport.requests
        XCTAssertEqual(requests.map { $0.httpMethod ?? "" }, ["GET", "GET", "GET", "POST"])
        XCTAssertEqual(requests[1].url?.encodedPath, "/v1/loops/loop%20%2F%20one")
        XCTAssertEqual(requests[1].url?.queryItems, [URLQueryItem(name: "project", value: "project / one"), URLQueryItem(name: "repo_root", value: "/repo")])
        XCTAssertEqual(requests[2].url?.queryItems, [URLQueryItem(name: "project", value: "project / one"), URLQueryItem(name: "loop_id", value: "loop / one"), URLQueryItem(name: "limit", value: "5")])
        let runBody = try XCTUnwrap(requests[3].httpBody).object()
        XCTAssertEqual(runBody["dry_run"] as? Bool, true)
        XCTAssertEqual(runBody["project"] as? String, "project / one")
        XCTAssertEqual(runBody["repo_root"] as? String, "/repo")
        XCTAssertEqual(runBody["scope_type"] as? String, "repo")
    }

    func testSettingsAndBothApprovalOutcomesAndEditUseExactPayloads() async throws {
        let profile = profile()
        let transport = BundlesRecordingTransport(responses: [
            "/v1/loops/loop/settings": #"{"setting":{"id":"30000000-0000-4000-8000-000000000001","updated_at":"2026-10-15T10:00:00Z"},"effective_settings":{"loop_id":"loop","enabled":true,"mode":"draft_output","scope_type":"repo","scope_id":"/repo","global_kill_switch":false,"blocked_reasons":[]}}"#,
            "/v1/loops/approvals/40000000-0000-4000-8000-000000000001/approve": approvalResponse(status: "approved"),
            "/v1/loops/approvals/40000000-0000-4000-8000-000000000002/reject": approvalResponse(status: "rejected"),
            "/v1/loops/approvals/40000000-0000-4000-8000-000000000003/edit": approvalResponse(status: "edited")
        ])
        let service = AutomationService(context: context(profile, transport: transport))
        let setting = try await service.update("loop", action: "enable", mode: .draftOutput)
        XCTAssertEqual(setting.effectiveSettings.mode, .draftOutput)
        let approve = try await service.decide(approval("40000000-0000-4000-8000-000000000001"), action: "approve")
        let reject = try await service.decide(approval("40000000-0000-4000-8000-000000000002"), action: "reject")
        let editedValue = AutomationJSON.object(["command": AutomationJSON(value: .string("preview-only"))])
        let edit = try await service.decide(approval("40000000-0000-4000-8000-000000000003"), action: "edit", edited: editedValue)

        XCTAssertEqual(approve.status, "approved"); XCTAssertEqual(reject.status, "rejected"); XCTAssertEqual(edit.status, "edited")
        let requests = await transport.requests
        XCTAssertEqual(requests.map { $0.url?.encodedPath }, [
            "/v1/loops/loop/settings",
            "/v1/loops/approvals/40000000-0000-4000-8000-000000000001/approve",
            "/v1/loops/approvals/40000000-0000-4000-8000-000000000002/reject",
            "/v1/loops/approvals/40000000-0000-4000-8000-000000000003/edit"
        ])
        let settings = try XCTUnwrap(requests[0].httpBody).object()
        XCTAssertEqual(settings["action"] as? String, "enable")
        XCTAssertEqual(settings["scope_type"] as? String, "repo")
        XCTAssertEqual(settings["scope_id"] as? String, "/repo")
        XCTAssertEqual(settings["mode"] as? String, "draft_output")
        XCTAssertEqual(settings["explicit_user_approval"] as? Bool, true)
        let editBody = try XCTUnwrap(requests[3].httpBody).object()
        XCTAssertEqual(editBody["reviewer"] as? String, "macos")
        XCTAssertEqual((editBody["edited_action"] as? [String: String])?["command"], "preview-only")
    }

    func testBundlePreviewExportAndImportReuseOptionsAndExactFileBytes() async throws {
        let profile = profile()
        let archive = Data([0x50, 0x4b, 0x03, 0x04, 0xaa, 0xbb])
        let optionsJSON = #"{"include_archived":false,"include_tags":true,"include_relations":false,"include_source_file_paths":false,"include_git_commits":false,"include_source_excerpts":false}"#
        let transport = BundlesRecordingTransport(responses: [
            "/v1/projects/project%20%2F%20one/bundle/export/preview": ##"{"bundle_id":"bundle-1","source_project":"project / one","exported_at":"2026-10-15T10:00:00Z","summary_markdown":"# Export","memory_count":3,"relation_count":1,"warning_count":1,"warnings":["One path omitted"],"options":{"include_archived":false,"include_tags":true,"include_relations":false,"include_source_file_paths":false,"include_git_commits":false,"include_source_excerpts":false}}"##,
            "/v1/projects/project%20%2F%20one/bundle/import/preview": ##"{"bundle_id":"bundle-1","bundle_hash":"hash-1","source_project":"source","target_project":"project / one","exported_at":"2026-10-15T10:00:00Z","summary_markdown":"# Import","memory_count":3,"relation_count":1,"new_count":2,"unchanged_count":1,"replacing_count":0,"warning_count":1,"warnings":["Review source"],"options":{"include_archived":false,"include_tags":true,"include_relations":true,"include_source_file_paths":false,"include_git_commits":false,"include_source_excerpts":false}}"##,
            "/v1/projects/project%20%2F%20one/bundle/import": #"{"target_project":"project / one","bundle_id":"bundle-1","bundle_hash":"hash-1","imported_count":2,"replaced_count":0,"skipped_count":1,"relation_count":1}"#
        ], binaryResponses: ["/v1/projects/project%20%2F%20one/bundle/export": archive])
        let service = BundleService(context: context(profile, transport: transport))
        var options = ExportOptions(); options.includeRelations = false

        let exportPreview = try await service.previewExport(options)
        let exportedBytes = try await service.export(options: exportPreview.options)
        let importPreview = try await service.previewImport(archive)
        let imported = try await service.importBundle(archive)

        XCTAssertEqual(exportPreview.memoryCount, 3); XCTAssertEqual(exportPreview.warnings.count, 1)
        XCTAssertEqual(exportedBytes, archive); XCTAssertEqual(importPreview.newCount, 2); XCTAssertEqual(imported.importedCount, 2)
        let requests = await transport.requests
        XCTAssertEqual(requests.map { request in
            request.url.flatMap { URLComponents(url: $0, resolvingAgainstBaseURL: false)?.percentEncodedPath }
        }, [
            "/v1/projects/project%20%2F%20one/bundle/export/preview",
            "/v1/projects/project%20%2F%20one/bundle/export",
            "/v1/projects/project%20%2F%20one/bundle/import/preview",
            "/v1/projects/project%20%2F%20one/bundle/import"
        ])
        let sentOptions = try JSONDecoder.codifix.decode(ExportOptions.self, from: XCTUnwrap(requests[0].httpBody))
        let expectedOptions = try JSONDecoder.codifix.decode(ExportOptions.self, from: Data(optionsJSON.utf8))
        XCTAssertEqual(sentOptions, expectedOptions)
        XCTAssertEqual(try JSONDecoder.codifix.decode(ExportOptions.self, from: XCTUnwrap(requests[1].httpBody)), sentOptions)
        XCTAssertEqual(requests[2].httpBody, archive); XCTAssertEqual(requests[3].httpBody, archive)
        XCTAssertEqual(requests[2].value(forHTTPHeaderField: "Content-Type"), "application/octet-stream")
    }

    func testBundleExportUsesRoutePermissionWithoutMemoryReadGrant() async throws {
        let profile = profile()
        let authJSON = #"{"authenticated":true,"mode":"multi_user","read_only":false,"principal":{"id":"principal-1","kind":"human_oidc","display_name":"Exporter","groups":[],"global_role":"reader","global_permissions":[],"projects":[{"project":"project / one","role":"reader","permissions":["bundle_export"],"source":"test"}]}}"#
        let auth = try JSONDecoder.codifix.decode(AuthMeResponse.self, from: Data(authJSON.utf8))
        let transport = BundlesRecordingTransport(responses: [
            "/v1/projects/project%20%2F%20one/bundle/export/preview": ##"{"bundle_id":"bundle-1","source_project":"project / one","exported_at":"2026-10-15T10:00:00Z","summary_markdown":"# Export","memory_count":1,"relation_count":0,"warning_count":0,"warnings":[],"options":{"include_archived":false,"include_tags":true,"include_relations":true,"include_source_file_paths":false,"include_git_commits":false,"include_source_excerpts":false}}"##
        ])
        let client = MemoryServiceClient(profile: profile, transport: transport, secrets: BundlesTestSecrets())
        let service = BundleService(context: FeatureContext(client: client, profile: profile, auth: auth, revision: 4))

        XCTAssertFalse(EffectivePermissions(auth: auth, projectSlug: profile.projectSlug).allows(.memoryRead))
        let preview = try await service.previewExport(ExportOptions())
        XCTAssertEqual(preview.memoryCount, 1)
        let requests = await transport.requests
        XCTAssertEqual(requests.count, 1)
    }

    func testPreviewSourceInvalidatesOnOptionsFileProjectOrConnectionChange() {
        var options = ExportOptions()
        let exportSource = ExportPreviewSource(options: options, project: "project-a", revision: 4)
        XCTAssertTrue(exportSource.matches(options: options, project: "project-a", revision: 4))
        options.includeArchived = true
        XCTAssertFalse(exportSource.matches(options: options, project: "project-a", revision: 4))
        XCTAssertFalse(exportSource.matches(options: options, project: "project-b", revision: 4))
        XCTAssertFalse(exportSource.matches(options: options, project: "project-a", revision: 5))

        let importSource = ImportPreviewSource(bytes: Data([1, 2, 3]), project: "project-a", revision: 4)
        XCTAssertTrue(importSource.matches(bytes: Data([1, 2, 3]), project: "project-a", revision: 4))
        XCTAssertFalse(importSource.matches(bytes: Data([1, 2, 4]), project: "project-a", revision: 4))
        XCTAssertFalse(importSource.matches(bytes: Data([1, 2, 3]), project: "project-b", revision: 4))
        XCTAssertFalse(importSource.matches(bytes: Data([1, 2, 3]), project: "project-a", revision: 5))
        XCTAssertFalse(importSource.matches(bytes: nil, project: "project-a", revision: 4))
    }

    func testReadOnlyWriteIsRejectedAndServiceFailureIsNotRetried() async throws {
        let profile = profile()
        let deniedTransport = BundlesRecordingTransport()
        let denied = AutomationService(context: FeatureContext(client: MemoryServiceClient(profile: profile, transport: deniedTransport, secrets: BundlesTestSecrets()), profile: profile, auth: nil, revision: 4))
        do { _ = try await denied.previewRun("loop"); XCTFail("read-only context must reject loop writes") }
        catch is AutomationServiceError { }
        let deniedRequests = await deniedTransport.requests
        XCTAssertTrue(deniedRequests.isEmpty)

        let failingTransport = BundlesRecordingTransport(statusCode: 500, failureBody: #"{"message":"temporary failure"}"#)
        let failing = AutomationService(context: context(profile, transport: failingTransport))
        do { _ = try await failing.previewRun("loop"); XCTFail("service failure should be reported") }
        catch { }
        let failedRequests = await failingTransport.requests
        XCTAssertEqual(failedRequests.count, 1, "a failed write must remain a single attempt")
    }

    private func profile() -> ConnectionProfile {
        ConnectionProfile(name: "Test", baseURL: URL(string: "https://memory.example.test")!, projectSlug: "project / one", repositoryRoot: URL(fileURLWithPath: "/repo"), kind: .serviceToken)
    }
    private func context(_ profile: ConnectionProfile, transport: BundlesRecordingTransport) -> FeatureContext {
        let authJSON = #"{"authenticated":true,"mode":"multi_user","read_only":false,"principal":{"id":"principal-1","kind":"human_oidc","display_name":"Test","groups":[],"global_role":"reader","global_permissions":["memory_read","loops_configure"],"projects":[{"project":"project / one","role":"operator","permissions":["memory_read","loops_configure","loops_run","bundle_export","bundle_import"],"source":"test"}]}}"#
        let auth = try! JSONDecoder.codifix.decode(AuthMeResponse.self, from: Data(authJSON.utf8))
        let client = MemoryServiceClient(profile: profile, transport: transport, secrets: BundlesTestSecrets())
        return FeatureContext(client: client, profile: profile, auth: auth, revision: 4)
    }
    private func approval(_ id: String) -> LoopApproval {
        let json = #"{"id":"\#(id)","run_id":null,"project":"project / one","loop_id":"loop","action_type":"update","proposed_action":{"x":1},"risk_reason":"review","status":"pending","created_at":"2026-10-15T10:00:00Z"}"#
        return try! JSONDecoder.codifix.decode(LoopApproval.self, from: Data(json.utf8))
    }
    private func approvalResponse(status: String) -> String {
        #"{"approval":{"id":"40000000-0000-4000-8000-000000000001","run_id":null,"project":"project / one","loop_id":"loop","action_type":"update","proposed_action":{"x":1},"risk_reason":"review","status":"\#(status)","created_at":"2026-10-15T10:00:00Z"}}"#
    }
    private var runResponse: String {
        #"{"run":{"summary":{"id":"20000000-0000-4000-8000-000000000009","loop_id":"loop / one","definition_version":2,"project":"project / one","repo_root":"/repo","mode":"suggest_only","status":"succeeded","started_at":"2026-10-15T10:30:00Z","finished_at":"2026-10-15T10:30:01Z","output_summary":"Dry-run preview","trace_count":1,"blocked_reasons":[]},"run_reason":"manual dry run","effective_settings":{},"policy_decisions":[],"cost":{},"output":{"preview":true},"traces":[{"id":"50000000-0000-4000-8000-000000000001","run_id":"20000000-0000-4000-8000-000000000009","sequence":1,"trace_type":"policy","title":"Preview only","payload":{"dry_run":true},"redacted":false,"created_at":"2026-10-15T10:30:01Z"}],"memory_proposals":[]}}"#
    }
}

private actor BundlesRecordingTransport: HTTPTransport {
    private(set) var requests: [URLRequest] = []
    private let responses: [String: Data]
    private let binaryResponses: [String: Data]
    private let statusCode: Int
    private let failureBody: Data
    init(responses: [String: String] = [:], binaryResponses: [String: Data] = [:], statusCode: Int = 200, failureBody: String = "{}") {
        self.responses = responses.mapValues { Data($0.utf8) }; self.binaryResponses = binaryResponses
        self.statusCode = statusCode; self.failureBody = Data(failureBody.utf8)
    }
    func send(_ request: URLRequest) async throws -> HTTPReply {
        requests.append(request)
        let path = request.url.flatMap { URLComponents(url: $0, resolvingAgainstBaseURL: false)?.percentEncodedPath } ?? ""
        let body = statusCode == 200 ? binaryResponses[path] ?? responses[path] ?? Data("{}".utf8) : failureBody
        return HTTPReply(statusCode: statusCode, headers: [:], data: body)
    }
    func cancelAll() async { }
}
private struct BundlesTestSecrets: SecretStore {
    func read(profileID: UUID) throws -> String? { "test-token" }
    func write(_ secret: String, profileID: UUID) throws { }
    func remove(profileID: UUID) throws { }
}
private extension Data {
    func object() throws -> [String: Any] { try XCTUnwrap(JSONSerialization.jsonObject(with: self) as? [String: Any]) }
}
private extension URL {
    var encodedPath: String? { URLComponents(url: self, resolvingAgainstBaseURL: false)?.percentEncodedPath }
    var queryItems: [URLQueryItem]? { URLComponents(url: self, resolvingAgainstBaseURL: false)?.queryItems }
}
