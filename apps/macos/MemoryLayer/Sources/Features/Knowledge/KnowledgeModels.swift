import Foundation
import Combine

struct ProjectMemoriesResponse: Decodable, Sendable {
    let project: String
    let total: Int
    let items: [MemoryListItem]
}

struct MemoryListItem: Decodable, Identifiable, Sendable, Hashable {
    let id: String
    let summary: String
    let preview: String
    let memoryType: String
    let confidence: Double
    let importance: Double
    let status: String
    let updatedAt: Date
    let tags: [String]
    let tagCount: Int
    let sourceCount: Int
    let canonicalID: String
    let versionNumber: Int
    let isTombstone: Bool

    private enum CodingKeys: String, CodingKey {
        case id, summary, preview, confidence, importance, status, tags
        case memoryType = "memory_type"
        case updatedAt = "updated_at"
        case tagCount = "tag_count"
        case sourceCount = "source_count"
        case canonicalID = "canonical_id"
        case versionNumber = "version_no"
        case isTombstone = "is_tombstone"
    }
}

struct MemoryEntry: Decodable, Identifiable, Sendable {
    let id: String
    let project: String
    let canonicalText: String
    let summary: String
    let memoryType: String
    let importance: Double
    let confidence: Double
    let status: String
    let tags: [String]
    let sources: [MemorySource]
    let relatedMemories: [RelatedMemory]
    let embeddingSpaces: [MemoryEmbeddingSpace]
    let createdAt: Date
    let updatedAt: Date
    let canonicalID: String
    let versionNumber: Int
    let isTombstone: Bool

    private enum CodingKeys: String, CodingKey {
        case id, project, summary, importance, confidence, status, tags, sources
        case canonicalText = "canonical_text"
        case memoryType = "memory_type"
        case relatedMemories = "related_memories"
        case embeddingSpaces = "embedding_spaces"
        case createdAt = "created_at"
        case updatedAt = "updated_at"
        case canonicalID = "canonical_id"
        case versionNumber = "version_no"
        case isTombstone = "is_tombstone"
    }
}

struct MemorySource: Decodable, Identifiable, Sendable, Hashable {
    let sourceID: String?
    let taskID: String?
    let filePath: String?
    let gitCommit: String?
    let symbolName: String?
    let symbolKind: String?
    let sourceKind: String?
    let excerpt: String?
    let provenance: SourceProvenance?

    var stableID: String {
        sourceID ?? [sourceKind, filePath, gitCommit, symbolName].compactMap { $0 }.joined(separator: ":")
    }
    var id: String { stableID }
    var displayName: String { filePath ?? gitCommit ?? taskID ?? sourceKind ?? "Source" }

    private enum CodingKeys: String, CodingKey {
        case sourceID = "id"
        case excerpt, provenance
        case taskID = "task_id"
        case filePath = "file_path"
        case gitCommit = "git_commit"
        case symbolName = "symbol_name"
        case symbolKind = "symbol_kind"
        case sourceKind = "source_kind"
    }
}

struct SourceProvenance: Decodable, Sendable, Hashable {
    let status: String
    let checkedAt: Date?
    let reason: String?
    let resolvedPath: String?

    private enum CodingKeys: String, CodingKey {
        case status, reason
        case checkedAt = "checked_at"
        case resolvedPath = "resolved_path"
    }
}

struct RelatedMemory: Decodable, Identifiable, Sendable, Hashable {
    let memoryID: String
    let relationType: String
    let summary: String
    let memoryType: String
    let confidence: Double
    var id: String { memoryID }

    private enum CodingKeys: String, CodingKey {
        case summary, confidence
        case memoryID = "memory_id"
        case relationType = "relation_type"
        case memoryType = "memory_type"
    }
}

struct MemoryEmbeddingSpace: Decodable, Sendable, Hashable {
    let provider: String
    let model: String
    let baseURL: String
    let chunkCount: Int
    let lastUpdated: Date?

    private enum CodingKeys: String, CodingKey {
        case provider, model
        case baseURL = "base_url"
        case chunkCount = "chunk_count"
        case lastUpdated = "last_updated"
    }
}

struct MemoryHistoryResponse: Decodable, Sendable {
    let canonicalID: String
    let project: String
    let versions: [MemoryEntry]

    private enum CodingKeys: String, CodingKey {
        case project, versions
        case canonicalID = "canonical_id"
    }
}

struct QueryRequest: Encodable, Sendable {
    let project: String
    let query: String
    let filters: [String: String]
    let topK: Int
    let minConfidence: Double?
    let includeStale: Bool
    let history: Bool

    private enum CodingKeys: String, CodingKey {
        case project, query, filters, history
        case topK = "top_k"
        case minConfidence = "min_confidence"
        case includeStale = "include_stale"
    }
}

struct QueryResponse: Decodable, Sendable {
    let answer: String
    let confidence: Double
    let insufficientEvidence: Bool
    let answerCitations: [QueryCitation]
    let results: [QueryResult]

    private enum CodingKeys: String, CodingKey {
        case answer, confidence, results
        case insufficientEvidence = "insufficient_evidence"
        case answerCitations = "answer_citations"
    }
}

struct QueryCitation: Decodable, Identifiable, Sendable, Hashable {
    let resultNumber: Int
    let memoryID: String
    let memoryType: String
    let summary: String
    let snippet: String
    var id: Int { resultNumber }

    private enum CodingKeys: String, CodingKey {
        case summary, snippet
        case resultNumber = "result_number"
        case memoryID = "memory_id"
        case memoryType = "memory_type"
    }
}

struct QueryResult: Decodable, Identifiable, Sendable {
    let memoryID: String
    let summary: String
    let snippet: String
    let memoryType: String
    let score: Double
    let matchKind: String
    let scoreExplanation: [String]
    let tags: [String]
    let sources: [MemorySource]
    var id: String { memoryID }

    private enum CodingKeys: String, CodingKey {
        case summary, snippet, score, tags, sources
        case memoryID = "memory_id"
        case memoryType = "memory_type"
        case matchKind = "match_kind"
        case scoreExplanation = "score_explanation"
    }
}

struct ReplacementProposalListResponse: Decodable, Sendable {
    let project: String
    let proposals: [ReplacementProposal]
}

struct ReplacementProposal: Decodable, Identifiable, Sendable, Hashable {
    let id: String
    let project: String
    let targetMemoryID: String
    let targetSummary: String
    let candidateSummary: String
    let candidateCanonicalText: String
    let candidateMemoryType: String
    let score: Double
    let policy: String
    let reasons: [String]
    let createdAt: Date

    private enum CodingKeys: String, CodingKey {
        case id, project, score, policy, reasons
        case targetMemoryID = "target_memory_id"
        case targetSummary = "target_summary"
        case candidateSummary = "candidate_summary"
        case candidateCanonicalText = "candidate_canonical_text"
        case candidateMemoryType = "candidate_memory_type"
        case createdAt = "created_at"
    }
}

struct ReplacementPolicyResponse: Decodable, Sendable {
    let project: String
    let repositoryRoot: String?
    let replacementPolicy: String
    let writable: Bool

    private enum CodingKeys: String, CodingKey {
        case project, writable
        case repositoryRoot = "repo_root"
        case replacementPolicy = "replacement_policy"
    }
}

struct ReplacementPolicySaveRequest: Encodable, Sendable {
    let repositoryRoot: String
    let replacementPolicy: String
    private enum CodingKeys: String, CodingKey {
        case repositoryRoot = "repo_root"
        case replacementPolicy = "replacement_policy"
    }
}

struct ReplacementProposalResolution: Decodable, Sendable {
    let project: String
    let proposalID: String
    let status: String
    let policy: String
    let targetMemoryID: String
    let targetSummary: String
    let candidateSummary: String
    let newMemoryID: String?

    private enum CodingKeys: String, CodingKey {
        case project, status, policy
        case proposalID = "proposal_id"
        case targetMemoryID = "target_memory_id"
        case targetSummary = "target_summary"
        case candidateSummary = "candidate_summary"
        case newMemoryID = "new_memory_id"
    }
}

struct DeleteMemoryResponse: Decodable, Sendable {
    let memoryID: String
    let project: String
    let deleted: Bool
    let summary: String
    private enum CodingKeys: String, CodingKey { case project, deleted, summary; case memoryID = "memory_id" }
}

struct QueryHistoryEntry: Identifiable {
    let id = UUID()
    let question: String
    let response: QueryResponse
    let duration: Duration
    let askedAt: Date
}

@MainActor
final class KnowledgeSessionStore: ObservableObject {
    @Published private var queryHistoryByConnection: [String: [QueryHistoryEntry]] = [:]

    func queryHistory(profileID: UUID, project: String) -> [QueryHistoryEntry] {
        queryHistoryByConnection[key(profileID: profileID, project: project), default: []]
    }

    func recordQuery(_ entry: QueryHistoryEntry, profileID: UUID, project: String) {
        let scope = key(profileID: profileID, project: project)
        var entries = queryHistoryByConnection[scope, default: []]
        if entries.last?.question == entry.question { entries.removeLast() }
        entries.append(entry)
        queryHistoryByConnection[scope] = Array(entries.suffix(50))
    }

    func clearQueryHistory(profileID: UUID, project: String) {
        queryHistoryByConnection.removeValue(forKey: key(profileID: profileID, project: project))
    }

    private func key(profileID: UUID, project: String) -> String { "\(profileID.uuidString):\(project)" }
}
