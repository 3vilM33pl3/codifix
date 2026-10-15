import XCTest
import Foundation
@testable import MemoryLayer

final class SharedClientTests: XCTestCase {
    func testEndpointEncodesProjectPathAsIndividualSegmentAndPreservesBasePath() throws {
        let profile = ConnectionProfile(name: "Local", baseURL: URL(string: "http://127.0.0.1:4250/prefix")!,
                                        projectSlug: "repo /%?é", kind: .localSession)
        let path = "/v1/projects/\(URLPathSegment.escape(profile.projectSlug))/memories"
        let url = try profile.endpoint(path: path)
        XCTAssertEqual(url.absoluteString, "http://127.0.0.1:4250/prefix/v1/projects/repo%20%2F%25%3F%C3%A9/memories")
    }

    func testLoopbackValidationRejectsLookalikeHosts() {
        func profile(_ host: String) -> ConnectionProfile {
            ConnectionProfile(name: "test", baseURL: URL(string: "http://\(host):4040")!, projectSlug: "p", kind: .localSession)
        }
        XCTAssertTrue(profile("localhost").isLoopback)
        XCTAssertTrue(profile("127.0.0.8").isLoopback)
        XCTAssertTrue(profile("[::1]").isLoopback)
        XCTAssertFalse(profile("127.evil.example").isLoopback)
        XCTAssertFalse(profile("localhost.attacker.test").isLoopback)
    }

    func testAuthMeDecodesServicePayloadAndPermissionPresets() throws {
        let json = #"{"authenticated":true,"mode":"multi_user","read_only":false,"principal":{"id":"p-1","kind":"service_token","display_name":"Robot","groups":[],"global_role":"reader","global_permissions":["memory_read","bundle_export"],"projects":[{"project":"alpha","role":"writer","permissions":["activity_capture"],"source":"token"}]}}"#
        let auth = try JSONDecoder.memoryLayer.decode(AuthMeResponse.self, from: Data(json.utf8))
        let permissions = EffectivePermissions(auth: auth, projectSlug: "alpha")
        XCTAssertTrue(permissions.allows(.memoryRead))
        XCTAssertTrue(permissions.allows(.activityCapture))
        XCTAssertTrue(permissions.allows(.bundleExport))
        XCTAssertFalse(permissions.allows(.memoryCurate))
        XCTAssertFalse(permissions.canAdministerAccess)

        let ungranted = EffectivePermissions(auth: auth, projectSlug: "other")
        XCTAssertTrue(ungranted.allowed.isEmpty)
    }

    func testProjectAdminIsNotGlobalAccessAdministrator() throws {
        let json = #"{"authenticated":true,"mode":"multi_user","read_only":false,"principal":{"id":"p-1","kind":"service_token","display_name":"Robot","groups":[],"projects":[{"project":"alpha","role":"admin","permissions":["auth_manage"],"source":"membership"}]}}"#
        let auth = try JSONDecoder.memoryLayer.decode(AuthMeResponse.self, from: Data(json.utf8))
        XCTAssertFalse(EffectivePermissions(auth: auth, projectSlug: "alpha").canAdministerAccess)
    }

    func testCookiesAreKeptPerConnectionJar() async {
        let one = MemoryCookieJar()
        let two = MemoryCookieJar()
        await one.store(setCookieHeaders: ["memory_session=one; Path=/; HttpOnly", "memory_csrf=csrf-one; Path=/"])
        await two.store(setCookieHeaders: ["memory_session=two; Path=/; HttpOnly", "memory_csrf=csrf-two; Path=/"])
        let oneCSRF = await one.value(named: "memory_csrf")
        let twoCSRF = await two.value(named: "memory_csrf")
        let oneCookie = await one.cookieHeader()
        let twoCookie = await two.cookieHeader()
        XCTAssertEqual(oneCSRF, "csrf-one")
        XCTAssertEqual(twoCSRF, "csrf-two")
        XCTAssertNotEqual(oneCookie, twoCookie)
    }

    func testWritesCarryScopedCookieAndCSRFHeaders() async throws {
        let profile = ConnectionProfile(name: "Local", baseURL: URL(string: "http://127.0.0.1:4250")!,
                                        projectSlug: "alpha", kind: .localSession)
        let cookies = MemoryCookieJar()
        await cookies.store(setCookieHeaders: ["memory_session=owner-session; HttpOnly", "memory_csrf=csrf-secret"])
        let transport = CapturingTransport(body: Data(#"{"ok":true}"#.utf8))
        let client = MemoryServiceClient(profile: profile, transport: transport, secrets: EmptySecretStore(), cookies: cookies)
        let route = APIRoute<OKPayload>(method: "POST", path: "/v1/query", body: Data("{}".utf8))
        _ = try await client.send(route, context: RequestContext(profile: profile))
        let savedRequest = await transport.lastRequest()
        let request = try XCTUnwrap(savedRequest)
        XCTAssertEqual(request.value(forHTTPHeaderField: "Cookie"), "memory_csrf=csrf-secret; memory_session=owner-session")
        XCTAssertEqual(request.value(forHTTPHeaderField: "x-csrf-token"), "csrf-secret")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Origin"), "http://127.0.0.1:4250")
    }

    func testProfileMismatchIsRejectedBeforeSending() async {
        let first = ConnectionProfile(name: "A", baseURL: URL(string: "http://127.0.0.1:4250")!, projectSlug: "a", kind: .localSession)
        let second = ConnectionProfile(name: "B", baseURL: URL(string: "http://127.0.0.1:4040")!, projectSlug: "b", kind: .localSession)
        let transport = CapturingTransport(body: Data(#"{"ok":true}"#.utf8))
        let client = MemoryServiceClient(profile: first, transport: transport, secrets: EmptySecretStore())
        do {
            _ = try await client.get(APIRoute<OKPayload>(path: "/healthz"), context: RequestContext(profile: second))
            XCTFail("Expected stale context rejection")
        } catch let error as MemoryServiceError {
            XCTAssertEqual(error, .staleConnection)
        } catch {
            XCTFail("Unexpected error: \(error)")
        }
        let savedRequest = await transport.lastRequest()
        XCTAssertNil(savedRequest)
    }
}

private struct OKPayload: Decodable, Sendable { let ok: Bool }

private actor CapturingTransport: HTTPTransport {
    private let body: Data
    private var request: URLRequest?

    init(body: Data) { self.body = body }

    func send(_ request: URLRequest) async throws -> HTTPReply {
        self.request = request
        return HTTPReply(statusCode: 200, headers: [:], data: body)
    }

    func cancelAll() async {}

    func lastRequest() -> URLRequest? { request }
}

private struct EmptySecretStore: SecretStore {
    func read(profileID: UUID) throws -> String? { nil }
    func write(_ secret: String, profileID: UUID) throws {}
    func remove(profileID: UUID) throws {}
}
