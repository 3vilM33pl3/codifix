import Foundation

struct AccessService: Sendable {
    let context: FeatureContext

    func principals() async throws -> [AccessPrincipal] {
        try requireAccess()
        return try await context.client.get(
            APIRoute(path: "/v1/auth/principals"), context: context.requestContext
        )
    }

    func tokens() async throws -> [AccessToken] {
        try requireAccess()
        return try await context.client.get(
            APIRoute(path: "/v1/auth/tokens"), context: context.requestContext
        )
    }

    func memberships() async throws -> [AccessMembership] {
        try requireAccess()
        return try await context.client.get(
            APIRoute(path: "/v1/auth/memberships"), context: context.requestContext
        )
    }

    func createToken(_ request: AccessTokenCreateRequest) async throws -> AccessCreatedTokenResponse {
        try requireMutationAccess()
        return try await context.client.send(
            APIRoute<AccessCreatedTokenResponse>(
                method: "POST", path: "/v1/auth/tokens",
                headers: ["Content-Type": "application/json"],
                body: try JSONEncoder.codifix.encode(request)
            ), context: context.requestContext
        )
    }

    func revokeToken(id: String) async throws -> AccessToken {
        try requireMutationAccess()
        return try await context.client.send(
            APIRoute<AccessToken>(method: "POST", path: "/v1/auth/tokens/\(URLPathSegment.escape(id))/revoke"),
            context: context.requestContext
        )
    }

    func grantMembership(_ request: AccessMembershipGrantRequest) async throws -> AccessMembership {
        try requireMutationAccess()
        return try await context.client.send(
            APIRoute<AccessMembership>(
                method: "POST", path: "/v1/auth/memberships",
                headers: ["Content-Type": "application/json"],
                body: try JSONEncoder.codifix.encode(request)
            ), context: context.requestContext
        )
    }

    func revokeMembership(id: String) async throws -> AccessMembership {
        try requireMutationAccess()
        return try await context.client.send(
            APIRoute<AccessMembership>(method: "DELETE", path: "/v1/auth/memberships/\(URLPathSegment.escape(id) )"),
            context: context.requestContext
        )
    }

    private func requireAccess() throws {
        guard context.permissions.globalAdministrator,
              context.permissions.globalAllowed.contains(.authManage) else {
            throw AccessFeatureError.restricted
        }
    }

    private func requireMutationAccess() throws {
        try requireAccess()
        guard !context.permissions.readOnly else { throw AccessFeatureError.readOnly }
    }
}

enum AccessFeatureError: LocalizedError, Equatable {
    case restricted
    case readOnly
    case missingOneTimeSecret
    case unknownWriteOutcome

    var errorDescription: String? {
        switch self {
        case .restricted:
            "Access administration requires a global administrator with auth_manage permission."
        case .readOnly:
            "This connection is read-only. Access changes are unavailable."
        case .missingOneTimeSecret:
            "The service created the token but did not return its one-time secret. Refresh and verify token status before creating another."
        case .unknownWriteOutcome:
            "The request was interrupted. Its outcome is unknown. Refresh the access lists before retrying."
        }
    }
}
