import Foundation
import XCTest
@testable import Codifix

final class ConnectionSwitchTests: XCTestCase {
    @MainActor
    func testLateFailedConnectionCannotReplaceNewConnection() async throws {
        let delayed = DelayedIdentityTransport()
        let immediate = ImmediateIdentityTransport()
        let old = profile("Old", port: 4250)
        let current = profile("Current", port: 4040)
        let model = AppModel(profileStore: TestProfileStore(), secretStore: TestTokenStore()) { profile, secrets in
            MemoryServiceClient(profile: profile,
                                transport: profile.id == old.id ? delayed : immediate,
                                secrets: secrets)
        }
        let first = Task { await model.connect(old) }
        await delayed.waitForRequest()
        await model.connect(current)
        await delayed.complete(status: 401)
        await first.value

        XCTAssertEqual(model.activeProfile?.id, current.id)
        XCTAssertEqual(model.connectionState, .connected)
        XCTAssertNil(model.alertMessage)
        XCTAssertEqual(model.auth?.principal.displayName, "Fixture principal")
    }

    @MainActor
    func testDisconnectRejectsLateSuccessfulIdentity() async {
        let delayed = DelayedIdentityTransport()
        let profile = profile("Delayed", port: 4250)
        let model = AppModel(profileStore: TestProfileStore(), secretStore: TestTokenStore()) { profile, secrets in
            MemoryServiceClient(profile: profile, transport: delayed, secrets: secrets)
        }
        let connection = Task { await model.connect(profile) }
        await delayed.waitForRequest()
        await model.disconnect()
        await delayed.complete(status: 200)
        await connection.value

        XCTAssertEqual(model.connectionState, .disconnected)
        XCTAssertNil(model.activeProfile)
        XCTAssertNil(model.auth)
        XCTAssertNil(model.featureContext)
        XCTAssertNil(model.alertMessage)
    }

    private func profile(_ name: String, port: Int) -> ConnectionProfile {
        ConnectionProfile(name: name, baseURL: URL(string: "http://127.0.0.1:\(port)")!,
                          projectSlug: "fixture", kind: .serviceToken)
    }
}

private let identityFixture = Data(#"{"authenticated":true,"mode":"multi_user","read_only":false,"principal":{"id":"fixture-principal","kind":"service_token","display_name":"Fixture principal","groups":[],"global_permissions":[],"projects":[{"project":"fixture","role":"reader","permissions":["memory_read","bundle_export"],"source":"token"}]}}"#.utf8)

private actor ImmediateIdentityTransport: HTTPTransport {
    func send(_ request: URLRequest) async throws -> HTTPReply {
        HTTPReply(statusCode: 200, headers: [:], data:
                    request.url?.path == "/healthz" ? Data(#"{"version":"fixture-dev"}"#.utf8) : identityFixture)
    }
    func cancelAll() async {}
}

/// Deliberately ignores cancellation to prove that state guards also reject a
/// response from a transport which has already completed its network work.
private actor DelayedIdentityTransport: HTTPTransport {
    private var pending: CheckedContinuation<HTTPReply, Error>?
    private var waiter: CheckedContinuation<Void, Never>?

    func send(_ request: URLRequest) async throws -> HTTPReply {
        try await withCheckedThrowingContinuation { continuation in
            pending = continuation
            waiter?.resume()
            waiter = nil
        }
    }

    func waitForRequest() async {
        if pending != nil { return }
        await withCheckedContinuation { waiter = $0 }
    }

    func complete(status: Int) {
        pending?.resume(returning: HTTPReply(statusCode: status, headers: [:], data: identityFixture))
        pending = nil
    }

    func cancelAll() async {}
}

private actor TestProfileStore: ConnectionProfileStore {
    private var profiles: [ConnectionProfile] = []
    func load() async throws -> [ConnectionProfile] { profiles }
    func save(_ profiles: [ConnectionProfile]) async throws { self.profiles = profiles }
}

private struct TestTokenStore: SecretStore {
    func read(profileID: UUID) throws -> String? { "fixture-token" }
    func write(_ secret: String, profileID: UUID) throws {}
    func remove(profileID: UUID) throws {}
}
