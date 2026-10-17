import Foundation

struct WorkService: Sendable {
    let context: FeatureContext

    func agentSnapshot() async throws -> WorkAgentSnapshot {
        try await context.client.get(APIRoute(path: "/v1/agents"), context: context.requestContext)
    }

    func activities(kind: String? = nil, limit: Int = 100) async throws -> WorkActivityList {
        var query = [
            URLQueryItem(name: "limit", value: String(limit)),
            URLQueryItem(name: "include_details", value: "true")
        ]
        if let kind { query.append(URLQueryItem(name: "kind", value: kind)) }
        let route = APIRoute<WorkActivityList>(
            path: "/v1/projects/\(URLPathSegment.escape(context.profile.projectSlug))/activities",
            query: query
        )
        return try await context.client.get(route, context: context.requestContext)
    }

    func briefing(includeLLMSummary: Bool, limit: Int = 20) async throws -> WorkBriefingResponse {
        let body = try JSONEncoder.workJSON.encode(WorkBriefingRequest(
            project: context.profile.projectSlug,
            includeLLMSummary: includeLLMSummary,
            limit: limit
        ))
        let route = APIRoute<WorkBriefingResponse>(
            method: "POST",
            path: "/v1/projects/\(URLPathSegment.escape(context.profile.projectSlug))/up-to-speed",
            body: body
        )
        return try await context.client.send(route, context: context.requestContext)
    }

    func llmAuditStatus() async throws -> WorkLlmAuditStatus {
        try await context.client.get(APIRoute(path: "/v1/config/llm-audit"), context: context.requestContext)
    }

    func setLLMAudit(enabled: Bool) async throws -> WorkLlmAuditStatus {
        guard !context.permissions.readOnly,
              context.permissions.globalAdministrator,
              context.permissions.globalAllowed.contains(.systemAdmin) else {
            throw MemoryServiceError.forbidden
        }
        let body = try JSONEncoder.workJSON.encode(WorkLlmAuditRequest(enabled: enabled))
        return try await context.client.send(
            APIRoute(method: "POST", path: "/v1/config/llm-audit", body: body),
            context: context.requestContext
        )
    }

    func resume(includeLLMSummary: Bool, limit: Int = 20) async throws -> WorkResumeResponse {
        let body = try JSONEncoder.workJSON.encode(WorkResumeRequest(
            project: context.profile.projectSlug,
            repositoryRoot: context.profile.repositoryRoot?.path,
            includeLLMSummary: includeLLMSummary,
            limit: limit
        ))
        let route = APIRoute<WorkResumeResponse>(
            method: "POST",
            path: "/v1/projects/\(URLPathSegment.escape(context.profile.projectSlug))/resume",
            body: body
        )
        return try await context.client.send(route, context: context.requestContext)
    }
}
