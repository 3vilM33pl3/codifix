import Foundation
import XCTest
@testable import Codifix

final class AccessFeatureTests: XCTestCase {
    func testAccessDTOsDecodeServiceSnakeCaseAndOptionalFields() throws {
        let principals = try JSONDecoder.codifix.decode([AccessPrincipal].self, from: Data(Self.principalsJSON.utf8))
        XCTAssertEqual(principals.first?.displayName, "Token worker")
        XCTAssertEqual(principals.first?.kind, "service_token")
        XCTAssertEqual(principals.first?.projects.first?.role, .operatorRole)
        XCTAssertEqual(principals.first?.globalPermissions, [])

        let tokens = try JSONDecoder.codifix.decode([AccessToken].self, from: Data(Self.tokensJSON.utf8))
        XCTAssertEqual(tokens.first?.tokenPrefix, "mlt_abcd1234")
        XCTAssertEqual(tokens.first?.principalID, "principal-1")
        XCTAssertNil(tokens.first?.lastUsedAt)
        XCTAssertEqual(tokens.first?.projects.first?.source, "direct")

        let memberships = try JSONDecoder.codifix.decode([AccessMembership].self, from: Data(Self.membershipsJSON.utf8))
        XCTAssertEqual(memberships.first?.principalID, "principal-1")
        XCTAssertEqual(memberships.first?.role, .operatorRole)
        XCTAssertEqual(memberships.first?.source, "direct")

        let created = try JSONDecoder.codifix.decode(AccessCreatedTokenResponse.self, from: Data(#"{"token":"mlt_one_time_secret"}"#.utf8))
        XCTAssertEqual(created.token, "mlt_one_time_secret")
    }

    func testReadRoutesAndMutationBodiesMatchAuthAPIContract() async throws {
        let transport = AccessFixtureTransport(responses: [
            "GET /v1/auth/principals": Self.principalsJSON,
            "GET /v1/auth/tokens": Self.tokensJSON,
            "GET /v1/auth/memberships": Self.membershipsJSON,
            "POST /v1/auth/tokens": #"{"token":"mlt_create_once"}"#,
            "POST /v1/auth/memberships": Self.membershipJSON,
            "POST /v1/auth/tokens/token%2Fwith%20space/revoke": Self.tokenJSON,
            "DELETE /v1/auth/memberships/membership%2Fwith%20space": Self.membershipJSON
        ])
        let context = try testContext(transport: transport, auth: Self.authorizedAdmin())
        let service = AccessService(context: context)

        let principals = try await service.principals()
        let tokens = try await service.tokens()
        let memberships = try await service.memberships()
        XCTAssertEqual(principals.count, 1)
        XCTAssertEqual(tokens.count, 1)
        XCTAssertEqual(memberships.count, 1)
        let created = try await service.createToken(AccessTokenCreateRequest(
            name: "Automation", project: "project / one", role: .operatorRole, expiresInSeconds: nil
        ))
        XCTAssertEqual(created.token, "mlt_create_once")
        _ = try await service.grantMembership(AccessMembershipGrantRequest(
            principalID: "principal-1", project: "project / one", role: .writer
        ))
        _ = try await service.revokeToken(id: "token/with space")
        _ = try await service.revokeMembership(id: "membership/with space")

        let requests = await transport.requests
        XCTAssertEqual(requests.map { $0.httpMethod ?? "" }, ["GET", "GET", "GET", "POST", "POST", "POST", "DELETE"])
        XCTAssertEqual(requests.map { encodedPath($0.url) }, [
            "/v1/auth/principals", "/v1/auth/tokens", "/v1/auth/memberships",
            "/v1/auth/tokens", "/v1/auth/memberships",
            "/v1/auth/tokens/token%2Fwith%20space/revoke",
            "/v1/auth/memberships/membership%2Fwith%20space"
        ])

        let createBody = try object(try XCTUnwrap(requests[3].httpBody))
        XCTAssertEqual(createBody["name"] as? String, "Automation")
        XCTAssertEqual(createBody["project"] as? String, "project / one")
        XCTAssertEqual(createBody["role"] as? String, "operator")
        XCTAssertNil(createBody["expires_in_seconds"], "An omitted expiration matches the browser request contract.")
        let grantBody = try object(try XCTUnwrap(requests[4].httpBody))
        XCTAssertEqual(grantBody["principal_id"] as? String, "principal-1")
        XCTAssertEqual(grantBody["project"] as? String, "project / one")
        XCTAssertEqual(grantBody["role"] as? String, "writer")
    }

    func testAccessRequiresGlobalAdminAndGlobalAuthManage() async throws {
        let projectOnly = try Self.auth(#"{"authenticated":true,"mode":"multi_user","read_only":false,"principal":{"id":"p-1","kind":"human_oidc","display_name":"Project admin","groups":[],"projects":[{"project":"alpha","role":"admin","permissions":["auth_manage"],"source":"direct"}]}}"#)
        let globalWithoutAuthManage = try Self.auth(#"{"authenticated":true,"mode":"multi_user","read_only":false,"principal":{"id":"p-1","kind":"human_oidc","display_name":"Limited admin","groups":[],"global_role":"admin","global_permissions":["memory_read"]}}"#)

        for auth in [projectOnly, globalWithoutAuthManage] {
            let transport = AccessFixtureTransport(responses: [:])
            let service = AccessService(context: try testContext(transport: transport, auth: auth))
            do {
                _ = try await service.principals()
                XCTFail("Access routes must require global admin and auth_manage.")
            } catch let error as AccessFeatureError {
                XCTAssertEqual(error, .restricted)
            }
            let requests = await transport.requests
            XCTAssertEqual(requests.count, 0)
        }
    }

    func testReadOnlyAdminCanReadButCannotMutate() async throws {
        let readOnlyAdmin = try Self.auth(#"{"authenticated":true,"mode":"multi_user","read_only":true,"principal":{"id":"p-1","kind":"human_oidc","display_name":"Admin","groups":[],"global_role":"admin","global_permissions":["auth_manage"]}}"#)
        let transport = AccessFixtureTransport(responses: ["GET /v1/auth/principals": Self.principalsJSON])
        let service = AccessService(context: try testContext(transport: transport, auth: readOnlyAdmin))
        let principals = try await service.principals()
        XCTAssertEqual(principals.count, 1)
        do {
            _ = try await service.createToken(AccessTokenCreateRequest(name: "x", project: "alpha", role: .reader, expiresInSeconds: nil))
            XCTFail("Read-only connections must not mutate access.")
        } catch let error as AccessFeatureError {
            XCTAssertEqual(error, .readOnly)
        }
        let requests = await transport.requests
        XCTAssertEqual(requests.count, 1)
    }

    func testFailedWriteIsNotRetried() async throws {
        let transport = AccessFixtureTransport(responses: [:], statusCode: 503, failureBody: #"{"message":"temporary failure"}"#)
        let service = AccessService(context: try testContext(transport: transport, auth: Self.authorizedAdmin()))
        do {
            _ = try await service.createToken(AccessTokenCreateRequest(name: "Agent", project: "alpha", role: .reader, expiresInSeconds: nil))
            XCTFail("Expected service failure.")
        } catch let error as MemoryServiceError {
            XCTAssertEqual(error, .httpStatus(503, "temporary failure"))
        }
        let requests = await transport.requests
        XCTAssertEqual(requests.count, 1)
    }

    func testRequestGateRejectsObsoleteRefreshAndConnectionRevision() {
        var gate = AccessRequestGate()
        let first = gate.begin(revision: 2)
        let second = gate.begin(revision: 2)

        XCTAssertFalse(gate.accepts(first, revision: 2))
        XCTAssertTrue(gate.accepts(second, revision: 2))
        XCTAssertFalse(gate.accepts(second, revision: 3))
        gate.invalidate()
        XCTAssertFalse(gate.accepts(second, revision: 2))
    }

    private func testContext(transport: AccessFixtureTransport, auth: AuthMeResponse?) throws -> FeatureContext {
        let profile = ConnectionProfile(name: "Access test", baseURL: URL(string: "https://memory.example.test")!,
                                        projectSlug: "alpha", repositoryRoot: nil, kind: .serviceToken)
        let client = MemoryServiceClient(profile: profile, transport: transport, secrets: AccessTestSecrets())
        return FeatureContext(client: client, profile: profile, auth: auth, revision: 4)
    }

    private static func auth(_ json: String) throws -> AuthMeResponse {
        try JSONDecoder.codifix.decode(AuthMeResponse.self, from: Data(json.utf8))
    }

    private func encodedPath(_ url: URL?) -> String? {
        guard let url else { return nil }
        return URLComponents(url: url, resolvingAgainstBaseURL: false)?.percentEncodedPath
    }

    private func object(_ data: Data) throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    private static func authorizedAdmin() throws -> AuthMeResponse {
        try auth(#"{"authenticated":true,"mode":"multi_user","read_only":false,"principal":{"id":"admin-1","kind":"human_oidc","display_name":"Admin","groups":[],"global_role":"admin","global_permissions":["auth_manage"]}}"#)
    }

    private static let principalsJSON = #"[{"id":"principal-1","kind":"service_token","display_name":"Token worker","groups":[],"projects":[{"project":"alpha","role":"operator","permissions":["memory_read"],"source":"direct"}]}]"#
    private static let tokensJSON = #"[{"id":"token-1","principal_id":"principal-1","name":"Token worker","token_prefix":"mlt_abcd1234","created_at":"2026-10-15T12:00:00Z","expires_at":null,"revoked_at":null,"last_used_at":null,"token":null,"projects":[{"project":"alpha","role":"operator","source":"direct"}]}]"#
    private static let membershipsJSON = #"[{"id":"membership-1","principal_id":"principal-1","project":"alpha","role":"operator","source":"direct","created_at":"2026-10-15T12:00:00Z"}]"#
    private static let membershipJSON = #"{"id":"membership-1","principal_id":"principal-1","project":"alpha","role":"writer","source":"direct","created_at":"2026-10-15T12:00:00Z"}"#
    private static let tokenJSON = #"{"id":"token-1","principal_id":"principal-1","name":"Token worker","token_prefix":"mlt_abcd1234","created_at":"2026-10-15T12:00:00Z","expires_at":null,"revoked_at":"2026-10-15T12:00:00Z","last_used_at":null,"token":null,"projects":[]}"#
}

private actor AccessFixtureTransport: HTTPTransport {
    private(set) var requests: [URLRequest] = []
    private let responses: [String: String]
    private let statusCode: Int
    private let failureBody: String

    init(responses: [String: String], statusCode: Int = 200, failureBody: String = "{}") {
        self.responses = responses
        self.statusCode = statusCode
        self.failureBody = failureBody
    }

    func send(_ request: URLRequest) async throws -> HTTPReply {
        requests.append(request)
        let path = URLComponents(url: request.url!, resolvingAgainstBaseURL: false)!.percentEncodedPath
        let body = responses["\(request.httpMethod ?? "GET") \(path)"] ?? "{}"
        return HTTPReply(statusCode: statusCode, headers: [:], data: Data((statusCode >= 400 ? failureBody : body).utf8))
    }

    func cancelAll() async {}
}

private struct AccessTestSecrets: SecretStore {
    func read(profileID: UUID) throws -> String? { nil }
    func write(_ secret: String, profileID: UUID) throws {}
    func remove(profileID: UUID) throws {}
}
