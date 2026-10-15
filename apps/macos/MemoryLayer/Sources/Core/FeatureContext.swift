import Foundation

/// Immutable connection snapshot passed into each feature screen.
public struct FeatureContext: Sendable {
    public let client: MemoryServiceClient
    public let profile: ConnectionProfile
    public let auth: AuthMeResponse?
    public let permissions: EffectivePermissions
    public let revision: Int

    public init(client: MemoryServiceClient, profile: ConnectionProfile,
                auth: AuthMeResponse?, revision: Int) {
        self.client = client
        self.profile = profile
        self.auth = auth
        permissions = EffectivePermissions(auth: auth, projectSlug: profile.projectSlug)
        self.revision = revision
    }

    public var requestContext: RequestContext { RequestContext(profile: profile, revision: revision) }
}

public enum MemoryFeatureGroup: String, CaseIterable, Identifiable, Hashable, Sendable {
    case knowledge = "Knowledge"
    case work = "Work"
    case project = "Project"
    case administration = "Administration"
    public var id: String { rawValue }
}

public enum AppDestination: String, CaseIterable, Identifiable, Hashable, Sendable {
    case memories = "Memories"
    case query = "Query"
    case graph = "Graph"
    case review = "Review"
    case agents = "Agents"
    case activity = "Activity"
    case resume = "Resume"
    case overview = "Overview"
    case errors = "Errors"
    case watchers = "Watchers"
    case skills = "Skills"
    case embeddings = "Embeddings"
    case automations = "Automations"
    case bundles = "Bundles"
    case access = "Access"

    public var id: String { rawValue }
    public var group: MemoryFeatureGroup {
        switch self {
        case .memories, .query, .graph, .review: .knowledge
        case .agents, .activity, .resume: .work
        case .overview, .errors, .watchers, .skills, .embeddings: .project
        case .automations, .bundles, .access: .administration
        }
    }

    public var symbol: String {
        switch self {
        case .memories: "square.stack.3d.up"
        case .query: "text.magnifyingglass"
        case .graph: "point.3.connected.trianglepath.dotted"
        case .review: "checkmark.square"
        case .agents: "person.crop.rectangle"
        case .activity: "waveform.path"
        case .resume: "arrow.clockwise"
        case .overview: "square.grid.2x2"
        case .errors: "exclamationmark.triangle"
        case .watchers: "eye"
        case .skills: "chevron.left.forwardslash.chevron.right"
        case .embeddings: "square.3.layers.3d"
        case .automations: "bolt"
        case .bundles: "shippingbox"
        case .access: "lock"
        }
    }
}

public enum ConnectionState: Equatable, Sendable {
    case disconnected
    case connecting
    case connected
    case failed(String)
}
