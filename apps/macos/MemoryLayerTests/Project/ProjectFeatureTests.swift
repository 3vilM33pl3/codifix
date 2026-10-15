import Foundation
import XCTest
@testable import MemoryLayer

final class ProjectFeatureTests: XCTestCase {
    func testProjectMaintenanceUsesServiceRoutesAndSnakeCaseBodies() async throws {
        let profile = ConnectionProfile(
            name: "Test", baseURL: URL(string: "http://127.0.0.1:4250")!, projectSlug: "project / one",
            repositoryRoot: URL(fileURLWithPath: "/repo"), kind: .localSession
        )
        let transport = ProjectRecordingTransport()
        let client = MemoryServiceClient(profile: profile, transport: transport, secrets: ProjectTestSecrets())
        let context = FeatureContext(client: client, profile: profile, auth: Self.adminAuth(), revision: 7)
        let service = ProjectService(context: context)

        let curate = try await service.curate()
        let reindex = try await service.reindex(backend: "local")
        let reembed = try await service.reembed(backend: "local")
        let archive = try await service.archiveLowValueMemories()
        let repair = try await service.repairSkills(filter: .all)
        let activated = try await service.activateEmbeddingBackend("local")
        let deactivated = try await service.deactivateEmbeddingBackend()
        let creation = try await service.setEmbeddingCreation(backend: "local", enabled: false)

        XCTAssertEqual(curate.outputCount, 1)
        XCTAssertEqual(reindex.reindexedEntries, 9)
        XCTAssertEqual(reembed.reembeddedChunks, 12)
        XCTAssertEqual(archive.archivedCount, 3)
        XCTAssertEqual(repair.inventory.bundleVersion, "1.2.0")
        XCTAssertEqual(activated.backends.count, 1)
        XCTAssertEqual(deactivated.backends.count, 1)
        XCTAssertEqual(creation.backends.count, 1)

        let requests = await transport.requests
        XCTAssertEqual(requests.map(\.httpMethod), Array(repeating: "POST", count: 8))
        XCTAssertEqual(requests.map { URLComponents(url: $0.url!, resolvingAgainstBaseURL: false)!.path }, [
            "/v1/curate", "/v1/reindex", "/v1/reembed", "/v1/archive", "/v1/skills/repair",
            "/v1/embeddings/activate", "/v1/embeddings/deactivate", "/v1/embeddings/create-enabled"
        ])

        let curateBody = try object(requests[0])
        XCTAssertEqual(curateBody["project"] as? String, profile.projectSlug)
        XCTAssertTrue(curateBody.keys.contains("batch_size"))
        XCTAssertTrue(curateBody["batch_size"] is NSNull)

        for index in [1, 2] {
            let body = try object(requests[index])
            XCTAssertEqual(body["project"] as? String, profile.projectSlug)
            XCTAssertEqual(body["backend"] as? String, "local")
        }
        let archiveBody = try object(requests[3])
        XCTAssertEqual(archiveBody["project"] as? String, profile.projectSlug)
        XCTAssertEqual(archiveBody["max_confidence"] as? Double, 0.3)
        XCTAssertEqual(archiveBody["max_importance"] as? Int, 1)
        XCTAssertNil(archiveBody["dry_run"], "The service defaults dry_run to false and the browser client omits it.")

        let repairBody = try object(requests[4])
        XCTAssertEqual(repairBody["repo_root"] as? String, "/repo")
        XCTAssertEqual(repairBody["filter"] as? String, "all")
        XCTAssertEqual(try object(requests[5])["name"] as? String, "local")
        XCTAssertEqual(try object(requests[6]).count, 0)
        let creationBody = try object(requests[7])
        XCTAssertEqual(creationBody["name"] as? String, "local")
        XCTAssertEqual(creationBody["enabled"] as? Bool, false)
    }

    func testProjectReadRoutesEscapeProjectAndPassQueryItems() async throws {
        let profile = ConnectionProfile(name: "Read", baseURL: URL(string: "http://127.0.0.1:4250")!,
                                        projectSlug: "project / one", repositoryRoot: URL(fileURLWithPath: "/repo space"), kind: .localSession)
        let transport = ProjectReadTransport()
        let client = MemoryServiceClient(profile: profile, transport: transport, secrets: ProjectTestSecrets())
        let service = ProjectService(context: FeatureContext(client: client, profile: profile, auth: Self.adminAuth(), revision: 2))

        let overview = try await service.overview()
        let activity = try await service.activities(limit: 15)
        let skills = try await service.skills(filter: .memoryLayer)
        let skill = try await service.skill(name: "memory-query-resume", filter: .all)
        let backends = try await service.embeddingBackends()

        XCTAssertEqual(overview.project, "synthetic-project")
        XCTAssertEqual(activity.items.count, 1)
        XCTAssertEqual(skills.skills.count, 1)
        XCTAssertEqual(skill.skill.name, "memory-query-resume")
        XCTAssertEqual(backends.backends.first?.projectMemoryCount, 2)
        let requests = await transport.requests
        XCTAssertEqual(URLComponents(url: requests[0].url!, resolvingAgainstBaseURL: false)!.percentEncodedPath,
                       "/v1/projects/project%20%2F%20one/overview")
        XCTAssertEqual(URLComponents(url: requests[1].url!, resolvingAgainstBaseURL: false)!.percentEncodedPath,
                       "/v1/projects/project%20%2F%20one/activities")
        let activityQuery = Dictionary(uniqueKeysWithValues: URLComponents(url: requests[1].url!, resolvingAgainstBaseURL: false)!.queryItems!.map { ($0.name, $0.value ?? "") })
        XCTAssertEqual(activityQuery, ["limit": "15", "include_details": "true"])
        XCTAssertEqual(URLComponents(url: requests[2].url!, resolvingAgainstBaseURL: false)!.path, "/v1/skills")
        let skillsQuery = Dictionary(uniqueKeysWithValues: URLComponents(url: requests[2].url!, resolvingAgainstBaseURL: false)!.queryItems!.map { ($0.name, $0.value ?? "") })
        XCTAssertEqual(skillsQuery["repo_root"], "/repo space")
        XCTAssertEqual(skillsQuery["filter"], "memory-layer")
        XCTAssertEqual(URLComponents(url: requests[3].url!, resolvingAgainstBaseURL: false)!.path,
                       "/v1/skills/memory-query-resume")
        XCTAssertEqual(URLComponents(url: requests[4].url!, resolvingAgainstBaseURL: false)!.path,
                       "/v1/embeddings/backends")
    }

    func testProjectWritesUseProjectPermissionsAndGlobalAdminPermissionsSeparately() async throws {
        let profile = ConnectionProfile(name: "Scoped", baseURL: URL(string: "http://127.0.0.1:4250")!,
                                        projectSlug: "alpha", repositoryRoot: URL(fileURLWithPath: "/repo"), kind: .localSession)
        let transport = ProjectRecordingTransport()
        let client = MemoryServiceClient(profile: profile, transport: transport, secrets: ProjectTestSecrets())
        let projectOperator = try Self.auth(#"{"authenticated":true,"mode":"multi_user","read_only":false,"principal":{"id":"p","kind":"service_token","display_name":"Operator","groups":[],"projects":[{"project":"alpha","role":"operator","source":"membership"}]}}"#)
        let service = ProjectService(context: FeatureContext(client: client, profile: profile, auth: projectOperator, revision: 1))
        _ = try await service.reindex()
        do {
            _ = try await service.activateEmbeddingBackend("local")
            XCTFail("A project-scoped operator must not mutate global embedding configuration")
        } catch let error as ProjectFeatureError {
            XCTAssertEqual(error, .restricted)
        }
        do {
            _ = try await service.repairSkills(filter: .all)
            XCTFail("A project-scoped operator must not repair global skills")
        } catch let error as ProjectFeatureError {
            XCTAssertEqual(error, .restricted)
        }
        let requestCountAfterDeniedGlobalWrites = await transport.requests.count
        XCTAssertEqual(requestCountAfterDeniedGlobalWrites, 1)

        let readOnlyContext = FeatureContext(client: client, profile: profile, auth: nil, revision: 2)
        do {
            _ = try await ProjectService(context: readOnlyContext).archiveLowValueMemories()
            XCTFail("Read-only contexts must not issue writes")
        } catch let error as ProjectFeatureError {
            XCTAssertEqual(error, .readOnly)
        }
        let requestCountAfterReadOnlyWrite = await transport.requests.count
        XCTAssertEqual(requestCountAfterReadOnlyWrite, 1)
    }

    func testProjectOverviewAndWatcherPayloadsDecodeServiceFields() throws {
        let response = try JSONDecoder.memoryLayer.decode(ProjectOverviewResponse.self, from: Data(Self.overviewJSON.utf8))
        XCTAssertEqual(response.memoryTypeBreakdown.first?.name, "architecture")
        XCTAssertEqual(response.watchers?.watchers.first?.health, "healthy")
        XCTAssertEqual(response.watchers?.watchers.first?.pid, 48120)
        XCTAssertEqual(response.missingEmbeddingChunks, 5)

        let eventJSON = #"{"project":"synthetic-project","total_returned":1,"items":[{"id":"event-1","recorded_at":"2026-10-14T09:30:00Z","project":"synthetic-project","kind":"diagnostic","summary":"Coverage warning","source":"service","details":{"type":"diagnostic","diagnostic":{"code":"coverage","source":"service","component":"embeddings","operation":"coverage","severity":"warning","message":"Coverage is incomplete","fix_hint":"Create missing embeddings."}}}]}"#
        let events = try JSONDecoder.memoryLayer.decode(ProjectActivityListResponse.self, from: Data(eventJSON.utf8))
        XCTAssertEqual(events.items.first?.details?["type"]?.stringValue, "diagnostic")
        XCTAssertEqual(events.items.first?.details?["diagnostic"]?["severity"]?.stringValue, "warning")
    }

    func testOverviewRequiresRequiredServiceFields() {
        let malformed = #"{"project":"p","service_status":"ok","database_status":"ok","memory_entries_total":0,"active_memories":0,"archived_memories":0}"#
        XCTAssertThrowsError(try JSONDecoder.memoryLayer.decode(ProjectOverviewResponse.self, from: Data(malformed.utf8)))
    }

    private func object(_ request: URLRequest, file: StaticString = #filePath, line: UInt = #line) throws -> [String: Any] {
        let body = try XCTUnwrap(request.httpBody, file: file, line: line)
        return try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any], file: file, line: line)
    }

    private static func adminAuth() -> AuthMeResponse {
        try! auth(#"{"authenticated":true,"mode":"single_user","read_only":false,"principal":{"id":"owner","kind":"local_session","display_name":"Owner","groups":[],"global_role":"admin"}}"#)
    }

    private static func auth(_ json: String) throws -> AuthMeResponse {
        try JSONDecoder.memoryLayer.decode(AuthMeResponse.self, from: Data(json.utf8))
    }

    fileprivate static let overviewJSON = #"{"project":"synthetic-project","service_status":"connected","database_status":"healthy","memory_entries_total":4,"active_memories":3,"archived_memories":1,"raw_captures_total":6,"uncurated_raw_captures":2,"tasks_total":5,"sessions_total":4,"curation_runs_total":3,"recent_memories_7d":2,"recent_captures_7d":3,"high_confidence_memories":2,"medium_confidence_memories":1,"low_confidence_memories":1,"last_memory_at":"2026-10-14T09:00:00Z","last_capture_at":null,"last_curation_at":null,"oldest_uncurated_capture_age_hours":3,"embedding_chunks_total":10,"fresh_embedding_chunks":5,"stale_embedding_chunks":1,"missing_embedding_chunks":5,"embedding_spaces_total":2,"active_embedding_provider":"local","active_embedding_model":"test-v1","memory_type_breakdown":[{"memory_type":"architecture","count":2}],"source_kind_breakdown":[{"source_kind":"file","count":2}],"top_tags":[{"name":"swift","count":2}],"top_files":[{"name":"Sources/App.swift","count":2}],"pending_replacement_proposals":1,"automation":null,"watchers":{"active_count":1,"unhealthy_count":0,"stale_after_seconds":60,"last_heartbeat_at":"2026-10-14T09:00:00Z","watchers":[{"watcher_id":"watcher-1","project":"synthetic-project","repo_root":"/synthetic/repo","hostname":"MacBook","host_service_id":"workshop","pid":48120,"mode":"auto","managed_by_service":true,"health":"healthy","started_at":"2026-10-14T08:00:00Z","last_heartbeat_at":"2026-10-14T09:00:00Z","agent_cli":"codex","agent_session_id":"session-1","agent_pid":48121,"last_restart_attempt_at":null,"restart_attempt_count":0}]}}"#
}

private actor ProjectRecordingTransport: HTTPTransport {
    private(set) var requests: [URLRequest] = []
    func send(_ request: URLRequest) async throws -> HTTPReply {
        requests.append(request)
        let path = URLComponents(url: request.url!, resolvingAgainstBaseURL: false)!.percentEncodedPath
        let body: String
        switch path {
        case "/v1/curate": body = #"{"project_id":"p","run_id":"r","input_count":2,"output_count":1,"replaced_count":0,"proposal_count":1,"dry_run":false}"#
        case "/v1/reindex": body = #"{"reindexed_entries":9,"dry_run":false}"#
        case "/v1/reembed": body = #"{"reembedded_chunks":12,"dry_run":false}"#
        case "/v1/archive": body = #"{"archived_count":3,"dry_run":false}"#
        case "/v1/skills/repair": body = #"{"dry_run":false,"force":false,"backup_root":null,"inventory":{"project_root":"/repo","project_skill_root":"/repo/.agents/skills","template_root":"/templates","bundle_version":"1.2.0","status":"ok","summary":"Skills current","filter":"all","skills":[]}}"#
        case "/v1/embeddings/activate", "/v1/embeddings/deactivate", "/v1/embeddings/create-enabled": body = #"{"backends":[{"name":"local","provider":"local","base_url":"http://127.0.0.1:8080","model":"test-v1","active":true,"ready":true,"create_enabled":false,"project_chunk_count":12,"project_memory_count":3}],"active":"local","create_enabled":false}"#
        default: body = #"{}"#
        }
        return HTTPReply(statusCode: 200, headers: [:], data: Data(body.utf8))
    }
    func cancelAll() async {}
}

private actor ProjectReadTransport: HTTPTransport {
    private(set) var requests: [URLRequest] = []
    func send(_ request: URLRequest) async throws -> HTTPReply {
        requests.append(request)
        let path = URLComponents(url: request.url!, resolvingAgainstBaseURL: false)!.percentEncodedPath
        let body: String
        switch path {
        case "/v1/projects/project%20%2F%20one/overview": body = ProjectFeatureTests.overviewJSON
        case "/v1/projects/project%20%2F%20one/activities":
            body = #"{"project":"project / one","total_returned":1,"items":[{"id":"event-1","recorded_at":"2026-10-14T09:30:00Z","project":"project / one","kind":"query","summary":"Query completed","details":null}]}"#
        case "/v1/skills":
            body = #"{"project_root":"/repo space","project_skill_root":"/repo space/.agents/skills","template_root":"/templates","bundle_version":"1.2.0","status":"ok","summary":"One current skill","filter":"memory-layer","skills":[{"name":"memory-query-resume","project_path":"/repo/memory-query-resume/SKILL.md","template_path":"/templates/memory-query-resume/SKILL.md","project_version":"0.9.5","template_version":"0.9.5","status":"up_to_date","action":"skip","detail":null}]}"#
        case "/v1/skills/memory-query-resume":
            body = #"{"skill":{"name":"memory-query-resume","project_path":"/repo/memory-query-resume/SKILL.md","template_path":"/templates/memory-query-resume/SKILL.md","project_version":"0.9.5","template_version":"0.9.5","status":"up_to_date","action":"skip","detail":null},"content":"Skill content","content_truncated":false}"#
        case "/v1/embeddings/backends":
            body = #"{"backends":[{"name":"local","provider":"local","base_url":"http://127.0.0.1:8080","model":"test-v1","active":true,"ready":true,"create_enabled":true,"project_chunk_count":10,"project_memory_count":2}],"active":"local","create_enabled":true}"#
        default: body = #"{}"#
        }
        return HTTPReply(statusCode: 200, headers: [:], data: Data(body.utf8))
    }
    func cancelAll() async {}
}

private struct ProjectTestSecrets: SecretStore {
    func read(profileID: UUID) throws -> String? { nil }
    func write(_ secret: String, profileID: UUID) throws {}
    func remove(profileID: UUID) throws {}
}
