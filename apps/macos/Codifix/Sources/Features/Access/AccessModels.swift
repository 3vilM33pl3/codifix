import Foundation

enum AccessRole: String, Codable, CaseIterable, Identifiable, Sendable {
    case reader
    case writer
    case operatorRole = "operator"
    case admin

    var id: String { rawValue }
    var title: String { rawValue.capitalized }
    static let serviceTokenRoles: [AccessRole] = [.reader, .writer, .operatorRole]
}

struct AccessPrincipal: Decodable, Identifiable, Sendable, Hashable {
    let id: String
    let kind: String
    let displayName: String
    let email: String?
    let issuer: String?
    let subject: String?
    let groups: [String]
    let globalRole: AccessRole?
    let globalPermissions: [String]
    let projects: [AccessProjectGrant]

    private enum CodingKeys: String, CodingKey {
        case id, kind, email, issuer, subject, groups, projects
        case displayName = "display_name"
        case globalRole = "global_role"
        case globalPermissions = "global_permissions"
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        kind = try c.decode(String.self, forKey: .kind)
        displayName = try c.decode(String.self, forKey: .displayName)
        email = try c.decodeIfPresent(String.self, forKey: .email)
        issuer = try c.decodeIfPresent(String.self, forKey: .issuer)
        subject = try c.decodeIfPresent(String.self, forKey: .subject)
        groups = try c.decodeIfPresent([String].self, forKey: .groups) ?? []
        globalRole = try c.decodeIfPresent(AccessRole.self, forKey: .globalRole)
        globalPermissions = try c.decodeIfPresent([String].self, forKey: .globalPermissions) ?? []
        projects = try c.decodeIfPresent([AccessProjectGrant].self, forKey: .projects) ?? []
    }
}

struct AccessProjectGrant: Decodable, Sendable, Hashable {
    let project: String
    let role: AccessRole
    let permissions: [String]
    let source: String

    private enum CodingKeys: String, CodingKey { case project, role, permissions, source }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        project = try c.decode(String.self, forKey: .project)
        role = try c.decode(AccessRole.self, forKey: .role)
        permissions = try c.decodeIfPresent([String].self, forKey: .permissions) ?? []
        source = try c.decodeIfPresent(String.self, forKey: .source) ?? ""
    }
}

struct AccessToken: Decodable, Identifiable, Sendable, Hashable {
    let id: String
    let principalID: String
    let name: String
    let tokenPrefix: String
    let createdAt: Date
    let expiresAt: Date?
    let revokedAt: Date?
    let lastUsedAt: Date?
    let projects: [AccessProjectGrant]

    private enum CodingKeys: String, CodingKey {
        case id, name, projects
        case principalID = "principal_id"
        case tokenPrefix = "token_prefix"
        case createdAt = "created_at"
        case expiresAt = "expires_at"
        case revokedAt = "revoked_at"
        case lastUsedAt = "last_used_at"
    }
}

/// Keeps the one-time secret in the transient response value only. Callers must
/// extract `token` immediately and must never put this response in view state.
struct AccessCreatedTokenResponse: Decodable, Sendable {
    let token: String?
}

struct AccessMembership: Decodable, Identifiable, Sendable, Hashable {
    let id: String
    let principalID: String
    let project: String
    let role: AccessRole
    let source: String
    let createdAt: Date

    private enum CodingKeys: String, CodingKey {
        case id, project, role, source
        case principalID = "principal_id"
        case createdAt = "created_at"
    }
}

struct AccessTokenCreateRequest: Encodable, Sendable {
    let name: String
    let project: String
    let role: AccessRole
    let expiresInSeconds: Int?

    private enum CodingKeys: String, CodingKey {
        case name, project, role
        case expiresInSeconds = "expires_in_seconds"
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(name, forKey: .name)
        try c.encode(project, forKey: .project)
        try c.encode(role, forKey: .role)
        try c.encodeIfPresent(expiresInSeconds, forKey: .expiresInSeconds)
    }
}

struct AccessMembershipGrantRequest: Encodable, Sendable {
    let principalID: String
    let project: String
    let role: AccessRole

    private enum CodingKeys: String, CodingKey {
        case project, role
        case principalID = "principal_id"
    }
}

enum AccessSection: String, CaseIterable, Identifiable {
    case principals = "Principals"
    case tokens = "Service tokens"
    case memberships = "Memberships"
    var id: String { rawValue }
}
