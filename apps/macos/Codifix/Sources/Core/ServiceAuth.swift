import Foundation

public enum AuthMode: Sendable, Codable, Equatable {
    case singleUser
    case multiUser
    case unknown(String)

    public init(from decoder: Decoder) throws {
        let value = try decoder.singleValueContainer().decode(String.self)
        switch value {
        case "single_user": self = .singleUser
        case "multi_user": self = .multiUser
        default: self = .unknown(value)
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(rawValue)
    }

    public var rawValue: String {
        switch self {
        case .singleUser: "single_user"
        case .multiUser: "multi_user"
        case let .unknown(value): value
        }
    }
}

public enum AuthRole: Sendable, Codable, Equatable, Comparable {
    case reader, writer, operatorRole, admin
    case unknown(String)

    public init(from decoder: Decoder) throws {
        let value = try decoder.singleValueContainer().decode(String.self)
        switch value {
        case "reader": self = .reader
        case "writer": self = .writer
        case "operator": self = .operatorRole
        case "admin": self = .admin
        default: self = .unknown(value)
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(rawValue)
    }

    public var rawValue: String {
        switch self {
        case .reader: "reader"
        case .writer: "writer"
        case .operatorRole: "operator"
        case .admin: "admin"
        case let .unknown(value): value
        }
    }

    private var rank: Int {
        switch self {
        case .reader: 0
        case .writer: 1
        case .operatorRole: 2
        case .admin: 3
        case .unknown: -1
        }
    }

    public static func < (lhs: AuthRole, rhs: AuthRole) -> Bool { lhs.rank < rhs.rank }
}

public struct AuthProjectAccess: Decodable, Sendable, Equatable {
    public let project: String
    public let role: AuthRole
    public let permissions: [String]?
    public let source: String

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        project = try container.decode(String.self, forKey: .project)
        role = try container.decode(AuthRole.self, forKey: .role)
        permissions = try container.decodeIfPresent([String].self, forKey: .permissions)
        source = try container.decodeIfPresent(String.self, forKey: .source) ?? ""
    }

    private enum CodingKeys: String, CodingKey { case project, role, permissions, source }
}

public struct AuthPrincipal: Decodable, Sendable, Equatable {
    public let id: String
    public let kind: String
    public let displayName: String
    public let email: String?
    public let groups: [String]
    public let globalRole: AuthRole?
    public let globalPermissions: [String]?
    public let projects: [AuthProjectAccess]

    private enum CodingKeys: String, CodingKey {
        case id, kind, email, groups, projects
        case displayName = "display_name"
        case globalRole = "global_role"
        case globalPermissions = "global_permissions"
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        kind = try c.decodeIfPresent(String.self, forKey: .kind) ?? "unknown"
        displayName = try c.decodeIfPresent(String.self, forKey: .displayName) ?? ""
        email = try c.decodeIfPresent(String.self, forKey: .email)
        groups = try c.decodeIfPresent([String].self, forKey: .groups) ?? []
        globalRole = try c.decodeIfPresent(AuthRole.self, forKey: .globalRole)
        globalPermissions = try c.decodeIfPresent([String].self, forKey: .globalPermissions)
        projects = try c.decodeIfPresent([AuthProjectAccess].self, forKey: .projects) ?? []
    }
}

public struct AuthMeResponse: Decodable, Sendable, Equatable {
    public let authenticated: Bool
    public let mode: AuthMode
    public let readOnly: Bool
    public let principal: AuthPrincipal

    private enum CodingKeys: String, CodingKey { case authenticated, mode, principal; case readOnly = "read_only" }
}

public enum MemoryPermission: String, Sendable, CaseIterable {
    case memoryRead = "memory_read"
    case activityCapture = "activity_capture"
    case memoryCurate = "memory_curate"
    case memoryDelete = "memory_delete"
    case loopsRun = "loops_run"
    case loopsConfigure = "loops_configure"
    case bundleExport = "bundle_export"
    case bundleImport = "bundle_import"
    case embeddingsManage = "embeddings_manage"
    case authManage = "auth_manage"
    case systemAdmin = "system_admin"
}

public struct EffectivePermissions: Sendable, Equatable {
    public let role: AuthRole
    public let allowed: Set<MemoryPermission>
    public let globalAllowed: Set<MemoryPermission>
    public let readOnly: Bool
    public let globalAdministrator: Bool

    public init(auth: AuthMeResponse?, projectSlug: String) {
        guard let auth, auth.authenticated else {
            role = .reader
            allowed = [.memoryRead]
            globalAllowed = [.memoryRead]
            readOnly = true
            globalAdministrator = false
            return
        }
        let projectGrant = auth.principal.projects.first { $0.project == projectSlug }
        let roles = [auth.principal.globalRole, projectGrant?.role].compactMap { $0 }
        let effectiveRole: AuthRole
        if auth.mode == .singleUser {
            effectiveRole = auth.principal.globalRole ?? .reader
        } else {
            effectiveRole = roles.max() ?? .unknown("ungranted")
        }
        role = effectiveRole
        readOnly = auth.readOnly
        globalAdministrator = auth.principal.globalRole == .admin
        let mappedGlobal = auth.principal.globalPermissions.map(Self.mapPermissions)
        globalAllowed = mappedGlobal ?? Self.preset(auth.principal.globalRole)
        let rolePermissions = Self.preset(effectiveRole)
        let hasExplicitPermissions = auth.principal.globalPermissions != nil || projectGrant?.permissions != nil
        if hasExplicitPermissions {
            allowed = (mappedGlobal ?? []).union(projectGrant?.permissions.map(Self.mapPermissions) ?? [])
        } else {
            allowed = rolePermissions
        }
    }

    public func allows(_ permission: MemoryPermission) -> Bool { allowed.contains(permission) }
    public var canViewAccess: Bool {
        globalAdministrator && globalAllowed.contains(.authManage)
    }
    public var canAdministerAccess: Bool {
        !readOnly && canViewAccess
    }

    private static func mapPermissions(_ values: [String]) -> Set<MemoryPermission> {
        let names = Set(values)
        return Set(MemoryPermission.allCases.filter { names.contains($0.rawValue) })
    }

    private static func preset(_ role: AuthRole?) -> Set<MemoryPermission> {
        guard let role else { return [] }
        return switch role {
        case .reader: [.memoryRead, .bundleExport]
        case .writer: [.memoryRead, .activityCapture, .bundleExport]
        case .operatorRole: [.memoryRead, .activityCapture, .memoryCurate, .loopsRun, .bundleExport, .bundleImport]
        case .admin: Set(MemoryPermission.allCases)
        case .unknown: []
        }
    }
}

extension JSONDecoder {
    static var codifix: JSONDecoder {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .custom { decoder in
            let container = try decoder.singleValueContainer()
            let value = try container.decode(String.self)
            for options: ISO8601DateFormatter.Options in [[.withInternetDateTime, .withFractionalSeconds], [.withInternetDateTime]] {
                let formatter = ISO8601DateFormatter()
                formatter.formatOptions = options
                if let date = formatter.date(from: value) { return date }
            }
            throw DecodingError.dataCorruptedError(in: container, debugDescription: "Invalid ISO-8601 date")
        }
        return decoder
    }
}

extension JSONEncoder {
    static var codifix: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        return encoder
    }
}
