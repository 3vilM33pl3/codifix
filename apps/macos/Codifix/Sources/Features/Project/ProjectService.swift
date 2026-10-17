import Foundation

struct ProjectService: Sendable {
    let context: FeatureContext

    private var projectPath: String {
        "/v1/projects/\(URLPathSegment.escape(context.profile.projectSlug))"
    }

    func overview() async throws -> ProjectOverviewResponse {
        try requireRead()
        return try await context.client.get(
            APIRoute(path: "\(projectPath)/overview"), context: context.requestContext
        )
    }

    func activities(limit: Int = 100) async throws -> ProjectActivityListResponse {
        try requireRead()
        return try await context.client.get(
            APIRoute(path: "\(projectPath)/activities", query: [
                URLQueryItem(name: "limit", value: String(limit)),
                URLQueryItem(name: "include_details", value: "true")
            ]),
            context: context.requestContext
        )
    }

    func embeddingBackends() async throws -> ProjectEmbeddingBackendsResponse {
        try requireRead()
        return try await context.client.get(
            APIRoute(path: "/v1/embeddings/backends", query: [
                URLQueryItem(name: "project", value: context.profile.projectSlug)
            ]), context: context.requestContext
        )
    }

    func skills(filter: ProjectSkillFilter) async throws -> ProjectSkillInventory {
        try requireRead()
        guard let root = context.profile.repositoryRoot?.path else { throw ProjectFeatureError.repositoryRootRequired }
        return try await context.client.get(
            APIRoute(path: "/v1/skills", query: [
                URLQueryItem(name: "repo_root", value: root),
                URLQueryItem(name: "filter", value: filter.rawValue)
            ]), context: context.requestContext
        )
    }

    func skill(name: String, filter: ProjectSkillFilter) async throws -> ProjectSkillContentResponse {
        try requireRead()
        guard let root = context.profile.repositoryRoot?.path else { throw ProjectFeatureError.repositoryRootRequired }
        return try await context.client.get(
            APIRoute(path: "/v1/skills/\(URLPathSegment.escape(name))", query: [
                URLQueryItem(name: "repo_root", value: root),
                URLQueryItem(name: "filter", value: filter.rawValue)
            ]), context: context.requestContext
        )
    }

    func repairSkills(filter: ProjectSkillFilter) async throws -> ProjectSkillRepairResponse {
        try requireGlobal(.systemAdmin)
        guard let root = context.profile.repositoryRoot?.path else { throw ProjectFeatureError.repositoryRootRequired }
        let body = try JSONEncoder.codifix.encode(SkillRepairRequest(repoRoot: root, filter: filter.rawValue))
        return try await context.client.send(
            APIRoute< ProjectSkillRepairResponse >(
                method: "POST", path: "/v1/skills/repair",
                headers: ["Content-Type": "application/json"], body: body
            ), context: context.requestContext
        )
    }

    func curate() async throws -> ProjectCurateResponse {
        try requireProjectWrite(.memoryCurate)
        let body = try JSONEncoder.codifix.encode(CurateRequest(project: context.profile.projectSlug, batchSize: nil))
        return try await context.client.send(
            APIRoute<ProjectCurateResponse>(method: "POST", path: "/v1/curate",
                                            headers: ["Content-Type": "application/json"], body: body),
            context: context.requestContext
        )
    }

    func reindex(backend: String? = nil) async throws -> ProjectReindexResponse {
        try requireProjectWrite(.memoryCurate)
        let body = try JSONEncoder.codifix.encode(ProjectEmbeddingRequest(project: context.profile.projectSlug, backend: backend))
        return try await context.client.send(
            APIRoute<ProjectReindexResponse>(method: "POST", path: "/v1/reindex",
                                             headers: ["Content-Type": "application/json"], body: body),
            context: context.requestContext
        )
    }

    func reembed(backend: String? = nil) async throws -> ProjectReembedResponse {
        try requireProjectWrite(.memoryCurate)
        let body = try JSONEncoder.codifix.encode(ProjectEmbeddingRequest(project: context.profile.projectSlug, backend: backend))
        return try await context.client.send(
            APIRoute<ProjectReembedResponse>(method: "POST", path: "/v1/reembed",
                                             headers: ["Content-Type": "application/json"], body: body),
            context: context.requestContext
        )
    }

    func archiveLowValueMemories() async throws -> ProjectArchiveResponse {
        try requireProjectWrite(.memoryCurate)
        let body = try JSONEncoder.codifix.encode(ArchiveRequest(
            project: context.profile.projectSlug, maxConfidence: 0.3, maxImportance: 1
        ))
        return try await context.client.send(
            APIRoute<ProjectArchiveResponse>(method: "POST", path: "/v1/archive",
                                             headers: ["Content-Type": "application/json"], body: body),
            context: context.requestContext
        )
    }

    func activateEmbeddingBackend(_ name: String) async throws -> ProjectEmbeddingBackendsResponse {
        try requireGlobal(.embeddingsManage)
        let body = try JSONEncoder.codifix.encode(ActivateEmbeddingRequest(name: name))
        return try await context.client.send(
            APIRoute<ProjectEmbeddingBackendsResponse>(method: "POST", path: "/v1/embeddings/activate",
                                                       headers: ["Content-Type": "application/json"], body: body),
            context: context.requestContext
        )
    }

    func deactivateEmbeddingBackend() async throws -> ProjectEmbeddingBackendsResponse {
        try requireGlobal(.embeddingsManage)
        return try await context.client.send(
            APIRoute<ProjectEmbeddingBackendsResponse>(method: "POST", path: "/v1/embeddings/deactivate",
                                                       headers: ["Content-Type": "application/json"],
                                                       body: try JSONEncoder.codifix.encode(EmptyRequest())),
            context: context.requestContext
        )
    }

    func setEmbeddingCreation(backend name: String, enabled: Bool) async throws -> ProjectEmbeddingBackendsResponse {
        try requireGlobal(.embeddingsManage)
        let body = try JSONEncoder.codifix.encode(EmbeddingCreationRequest(name: name, enabled: enabled))
        return try await context.client.send(
            APIRoute<ProjectEmbeddingBackendsResponse>(method: "POST", path: "/v1/embeddings/create-enabled",
                                                       headers: ["Content-Type": "application/json"], body: body),
            context: context.requestContext
        )
    }

    private func requireRead() throws {
        guard context.permissions.allows(.memoryRead) else { throw ProjectFeatureError.restricted }
    }

    private func requireProjectWrite(_ permission: MemoryPermission) throws {
        guard !context.permissions.readOnly else { throw ProjectFeatureError.readOnly }
        guard context.permissions.allows(permission) else { throw ProjectFeatureError.restricted }
    }

    private func requireGlobal(_ permission: MemoryPermission) throws {
        guard !context.permissions.readOnly else { throw ProjectFeatureError.readOnly }
        guard context.permissions.globalAllowed.contains(permission) else { throw ProjectFeatureError.restricted }
    }
}

enum ProjectFeatureError: Error, LocalizedError, Equatable {
    case restricted
    case readOnly
    case repositoryRootRequired

    var errorDescription: String? {
        switch self {
        case .restricted: "This account does not have permission for this operation."
        case .readOnly: "This connection is read-only."
        case .repositoryRootRequired: "Set a repository root in connection settings to inspect project skills."
        }
    }
}

private struct CurateRequest: Encodable {
    let project: String
    let batchSize: Int?
    private enum CodingKeys: String, CodingKey { case project; case batchSize = "batch_size" }
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(project, forKey: .project)
        if let batchSize { try c.encode(batchSize, forKey: .batchSize) }
        else { try c.encodeNil(forKey: .batchSize) }
    }
}

private struct ProjectEmbeddingRequest: Encodable {
    let project: String
    let backend: String?
    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(project, forKey: .project)
        if let backend { try c.encode(backend, forKey: .backend) }
        else { try c.encodeNil(forKey: .backend) }
    }
    private enum CodingKeys: String, CodingKey { case project, backend }
}

private struct ArchiveRequest: Encodable {
    let project: String
    let maxConfidence: Double
    let maxImportance: Int
    private enum CodingKeys: String, CodingKey {
        case project
        case maxConfidence = "max_confidence"
        case maxImportance = "max_importance"
    }
}

private struct SkillRepairRequest: Encodable {
    let repoRoot: String
    let filter: String
    private enum CodingKeys: String, CodingKey { case filter; case repoRoot = "repo_root" }
}

private struct ActivateEmbeddingRequest: Encodable { let name: String }
private struct EmbeddingCreationRequest: Encodable { let name: String; let enabled: Bool }
private struct EmptyRequest: Encodable {}
