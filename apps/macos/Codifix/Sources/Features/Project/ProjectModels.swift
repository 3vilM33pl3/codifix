import Foundation

struct ProjectOverviewResponse: Decodable, Sendable {
    let project: String
    let serviceStatus: String
    let databaseStatus: String
    let memoryEntriesTotal: Int
    let activeMemories: Int
    let archivedMemories: Int
    let rawCapturesTotal: Int
    let uncuratedRawCaptures: Int
    let tasksTotal: Int
    let sessionsTotal: Int
    let curationRunsTotal: Int
    let recentMemories7d: Int
    let recentCaptures7d: Int
    let highConfidenceMemories: Int
    let mediumConfidenceMemories: Int
    let lowConfidenceMemories: Int
    let lastMemoryAt: Date?
    let lastCaptureAt: Date?
    let lastCurationAt: Date?
    let oldestUncuratedCaptureAgeHours: Int?
    let embeddingChunksTotal: Int
    let freshEmbeddingChunks: Int
    let staleEmbeddingChunks: Int
    let missingEmbeddingChunks: Int
    let embeddingSpacesTotal: Int
    let activeEmbeddingProvider: String?
    let activeEmbeddingModel: String?
    let memoryTypeBreakdown: [ProjectNamedCount]
    let sourceKindBreakdown: [ProjectNamedCount]
    let topTags: [ProjectNamedCount]
    let topFiles: [ProjectNamedCount]
    let pendingReplacementProposals: Int
    let automation: ProjectAutomationStatus?
    let watchers: ProjectWatcherSummary?

    private enum CodingKeys: String, CodingKey {
        case project
        case serviceStatus = "service_status"
        case databaseStatus = "database_status"
        case memoryEntriesTotal = "memory_entries_total"
        case activeMemories = "active_memories"
        case archivedMemories = "archived_memories"
        case rawCapturesTotal = "raw_captures_total"
        case uncuratedRawCaptures = "uncurated_raw_captures"
        case tasksTotal = "tasks_total"
        case sessionsTotal = "sessions_total"
        case curationRunsTotal = "curation_runs_total"
        case recentMemories7d = "recent_memories_7d"
        case recentCaptures7d = "recent_captures_7d"
        case highConfidenceMemories = "high_confidence_memories"
        case mediumConfidenceMemories = "medium_confidence_memories"
        case lowConfidenceMemories = "low_confidence_memories"
        case lastMemoryAt = "last_memory_at"
        case lastCaptureAt = "last_capture_at"
        case lastCurationAt = "last_curation_at"
        case oldestUncuratedCaptureAgeHours = "oldest_uncurated_capture_age_hours"
        case embeddingChunksTotal = "embedding_chunks_total"
        case freshEmbeddingChunks = "fresh_embedding_chunks"
        case staleEmbeddingChunks = "stale_embedding_chunks"
        case missingEmbeddingChunks = "missing_embedding_chunks"
        case embeddingSpacesTotal = "embedding_spaces_total"
        case activeEmbeddingProvider = "active_embedding_provider"
        case activeEmbeddingModel = "active_embedding_model"
        case memoryTypeBreakdown = "memory_type_breakdown"
        case sourceKindBreakdown = "source_kind_breakdown"
        case topTags = "top_tags"
        case topFiles = "top_files"
        case pendingReplacementProposals = "pending_replacement_proposals"
        case automation, watchers
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        project = try c.decode(String.self, forKey: .project)
        serviceStatus = try c.decode(String.self, forKey: .serviceStatus)
        databaseStatus = try c.decode(String.self, forKey: .databaseStatus)
        memoryEntriesTotal = try c.decode(Int.self, forKey: .memoryEntriesTotal)
        activeMemories = try c.decode(Int.self, forKey: .activeMemories)
        archivedMemories = try c.decode(Int.self, forKey: .archivedMemories)
        rawCapturesTotal = try c.decode(Int.self, forKey: .rawCapturesTotal)
        uncuratedRawCaptures = try c.decode(Int.self, forKey: .uncuratedRawCaptures)
        tasksTotal = try c.decode(Int.self, forKey: .tasksTotal)
        sessionsTotal = try c.decode(Int.self, forKey: .sessionsTotal)
        curationRunsTotal = try c.decode(Int.self, forKey: .curationRunsTotal)
        recentMemories7d = try c.decode(Int.self, forKey: .recentMemories7d)
        recentCaptures7d = try c.decode(Int.self, forKey: .recentCaptures7d)
        highConfidenceMemories = try c.decode(Int.self, forKey: .highConfidenceMemories)
        mediumConfidenceMemories = try c.decode(Int.self, forKey: .mediumConfidenceMemories)
        lowConfidenceMemories = try c.decode(Int.self, forKey: .lowConfidenceMemories)
        lastMemoryAt = try c.decodeIfPresent(Date.self, forKey: .lastMemoryAt)
        lastCaptureAt = try c.decodeIfPresent(Date.self, forKey: .lastCaptureAt)
        lastCurationAt = try c.decodeIfPresent(Date.self, forKey: .lastCurationAt)
        oldestUncuratedCaptureAgeHours = try c.decodeIfPresent(Int.self, forKey: .oldestUncuratedCaptureAgeHours)
        embeddingChunksTotal = try c.decodeIfPresent(Int.self, forKey: .embeddingChunksTotal) ?? 0
        freshEmbeddingChunks = try c.decodeIfPresent(Int.self, forKey: .freshEmbeddingChunks) ?? 0
        staleEmbeddingChunks = try c.decodeIfPresent(Int.self, forKey: .staleEmbeddingChunks) ?? 0
        missingEmbeddingChunks = try c.decodeIfPresent(Int.self, forKey: .missingEmbeddingChunks) ?? 0
        embeddingSpacesTotal = try c.decodeIfPresent(Int.self, forKey: .embeddingSpacesTotal) ?? 0
        activeEmbeddingProvider = try c.decodeIfPresent(String.self, forKey: .activeEmbeddingProvider)
        activeEmbeddingModel = try c.decodeIfPresent(String.self, forKey: .activeEmbeddingModel)
        memoryTypeBreakdown = try c.decodeIfPresent([ProjectNamedCount].self, forKey: .memoryTypeBreakdown) ?? []
        sourceKindBreakdown = try c.decodeIfPresent([ProjectNamedCount].self, forKey: .sourceKindBreakdown) ?? []
        topTags = try c.decodeIfPresent([ProjectNamedCount].self, forKey: .topTags) ?? []
        topFiles = try c.decodeIfPresent([ProjectNamedCount].self, forKey: .topFiles) ?? []
        pendingReplacementProposals = try c.decodeIfPresent(Int.self, forKey: .pendingReplacementProposals) ?? 0
        automation = try c.decodeIfPresent(ProjectAutomationStatus.self, forKey: .automation)
        watchers = try c.decodeIfPresent(ProjectWatcherSummary.self, forKey: .watchers)
    }
}

struct ProjectNamedCount: Decodable, Sendable, Identifiable {
    let name: String
    let count: Int
    var id: String { name }

    private enum CodingKeys: String, CodingKey { case name, count, memoryType = "memory_type", sourceKind = "source_kind" }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decodeIfPresent(String.self, forKey: .name)
            ?? c.decodeIfPresent(String.self, forKey: .memoryType)
            ?? c.decodeIfPresent(String.self, forKey: .sourceKind)
            ?? "Unknown"
        count = try c.decode(Int.self, forKey: .count)
    }
}

struct ProjectAutomationStatus: Decodable, Sendable {
    let enabled: Bool
    let mode: String
    let repoRoot: String
    let dirtyFileCount: Int?
    let pendingNoteCount: Int?
    private enum CodingKeys: String, CodingKey {
        case enabled, mode
        case repoRoot = "repo_root"
        case dirtyFileCount = "dirty_file_count"
        case pendingNoteCount = "pending_note_count"
    }
}

struct ProjectWatcherSummary: Decodable, Sendable {
    let activeCount: Int
    let unhealthyCount: Int
    let staleAfterSeconds: Int
    let lastHeartbeatAt: Date?
    let watchers: [ProjectWatcher]
    private enum CodingKeys: String, CodingKey {
        case watchers
        case activeCount = "active_count"
        case unhealthyCount = "unhealthy_count"
        case staleAfterSeconds = "stale_after_seconds"
        case lastHeartbeatAt = "last_heartbeat_at"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        activeCount = try c.decode(Int.self, forKey: .activeCount)
        unhealthyCount = try c.decode(Int.self, forKey: .unhealthyCount)
        staleAfterSeconds = try c.decode(Int.self, forKey: .staleAfterSeconds)
        lastHeartbeatAt = try c.decodeIfPresent(Date.self, forKey: .lastHeartbeatAt)
        watchers = try c.decodeIfPresent([ProjectWatcher].self, forKey: .watchers) ?? []
    }
}

struct ProjectWatcher: Decodable, Identifiable, Sendable {
    let watcherID: String
    let project: String
    let repoRoot: String
    let hostname: String
    let hostServiceID: String
    let pid: Int
    let mode: String
    let managedByService: Bool
    let health: String
    let startedAt: Date
    let lastHeartbeatAt: Date
    let agentCLI: String?
    let agentSessionID: String?
    let agentPID: Int?
    let lastRestartAttemptAt: Date?
    let restartAttemptCount: Int
    var id: String { watcherID }
    private enum CodingKeys: String, CodingKey {
        case project, hostname, pid, mode, health
        case watcherID = "watcher_id"
        case repoRoot = "repo_root"
        case hostServiceID = "host_service_id"
        case managedByService = "managed_by_service"
        case startedAt = "started_at"
        case lastHeartbeatAt = "last_heartbeat_at"
        case agentCLI = "agent_cli"
        case agentSessionID = "agent_session_id"
        case agentPID = "agent_pid"
        case lastRestartAttemptAt = "last_restart_attempt_at"
        case restartAttemptCount = "restart_attempt_count"
    }
}

struct ProjectActivityListResponse: Decodable, Sendable {
    let project: String
    let totalReturned: Int
    let items: [ProjectActivity]
    private enum CodingKeys: String, CodingKey {
        case project, items
        case totalReturned = "total_returned"
    }
}

struct ProjectActivity: Decodable, Identifiable, Sendable {
    let id: String
    let project: String
    let kind: String
    let summary: String
    let details: ProjectJSONValue?
    let source: String?
    let recordedAt: Date
    private enum CodingKeys: String, CodingKey {
        case id, project, kind, summary, details, source
        case recordedAt = "recorded_at"
    }
}

indirect enum ProjectJSONValue: Decodable, Sendable {
    case string(String)
    case number(Double)
    case bool(Bool)
    case object([String: ProjectJSONValue])
    case array([ProjectJSONValue])
    case null

    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let value = try? c.decode(String.self) { self = .string(value) }
        else if let value = try? c.decode(Bool.self) { self = .bool(value) }
        else if let value = try? c.decode(Double.self) { self = .number(value) }
        else if let value = try? c.decode([String: ProjectJSONValue].self) { self = .object(value) }
        else if let value = try? c.decode([ProjectJSONValue].self) { self = .array(value) }
        else { throw DecodingError.dataCorruptedError(in: c, debugDescription: "Unsupported JSON value") }
    }

    subscript(_ key: String) -> ProjectJSONValue? {
        guard case let .object(values) = self else { return nil }
        return values[key]
    }
    var stringValue: String? { if case let .string(value) = self { value } else { nil } }
    var objectValue: [String: ProjectJSONValue]? { if case let .object(value) = self { value } else { nil } }
}

struct ProjectDiagnostic: Decodable, Sendable, Identifiable {
    let code: String
    let source: String
    let component: String
    let operation: String
    let severity: String
    let message: String
    let rawError: String?
    let explanation: String?
    let fixHint: String?
    let doctorHint: String?
    let commandHint: String?
    var id: String { "\(code)-\(component)-\(operation)-\(message)" }

    private enum CodingKeys: String, CodingKey {
        case code, source, component, operation, severity, message
        case rawError = "raw_error"
        case explanation
        case fixHint = "fix_hint"
        case doctorHint = "doctor_hint"
        case commandHint = "command_hint"
    }
}

struct ProjectEmbeddingBackendsResponse: Decodable, Sendable {
    let backends: [ProjectEmbeddingBackend]
    let active: String?
    let createEnabled: Bool
    private enum CodingKeys: String, CodingKey {
        case backends, active
        case createEnabled = "create_enabled"
    }
}

struct ProjectEmbeddingBackend: Decodable, Identifiable, Sendable {
    let name: String
    let provider: String
    let baseURL: String
    let model: String
    let active: Bool
    let ready: Bool
    let createEnabled: Bool
    let projectChunkCount: Int?
    let projectMemoryCount: Int?
    var id: String { name }
    private enum CodingKeys: String, CodingKey {
        case name, provider, model, active, ready
        case baseURL = "base_url"
        case createEnabled = "create_enabled"
        case projectChunkCount = "project_chunk_count"
        case projectMemoryCount = "project_memory_count"
    }
}

struct ProjectSkillVersion: Decodable, Identifiable, Sendable {
    let name: String
    let projectPath: String
    let templatePath: String?
    let projectVersion: String?
    let templateVersion: String?
    let status: String
    let action: String
    let detail: String?
    var id: String { name }
    private enum CodingKeys: String, CodingKey {
        case name, status, action, detail
        case projectPath = "project_path"
        case templatePath = "template_path"
        case projectVersion = "project_version"
        case templateVersion = "template_version"
    }
}

struct ProjectSkillInventory: Decodable, Sendable {
    let projectRoot: String
    let projectSkillRoot: String
    let templateRoot: String?
    let bundleVersion: String
    let status: String
    let summary: String
    let filter: String
    let skills: [ProjectSkillVersion]
    private enum CodingKeys: String, CodingKey {
        case status, summary, filter, skills
        case projectRoot = "project_root"
        case projectSkillRoot = "project_skill_root"
        case templateRoot = "template_root"
        case bundleVersion = "bundle_version"
    }
}

struct ProjectSkillContentResponse: Decodable, Sendable {
    let skill: ProjectSkillVersion
    let content: String?
    let contentTruncated: Bool
    private enum CodingKeys: String, CodingKey {
        case skill, content
        case contentTruncated = "content_truncated"
    }
}

struct ProjectSkillRepairResponse: Decodable, Sendable {
    let dryRun: Bool
    let force: Bool
    let backupRoot: String?
    let inventory: ProjectSkillInventory
    private enum CodingKeys: String, CodingKey {
        case force, inventory
        case dryRun = "dry_run"
        case backupRoot = "backup_root"
    }
}

struct ProjectReindexResponse: Decodable, Sendable {
    let reindexedEntries: Int
    let dryRun: Bool
    private enum CodingKeys: String, CodingKey { case dryRun = "dry_run"; case reindexedEntries = "reindexed_entries" }
}

struct ProjectReembedResponse: Decodable, Sendable {
    let reembeddedChunks: Int
    let dryRun: Bool
    private enum CodingKeys: String, CodingKey { case dryRun = "dry_run"; case reembeddedChunks = "reembedded_chunks" }
}

struct ProjectCurateResponse: Decodable, Sendable {
    let inputCount: Int
    let outputCount: Int
    let replacedCount: Int
    let proposalCount: Int
    let dryRun: Bool
    private enum CodingKeys: String, CodingKey {
        case dryRun = "dry_run"
        case inputCount = "input_count"
        case outputCount = "output_count"
        case replacedCount = "replaced_count"
        case proposalCount = "proposal_count"
    }
}

struct ProjectArchiveResponse: Decodable, Sendable {
    let archivedCount: Int
    let dryRun: Bool
    private enum CodingKeys: String, CodingKey { case dryRun = "dry_run"; case archivedCount = "archived_count" }
}

enum ProjectSkillFilter: String, CaseIterable, Identifiable, Sendable {
    case codifix = "codifix"
    case all
    var id: String { rawValue }
    var label: String { self == .codifix ? "Codifix" : "All skills" }
}

enum ProjectAction: String, Identifiable, Sendable {
    case curate, reindex, reembed, archive
    var id: String { rawValue }
    var title: String {
        switch self {
        case .curate: "Curate"
        case .reindex: "Reindex"
        case .reembed: "Re-embed"
        case .archive: "Archive low-value memories"
        }
    }
}
