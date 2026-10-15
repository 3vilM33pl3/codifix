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
