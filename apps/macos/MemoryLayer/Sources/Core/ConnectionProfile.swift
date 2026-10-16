import Foundation
import Darwin

public enum ConnectionKind: String, Codable, Hashable, Sendable {
    case localSession
    case serviceToken
}

public struct ConnectionProfile: Codable, Identifiable, Hashable, Sendable {
    public let id: UUID
    public var name: String
    public var baseURL: URL
    public var projectSlug: String
    public var repositoryRoot: URL?
    public var kind: ConnectionKind
    public var createdAt: Date
    public var lastConnectedAt: Date?

    public init(id: UUID = UUID(), name: String, baseURL: URL, projectSlug: String,
                repositoryRoot: URL? = nil, kind: ConnectionKind, createdAt: Date = .now,
                lastConnectedAt: Date? = nil) {
        self.id = id
        self.name = name
        self.baseURL = baseURL
        self.projectSlug = projectSlug
        self.repositoryRoot = repositoryRoot
        self.kind = kind
        self.createdAt = createdAt
        self.lastConnectedAt = lastConnectedAt
    }

    public var isLoopback: Bool {
        guard let rawHost = baseURL.host?.lowercased() else { return false }
        let host = rawHost.trimmingCharacters(in: CharacterSet(charactersIn: "[]"))
        if host == "localhost" || host == "::1" { return true }
        var address = in_addr()
        guard host.withCString({ inet_pton(AF_INET, $0, &address) }) == 1 else { return false }
        return (UInt32(bigEndian: address.s_addr) >> 24) == 127
    }

    public func endpoint(path: String, query: [URLQueryItem] = []) throws -> URL {
        guard let scheme = baseURL.scheme?.lowercased(), ["http", "https"].contains(scheme),
              baseURL.host != nil, baseURL.user == nil, baseURL.password == nil else {
            throw MemoryServiceError.invalidBaseURL
        }
        guard path.hasPrefix("/"), !path.hasPrefix("//") else { throw MemoryServiceError.invalidRoute(path) }
        var parts = URLComponents(url: baseURL, resolvingAgainstBaseURL: false)
        guard !path.contains("?") && !path.contains("#") && !path.contains(" ") else {
            throw MemoryServiceError.invalidRoute(path)
        }
        let prefix = parts?.percentEncodedPath.trimmingCharacters(in: CharacterSet(charactersIn: "/")) ?? ""
        parts?.percentEncodedPath = "/" + [prefix, path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))]
            .filter { !$0.isEmpty }.joined(separator: "/")
        parts?.queryItems = query.isEmpty ? nil : query
        guard let url = parts?.url else { throw MemoryServiceError.invalidBaseURL }
        return url
    }
}

public struct RequestContext: Sendable, Hashable {
    public let profileID: UUID
    public let projectSlug: String
    public let repositoryRoot: URL?
    public let revision: Int

    public init(profile: ConnectionProfile, revision: Int = 0) {
        profileID = profile.id
        projectSlug = profile.projectSlug
        repositoryRoot = profile.repositoryRoot
        self.revision = revision
    }
}

public protocol ConnectionProfileStore: Sendable {
    func load() async throws -> [ConnectionProfile]
    func save(_ profiles: [ConnectionProfile]) async throws
}

public actor DefaultsConnectionProfileStore: ConnectionProfileStore {
    private let defaults: UserDefaults
    private let key: String

    public init(defaults: UserDefaults = .standard, key: String = "memory.layer.connections.v1") {
        self.defaults = defaults
        self.key = key
    }

    public func load() throws -> [ConnectionProfile] {
        guard let data = defaults.data(forKey: key) else { return [] }
        return try JSONDecoder.memoryLayer.decode([ConnectionProfile].self, from: data)
    }

    public func save(_ profiles: [ConnectionProfile]) throws {
        defaults.set(try JSONEncoder.memoryLayer.encode(profiles), forKey: key)
    }
}
