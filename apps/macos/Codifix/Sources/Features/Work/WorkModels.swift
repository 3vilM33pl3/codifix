import Foundation

struct WorkAgentSnapshot: Decodable, Sendable {
    let collectedAt: Date
    let sessions: [WorkAgentSession]
    let orphanPorts: [WorkOrphanPort]
    let rateLimits: [WorkRateLimit]

    private enum CodingKeys: String, CodingKey {
        case collectedAt = "collected_at"
        case sessions
        case orphanPorts = "orphan_ports"
        case rateLimits = "rate_limits"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        collectedAt = try container.decode(Date.self, forKey: .collectedAt)
        sessions = try container.decode([WorkAgentSession].self, forKey: .sessions)
        orphanPorts = try container.decode([WorkOrphanPort].self, forKey: .orphanPorts)
        rateLimits = try container.decode([WorkRateLimit].self, forKey: .rateLimits)
    }
}

struct WorkAgentSession: Decodable, Identifiable, Sendable, Hashable {
    let sessionID: String
    let agentCLI: String?
    let pid: Int?
    let cwd: String?
    let projectName: String?
    let startedAt: TimeInterval?
    let status: String?
    let model: String?
    let contextPercent: Double?
    let totalInputTokens: Int?
    let totalOutputTokens: Int?
    let totalCacheRead: Int?
    let totalCacheCreate: Int?
    let turnCount: Int?
    let currentTasks: [String]
    let memoryMB: Double?
    let version: String?
    let gitBranch: String?
    let gitAdded: Int?
    let gitModified: Int?
    let tokenHistory: [Int]
    let subagents: [WorkSubagent]
    let memFileCount: Int?
    let memLineCount: Int?
    let children: [WorkChildProcess]
    let initialPrompt: String?
    let firstAssistantText: String?

    var id: String { sessionID }
    var totalTokens: Int { (totalInputTokens ?? 0) + (totalOutputTokens ?? 0) }

    private enum CodingKeys: String, CodingKey {
        case sessionID = "session_id", agentCLI = "agent_cli", pid, cwd, status, model, version, subagents, children
        case projectName = "project_name", startedAt = "started_at", contextPercent = "context_percent"
        case totalInputTokens = "total_input_tokens", totalOutputTokens = "total_output_tokens"
        case totalCacheRead = "total_cache_read", totalCacheCreate = "total_cache_create"
        case turnCount = "turn_count", currentTasks = "current_tasks", memoryMB = "mem_mb"
        case gitBranch = "git_branch", gitAdded = "git_added", gitModified = "git_modified"
        case tokenHistory = "token_history", memFileCount = "mem_file_count", memLineCount = "mem_line_count"
        case initialPrompt = "initial_prompt", firstAssistantText = "first_assistant_text"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        sessionID = try c.decode(String.self, forKey: .sessionID)
        agentCLI = try c.decodeIfPresent(String.self, forKey: .agentCLI)
        pid = try c.decodeIfPresent(Int.self, forKey: .pid)
        cwd = try c.decodeIfPresent(String.self, forKey: .cwd)
        projectName = try c.decodeIfPresent(String.self, forKey: .projectName)
        startedAt = try c.decodeIfPresent(TimeInterval.self, forKey: .startedAt)
        status = try c.decodeIfPresent(String.self, forKey: .status)
        model = try c.decodeIfPresent(String.self, forKey: .model)
        contextPercent = try c.decodeIfPresent(Double.self, forKey: .contextPercent)
        totalInputTokens = try c.decodeIfPresent(Int.self, forKey: .totalInputTokens)
        totalOutputTokens = try c.decodeIfPresent(Int.self, forKey: .totalOutputTokens)
        totalCacheRead = try c.decodeIfPresent(Int.self, forKey: .totalCacheRead)
        totalCacheCreate = try c.decodeIfPresent(Int.self, forKey: .totalCacheCreate)
        turnCount = try c.decodeIfPresent(Int.self, forKey: .turnCount)
        currentTasks = try c.decodeIfPresent([String].self, forKey: .currentTasks) ?? []
        memoryMB = try c.decodeIfPresent(Double.self, forKey: .memoryMB)
        version = try c.decodeIfPresent(String.self, forKey: .version)
        gitBranch = try c.decodeIfPresent(String.self, forKey: .gitBranch)
        gitAdded = try c.decodeIfPresent(Int.self, forKey: .gitAdded)
        gitModified = try c.decodeIfPresent(Int.self, forKey: .gitModified)
        tokenHistory = try c.decodeIfPresent([Int].self, forKey: .tokenHistory) ?? []
        subagents = try c.decodeIfPresent([WorkSubagent].self, forKey: .subagents) ?? []
        memFileCount = try c.decodeIfPresent(Int.self, forKey: .memFileCount)
        memLineCount = try c.decodeIfPresent(Int.self, forKey: .memLineCount)
        children = try c.decodeIfPresent([WorkChildProcess].self, forKey: .children) ?? []
        initialPrompt = try c.decodeIfPresent(String.self, forKey: .initialPrompt)
        firstAssistantText = try c.decodeIfPresent(String.self, forKey: .firstAssistantText)
    }
}

struct WorkSubagent: Decodable, Identifiable, Sendable, Hashable {
    let name: String
    let status: String
    let tokens: Int
    var id: String { name }
}

struct WorkChildProcess: Decodable, Identifiable, Sendable, Hashable {
    let pid: Int
    let command: String
    let memoryKB: Int?
    let port: Int?
    var id: Int { pid }

    private enum CodingKeys: String, CodingKey {
        case pid, command, port
        case memoryKB = "mem_kb"
    }
}

struct WorkOrphanPort: Decodable, Identifiable, Sendable, Hashable {
    let port: Int
    let pid: Int?
    let command: String?
    let projectName: String?
    var id: Int { port }

    private enum CodingKeys: String, CodingKey {
        case port, pid, command
        case projectName = "project_name"
    }
}

struct WorkRateLimit: Decodable, Identifiable, Sendable, Hashable {
    let source: String
    let fiveHourPercent: Double?
    let fiveHourResetsAt: TimeInterval?
    let sevenDayPercent: Double?
    let sevenDayResetsAt: TimeInterval?
    let updatedAt: TimeInterval?
    var id: String { source }

    private enum CodingKeys: String, CodingKey {
        case source
        case fiveHourPercent = "five_hour_pct"
        case fiveHourResetsAt = "five_hour_resets_at"
        case sevenDayPercent = "seven_day_pct"
        case sevenDayResetsAt = "seven_day_resets_at"
        case updatedAt = "updated_at"
    }
}

struct WorkActivityList: Decodable, Sendable {
    let project: String?
    let totalReturned: Int?
    let items: [WorkActivityEvent]

    private enum CodingKeys: String, CodingKey { case project, items; case totalReturned = "total_returned" }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        project = try c.decodeIfPresent(String.self, forKey: .project)
        totalReturned = try c.decodeIfPresent(Int.self, forKey: .totalReturned)
        items = try c.decode([WorkActivityEvent].self, forKey: .items)
    }
}

struct WorkActivityEvent: Decodable, Identifiable, Sendable, Hashable {
    let id: String
    let sequence: Int?
    let project: String?
    let kind: String
    let memoryID: String?
    let summary: String
    let details: WorkJSONValue?
    let actorName: String?
    let source: String?
    let operationID: String?
    let durationMS: Int?
    let provider: String?
    let model: String?
    let tokenUsage: WorkTokenUsage?
    let recordedAt: String?

    private enum CodingKeys: String, CodingKey {
        case id, project, kind, summary, details, source, provider, model
        case sequence = "seq", memoryID = "memory_id", actorName = "actor_name"
        case operationID = "operation_id", durationMS = "duration_ms", tokenUsage = "token_usage"
        case recordedAt = "recorded_at"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        sequence = try c.decodeIfPresent(Int.self, forKey: .sequence)
        project = try c.decodeIfPresent(String.self, forKey: .project)
        kind = try c.decode(String.self, forKey: .kind)
        memoryID = try c.decodeIfPresent(String.self, forKey: .memoryID)
        summary = try c.decode(String.self, forKey: .summary)
        details = try c.decodeIfPresent(WorkJSONValue.self, forKey: .details)
        actorName = try c.decodeIfPresent(String.self, forKey: .actorName)
        source = try c.decodeIfPresent(String.self, forKey: .source)
        operationID = try c.decodeIfPresent(String.self, forKey: .operationID)
        durationMS = try c.decodeIfPresent(Int.self, forKey: .durationMS)
        provider = try c.decodeIfPresent(String.self, forKey: .provider)
        model = try c.decodeIfPresent(String.self, forKey: .model)
        tokenUsage = try c.decodeIfPresent(WorkTokenUsage.self, forKey: .tokenUsage)
        recordedAt = try c.decode(String.self, forKey: .recordedAt)
    }
}

struct WorkTokenUsage: Decodable, Sendable, Hashable {
    let inputTokens: Int?
    let outputTokens: Int?
    let totalTokens: Int?
    private enum CodingKeys: String, CodingKey {
        case inputTokens = "input_tokens", outputTokens = "output_tokens", totalTokens = "total_tokens"
    }
}

indirect enum WorkJSONValue: Decodable, Sendable, Hashable {
    case object([String: WorkJSONValue])
    case array([WorkJSONValue])
    case string(String)
    case number(Double)
    case bool(Bool)
    case null

    init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() { self = .null }
        else if let value = try? c.decode(Bool.self) { self = .bool(value) }
        else if let value = try? c.decode(Double.self) { self = .number(value) }
        else if let value = try? c.decode(String.self) { self = .string(value) }
        else if let value = try? c.decode([WorkJSONValue].self) { self = .array(value) }
        else { self = .object(try c.decode([String: WorkJSONValue].self)) }
    }

    var displayText: String {
        switch self {
        case let .string(value): Self.quoted(value)
        case .null: "null"
        case let .bool(value): String(value)
        case let .number(value): String(value)
        case let .array(values): "[\n" + values.map { "  " + $0.displayText }.joined(separator: ",\n") + "\n]"
        case let .object(values):
            "{\n" + values.keys.sorted().map { key in
                "  \(Self.quoted(key)): \(values[key]?.displayText ?? "null")"
            }.joined(separator: ",\n") + "\n}"
        }
    }

    private static func quoted(_ value: String) -> String {
        let data = try? JSONSerialization.data(withJSONObject: [value], options: [.fragmentsAllowed])
        let encoded = data.flatMap { String(data: $0, encoding: .utf8) } ?? "[\(value)]"
        return String(encoded.dropFirst().dropLast())
    }
}

struct WorkBriefingRequest: Encodable, Sendable {
    let project: String
    let includeLLMSummary: Bool
    let limit: Int
    private enum CodingKeys: String, CodingKey {
        case project, limit
        case includeLLMSummary = "include_llm_summary"
    }
}

struct WorkTokenUsageSummary: Decodable, Sendable {
    let actionCount: Int?
    let totalInputTokens: Int?
    let totalOutputTokens: Int?
    let totalCacheReadTokens: Int?
    let totalCacheWriteTokens: Int?
    let totalTokens: Int?
    private enum CodingKeys: String, CodingKey {
        case actionCount = "action_count", totalInputTokens = "total_input_tokens"
        case totalOutputTokens = "total_output_tokens", totalCacheReadTokens = "total_cache_read_tokens"
        case totalCacheWriteTokens = "total_cache_write_tokens", totalTokens = "total_tokens"
    }
}

struct WorkBriefingResponse: Decodable, Sendable {
    let project: String?
    let generatedAt: String?
    let briefing: String
    let currentFocus: [String]
    let recentWork: [String]
    let blockers: [String]
    let nextActions: [WorkResumeAction]
    let usefulMemories: [WorkMemoryReference]
    let recentActivities: [WorkActivityEvent]
    let tokenUsage: WorkTokenUsageSummary
    let warnings: [String]

    private enum CodingKeys: String, CodingKey {
        case project, briefing, blockers, warnings
        case generatedAt = "generated_at", currentFocus = "current_focus", recentWork = "recent_work"
        case nextActions = "next_actions", usefulMemories = "useful_memories", recentActivities = "recent_activities"
        case tokenUsage = "token_usage"
    }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        project = try c.decodeIfPresent(String.self, forKey: .project)
        generatedAt = try c.decodeIfPresent(String.self, forKey: .generatedAt)
        briefing = try c.decode(String.self, forKey: .briefing)
        currentFocus = try c.decodeIfPresent([String].self, forKey: .currentFocus) ?? []
        recentWork = try c.decodeIfPresent([String].self, forKey: .recentWork) ?? []
        blockers = try c.decodeIfPresent([String].self, forKey: .blockers) ?? []
        nextActions = try c.decodeIfPresent([WorkResumeAction].self, forKey: .nextActions) ?? []
        usefulMemories = try c.decodeIfPresent([WorkMemoryReference].self, forKey: .usefulMemories) ?? []
        recentActivities = try c.decodeIfPresent([WorkActivityEvent].self, forKey: .recentActivities) ?? []
        tokenUsage = try c.decode(WorkTokenUsageSummary.self, forKey: .tokenUsage)
        warnings = try c.decodeIfPresent([String].self, forKey: .warnings) ?? []
    }
}

struct WorkLlmAuditStatus: Decodable, Sendable {
    let enabled: Bool
    let redacted: Bool
    let profile: String?
    let maxMessageChars: Int?
    let maxTotalChars: Int?
    private enum CodingKeys: String, CodingKey {
        case enabled, redacted, profile
        case maxMessageChars = "max_message_chars", maxTotalChars = "max_total_chars"
    }
}

struct WorkLlmAuditRequest: Encodable, Sendable { let enabled: Bool }

struct WorkResumeRequest: Encodable, Sendable {
    let project: String
    let repositoryRoot: String?
    let includeLLMSummary: Bool
    let limit: Int
    private enum CodingKeys: String, CodingKey {
        case project, limit
        case repositoryRoot = "repo_root"
        case includeLLMSummary = "include_llm_summary"
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(project, forKey: .project)
        try c.encode(repositoryRoot, forKey: .repositoryRoot)
        try c.encode(includeLLMSummary, forKey: .includeLLMSummary)
        try c.encode(limit, forKey: .limit)
    }
}

struct WorkResumeCheckpoint: Decodable, Sendable {
    let project: String?
    let repositoryRoot: String?
    let markedAt: String?
    let note: String?
    let gitBranch: String?
    let gitHead: String?
    private enum CodingKeys: String, CodingKey {
        case project, note
        case repositoryRoot = "repo_root", markedAt = "marked_at", gitBranch = "git_branch", gitHead = "git_head"
    }
}

struct WorkResumeAction: Decodable, Identifiable, Sendable, Hashable {
    let title: String
    let rationale: String?
    let commandHint: String?
    var id: String { title }
    private enum CodingKeys: String, CodingKey { case title, rationale; case commandHint = "command_hint" }
}

struct WorkMemoryReference: Decodable, Identifiable, Sendable, Hashable {
    let id: String
    let summary: String
    let preview: String?
    let memoryType: String?
    let status: String?
    private enum CodingKeys: String, CodingKey { case id, summary, preview, status; case memoryType = "memory_type" }
}

struct WorkResumeResponse: Decodable, Sendable {
    let project: String?
    let generatedAt: String?
    let checkpoint: WorkResumeCheckpoint?
    let briefing: String
    let currentThread: String?
    let changeSummary: [String]
    let attentionItems: [String]
    let primaryNextStep: WorkResumeAction?
    let secondaryNextSteps: [WorkResumeAction]
    let contextItems: [WorkMemoryReference]
    let timeline: [WorkActivityEvent]
    let commits: [WorkCommit]
    let changedMemories: [WorkMemoryReference]
    let durableContext: [WorkMemoryReference]
    let warnings: [String]
    let actions: [WorkResumeAction]

    var memories: [WorkMemoryReference] {
        var seen = Set<String>()
        return (contextItems + changedMemories + durableContext).filter { seen.insert($0.id).inserted }
    }
    var nextSteps: [WorkResumeAction] {
        var seen = Set<String>()
        return ([primaryNextStep].compactMap { $0 } + secondaryNextSteps + actions).filter {
            seen.insert("\($0.title)\n\($0.rationale ?? "")").inserted
        }
    }

    private enum CodingKeys: String, CodingKey {
        case project, checkpoint, briefing, timeline, warnings, actions
        case generatedAt = "generated_at", currentThread = "current_thread", changeSummary = "change_summary"
        case attentionItems = "attention_items", primaryNextStep = "primary_next_step"
        case secondaryNextSteps = "secondary_next_steps", contextItems = "context_items"
        case changedMemories = "changed_memories", durableContext = "durable_context", commits
    }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        project = try c.decodeIfPresent(String.self, forKey: .project)
        generatedAt = try c.decodeIfPresent(String.self, forKey: .generatedAt)
        checkpoint = try c.decodeIfPresent(WorkResumeCheckpoint.self, forKey: .checkpoint)
        briefing = try c.decode(String.self, forKey: .briefing)
        currentThread = try c.decodeIfPresent(String.self, forKey: .currentThread)
        changeSummary = try c.decodeIfPresent([String].self, forKey: .changeSummary) ?? []
        attentionItems = try c.decodeIfPresent([String].self, forKey: .attentionItems) ?? []
        primaryNextStep = try c.decodeIfPresent(WorkResumeAction.self, forKey: .primaryNextStep)
        secondaryNextSteps = try c.decodeIfPresent([WorkResumeAction].self, forKey: .secondaryNextSteps) ?? []
        contextItems = try c.decodeIfPresent([WorkMemoryReference].self, forKey: .contextItems) ?? []
        timeline = try c.decodeIfPresent([WorkActivityEvent].self, forKey: .timeline) ?? []
        commits = try c.decodeIfPresent([WorkCommit].self, forKey: .commits) ?? []
        changedMemories = try c.decodeIfPresent([WorkMemoryReference].self, forKey: .changedMemories) ?? []
        durableContext = try c.decodeIfPresent([WorkMemoryReference].self, forKey: .durableContext) ?? []
        warnings = try c.decodeIfPresent([String].self, forKey: .warnings) ?? []
        actions = try c.decodeIfPresent([WorkResumeAction].self, forKey: .actions) ?? []
    }
}

struct WorkCommit: Decodable, Identifiable, Sendable, Hashable {
    let hash: String
    let shortHash: String
    let subject: String
    let committedAt: String
    var id: String { hash }
    private enum CodingKeys: String, CodingKey {
        case hash, subject
        case shortHash = "short_hash", committedAt = "committed_at"
    }
}

struct WorkRequestTicket: Hashable, Sendable {
    let id: UInt64
    let revision: Int
    let resourceID: String
}

struct WorkRequestGate: Sendable {
    private(set) var generation: UInt64 = 0
    mutating func begin(revision: Int, resourceID: String) -> WorkRequestTicket {
        generation &+= 1
        return WorkRequestTicket(id: generation, revision: revision, resourceID: resourceID)
    }
    func accepts(_ ticket: WorkRequestTicket, revision: Int, resourceID: String) -> Bool {
        ticket.id == generation && ticket.revision == revision && ticket.resourceID == resourceID
    }
}

extension JSONEncoder {
    static var workJSON: JSONEncoder { JSONEncoder() }
}
