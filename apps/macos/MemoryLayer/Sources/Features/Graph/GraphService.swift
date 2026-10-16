import Foundation

struct GraphService: Sendable {
    let context: FeatureContext

    private var projectPath: String {
        "/v1/projects/\(URLPathSegment.escape(context.profile.projectSlug))"
    }

    func status() async throws -> CodeGraphStatus {
        try await context.client.get(
            APIRoute(path: "\(projectPath)/graph/status"),
            context: context.requestContext
        )
    }

    func codeGraph(filters: GraphFilterDraft) async throws -> CodeGraphResponse {
        try await context.client.get(
            APIRoute(path: "\(projectPath)/graph", query: try filters.queryItems()),
            context: context.requestContext
        )
    }

    func memoryGraph(limit: Int = 250) async throws -> ProjectMemoryGraphResponse {
        try await context.client.get(
            APIRoute(path: "\(projectPath)/memory-graph", query: [
                URLQueryItem(name: "limit", value: String(min(250, max(1, limit))))
            ]),
            context: context.requestContext
        )
    }

    func load(filters: GraphFilterDraft) async throws -> GraphServiceSnapshot {
        async let status = status()
        async let codeGraph = codeGraph(filters: filters)
        async let memoryGraph = memoryGraph()
        return try await GraphServiceSnapshot(status: status, codeGraph: codeGraph, memoryGraph: memoryGraph)
    }
}

struct GraphServiceSnapshot: Sendable {
    let status: CodeGraphStatus
    let codeGraph: CodeGraphResponse
    let memoryGraph: ProjectMemoryGraphResponse
}
