import Foundation

struct KnowledgeService: Sendable {
    let context: FeatureContext

    private var projectPath: String {
        "/v1/projects/\(URLPathSegment.escape(context.profile.projectSlug))"
    }

    func memories() async throws -> ProjectMemoriesResponse {
        try await context.client.get(
            APIRoute(path: "\(projectPath)/memories"),
            context: context.requestContext
        )
    }

    func memory(id: String) async throws -> MemoryEntry {
        try await context.client.get(
            APIRoute(path: "/v1/memory/\(URLPathSegment.escape(id))"),
            context: context.requestContext
        )
    }

    func history(memoryID: String) async throws -> MemoryHistoryResponse {
        try await context.client.get(
            APIRoute(path: "/v1/memory/\(URLPathSegment.escape(memoryID))/history"),
            context: context.requestContext
        )
    }

    func delete(memoryID: String) async throws -> DeleteMemoryResponse {
        let body = try JSONEncoder.codifix.encode(DeleteRequest(memoryID: memoryID))
        return try await context.client.send(
            APIRoute<DeleteMemoryResponse>(
                method: "DELETE",
                path: "/v1/memory",
                headers: ["Content-Type": "application/json"],
                body: body
            ),
            context: context.requestContext
        )
    }

    func query(_ request: QueryRequest) async throws -> QueryResponse {
        try await context.client.send(
            APIRoute<QueryResponse>(
                method: "POST",
                path: "/v1/query",
                headers: ["Content-Type": "application/json"],
                body: try JSONEncoder.codifix.encode(request)
            ),
            context: context.requestContext
        )
    }

    func proposals() async throws -> ReplacementProposalListResponse {
        try await context.client.get(
            APIRoute(path: "\(projectPath)/replacement-proposals"),
            context: context.requestContext
        )
    }

    func replacementPolicy() async throws -> ReplacementPolicyResponse {
        let query = context.profile.repositoryRoot.map {
            [URLQueryItem(name: "repo_root", value: $0.path)]
        } ?? []
        return try await context.client.get(
            APIRoute(path: "\(projectPath)/replacement-policy", query: query),
            context: context.requestContext
        )
    }

    func saveReplacementPolicy(_ policy: String, repositoryRoot: URL) async throws -> ReplacementPolicyResponse {
        let body = try JSONEncoder.codifix.encode(
            ReplacementPolicySaveRequest(repositoryRoot: repositoryRoot.path, replacementPolicy: policy)
        )
        return try await context.client.send(
            APIRoute<ReplacementPolicyResponse>(
                method: "PUT",
                path: "\(projectPath)/replacement-policy",
                headers: ["Content-Type": "application/json"],
                body: body
            ),
            context: context.requestContext
        )
    }

    func resolveProposal(id: String, approve: Bool) async throws -> ReplacementProposalResolution {
        let action = approve ? "approve" : "reject"
        return try await context.client.send(
            APIRoute<ReplacementProposalResolution>(
                method: "POST",
                path: "\(projectPath)/replacement-proposals/\(URLPathSegment.escape(id))/\(action)"
            ),
            context: context.requestContext
        )
    }
}

private struct DeleteRequest: Encodable, Sendable {
    let memoryID: String
    private enum CodingKeys: String, CodingKey { case memoryID = "memory_id" }
}
