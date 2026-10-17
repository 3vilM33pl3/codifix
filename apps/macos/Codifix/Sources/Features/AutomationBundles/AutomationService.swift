import Foundation

struct AutomationService: Sendable {
    let context: FeatureContext
    private var project: String { context.profile.projectSlug }
    private var projectPath: String { URLPathSegment.escape(project) }

    func definitions() async throws -> [LoopDefinition] {
        try requireRead()
        let result: LoopDefinitions = try await context.client.get(APIRoute(path: "/v1/loops"), context: context.requestContext)
        return result.definitions
    }

    func definition(_ loopID: String) async throws -> LoopDefinitionDetail {
        try requireRead()
        var query = [URLQueryItem(name: "project", value: project)]
        if let root = context.profile.repositoryRoot?.path { query.append(URLQueryItem(name: "repo_root", value: root)) }
        return try await context.client.get(APIRoute(path: "/v1/loops/\(URLPathSegment.escape(loopID))", query: query), context: context.requestContext)
    }

    func recentRuns(loopID: String, limit: Int = 5) async throws -> [LoopRunSummary] {
        try requireRead()
        let response: LoopRuns = try await context.client.get(APIRoute(path: "/v1/loops/runs", query: [
            URLQueryItem(name: "project", value: project), URLQueryItem(name: "loop_id", value: loopID), URLQueryItem(name: "limit", value: String(limit))
        ]), context: context.requestContext)
        return response.runs
    }

    func run(id: String) async throws -> LoopRunDetail {
        try requireRead()
        let response: LoopRunResponse = try await context.client.get(APIRoute(path: "/v1/loops/runs/\(URLPathSegment.escape(id))"), context: context.requestContext)
        return response.run
    }

    func pendingApprovals(runID: String? = nil) async throws -> [LoopApproval] {
        try requireRead()
        var query = [URLQueryItem(name: "project", value: project), URLQueryItem(name: "status", value: "pending"), URLQueryItem(name: "limit", value: "50")]
        if let runID { query.append(URLQueryItem(name: "run_id", value: runID)) }
        let response: LoopApprovals = try await context.client.get(APIRoute(path: "/v1/loops/approvals", query: query), context: context.requestContext)
        return response.approvals
    }

    func pendingMemoryProposals(runID: String? = nil) async throws -> [LoopMemoryProposal] {
        try requireRead()
        var query = [URLQueryItem(name: "project", value: project), URLQueryItem(name: "status", value: "pending"), URLQueryItem(name: "limit", value: "50")]
        if let runID { query.append(URLQueryItem(name: "run_id", value: runID)) }
        let response: LoopMemoryProposals = try await context.client.get(APIRoute(path: "/v1/loops/memory-proposals", query: query), context: context.requestContext)
        return response.proposals
    }

    func globalState() async throws -> LoopGlobalState {
        try requireRead()
        return try await context.client.get(APIRoute(path: "/v1/loops/global-kill-switch"), context: context.requestContext)
    }

    func setGlobalStop(_ stopped: Bool) async throws -> LoopGlobalState {
        try requireGlobalConfigure()
        let body = try JSONEncoder.codifix.encode(LoopGlobalRequest(killSwitchEnabled: stopped, updatedBy: "macos", reason: stopped ? "Global stop requested from the macOS app." : "Global stop cleared from the macOS app."))
        return try await context.client.send(APIRoute<LoopGlobalState>(method: "POST", path: "/v1/loops/global-kill-switch", headers: ["Content-Type": "application/json"], body: body), context: context.requestContext)
    }

    func update(_ loopID: String, action: String, mode: LoopMode? = nil) async throws -> LoopSettingResponse {
        try requireProjectWrite(.loopsConfigure)
        let now = Date()
        let root = context.profile.repositoryRoot?.path
        let scope: LoopScope = root == nil ? .project : .repo
        let payload = LoopSettingRequest(action: action, scopeType: scope, scopeID: root ?? project, project: project, repoRoot: root, mode: mode,
                                         pausedUntil: action == "pause" ? now.addingTimeInterval(3600) : nil,
                                         snoozedUntil: action == "snooze" ? now.addingTimeInterval(86400) : nil,
                                         updatedBy: "macos", reason: "\(action.capitalized) from the macOS app.", explicitUserApproval: true)
        let body = try JSONEncoder.codifix.encode(payload)
        return try await context.client.send(APIRoute(method: "POST", path: "/v1/loops/\(URLPathSegment.escape(loopID))/settings", headers: ["Content-Type": "application/json"], body: body), context: context.requestContext)
    }

    func previewRun(_ loopID: String) async throws -> LoopRunDetail {
        try requireProjectWrite(.loopsRun)
        let root = context.profile.repositoryRoot?.path
        let scope: LoopScope = root == nil ? .project : .repo
        let payload = LoopRunRequest(project: project, repoRoot: root, scopeType: scope, scopeID: root ?? project,
                                     dryRun: true, reason: "Manual dry-run preview from the macOS app.", triggerPayload: ["source": "macos"])
        let body = try JSONEncoder.codifix.encode(payload)
        let response: LoopRunResponse = try await context.client.send(APIRoute(method: "POST", path: "/v1/loops/\(URLPathSegment.escape(loopID))/run", headers: ["Content-Type": "application/json"], body: body), context: context.requestContext)
        return response.run
    }

    func decide(_ approval: LoopApproval, action: String, edited: AutomationJSON? = nil) async throws -> LoopApproval {
        try requireProjectWrite(.loopsRun)
        let reason = action == "approve" ? "Approved from the macOS app." : action == "reject" ? "Rejected from the macOS app." : "Edited from the macOS app."
        let body = try JSONEncoder.codifix.encode(LoopApprovalDecision(reviewer: "macos", reason: reason, editedAction: edited))
        let response: LoopApprovalResponse = try await context.client.send(APIRoute(method: "POST", path: "/v1/loops/approvals/\(URLPathSegment.escape(approval.id))/\(action)", headers: ["Content-Type": "application/json"], body: body), context: context.requestContext)
        return response.approval
    }

    func decide(_ proposal: LoopMemoryProposal, action: String, edited: AutomationJSON? = nil) async throws -> LoopMemoryProposal {
        try requireProjectWrite(.loopsRun)
        let reason = action == "approve" ? "Approved from the macOS app." : action == "reject" ? "Rejected from the macOS app." : "Edited from the macOS app."
        let body = try JSONEncoder.codifix.encode(LoopMemoryProposalDecision(reviewer: "macos", reason: reason, editedCandidate: edited))
        let response: LoopMemoryProposalResponse = try await context.client.send(APIRoute(method: "POST", path: "/v1/loops/memory-proposals/\(URLPathSegment.escape(proposal.id))/\(action)", headers: ["Content-Type": "application/json"], body: body), context: context.requestContext)
        return response.proposal
    }

    private func requireRead() throws {
        guard context.permissions.allows(.memoryRead) else { throw AutomationServiceError.restricted("This identity cannot read automation data.") }
    }
    private func requireProjectWrite(_ permission: MemoryPermission) throws {
        guard !context.permissions.readOnly else { throw AutomationServiceError.restricted("The service is read-only.") }
        guard context.permissions.allows(permission) else { throw AutomationServiceError.restricted("This identity cannot perform this automation action in \(project).") }
    }
    private func requireGlobalConfigure() throws {
        guard !context.permissions.readOnly else { throw AutomationServiceError.restricted("The service is read-only.") }
        guard context.permissions.globalAllowed.contains(.loopsConfigure) else { throw AutomationServiceError.restricted("Global loop control requires global loops_configure permission.") }
    }
}

enum AutomationServiceError: LocalizedError {
    case restricted(String)
    var errorDescription: String? { if case let .restricted(message) = self { return message }; return nil }
}

struct BundleService: Sendable {
    static let maximumImportBytes = 2 * 1024 * 1024
    let context: FeatureContext
    private var path: String { "/v1/projects/\(URLPathSegment.escape(context.profile.projectSlug))/bundle" }

    func previewExport(_ options: ExportOptions) async throws -> BundleExportPreview {
        try require(.bundleExport, readOnlyAllowed: true)
        let body = try JSONEncoder.codifix.encode(options)
        return try await context.client.send(APIRoute(method: "POST", path: "\(path)/export/preview", headers: ["Content-Type": "application/json"], body: body), context: context.requestContext)
    }
    func export(options: ExportOptions) async throws -> Data {
        try require(.bundleExport, readOnlyAllowed: true)
        let body = try JSONEncoder.codifix.encode(options)
        let reply = try await context.client.sendData(method: "POST", path: "\(path)/export", headers: ["Content-Type": "application/json"], body: body, context: context.requestContext)
        guard !reply.data.isEmpty else { throw BundleServiceError.emptyExport }
        return reply.data
    }
    func previewImport(_ bytes: Data) async throws -> BundleImportPreview {
        try require(.bundleImport)
        return try await context.client.send(APIRoute(method: "POST", path: "\(path)/import/preview", headers: ["Content-Type": "application/octet-stream"], body: bytes), context: context.requestContext)
    }
    func importBundle(_ bytes: Data) async throws -> BundleImportResponse {
        try require(.bundleImport)
        return try await context.client.send(APIRoute(method: "POST", path: "\(path)/import", headers: ["Content-Type": "application/octet-stream"], body: bytes), context: context.requestContext)
    }
    private func require(_ permission: MemoryPermission, readOnlyAllowed: Bool = false) throws {
        guard readOnlyAllowed || !context.permissions.readOnly else { throw BundleServiceError.restricted("The service is read-only.") }
        guard context.permissions.allows(permission) else { throw BundleServiceError.restricted("This identity does not have \(permission.rawValue) permission for \(context.profile.projectSlug).") }
    }
}
enum BundleServiceError: LocalizedError {
    case restricted(String), emptyExport
    var errorDescription: String? {
        switch self { case let .restricted(message): message; case .emptyExport: "The service returned an empty bundle." }
    }
}
