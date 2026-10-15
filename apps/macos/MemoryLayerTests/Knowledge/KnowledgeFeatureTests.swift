import XCTest
import Foundation
@testable import MemoryLayer

final class KnowledgeFeatureTests: XCTestCase {
    func testKnowledgeWritesUseExpectedRoutesAndPayloads() async throws {
        let profile = ConnectionProfile(name: "Test", baseURL: URL(string: "https://memory.example.test")!,
                                        projectSlug: "project / one", repositoryRoot: URL(fileURLWithPath: "/repo"),
                                        kind: .serviceToken)
        let transport = KnowledgeRecordingTransport()
        let client = MemoryServiceClient(profile: profile, transport: transport,
                                         secrets: KnowledgeTestSecrets(token: "test-token"))
        let service = KnowledgeService(context: FeatureContext(client: client, profile: profile, auth: nil, revision: 4))

        let answer = try await service.query(QueryRequest(project: profile.projectSlug, query: "Where is the cache?",
                                                          filters: [:], topK: 8, minConfidence: nil,
                                                          includeStale: false, history: false))
        XCTAssertEqual(answer.answer, "The cache is local.")
        let deleted = try await service.delete(memoryID: "memory / 1")
        XCTAssertTrue(deleted.deleted)
        let approved = try await service.resolveProposal(id: "proposal / 2", approve: true)
        XCTAssertEqual(approved.status, "approved")
        let rejected = try await service.resolveProposal(id: "proposal-3", approve: false)
        XCTAssertEqual(rejected.status, "rejected")
        _ = try await service.saveReplacementPolicy("conservative", repositoryRoot: URL(fileURLWithPath: "/repo"))

        let requests = await transport.requests
        XCTAssertEqual(requests.map(\.httpMethod), ["POST", "DELETE", "POST", "POST", "PUT"])
        XCTAssertEqual(requests.map { $0.url?.absoluteString }, [
            "https://memory.example.test/v1/query",
            "https://memory.example.test/v1/memory",
            "https://memory.example.test/v1/projects/project%20%2F%20one/replacement-proposals/proposal%20%2F%202/approve",
            "https://memory.example.test/v1/projects/project%20%2F%20one/replacement-proposals/proposal-3/reject",
            "https://memory.example.test/v1/projects/project%20%2F%20one/replacement-policy"
        ])
        let queryBody = try XCTUnwrap(requests[0].httpBody)
        let queryObject = try XCTUnwrap(JSONSerialization.jsonObject(with: queryBody) as? [String: Any])
        XCTAssertEqual(queryObject["project"] as? String, "project / one")
        XCTAssertEqual(queryObject["query"] as? String, "Where is the cache?")
        XCTAssertEqual(queryObject["top_k"] as? Int, 8)
        XCTAssertEqual(queryObject["history"] as? Bool, false)
        let deleteBody = try XCTUnwrap(requests[1].httpBody)
        XCTAssertEqual(try JSONSerialization.jsonObject(with: deleteBody) as? [String: String], ["memory_id": "memory / 1"])
        let policyBody = try XCTUnwrap(requests[4].httpBody)
        XCTAssertEqual(try JSONSerialization.jsonObject(with: policyBody) as? [String: String],
                       ["repo_root": "/repo", "replacement_policy": "conservative"])
    }

    func testRequestGateRejectsLateSelectionAndConnectionResponses() {
        var gate = KnowledgeRequestGate()
        let oldSelection = gate.begin(revision: 1, resourceID: "memory-a")
        let currentSelection = gate.begin(revision: 1, resourceID: "memory-b")
        XCTAssertFalse(gate.accepts(oldSelection, revision: 1, resourceID: "memory-a"))
        XCTAssertTrue(gate.accepts(currentSelection, revision: 1, resourceID: "memory-b"))
        XCTAssertFalse(gate.accepts(currentSelection, revision: 2, resourceID: "memory-b"))
    }

    func testPopulatedServiceContractDecodesAcrossKnowledgeDTOs() throws {
        let listJSON = #"""
        {"project":"synthetic-project","total":2,"items":[
          {"id":"10000000-0000-4000-8000-000000000001","summary":"Cache location","preview":"The application cache lives under the user cache directory.","memory_type":"fact","confidence":0.92,"importance":3,"status":"active","updated_at":"2026-10-14T11:10:00Z","tags":["storage","desktop"],"tag_count":2,"source_count":2,"canonical_id":"10000000-0000-4000-8000-000000000011","version_no":2,"is_tombstone":false},
          {"id":"10000000-0000-4000-8000-000000000002","summary":"Build check","preview":"Run the fast checks before the full suite.","memory_type":"procedure","confidence":0.81,"importance":2,"status":"archived","updated_at":"2026-10-13T09:00:00Z","tags":["testing"],"tag_count":1,"source_count":1,"canonical_id":"10000000-0000-4000-8000-000000000012","version_no":1,"is_tombstone":false}
        ]}
        """#
        let list = try JSONDecoder.memoryLayer.decode(ProjectMemoriesResponse.self, from: Data(listJSON.utf8))
        XCTAssertEqual(list.items.count, 2)
        XCTAssertEqual(list.items.map(\.status), ["active", "archived"])
        XCTAssertEqual(list.items.first?.sourceCount, 2)

        let detailJSON = #"""
        {"id":"10000000-0000-4000-8000-000000000001","project":"synthetic-project","canonical_text":"Use the operating system's user cache directory for disposable application data.","summary":"Cache location","memory_type":"fact","importance":3,"confidence":0.92,"status":"active","tags":["storage","desktop"],"sources":[
          {"id":"20000000-0000-4000-8000-000000000001","task_id":"30000000-0000-4000-8000-000000000001","file_path":"Sources/CacheStore.swift","git_commit":"0123456789abcdef","symbol_name":"CacheStore","symbol_kind":"struct","source_kind":"file","excerpt":"Caches are disposable and isolated per user.","provenance":{"status":"verified","checked_at":"2026-10-14T10:00:00Z","reason":null,"resolved_path":"/synthetic/Sources/CacheStore.swift"}},
          {"id":"20000000-0000-4000-8000-000000000002","task_id":"30000000-0000-4000-8000-000000000001","file_path":null,"git_commit":null,"source_kind":"task_prompt","excerpt":"Keep cache data outside the repository.","provenance":null}
        ],"related_memories":[{"memory_id":"10000000-0000-4000-8000-000000000009","relation_type":"supports","summary":"Keep generated files out of source control.","memory_type":"decision","confidence":0.88}],"embedding_spaces":[{"provider":"local","model":"test-embedding-v1","base_url":"http://127.0.0.1:8080","chunk_count":3,"last_updated":"2026-10-14T10:30:00Z"}],"created_at":"2026-10-10T08:00:00Z","updated_at":"2026-10-14T11:10:00Z","canonical_id":"10000000-0000-4000-8000-000000000011","version_no":2,"is_tombstone":false}
        """#
        let detail = try JSONDecoder.memoryLayer.decode(MemoryEntry.self, from: Data(detailJSON.utf8))
        XCTAssertEqual(detail.sources.count, 2)
        XCTAssertEqual(Set(detail.sources.map(\.id)).count, 2, "service source UUIDs must remain distinct list identities")
        XCTAssertEqual(detail.sources[0].provenance?.status, "verified")
        XCTAssertEqual(detail.relatedMemories.first?.memoryID, "10000000-0000-4000-8000-000000000009")
        XCTAssertEqual(detail.embeddingSpaces.first?.chunkCount, 3)

        let historyJSON = #"""
        {"canonical_id":"10000000-0000-4000-8000-000000000011","project":"synthetic-project","versions":[
          {"id":"10000000-0000-4000-8000-000000000003","project":"synthetic-project","canonical_text":"Use a cache directory.","summary":"Cache location","memory_type":"fact","importance":2,"confidence":0.75,"status":"archived","tags":["storage"],"sources":[],"related_memories":[],"embedding_spaces":[],"created_at":"2026-10-10T08:00:00Z","updated_at":"2026-10-11T08:00:00Z","canonical_id":"10000000-0000-4000-8000-000000000011","version_no":1,"is_tombstone":false},
          {"id":"10000000-0000-4000-8000-000000000001","project":"synthetic-project","canonical_text":"Use the operating system's user cache directory.","summary":"Cache location","memory_type":"fact","importance":3,"confidence":0.92,"status":"active","tags":["storage"],"sources":[],"related_memories":[],"embedding_spaces":[],"created_at":"2026-10-10T08:00:00Z","updated_at":"2026-10-14T11:10:00Z","canonical_id":"10000000-0000-4000-8000-000000000011","version_no":2,"is_tombstone":false}
        ]}
        """#
        let history = try JSONDecoder.memoryLayer.decode(MemoryHistoryResponse.self, from: Data(historyJSON.utf8))
        XCTAssertEqual(history.versions.map(\.versionNumber), [1, 2])

        let queryJSON = #"""
        {"answer":"Use the user cache directory [1].","confidence":0.9,"insufficient_evidence":false,"answer_generation":{"method":"deterministic","cited_result_numbers":[1],"evidence_count":1,"duration_ms":2,"fallback_reason":null},"answer_citations":[{"result_number":1,"memory_id":"10000000-0000-4000-8000-000000000001","memory_type":"fact","summary":"Cache location","snippet":"The cache is disposable."}],"results":[{"memory_id":"10000000-0000-4000-8000-000000000001","canonical_id":"10000000-0000-4000-8000-000000000011","version_no":2,"summary":"Cache location","snippet":"The cache is disposable.","memory_type":"fact","score":0.87,"match_kind":"hybrid","score_explanation":["Matched cache","Matched storage tag"],"debug":{"chunk_fts":0.7,"entry_fts":0.5,"semantic_similarity":0.8,"exact_phrase_matches":0,"term_overlap":2,"tag_match_count":1,"path_match_count":0,"relation_boost":0,"graph_boost":0,"graph_match_count":0,"graph_edge_count":0,"importance":3,"memory_confidence":0.92,"recency_boost":0.1},"tags":["storage"],"sources":[{"id":"20000000-0000-4000-8000-000000000001","source_kind":"file","file_path":"Sources/CacheStore.swift","excerpt":"The cache is disposable."}],"graph_connections":[]}],"diagnostics":{"retrieval_mode":"hybrid","lexical_enabled":true,"semantic_enabled":true,"graph_enabled":false,"relation_boost_enabled":true,"lexical_candidates":1,"semantic_candidates":1,"merged_candidates":1,"returned_results":1,"relation_augmented_candidates":0,"graph_candidates":0,"graph_augmented_candidates":0,"provenance_decayed_candidates":0,"provenance_unverified_candidates":0,"lexical_duration_ms":1,"semantic_duration_ms":1,"rerank_duration_ms":0,"graph_duration_ms":0,"total_duration_ms":2,"semantic_status":"ready","graph_status":"disabled","provenance_warnings":[]}}
        """#
        let query = try JSONDecoder.memoryLayer.decode(QueryResponse.self, from: Data(queryJSON.utf8))
        XCTAssertEqual(query.answerCitations.first?.resultNumber, 1)
        XCTAssertEqual(query.results.first?.scoreExplanation.count, 2)
        XCTAssertEqual(query.results.first?.sources.first?.displayName, "Sources/CacheStore.swift")

        let proposalsJSON = #"""
        {"project":"synthetic-project","proposals":[{"id":"40000000-0000-4000-8000-000000000001","project":"synthetic-project","target_memory_id":"10000000-0000-4000-8000-000000000001","target_summary":"Cache location","candidate_summary":"Cache path is per user","candidate_canonical_text":"Store disposable data under the operating system user cache directory.","candidate_memory_type":"fact","score":0.94,"policy":"balanced","reasons":["Newer source has a verified path.","Candidate is more precise."],"created_at":"2026-10-14T12:00:00Z"}]}
        """#
        let proposals = try JSONDecoder.memoryLayer.decode(ReplacementProposalListResponse.self, from: Data(proposalsJSON.utf8))
        XCTAssertEqual(proposals.proposals.first?.reasons.count, 2)
        XCTAssertEqual(proposals.proposals.first?.candidateMemoryType, "fact")

        let policyJSON = #"{"project":"synthetic-project","repo_root":"/synthetic/repo","replacement_policy":"balanced","writable":true}"#
        let policy = try JSONDecoder.memoryLayer.decode(ReplacementPolicyResponse.self, from: Data(policyJSON.utf8))
        XCTAssertEqual(policy.repositoryRoot, "/synthetic/repo")
        XCTAssertTrue(policy.writable)
    }

    func testErrorClassificationUsesServiceStatusTypes() {
        guard case .authRequired = knowledgeErrorKind(MemoryServiceError.unauthorized, fallbackMessage: "unauthorized"),
              case .restricted = knowledgeErrorKind(MemoryServiceError.forbidden, fallbackMessage: "forbidden"),
              case .authRequired = knowledgeErrorKind(MemoryServiceError.httpStatus(401, ""), fallbackMessage: ""),
              case .restricted = knowledgeErrorKind(MemoryServiceError.httpStatus(403, ""), fallbackMessage: "") else {
            XCTFail("401 and 403 service errors must map to authentication and restricted states")
            return
        }
    }

    func testLiveReadSmokeDecodesDevelopmentServiceWhenEnabled() async throws {
        guard ProcessInfo.processInfo.environment["MEMORY_LAYER_LIVE_READ_SMOKE"] == "1" ||
                ProcessInfo.processInfo.environment["TEST_RUNNER_MEMORY_LAYER_LIVE_READ_SMOKE"] == "1" else {
            throw XCTSkip("Set MEMORY_LAYER_LIVE_READ_SMOKE=1 to run GET-only smoke against the existing dev service.")
        }
        let environment = ProcessInfo.processInfo.environment
        let baseURL = URL(string: environment["MEMORY_LAYER_LIVE_BASE_URL"] ?? "http://127.0.0.1:4250")!
        let project = environment["MEMORY_LAYER_LIVE_PROJECT"] ?? "memory"
        let profile = ConnectionProfile(name: "Live read smoke", baseURL: baseURL, projectSlug: project, kind: .localSession)
        let service = KnowledgeService(context: FeatureContext(
            client: MemoryServiceClient(profile: profile), profile: profile, auth: nil, revision: 1
        ))
        let list = try await service.memories()
        var detailCount = 0
        var historyCount = 0
        if let first = list.items.first {
            let detail = try await service.memory(id: first.id)
            let history = try await service.history(memoryID: first.id)
            XCTAssertEqual(detail.id, first.id)
            XCTAssertEqual(history.project, list.project)
            detailCount = 1
            historyCount = history.versions.count
        }
        print("Live GET DTO smoke: list_items=\(list.items.count) detail_reads=\(detailCount) history_versions=\(historyCount)")
    }

    @MainActor
    func testQuerySessionHistoryIsScopedAndBounded() {
        let store = KnowledgeSessionStore()
        let firstProfile = UUID()
        let otherProfile = UUID()
        let response = QueryResponse(answer: "Answer", confidence: 0.8, insufficientEvidence: false,
                                     answerCitations: [], results: [])
        for index in 0..<53 {
            store.recordQuery(QueryHistoryEntry(question: "Question \(index)", response: response,
                                                duration: .seconds(1), askedAt: Date(timeIntervalSince1970: Double(index))),
                              profileID: firstProfile, project: "alpha")
        }
        XCTAssertEqual(store.queryHistory(profileID: firstProfile, project: "alpha").count, 50)
        XCTAssertEqual(store.queryHistory(profileID: firstProfile, project: "alpha").first?.question, "Question 3")
        XCTAssertTrue(store.queryHistory(profileID: firstProfile, project: "beta").isEmpty)
        XCTAssertTrue(store.queryHistory(profileID: otherProfile, project: "alpha").isEmpty)
    }
}

private actor KnowledgeRecordingTransport: HTTPTransport {
    private(set) var requests: [URLRequest] = []

    func send(_ request: URLRequest) async throws -> HTTPReply {
        requests.append(request)
        let path = request.url?.path ?? ""
        let body: String
        if path == "/v1/memory" {
            body = #"{"memory_id":"memory / 1","project":"project / one","deleted":true,"summary":"Removed"}"#
        } else if path == "/v1/query" {
            body = #"{"answer":"The cache is local.","confidence":0.9,"insufficient_evidence":false,"answer_citations":[],"results":[]}"#
        } else if path.hasSuffix("/approve") || path.hasSuffix("/reject") {
            let status = path.hasSuffix("/approve") ? "approved" : "rejected"
            body = #"{"project":"project / one","proposal_id":"proposal","status":"\#(status)","policy":"balanced","target_memory_id":"memory","target_summary":"Existing","candidate_summary":"Proposed"}"#
        } else if request.httpMethod == "PUT" {
            body = #"{"project":"project / one","repo_root":"/repo","replacement_policy":"conservative","writable":true}"#
        } else {
            body = "{}"
        }
        return HTTPReply(statusCode: 200, headers: [:], data: Data(body.utf8))
    }

    func cancelAll() async {}
}

private struct KnowledgeTestSecrets: SecretStore {
    let token: String
    func read(profileID: UUID) throws -> String? { token }
    func write(_ secret: String, profileID: UUID) throws {}
    func remove(profileID: UUID) throws {}
}
