import Foundation
import SwiftUI

@MainActor
public final class AppModel: ObservableObject {
    @Published public private(set) var profiles: [ConnectionProfile] = []
    @Published public private(set) var activeProfile: ConnectionProfile?
    @Published public private(set) var auth: AuthMeResponse?
    @Published public private(set) var connectionState: ConnectionState = .disconnected
    @Published public private(set) var revision = 0
    @Published public var selectedDestination: AppDestination = .memories
    @Published public var isInspectorPresented = false
    @Published public var alertMessage: String?

    private let profileStore: any ConnectionProfileStore
    private let secretStore: any SecretStore
    private let clientFactory: @Sendable (ConnectionProfile, any SecretStore) -> MemoryServiceClient
    private var client: MemoryServiceClient?

    public init(profileStore: any ConnectionProfileStore = DefaultsConnectionProfileStore(),
                secretStore: any SecretStore = KeychainSecretStore(),
                clientFactory: @escaping @Sendable (ConnectionProfile, any SecretStore) -> MemoryServiceClient = {
                    MemoryServiceClient(profile: $0, secrets: $1)
                }) {
        self.profileStore = profileStore
        self.secretStore = secretStore
        self.clientFactory = clientFactory
    }

    public var featureContext: FeatureContext? {
        guard case .connected = connectionState, let activeProfile, let client, let auth else { return nil }
        return FeatureContext(client: client, profile: activeProfile, auth: auth, revision: revision)
    }

    public func loadProfiles() async {
        do {
            profiles = try await profileStore.load()
        } catch {
            alertMessage = "Saved connections could not be loaded: \(error.localizedDescription)"
        }
    }

    public func connect(_ profile: ConnectionProfile, token: String? = nil) async {
        revision += 1
        let attempt = revision
        let previousClient = client
        client = nil
        await previousClient?.cancelAll()
        guard attempt == revision else { return }
        connectionState = .connecting
        auth = nil
        activeProfile = profile
        do {
            if profile.kind == .serviceToken, let token {
                try secretStore.write(token, profileID: profile.id)
            }
            if profile.kind == .serviceToken, try secretStore.read(profileID: profile.id) == nil {
                throw MemoryServiceError.unauthorized
            }
            let client = clientFactory(profile, secretStore)
            self.client = client
            let identity: AuthMeResponse
            if profile.kind == .localSession {
                identity = try await client.bootstrapLocalSession(context: RequestContext(profile: profile, revision: attempt))
            } else {
                identity = try await client.refreshIdentity(context: RequestContext(profile: profile, revision: attempt))
            }
            guard attempt == revision, activeProfile?.id == profile.id else { return }
            var saved = profiles.filter { $0.id != profile.id }
            var connected = profile
            connected.lastConnectedAt = .now
            saved.append(connected)
            saved.sort { ($0.lastConnectedAt ?? .distantPast) > ($1.lastConnectedAt ?? .distantPast) }
            try await profileStore.save(saved)
            guard attempt == revision, activeProfile?.id == profile.id else { return }
            profiles = saved
            activeProfile = connected
            auth = identity
            connectionState = .connected
        } catch is CancellationError {
            return
        } catch {
            guard attempt == revision, activeProfile?.id == profile.id else { return }
            connectionState = .failed(error.localizedDescription)
            alertMessage = error.localizedDescription
        }
    }

    public func disconnect() async {
        revision += 1
        let previousClient = client
        client = nil
        activeProfile = nil
        auth = nil
        connectionState = .disconnected
        await previousClient?.cancelAll()
    }

    public func refreshIdentity() async {
        guard let client, let profile = activeProfile else { return }
        let attempt = revision
        do {
            let identity = try await client.refreshIdentity(context: RequestContext(profile: profile, revision: attempt))
            guard revision == attempt, activeProfile?.id == profile.id else { return }
            auth = identity
            connectionState = .connected
            revision += 1
        } catch {
            guard revision == attempt, activeProfile?.id == profile.id else { return }
            connectionState = .failed(error.localizedDescription)
            alertMessage = error.localizedDescription
        }
    }

    public func removeProfile(_ profile: ConnectionProfile) async {
        if activeProfile?.id == profile.id { await disconnect() }
        do {
            try secretStore.remove(profileID: profile.id)
            profiles.removeAll { $0.id == profile.id }
            try await profileStore.save(profiles)
        } catch {
            alertMessage = "The connection could not be removed: \(error.localizedDescription)"
        }
    }
}
