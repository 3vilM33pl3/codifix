import XCTest
import Foundation
@testable import Codifix

final class GraphFeatureTests: XCTestCase {
    func testGraphDTOsDecodePopulatedServicePayloadsAndKeepProvenanceDirection() throws {
        let status = try JSONDecoder.codifix.decode(CodeGraphStatus.self, from: Data(statusJSON.utf8))
        XCTAssertTrue(status.hasGraph)
        XCTAssertEqual(status.project, "project / one")
        XCTAssertEqual(status.graphNodeCount, 2)
        XCTAssertNotNil(status.completedAt)

        let graph = try JSONDecoder.codifix.decode(CodeGraphResponse.self, from: Data(codeGraphJSON.utf8))
        XCTAssertEqual(graph.nodes.first?.qualifiedName, "crate::handle_query")
        XCTAssertEqual(graph.edges.first?.source, graph.nodes[0].id)
        XCTAssertEqual(graph.filters.depth, 2)
        XCTAssertTrue(graph.truncated)
        XCTAssertEqual(graph.truncationReason, "node limit")

        let memoryGraph = try JSONDecoder.codifix.decode(ProjectMemoryGraphResponse.self, from: Data(memoryGraphJSON.utf8))
        XCTAssertEqual(memoryGraph.returnedMemories, 3)
        let provenance = try XCTUnwrap(memoryGraph.edges.first)
        XCTAssertEqual(provenance.edgeKind, .provenance)
        XCTAssertEqual(provenance.source, "memory:11111111-1111-4111-8111-111111111111")
        XCTAssertEqual(provenance.target, "source:file:crates/mem-search/src/lib.rs::handle_query")

        XCTAssertThrowsError(try JSONDecoder.codifix.decode(CodeGraphNode.self, from: Data(#"{"label":"missing required id"}"#.utf8)))
        XCTAssertThrowsError(try JSONDecoder.codifix.decode(ProjectMemoryGraphNode.self, from: Data(#"{"label":"missing required id","node_kind":"memory","tags":[]}"#.utf8)))
    }

    func testGraphReadRoutesUseEscapedProjectAndBoundedQueryFilters() async throws {
        let transport = GraphRecordingTransport(responses: [
            "/v1/projects/project%20%2F%20one/graph/status": statusJSON,
            "/v1/projects/project%20%2F%20one/graph": codeGraphJSON,
            "/v1/projects/project%20%2F%20one/memory-graph": memoryGraphJSON
        ])
        let context = testContext(transport: transport)
        let filters = GraphFilterDraft(query: "query service", filePath: "crates/mem-search/src/lib.rs",
                                       symbol: "handle_query", edgeKind: "call", runID: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                                       depth: 9, limitNodes: 2_000, limitEdges: 4_000)

        let result = try await GraphService(context: context).load(filters: filters)
        XCTAssertEqual(result.status.project, "project / one")
        XCTAssertEqual(result.codeGraph.nodes.count, 2)
        XCTAssertEqual(result.memoryGraph.nodes.count, 4)

        let requests = await transport.requests
        XCTAssertEqual(requests.count, 3)
        XCTAssertTrue(requests.allSatisfy { $0.httpMethod == "GET" })
        XCTAssertEqual(Set(requests.compactMap { $0.url?.encodedPath }), Set([
            "/v1/projects/project%20%2F%20one/graph/status",
            "/v1/projects/project%20%2F%20one/graph",
            "/v1/projects/project%20%2F%20one/memory-graph"
        ]))
        let graphRequest = try XCTUnwrap(requests.first { $0.url?.encodedPath?.hasSuffix("/graph") == true })
        let queryItems = try XCTUnwrap(URLComponents(url: XCTUnwrap(graphRequest.url), resolvingAgainstBaseURL: false)?.queryItems)
        let query: [String: String] = Dictionary(uniqueKeysWithValues: queryItems.compactMap { item in
            item.value.map { (item.name, $0) }
        })
        XCTAssertEqual(query["run_id"], "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
        XCTAssertEqual(query["q"], "query service")
        XCTAssertEqual(query["file_path"], "crates/mem-search/src/lib.rs")
        XCTAssertEqual(query["depth"], "2")
        XCTAssertEqual(query["limit_nodes"], "1000")
        XCTAssertEqual(query["limit_edges"], "2000")
        let memoryRequest = try XCTUnwrap(requests.first { $0.url?.encodedPath?.hasSuffix("/memory-graph") == true })
        XCTAssertEqual(URLComponents(url: try XCTUnwrap(memoryRequest.url), resolvingAgainstBaseURL: false)?.queryItems?.first?.value, "250")
    }

    func testGraphFilterRejectsMalformedRunIDBeforeRequest() {
        var filter = GraphFilterDraft()
        filter.runID = "not-a-run-id"
        XCTAssertThrowsError(try filter.queryItems()) { error in
            XCTAssertEqual(error as? GraphFilterValidationError, .invalidRunID)
        }
    }

    func testGraphPresentationPreservesSourceOrientationAndIsolatedMemoryNodes() throws {
        let status = try JSONDecoder.codifix.decode(CodeGraphStatus.self, from: Data(statusJSON.utf8))
        let codeGraph = try JSONDecoder.codifix.decode(CodeGraphResponse.self, from: Data(codeGraphJSON.utf8))
        let memoryGraph = try JSONDecoder.codifix.decode(ProjectMemoryGraphResponse.self, from: Data(memoryGraphJSON.utf8))
        let snapshot = GraphServiceSnapshot(status: status, codeGraph: codeGraph, memoryGraph: memoryGraph)

        let provenance = GraphPresentation.make(from: snapshot, visibleLayers: [.provenance], selectedCodeNodeID: nil)
        let provenanceEdge = try XCTUnwrap(provenance.edges.first { $0.layer == .provenance })
        XCTAssertEqual(provenanceEdge.source, .memory("memory:11111111-1111-4111-8111-111111111111"))
        XCTAssertEqual(provenanceEdge.target, .source("source:file:crates/mem-search/src/lib.rs::handle_query"))

        let relationships = GraphPresentation.make(from: snapshot,
                                                   visibleLayers: [.code, .memoryRelations],
                                                   selectedCodeNodeID: codeGraph.nodes[0].id)
        XCTAssertTrue(relationships.nodes.contains { $0.reference == .memory("memory:22222222-2222-4222-8222-222222222222") })
        XCTAssertTrue(relationships.nodes.contains { $0.reference == .memory("memory:33333333-3333-4333-8333-333333333333") })
        let attachment = try XCTUnwrap(relationships.edges.first { $0.isAttachment })
        XCTAssertEqual(attachment.source, .code(codeGraph.nodes[0].id))
        XCTAssertEqual(attachment.target, .source("source:file:crates/mem-search/src/lib.rs::handle_query"))
    }

    func testLayoutIsStableAndLinearForLargeBoundedGraph() {
        let nodes = (0..<1_000).map { index in
            GraphNodeSeed(id: .code(UUID(uuidString: String(format: "00000000-0000-4000-8000-%012d", index))!),
                          grouping: "group-\(index % 12)")
        }
        let first = GraphSpatialLayout.positions(for: nodes)
        let second = GraphSpatialLayout.positions(for: nodes.reversed())
        XCTAssertEqual(first.count, 1_000)
        XCTAssertEqual(first, second)
        XCTAssertTrue(first.values.allSatisfy { $0.x.isFinite && $0.y.isFinite && $0.z.isFinite })
    }

    func testRequestGateRejectsSupersededAndOldConnectionRevisions() {
        var gate = GraphRequestGate()
        let old = gate.begin(revision: 2, key: "project-a")
        let current = gate.begin(revision: 2, key: "project-b")
        XCTAssertFalse(gate.accepts(old, revision: 2, key: "project-a"))
        XCTAssertTrue(gate.accepts(current, revision: 2, key: "project-b"))
        XCTAssertFalse(gate.accepts(current, revision: 3, key: "project-b"))
        XCTAssertFalse(gate.accepts(current, revision: 2, key: "project-a"))
    }

    func testGraphForbiddenResponseRemainsTyped() async throws {
        let transport = GraphRecordingTransport(statusCode: 403, failureBody: #"{"message":"Project access is required."}"#)
        do {
            _ = try await GraphService(context: testContext(transport: transport)).status()
            XCTFail("Expected forbidden response")
        } catch let error as MemoryServiceError {
            XCTAssertEqual(error, .forbidden)
        }
    }

    private func testContext(transport: GraphRecordingTransport) -> FeatureContext {
        let profile = ConnectionProfile(name: "Graph test",
                                        baseURL: URL(string: "https://memory.example.test")!,
                                        projectSlug: "project / one",
                                        repositoryRoot: URL(fileURLWithPath: "/repo"),
                                        kind: .serviceToken)
        let client = MemoryServiceClient(profile: profile, transport: transport, secrets: GraphTestSecrets())
        return FeatureContext(client: client, profile: profile, auth: nil, revision: 7)
    }
}

private actor GraphRecordingTransport: HTTPTransport {
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
        let body = statusCode == 200 ? responses[request.url?.encodedPath ?? ""] ?? "{}" : failureBody
        return HTTPReply(statusCode: statusCode, headers: [:], data: Data(body.utf8))
    }

    func cancelAll() async {}
}

private struct GraphTestSecrets: SecretStore {
    func read(profileID: UUID) throws -> String? { "test-token" }
    func write(_ secret: String, profileID: UUID) throws {}
    func remove(profileID: UUID) throws {}
}

private extension URL {
    var encodedPath: String? { URLComponents(url: self, resolvingAgainstBaseURL: false)?.percentEncodedPath }
}

private let statusJSON = #"{"project":"project / one","has_graph":true,"latest_run_id":"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa","repo_root":"/repo","git_head":"f00dbabe","analyzer_version":"1","strategy_version":"1","status":"complete","completed_at":"2026-10-15T12:00:00Z","symbol_count":2,"reference_count":1,"resolved_reference_count":1,"unresolved_reference_count":0,"ambiguous_reference_count":0,"graph_node_count":2,"graph_edge_count":1,"evidence_count":1}"#

private let codeGraphJSON = #"{"project":"project / one","status":{"project":"project / one","has_graph":true,"symbol_count":2,"reference_count":1,"resolved_reference_count":1,"unresolved_reference_count":0,"ambiguous_reference_count":0,"graph_node_count":2,"graph_edge_count":1,"evidence_count":1},"filters":{"run_id":null,"q":null,"file_path":null,"symbol":null,"edge_kind":null,"depth":2,"limit_nodes":250,"limit_edges":500},"stats":{"total_nodes":4,"total_edges":2,"total_symbols":4,"total_references":2,"unresolved_references":0,"returned_nodes":2,"returned_edges":1,"seed_nodes":1},"truncated":true,"truncation_reason":"node limit","nodes":[{"id":"aaaaaaaa-0000-4000-8000-000000000001","stable_identity":"file:crates/mem-search/src/lib.rs::handle_query","label":"handle_query","node_kind":"symbol","language":"rust","symbol_kind":"function","file_path":"crates/mem-search/src/lib.rs","name":"handle_query","qualified_name":"crate::handle_query","start_line":20,"end_line":40,"degree":3,"seed":true,"group":"mem-search"},{"id":"aaaaaaaa-0000-4000-8000-000000000002","stable_identity":"file:crates/mem-search/src/lib.rs::query","label":"query","node_kind":"symbol","language":"rust","symbol_kind":"function","file_path":"crates/mem-search/src/lib.rs","name":"query","qualified_name":"crate::query","start_line":50,"end_line":80,"degree":1,"seed":false,"group":"mem-search"}],"edges":[{"id":"bbbbbbbb-0000-4000-8000-000000000001","source":"aaaaaaaa-0000-4000-8000-000000000001","target":"aaaaaaaa-0000-4000-8000-000000000002","edge_kind":"calls","reference_kind":"call","confidence":0.9,"file_path":"crates/mem-search/src/lib.rs","start_line":32,"end_line":32,"resolution_status":"resolved"}]}"#

private let memoryGraphJSON = #"{"project":"project / one","total_memories":4,"returned_memories":3,"nodes":[{"id":"memory:11111111-1111-4111-8111-111111111111","label":"Graph service contract","node_kind":"memory","memory_id":"11111111-1111-4111-8111-111111111111","source_id":null,"memory_type":"decision","source_kind":null,"confidence":0.95,"importance":8,"tags":["graph"],"file_path":null,"git_commit":null,"symbol_name":null,"symbol_kind":null,"provenance_status":null,"summary":"Graph service contract","activation":1.5},{"id":"memory:22222222-2222-4222-8222-222222222222","label":"Second memory","node_kind":"memory","memory_id":"22222222-2222-4222-8222-222222222222","source_id":null,"memory_type":"implementation","source_kind":null,"confidence":0.8,"importance":5,"tags":[],"file_path":null,"git_commit":null,"symbol_name":null,"symbol_kind":null,"provenance_status":null,"summary":"Second memory","activation":0.2},{"id":"memory:33333333-3333-4333-8333-333333333333","label":"Isolated memory","node_kind":"memory","memory_id":"33333333-3333-4333-8333-333333333333","source_id":null,"memory_type":"note","source_kind":null,"confidence":0.7,"importance":3,"tags":[],"file_path":null,"git_commit":null,"symbol_name":null,"symbol_kind":null,"provenance_status":null,"summary":"Isolated memory","activation":null},{"id":"source:file:crates/mem-search/src/lib.rs::handle_query","label":"handle_query","node_kind":"source","memory_id":null,"source_id":"44444444-4444-4444-8444-444444444444","memory_type":null,"source_kind":"file","confidence":null,"importance":null,"tags":[],"file_path":"crates/mem-search/src/lib.rs","git_commit":null,"symbol_name":"handle_query","symbol_kind":"function","provenance_status":"verified","summary":null,"activation":null}],"edges":[{"id":"provenance:11111111:source:file:crates/mem-search/src/lib.rs::handle_query","source":"memory:11111111-1111-4111-8111-111111111111","target":"source:file:crates/mem-search/src/lib.rs::handle_query","edge_kind":"provenance","relation_type":null,"source_kind":"file"},{"id":"relation:11111111:supports:22222222","source":"memory:11111111-1111-4111-8111-111111111111","target":"memory:22222222-2222-4222-8222-222222222222","edge_kind":"memory_relation","relation_type":"supports","source_kind":null}]}"#
