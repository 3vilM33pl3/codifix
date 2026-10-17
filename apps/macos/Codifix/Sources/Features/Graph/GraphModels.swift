import Foundation
import simd

struct CodeGraphStatus: Decodable, Sendable {
    let project: String
    let hasGraph: Bool
    let latestRunID: String?
    let repoRoot: String?
    let gitHead: String?
    let analyzerVersion: String?
    let strategyVersion: String?
    let status: String?
    let completedAt: Date?
    let symbolCount: Int
    let referenceCount: Int
    let resolvedReferenceCount: Int
    let unresolvedReferenceCount: Int
    let ambiguousReferenceCount: Int
    let graphNodeCount: Int
    let graphEdgeCount: Int
    let evidenceCount: Int

    private enum CodingKeys: String, CodingKey {
        case project, status
        case hasGraph = "has_graph"
        case latestRunID = "latest_run_id"
        case repoRoot = "repo_root"
        case gitHead = "git_head"
        case analyzerVersion = "analyzer_version"
        case strategyVersion = "strategy_version"
        case completedAt = "completed_at"
        case symbolCount = "symbol_count"
        case referenceCount = "reference_count"
        case resolvedReferenceCount = "resolved_reference_count"
        case unresolvedReferenceCount = "unresolved_reference_count"
        case ambiguousReferenceCount = "ambiguous_reference_count"
        case graphNodeCount = "graph_node_count"
        case graphEdgeCount = "graph_edge_count"
        case evidenceCount = "evidence_count"
    }
}

struct CodeGraphFilters: Decodable, Sendable, Equatable {
    let runID: String?
    let query: String?
    let filePath: String?
    let symbol: String?
    let edgeKind: String?
    let depth: Int
    let limitNodes: Int
    let limitEdges: Int

    private enum CodingKeys: String, CodingKey {
        case depth, symbol
        case runID = "run_id"
        case query = "q"
        case filePath = "file_path"
        case edgeKind = "edge_kind"
        case limitNodes = "limit_nodes"
        case limitEdges = "limit_edges"
    }
}

struct CodeGraphStats: Decodable, Sendable {
    let totalNodes: Int
    let totalEdges: Int
    let totalSymbols: Int
    let totalReferences: Int
    let unresolvedReferences: Int
    let returnedNodes: Int
    let returnedEdges: Int
    let seedNodes: Int

    private enum CodingKeys: String, CodingKey {
        case totalNodes = "total_nodes"
        case totalEdges = "total_edges"
        case totalSymbols = "total_symbols"
        case totalReferences = "total_references"
        case unresolvedReferences = "unresolved_references"
        case returnedNodes = "returned_nodes"
        case returnedEdges = "returned_edges"
        case seedNodes = "seed_nodes"
    }
}

struct CodeGraphNode: Decodable, Identifiable, Sendable {
    let id: UUID
    let stableIdentity: String
    let label: String
    let nodeKind: String
    let language: String?
    let symbolKind: String?
    let filePath: String?
    let name: String?
    let qualifiedName: String?
    let startLine: Int?
    let endLine: Int?
    let degree: Int
    let seed: Bool
    let group: String

    private enum CodingKeys: String, CodingKey {
        case id, label, language, name, degree, seed, group
        case stableIdentity = "stable_identity"
        case nodeKind = "node_kind"
        case symbolKind = "symbol_kind"
        case filePath = "file_path"
        case qualifiedName = "qualified_name"
        case startLine = "start_line"
        case endLine = "end_line"
    }
}

struct CodeGraphEdge: Decodable, Identifiable, Sendable {
    let id: UUID
    let source: UUID
    let target: UUID
    let edgeKind: String
    let referenceKind: String?
    let confidence: Double
    let filePath: String?
    let startLine: Int?
    let endLine: Int?
    let resolutionStatus: String

    private enum CodingKeys: String, CodingKey {
        case id, source, target, confidence
        case edgeKind = "edge_kind"
        case referenceKind = "reference_kind"
        case filePath = "file_path"
        case startLine = "start_line"
        case endLine = "end_line"
        case resolutionStatus = "resolution_status"
    }
}

struct CodeGraphResponse: Decodable, Sendable {
    let project: String
    let status: CodeGraphStatus
    let filters: CodeGraphFilters
    let stats: CodeGraphStats
    let truncated: Bool
    let truncationReason: String?
    let nodes: [CodeGraphNode]
    let edges: [CodeGraphEdge]

    private enum CodingKeys: String, CodingKey {
        case project, status, filters, stats, nodes, edges
        case truncated
        case truncationReason = "truncation_reason"
    }
}

enum MemoryGraphNodeKind: String, Decodable, Sendable {
    case memory
    case source
}

enum MemoryGraphEdgeKind: String, Decodable, Sendable {
    case provenance
    case memoryRelation = "memory_relation"
}

struct ProjectMemoryGraphNode: Decodable, Identifiable, Sendable {
    let id: String
    let label: String
    let nodeKind: MemoryGraphNodeKind
    let memoryID: String?
    let sourceID: String?
    let memoryType: String?
    let sourceKind: String?
    let confidence: Double?
    let importance: Int?
    let tags: [String]
    let filePath: String?
    let gitCommit: String?
    let symbolName: String?
    let symbolKind: String?
    let provenanceStatus: String?
    let summary: String?
    let activation: Double?

    private enum CodingKeys: String, CodingKey {
        case id, label, tags, confidence, importance, summary, activation
        case nodeKind = "node_kind"
        case memoryID = "memory_id"
        case sourceID = "source_id"
        case memoryType = "memory_type"
        case sourceKind = "source_kind"
        case filePath = "file_path"
        case gitCommit = "git_commit"
        case symbolName = "symbol_name"
        case symbolKind = "symbol_kind"
        case provenanceStatus = "provenance_status"
    }
}

struct ProjectMemoryGraphEdge: Decodable, Identifiable, Sendable {
    let id: String
    let source: String
    let target: String
    let edgeKind: MemoryGraphEdgeKind
    let relationType: String?
    let sourceKind: String?

    private enum CodingKeys: String, CodingKey {
        case id, source, target
        case edgeKind = "edge_kind"
        case relationType = "relation_type"
        case sourceKind = "source_kind"
    }
}

struct ProjectMemoryGraphResponse: Decodable, Sendable {
    let project: String
    let totalMemories: Int
    let returnedMemories: Int
    let nodes: [ProjectMemoryGraphNode]
    let edges: [ProjectMemoryGraphEdge]

    private enum CodingKeys: String, CodingKey {
        case project, nodes, edges
        case totalMemories = "total_memories"
        case returnedMemories = "returned_memories"
    }
}

struct GraphFilterDraft: Equatable, Sendable {
    var query = ""
    var filePath = ""
    var symbol = ""
    var edgeKind = ""
    var runID = ""
    var depth = 2
    var limitNodes = 250
    var limitEdges = 500

    var isActive: Bool {
        !query.isEmpty || !filePath.isEmpty || !symbol.isEmpty || !edgeKind.isEmpty || !runID.isEmpty
    }

    func queryItems() throws -> [URLQueryItem] {
        var items: [URLQueryItem] = []
        func append(_ name: String, _ value: String) {
            let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty { items.append(URLQueryItem(name: name, value: trimmed)) }
        }
        if !runID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            guard UUID(uuidString: runID.trimmingCharacters(in: .whitespacesAndNewlines)) != nil else {
                throw GraphFilterValidationError.invalidRunID
            }
            append("run_id", runID)
        }
        append("q", query)
        append("file_path", filePath)
        append("symbol", symbol)
        append("edge_kind", edgeKind)
        items.append(URLQueryItem(name: "depth", value: String(min(2, max(0, depth)))))
        items.append(URLQueryItem(name: "limit_nodes", value: String(min(1_000, max(1, limitNodes)))))
        items.append(URLQueryItem(name: "limit_edges", value: String(min(2_000, max(1, limitEdges)))))
        return items
    }
}

enum GraphFilterValidationError: Error, LocalizedError, Equatable {
    case invalidRunID

    var errorDescription: String? {
        "Run ID must be a valid UUID."
    }
}

enum GraphLayer: String, CaseIterable, Identifiable, Sendable {
    case code
    case provenance
    case memoryRelations = "memory_relations"

    var id: String { rawValue }
    var title: String {
        switch self {
        case .code: "Code"
        case .provenance: "Provenance"
        case .memoryRelations: "Memory relationships"
        }
    }
    var symbol: String {
        switch self {
        case .code: "chevron.left.forwardslash.chevron.right"
        case .provenance: "arrow.triangle.branch"
        case .memoryRelations: "point.3.connected.trianglepath.dotted"
        }
    }
}

struct GraphRequestStamp: Equatable, Sendable {
    let sequence: Int
    let revision: Int
    let key: String
}

struct GraphRequestGate: Sendable {
    private(set) var sequence = 0

    mutating func begin(revision: Int, key: String) -> GraphRequestStamp {
        sequence += 1
        return GraphRequestStamp(sequence: sequence, revision: revision, key: key)
    }

    func accepts(_ stamp: GraphRequestStamp, revision: Int, key: String) -> Bool {
        stamp.sequence == sequence && stamp.revision == revision && stamp.key == key
    }
}

enum GraphNodeReference: Hashable, Sendable, Identifiable {
    case code(UUID)
    case memory(String)
    case source(String)

    var id: String {
        switch self {
        case let .code(id): "code:\(id.uuidString.lowercased())"
        case let .memory(id): "memory:\(id)"
        case let .source(id): "source:\(id)"
        }
    }
}

struct GraphNodeSeed: Sendable {
    let id: GraphNodeReference
    let grouping: String
}

enum GraphSpatialLayout {
    /// A stable spherical layout: O(nodes log nodes), bounded independently of graph degree.
    /// Group centres preserve the API's package/type grouping without rerunning a
    /// force simulation every time a filter or camera control changes.
    static func positions(for nodes: [GraphNodeSeed]) -> [GraphNodeReference: SIMD3<Float>] {
        let sorted = nodes.sorted { $0.id.id < $1.id.id }
        guard !sorted.isEmpty else { return [:] }
        let groupedNodes = Dictionary(grouping: sorted, by: \.grouping)
        let groups = groupedNodes.keys.sorted()
        let groupIndex = Dictionary(uniqueKeysWithValues: groups.enumerated().map { ($1, $0) })
        let groupCount = max(1, groups.count)
        var positions: [GraphNodeReference: SIMD3<Float>] = [:]

        for group in groups {
            guard let index = groupIndex[group] else { continue }
            let members = groupedNodes[group] ?? []
            let centerAngle = Float(index) * 2.3999632
            let centerRadius: Float = groupCount == 1 ? 0 : min(4.5, Float(groupCount) * 0.45)
            let center = SIMD3<Float>(cos(centerAngle) * centerRadius,
                                      sin(centerAngle) * centerRadius,
                                      (Float(index % 3) - 1) * 0.8)
            let spread = min(3.4, max(1.3, sqrt(Float(members.count)) * 0.72))
            for (memberIndex, node) in members.enumerated() {
                let t = Float(memberIndex) + 0.5
                let fraction = t / Float(members.count)
                let y = 1 - 2 * fraction
                let ringRadius = sqrt(max(0, 1 - y * y))
                let angle = t * 2.3999632 + Float(stableHash(node.id.id) % 10_000) / 10_000
                let local = SIMD3<Float>(cos(angle) * ringRadius * spread,
                                         y * spread,
                                         sin(angle) * ringRadius * spread)
                positions[node.id] = center + local
            }
        }
        return positions
    }

    private static func stableHash(_ string: String) -> UInt64 {
        string.utf8.reduce(14_695_981_039_346_656_037) { ($0 ^ UInt64($1)) &* 1_099_511_628_211 }
    }
}
