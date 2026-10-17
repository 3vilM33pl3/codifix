import Foundation

enum LoopMode: String, Codable, CaseIterable, Sendable, Identifiable {
    case off, observe, suggestOnly = "suggest_only", draftOutput = "draft_output"
    case autonomousSafe = "autonomous_safe", paused, snoozed
    var id: String { rawValue }
    var label: String { rawValue.replacingOccurrences(of: "_", with: " ").capitalized }
    static var configurable: [LoopMode] { [.observe, .suggestOnly, .draftOutput, .autonomousSafe] }
    static func configurationValue(effective: LoopMode?, default defaultMode: LoopMode) -> LoopMode {
        if let effective, configurable.contains(effective) { return effective }
        if configurable.contains(defaultMode) { return defaultMode }
        return .observe
    }
}

enum LoopScope: String, Codable, Sendable { case user, workspace, project, repo }

struct AutomationJSON: Codable, Sendable, Equatable {
    var value: Value
    enum Value: Codable, Sendable, Equatable {
        case object([String: AutomationJSON]), array([AutomationJSON]), string(String), number(Double), bool(Bool), null
        init(from decoder: Decoder) throws {
            let c = try decoder.singleValueContainer()
            if c.decodeNil() { self = .null }
            else if let x = try? c.decode(Bool.self) { self = .bool(x) }
            else if let x = try? c.decode(Double.self) { self = .number(x) }
            else if let x = try? c.decode(String.self) { self = .string(x) }
            else if let x = try? c.decode([AutomationJSON].self) { self = .array(x) }
            else { self = .object(try c.decode([String: AutomationJSON].self)) }
        }
        func encode(to encoder: Encoder) throws {
            var c = encoder.singleValueContainer()
            switch self {
            case let .object(x): try c.encode(x)
            case let .array(x): try c.encode(x)
            case let .string(x): try c.encode(x)
            case let .number(x): try c.encode(x)
            case let .bool(x): try c.encode(x)
            case .null: try c.encodeNil()
            }
        }
    }
    init(from decoder: Decoder) throws { value = try Value(from: decoder) }
    func encode(to encoder: Encoder) throws { try value.encode(to: encoder) }
    init(value: Value) { self.value = value }
    static func object(_ value: [String: AutomationJSON] = [:]) -> AutomationJSON { .init(value: .object(value)) }
    func prettyPrinted() -> String {
        let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        guard let data = try? encoder.encode(self), let text = String(data: data, encoding: .utf8) else { return "{}" }
        return text
    }
}

struct LoopDefinition: Decodable, Sendable, Identifiable {
    let id: String
    let loopID: String
    let version: Int
    let name: String
    let description: String
    let riskLevel: String
    let defaultMode: LoopMode
    let triggerSpec: [String: AutomationJSON]
    let contextSpec: [String: AutomationJSON]
    let policySpec: [String: AutomationJSON]
    let outputSpec: [String: AutomationJSON]
    let createdAt: Date
    var ident: String { loopID }
    private enum CodingKeys: String, CodingKey {
        case id, version, name, description
        case loopID = "loop_id", riskLevel = "risk_level", defaultMode = "default_mode"
        case triggerSpec = "trigger_spec", contextSpec = "context_spec", policySpec = "policy_spec", outputSpec = "output_spec", createdAt = "created_at"
    }
}

struct LoopDefinitions: Decodable, Sendable { let definitions: [LoopDefinition] }

struct EffectiveLoopSettings: Decodable, Sendable {
    let loopID: String
    let enabled: Bool
    let mode: LoopMode
    let scopeType: LoopScope
    let scopeID: String
    let globalKillSwitch: Bool
    let blockedReasons: [String]
    let budgets: [String: AutomationJSON]?
    let approvalOverrides: [String: AutomationJSON]?
    let pausedUntil: Date?
    let snoozedUntil: Date?
    private enum CodingKeys: String, CodingKey {
        case enabled, mode, budgets
        case loopID = "loop_id", scopeType = "scope_type", scopeID = "scope_id", globalKillSwitch = "global_kill_switch"
        case blockedReasons = "blocked_reasons", approvalOverrides = "approval_overrides", pausedUntil = "paused_until", snoozedUntil = "snoozed_until"
    }
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        loopID = try c.decode(String.self, forKey: .loopID); enabled = try c.decode(Bool.self, forKey: .enabled)
        mode = try c.decode(LoopMode.self, forKey: .mode); scopeType = try c.decode(LoopScope.self, forKey: .scopeType)
        scopeID = try c.decode(String.self, forKey: .scopeID); globalKillSwitch = try c.decodeIfPresent(Bool.self, forKey: .globalKillSwitch) ?? false
        blockedReasons = try c.decodeIfPresent([String].self, forKey: .blockedReasons) ?? []
        budgets = try c.decodeIfPresent([String: AutomationJSON].self, forKey: .budgets)
        approvalOverrides = try c.decodeIfPresent([String: AutomationJSON].self, forKey: .approvalOverrides)
        pausedUntil = try c.decodeIfPresent(Date.self, forKey: .pausedUntil); snoozedUntil = try c.decodeIfPresent(Date.self, forKey: .snoozedUntil)
    }
}

struct LoopDefinitionDetail: Decodable, Sendable { let definition: LoopDefinition; let effectiveSettings: EffectiveLoopSettings?; private enum CodingKeys: String, CodingKey { case definition; case effectiveSettings = "effective_settings" } }

struct LoopSettingRequest: Encodable, Sendable {
    let action: String
    let scopeType: LoopScope
    let scopeID: String
    let project: String
    let repoRoot: String?
    let mode: LoopMode?
    let pausedUntil: Date?
    let snoozedUntil: Date?
    let updatedBy: String
    let reason: String
    let explicitUserApproval: Bool
    private enum CodingKeys: String, CodingKey {
        case action, project, mode, reason
        case scopeType = "scope_type", scopeID = "scope_id", repoRoot = "repo_root"
        case pausedUntil = "paused_until", snoozedUntil = "snoozed_until", updatedBy = "updated_by", explicitUserApproval = "explicit_user_approval"
    }
}

struct LoopSettingRecord: Decodable, Sendable { let id: String; let updatedAt: Date; private enum CodingKeys: String, CodingKey { case id; case updatedAt = "updated_at" } }
struct LoopSettingResponse: Decodable, Sendable { let setting: LoopSettingRecord; let effectiveSettings: EffectiveLoopSettings; private enum CodingKeys: String, CodingKey { case setting; case effectiveSettings = "effective_settings" } }

struct LoopGlobalState: Decodable, Sendable { let killSwitchEnabled: Bool; let reason: String?; private enum CodingKeys: String, CodingKey { case reason; case killSwitchEnabled = "kill_switch_enabled" } }
struct LoopGlobalRequest: Encodable, Sendable { let killSwitchEnabled: Bool; let updatedBy: String; let reason: String; private enum CodingKeys: String, CodingKey { case reason; case killSwitchEnabled = "kill_switch_enabled"; case updatedBy = "updated_by" } }

struct LoopRunRequest: Encodable, Sendable {
    let project: String; let repoRoot: String?; let scopeType: LoopScope; let scopeID: String
    let dryRun: Bool; let reason: String; let triggerPayload: [String: String]
    private enum CodingKeys: String, CodingKey { case project, reason; case repoRoot = "repo_root"; case scopeType = "scope_type"; case scopeID = "scope_id"; case dryRun = "dry_run"; case triggerPayload = "trigger_payload" }
}

struct LoopRunSummary: Decodable, Sendable, Identifiable {
    let id: String; let loopID: String; let definitionVersion: Int; let project: String?; let repoRoot: String?
    let mode: LoopMode; let status: String; let startedAt: Date; let finishedAt: Date?; let outputSummary: String?
    let traceCount: Int; let blockedReasons: [String]
    private enum CodingKeys: String, CodingKey { case id, mode, status, project; case loopID = "loop_id"; case definitionVersion = "definition_version"; case repoRoot = "repo_root"; case startedAt = "started_at"; case finishedAt = "finished_at"; case outputSummary = "output_summary"; case traceCount = "trace_count"; case blockedReasons = "blocked_reasons" }
}
struct LoopRuns: Decodable, Sendable { let runs: [LoopRunSummary] }
struct LoopTrace: Decodable, Sendable, Identifiable { let id: String; let traceType: String; let title: String; let payload: AutomationJSON; let createdAt: Date; private enum CodingKeys: String, CodingKey { case id, title, payload; case traceType = "trace_type"; case createdAt = "created_at" } }
struct LoopRunDetail: Decodable, Sendable { let summary: LoopRunSummary; let runReason: String?; let effectiveSettings: AutomationJSON; let policyDecisions: AutomationJSON; let cost: AutomationJSON; let output: AutomationJSON; let traces: [LoopTrace]; let memoryProposals: [LoopMemoryProposal]; private enum CodingKeys: String, CodingKey { case summary, cost, output, traces; case runReason = "run_reason"; case effectiveSettings = "effective_settings"; case policyDecisions = "policy_decisions"; case memoryProposals = "memory_proposals" } }
struct LoopRunResponse: Decodable, Sendable { let run: LoopRunDetail }

struct LoopApproval: Decodable, Sendable, Identifiable {
    let id: String; let runID: String?; let project: String?; let loopID: String; let actionType: String; let proposedAction: [String: AutomationJSON]
    let riskReason: String; let status: String; let createdAt: Date; let decisionReason: String?
    private enum CodingKeys: String, CodingKey { case id, project, status; case runID = "run_id"; case loopID = "loop_id"; case actionType = "action_type"; case proposedAction = "proposed_action"; case riskReason = "risk_reason"; case createdAt = "created_at"; case decisionReason = "decision_reason" }
}
struct LoopApprovals: Decodable, Sendable { let approvals: [LoopApproval] }
struct LoopApprovalDecision: Encodable, Sendable { let reviewer: String; let reason: String; let editedAction: AutomationJSON?; private enum CodingKeys: String, CodingKey { case reviewer, reason; case editedAction = "edited_action" } }
struct LoopApprovalResponse: Decodable, Sendable { let approval: LoopApproval }

struct LoopMemoryProposal: Decodable, Sendable, Identifiable {
    let id: String; let runID: String?; let project: String?; let loopID: String; let proposalType: String; let candidate: [String: AutomationJSON]; let evidence: AutomationJSON; let confidence: Double; let riskNotes: String?; let status: String; let createdAt: Date
    private enum CodingKeys: String, CodingKey { case id, project, candidate, evidence, confidence, status; case runID = "run_id"; case loopID = "loop_id"; case proposalType = "proposal_type"; case riskNotes = "risk_notes"; case createdAt = "created_at" }
}
struct LoopMemoryProposals: Decodable, Sendable { let proposals: [LoopMemoryProposal] }
struct LoopMemoryProposalDecision: Encodable, Sendable { let reviewer: String; let reason: String; let editedCandidate: AutomationJSON?; private enum CodingKeys: String, CodingKey { case reviewer, reason; case editedCandidate = "edited_candidate" } }
struct LoopMemoryProposalResponse: Decodable, Sendable { let proposal: LoopMemoryProposal }

struct ExportOptions: Codable, Sendable, Equatable {
    var includeArchived = false; var includeTags = true; var includeRelations = true
    var includeSourceFilePaths = false; var includeGitCommits = false; var includeSourceExcerpts = false
    private enum CodingKeys: String, CodingKey { case includeArchived = "include_archived", includeTags = "include_tags", includeRelations = "include_relations"; case includeSourceFilePaths = "include_source_file_paths"; case includeGitCommits = "include_git_commits"; case includeSourceExcerpts = "include_source_excerpts" }
}
struct BundleExportPreview: Decodable, Sendable { let bundleID: String; let sourceProject: String; let exportedAt: Date; let summaryMarkdown: String; let memoryCount: Int; let relationCount: Int; let warningCount: Int; let warnings: [String]; let options: ExportOptions; private enum CodingKeys: String, CodingKey { case options, warnings; case bundleID = "bundle_id"; case sourceProject = "source_project"; case exportedAt = "exported_at"; case summaryMarkdown = "summary_markdown"; case memoryCount = "memory_count"; case relationCount = "relation_count"; case warningCount = "warning_count" } }
struct BundleImportPreview: Decodable, Sendable { let bundleID: String; let bundleHash: String; let sourceProject: String; let targetProject: String; let exportedAt: Date; let summaryMarkdown: String; let memoryCount: Int; let relationCount: Int; let newCount: Int; let unchangedCount: Int; let replacingCount: Int; let warningCount: Int; let warnings: [String]; let options: ExportOptions; private enum CodingKeys: String, CodingKey { case warnings, options; case bundleID = "bundle_id"; case bundleHash = "bundle_hash"; case sourceProject = "source_project"; case targetProject = "target_project"; case exportedAt = "exported_at"; case summaryMarkdown = "summary_markdown"; case memoryCount = "memory_count"; case relationCount = "relation_count"; case newCount = "new_count"; case unchangedCount = "unchanged_count"; case replacingCount = "replacing_count"; case warningCount = "warning_count" } }
struct BundleImportResponse: Decodable, Sendable { let targetProject: String; let bundleID: String; let importedCount: Int; let replacedCount: Int; let skippedCount: Int; let relationCount: Int; private enum CodingKeys: String, CodingKey { case targetProject = "target_project"; case bundleID = "bundle_id"; case importedCount = "imported_count"; case replacedCount = "replaced_count"; case skippedCount = "skipped_count"; case relationCount = "relation_count" } }

struct ExportPreviewSource: Equatable, Sendable {
    let options: ExportOptions
    let project: String
    let revision: Int
    func matches(options: ExportOptions, project: String, revision: Int) -> Bool {
        self.options == options && self.project == project && self.revision == revision
    }
}

struct ImportPreviewSource: Equatable, Sendable {
    let bytes: Data
    let project: String
    let revision: Int
    func matches(bytes: Data?, project: String, revision: Int) -> Bool {
        self.bytes == bytes && self.project == project && self.revision == revision
    }
}
