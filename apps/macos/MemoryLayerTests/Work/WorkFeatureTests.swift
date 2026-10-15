import XCTest
import Foundation
@testable import MemoryLayer

final class WorkFeatureTests: XCTestCase {
    func testWorkReadRoutesAndQueryParametersMatchServiceContract() async throws {
        let profile = testProfile()
        let transport = WorkRecordingTransport(responses: [
            "/v1/agents": #"{"collected_at":"2026-10-15T12:00:00Z","sessions":[],"orphan_ports":[],"rate_limits":[]}"#,
            "/v1/projects/project%20%2F%20one/activities": #"{"project":"project / one","total_returned":1,"items":[{"id":"event-1","project":"project / one","kind":"plan","summary":"Plan updated","details":{"type":"plan","remaining_items":["Review"]},"recorded_at":"2026-10-15T12:00:00Z"}]}"#
        ])
        let service = WorkService(context: testContext(profile: profile, transport: transport))

        let snapshot = try await service.agentSnapshot()
        let activities = try await service.activities(kind: "plan", limit: 100)

        XCTAssertEqual(snapshot.sessions.count, 0)
        XCTAssertEqual(activities.items.first?.summary, "Plan updated")
        XCTAssertEqual(activities.items.first?.details?.displayText.contains("remaining_items"), true)
        let requests = await transport.requests
        XCTAssertEqual(requests.map { $0.httpMethod ?? "" }, ["GET", "GET"])
        XCTAssertEqual(requests[0].url?.encodedPath, "/v1/agents")
        XCTAssertEqual(requests[1].url?.encodedPath, "/v1/projects/project%20%2F%20one/activities")
        XCTAssertEqual(requests[1].url?.query, "limit=100&include_details=true&kind=plan")
    }

    func testBriefingAndResumeUseReadLikePostRoutesAndExactBodies() async throws {
        let profile = testProfile()
        let transport = WorkRecordingTransport(responses: [
            "/v1/projects/project%20%2F%20one/up-to-speed": #"{"project":"project / one","generated_at":"2026-10-15T12:00:00Z","briefing":"Focus on review","current_focus":["Review"],"recent_work":[],"blockers":[],"next_actions":[],"useful_memories":[],"recent_activities":[],"token_usage":{"action_count":2,"total_input_tokens":10,"total_output_tokens":5,"total_cache_read_tokens":0,"total_cache_write_tokens":0,"total_tokens":15},"warnings":[]}"#,
            "/v1/projects/project%20%2F%20one/resume": #"{"project":"project / one","generated_at":"2026-10-15T12:00:00Z","briefing":"Continue review","checkpoint":{"project":"project / one","repo_root":"/repo","marked_at":"2026-10-15T12:00:00Z","git_branch":"main"},"current_thread":null,"change_summary":[],"attention_items":[],"primary_next_step":null,"secondary_next_steps":[{"title":"Review result","rationale":"The next action"}],"context_items":[{"id":"m-1","summary":"Memory one","memory_type":"decision"}],"timeline":[],"commits":[{"hash":"abc123","short_hash":"abc123","subject":"Keep work state","committed_at":"2026-10-15T11:59:00Z"}],"changed_memories":[],"durable_context":[],"warnings":["One warning"],"actions":[]}"#
        ])
        let service = WorkService(context: testContext(profile: profile, transport: transport))

        let briefing = try await service.briefing(includeLLMSummary: true, limit: 20)
        let resume = try await service.resume(includeLLMSummary: false, limit: 20)

        XCTAssertEqual(briefing.briefing, "Focus on review")
        XCTAssertEqual(briefing.tokenUsage.totalTokens, 15)
        XCTAssertEqual(resume.checkpoint?.gitBranch, "main")
        XCTAssertEqual(resume.memories.map(\.id), ["m-1"])
        XCTAssertEqual(resume.nextSteps.first?.title, "Review result")
        XCTAssertEqual(resume.commits.first?.subject, "Keep work state")
        let requests = await transport.requests
        XCTAssertEqual(requests.map { $0.httpMethod ?? "" }, ["POST", "POST"])
        XCTAssertEqual(requests.map { $0.url?.encodedPath }, [
            "/v1/projects/project%20%2F%20one/up-to-speed",
            "/v1/projects/project%20%2F%20one/resume"
        ])
        let briefingBody = try XCTUnwrap(requests[0].httpBody).jsonObject()
        XCTAssertEqual(briefingBody["project"] as? String, "project / one")
        XCTAssertEqual(briefingBody["include_llm_summary"] as? Bool, true)
        XCTAssertEqual(briefingBody["limit"] as? Int, 20)
        let resumeBody = try XCTUnwrap(requests[1].httpBody).jsonObject()
        XCTAssertEqual(resumeBody["project"] as? String, "project / one")
        XCTAssertEqual(resumeBody["repo_root"] as? String, "/repo")
        XCTAssertEqual(resumeBody["include_llm_summary"] as? Bool, false)
        XCTAssertEqual(resumeBody["limit"] as? Int, 20)
    }

    func testAuditSettingUsesExistingReadAndGlobalAdminRoutes() async throws {
        let profile = testProfile()
        let transport = WorkRecordingTransport(responses: [
            "/v1/config/llm-audit": #"{"enabled":true,"redacted":true,"profile":"Default","max_message_chars":8000,"max_total_chars":32000}"#
        ])
        let service = WorkService(context: testContext(profile: profile, transport: transport))

        let status = try await service.llmAuditStatus()
        let auth = try JSONDecoder.memoryLayer.decode(AuthMeResponse.self, from: Data(#"{"authenticated":true,"mode":"single_user","read_only":false,"principal":{"id":"test-admin","global_role":"admin","global_permissions":["system_admin"],"projects":[]}}"#.utf8))
        let adminService = WorkService(context: testContext(profile: profile, transport: transport, auth: auth))
        let changed = try await adminService.setLLMAudit(enabled: true)

        XCTAssertTrue(status.enabled)
        XCTAssertTrue(changed.redacted)
        let requests = await transport.requests
        XCTAssertEqual(requests.map { $0.httpMethod ?? "" }, ["GET", "POST"])
        XCTAssertEqual(requests.map { $0.url?.encodedPath }, ["/v1/config/llm-audit", "/v1/config/llm-audit"])
        let update = try XCTUnwrap(requests[1].httpBody).jsonObject()
        XCTAssertEqual(update["enabled"] as? Bool, true)
    }

    func testPartialServicePayloadsDecodeWithoutInventingMissingData() throws {
        let snapshot = try JSONDecoder.memoryLayer.decode(WorkAgentSnapshot.self, from: Data(#"{"collected_at":"2026-10-15T12:00:00Z","sessions":[{"session_id":"s-1","status":"working"}],"orphan_ports":[],"rate_limits":[]}"#.utf8))
        XCTAssertEqual(snapshot.sessions.first?.sessionID, "s-1")
        XCTAssertEqual(snapshot.sessions.first?.currentTasks, [])
        XCTAssertEqual(snapshot.orphanPorts, [])
        XCTAssertTrue(snapshot.rateLimits.isEmpty)

        let activities = try JSONDecoder.memoryLayer.decode(WorkActivityList.self, from: Data(#"{"items":[{"id":"e-1","kind":"diagnostic","summary":"Service warning","recorded_at":"2026-10-15T12:00:00Z"}]}"#.utf8))
        XCTAssertNil(activities.project)
        XCTAssertNil(activities.items[0].details)
        XCTAssertNil(activities.items[0].provider)

        XCTAssertThrowsError(try JSONDecoder.memoryLayer.decode(WorkAgentSession.self, from: Data(#"{"status":"working"}"#.utf8)))
        XCTAssertThrowsError(try JSONDecoder.memoryLayer.decode(WorkActivityEvent.self, from: Data(#"{"kind":"query","summary":"Missing ID and time"}"#.utf8)))
        XCTAssertThrowsError(try JSONDecoder.memoryLayer.decode(WorkActivityList.self, from: Data(#"{"project":"project / one"}"#.utf8)))

        let resume = try JSONDecoder.memoryLayer.decode(WorkResumeResponse.self, from: Data(#"{"briefing":"Resume without optional context"}"#.utf8))
        XCTAssertTrue(resume.memories.isEmpty)
        XCTAssertTrue(resume.nextSteps.isEmpty)
        XCTAssertNil(resume.checkpoint)
    }

    func testRequestGateRejectsOldSelectionAndConnectionRevision() {
        var gate = WorkRequestGate()
        let first = gate.begin(revision: 2, resourceID: "session-a")
        let second = gate.begin(revision: 2, resourceID: "session-b")

        XCTAssertFalse(gate.accepts(first, revision: 2, resourceID: "session-a"))
        XCTAssertTrue(gate.accepts(second, revision: 2, resourceID: "session-b"))
        XCTAssertFalse(gate.accepts(second, revision: 3, resourceID: "session-b"))
        XCTAssertFalse(gate.accepts(second, revision: 2, resourceID: "session-a"))
    }

    func testServiceFailureRemainsTypedAsForbidden() async throws {
        let profile = testProfile()
        let transport = WorkRecordingTransport(statusCode: 403, failureBody: #"{"message":"Project access is required."}"#)
        let service = WorkService(context: testContext(profile: profile, transport: transport))

        do {
            _ = try await service.agentSnapshot()
            XCTFail("Expected access failure")
        } catch let error as MemoryServiceError {
            XCTAssertEqual(error, .forbidden)
        }
    }

    func testAuditMutationDeniesUnprivilegedContextBeforeSending() async throws {
        let transport = WorkRecordingTransport()
        let service = WorkService(context: testContext(profile: testProfile(), transport: transport))
        do {
            _ = try await service.setLLMAudit(enabled: true)
            XCTFail("Expected local permission guard")
        } catch let error as MemoryServiceError {
            XCTAssertEqual(error, .forbidden)
        }
        let requests = await transport.requests
        XCTAssertTrue(requests.isEmpty)
    }

    func testOptionalLiveReadSmoke() async throws {
        guard ProcessInfo.processInfo.environment["MEMORY_LAYER_LIVE_READ_SMOKE"] == "1" ||
                ProcessInfo.processInfo.environment["TEST_RUNNER_MEMORY_LAYER_LIVE_READ_SMOKE"] == "1" else {
            throw XCTSkip("Set MEMORY_LAYER_LIVE_READ_SMOKE=1 to exercise the local development service.")
        }

        let profile = ConnectionProfile(name: "Local development service",
                                        baseURL: URL(string: "http://127.0.0.1:4250")!,
                                        projectSlug: "memory",
                                        repositoryRoot: URL(fileURLWithPath: "/Users/olivier/Projects/memory"),
                                        kind: .localSession)
        let context = FeatureContext(
            client: MemoryServiceClient(profile: profile, transport: URLSessionTransport(), secrets: WorkTestSecrets()),
            profile: profile, auth: nil, revision: 1
        )
        let service = WorkService(context: context)
        _ = try await context.client.bootstrapLocalSession(context: context.requestContext)
        let snapshot = try await service.agentSnapshot()
        let activityList = try await service.activities(kind: nil, limit: 100)
        print("Live Work read smoke decoded agents=\(snapshot.sessions.count) activities=\(activityList.items.count)")
    }

    private func testProfile() -> ConnectionProfile {
        ConnectionProfile(name: "Test", baseURL: URL(string: "https://memory.example.test")!,
                          projectSlug: "project / one", repositoryRoot: URL(fileURLWithPath: "/repo"), kind: .serviceToken)
    }

    private func testContext(profile: ConnectionProfile, transport: WorkRecordingTransport, auth: AuthMeResponse? = nil) -> FeatureContext {
        let client = MemoryServiceClient(profile: profile, transport: transport, secrets: WorkTestSecrets())
        return FeatureContext(client: client, profile: profile, auth: auth, revision: 7)
    }
}

private actor WorkRecordingTransport: HTTPTransport {
    private(set) var requests: [URLRequest] = []
    private let responses: [String: String]
    private let statusCode: Int
    private let failureBody: String

    init(responses: [String: String] = [:], statusCode: Int = 200, failureBody: String = "{}") {
        self.responses = responses
        self.statusCode = statusCode
        self.failureBody = failureBody
    }

    func send(_ request: URLRequest) async throws -> HTTPReply {
        requests.append(request)
        let responseBody = statusCode == 200 ? responses[request.url?.encodedPath ?? ""] ?? "{}" : failureBody
        return HTTPReply(statusCode: statusCode, headers: [:], data: Data(responseBody.utf8))
    }

    func cancelAll() async {}
}

private struct WorkTestSecrets: SecretStore {
    func read(profileID: UUID) throws -> String? { "test-token" }
    func write(_ secret: String, profileID: UUID) throws {}
    func remove(profileID: UUID) throws {}
}

private extension Data {
    func jsonObject() throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: self) as? [String: Any])
    }
}

private extension URL {
    var encodedPath: String? { URLComponents(url: self, resolvingAgainstBaseURL: false)?.percentEncodedPath }
}
