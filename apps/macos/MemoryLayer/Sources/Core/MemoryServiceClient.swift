import Foundation
import Security

public struct HTTPReply: Sendable {
    public let statusCode: Int
    public let headers: [String: [String]]
    public let data: Data

    public init(statusCode: Int, headers: [String: [String]], data: Data) {
        self.statusCode = statusCode
        self.headers = headers
        self.data = data
    }
}

public protocol HTTPTransport: Sendable {
    func send(_ request: URLRequest) async throws -> HTTPReply
    func cancelAll() async
}

private final class RedirectBlocker: NSObject, URLSessionTaskDelegate, @unchecked Sendable {
    func urlSession(_ session: URLSession, task: URLSessionTask,
                   willPerformHTTPRedirection response: HTTPURLResponse,
                   newRequest request: URLRequest,
                   completionHandler: @escaping (URLRequest?) -> Void) {
        completionHandler(nil)
    }
}

public actor URLSessionTransport: HTTPTransport {
    private let session: URLSession
    private let delegate: RedirectBlocker
    private var tasks: [UUID: URLSessionDataTask] = [:]

    public init(timeout: TimeInterval = 30) {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = timeout
        configuration.timeoutIntervalForResource = timeout
        configuration.httpCookieStorage = nil
        configuration.httpShouldSetCookies = false
        configuration.urlCache = nil
        delegate = RedirectBlocker()
        session = URLSession(configuration: configuration, delegate: delegate, delegateQueue: nil)
    }

    public func send(_ request: URLRequest) async throws -> HTTPReply {
        let id = UUID()
        let reply = try await withTaskCancellationHandler(operation: {
            try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<HTTPReply, Error>) in
                let task = session.dataTask(with: request) { data, response, error in
                    if let error { continuation.resume(throwing: error); return }
                    guard let response = response as? HTTPURLResponse, let data else {
                        continuation.resume(throwing: MemoryServiceError.invalidResponse)
                        return
                    }
                    var headers: [String: [String]] = [:]
                    for (key, value) in response.allHeaderFields {
                        let name = String(describing: key).lowercased()
                        headers[name, default: []].append(String(describing: value))
                    }
                    if let cookies = response.value(forHTTPHeaderField: "Set-Cookie") {
                        headers["set-cookie"] = Self.splitSetCookie(cookies)
                    }
                    continuation.resume(returning: HTTPReply(statusCode: response.statusCode, headers: headers, data: data))
                }
                tasks[id] = task
                task.resume()
            }
        }, onCancel: {
            Task { await self.cancel(taskID: id) }
        })
        defer { tasks[id] = nil }
        return reply
    }

    public func cancelAll() {
        let active = tasks.values
        tasks.removeAll()
        active.forEach { $0.cancel() }
    }

    private func cancel(taskID: UUID) {
        tasks.removeValue(forKey: taskID)?.cancel()
    }

    private static func splitSetCookie(_ value: String) -> [String] {
        guard let regex = try? NSRegularExpression(pattern: #",\s*(?=[^;,\s]+=)"#) else { return [value] }
        let nsValue = value as NSString
        let matches = regex.matches(in: value, range: NSRange(location: 0, length: nsValue.length))
        var result: [String] = []
        var start = 0
        for match in matches {
            result.append(nsValue.substring(with: NSRange(location: start, length: match.range.location - start)).trimmingCharacters(in: .whitespaces))
            start = NSMaxRange(match.range)
        }
        result.append(nsValue.substring(from: start).trimmingCharacters(in: .whitespaces))
        return result
    }
}

public enum MemoryServiceError: Error, LocalizedError, Sendable, Equatable {
    case invalidBaseURL
    case invalidRoute(String)
    case invalidResponse
    case unauthorized
    case forbidden
    case httpStatus(Int, String)
    case staleConnection
    case localBootstrapUnavailable
    case keychain(OSStatus)

    public var errorDescription: String? {
        switch self {
        case .invalidBaseURL: "Enter a valid HTTP or HTTPS service URL."
        case let .invalidRoute(path): "Invalid API route: \(path)"
        case .invalidResponse: "The service returned an invalid response."
        case .unauthorized: "The service did not accept this connection's credentials."
        case .forbidden: "This account does not have permission for this action."
        case let .httpStatus(code, message): message.isEmpty ? "Memory Layer returned HTTP \(code)." : "HTTP \(code): \(message)"
        case .staleConnection: "The connection changed before this response completed."
        case .localBootstrapUnavailable: "Local owner sessions require a loopback single-user service."
        case let .keychain(status): "Could not access this connection's Keychain item (\(status))."
        }
    }
}

public struct APIRoute<Response: Decodable & Sendable>: Sendable {
    public let method: String
    public let path: String
    public let query: [URLQueryItem]
    public let headers: [String: String]
    public let body: Data?

    public init(method: String = "GET", path: String, query: [URLQueryItem] = [],
                headers: [String: String] = [:], body: Data? = nil) {
        self.method = method
        self.path = path
        self.query = query
        self.headers = headers
        self.body = body
    }
}

public enum URLPathSegment {
    public static func escape(_ value: String) -> String {
        let unreservedASCII = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~")
        return value.addingPercentEncoding(withAllowedCharacters: unreservedASCII) ?? value
    }
}

public protocol SecretStore: Sendable {
    func read(profileID: UUID) throws -> String?
    func write(_ secret: String, profileID: UUID) throws
    func remove(profileID: UUID) throws
}

public struct KeychainSecretStore: SecretStore {
    private let service: String

    public init(service: String = "app.memorylayer.macOS") { self.service = service }

    public func read(profileID: UUID) throws -> String? {
        var query = baseQuery(profileID)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess, let data = result as? Data,
              let secret = String(data: data, encoding: .utf8) else { throw MemoryServiceError.keychain(status) }
        return secret
    }

    public func write(_ secret: String, profileID: UUID) throws {
        let query = baseQuery(profileID)
        let data = Data(secret.utf8)
        let update = SecItemUpdate(query as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        if update == errSecItemNotFound {
            var insert = query
            insert[kSecValueData as String] = data
            insert[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
            let status = SecItemAdd(insert as CFDictionary, nil)
            guard status == errSecSuccess else { throw MemoryServiceError.keychain(status) }
        } else if update != errSecSuccess {
            throw MemoryServiceError.keychain(update)
        }
    }

    public func remove(profileID: UUID) throws {
        let status = SecItemDelete(baseQuery(profileID) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw MemoryServiceError.keychain(status) }
    }

    private func baseQuery(_ profileID: UUID) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: profileID.uuidString]
    }
}

/// Cookie state belongs to exactly one connection profile. It is deliberately
/// not backed by shared HTTPCookieStorage, whose domain matching ignores ports.
public actor MemoryCookieJar {
    private struct StoredCookie {
        let value: String
        let expiresAt: Date?
    }
    private var values: [String: StoredCookie] = [:]

    public init() {}

    public func store(setCookieHeaders: [String]) {
        for header in setCookieHeaders {
            guard let pair = header.split(separator: ";", maxSplits: 1).first,
                  let separator = pair.firstIndex(of: "=") else { continue }
            let name = String(pair[..<separator]).trimmingCharacters(in: .whitespaces)
            let value = String(pair[pair.index(after: separator)...])
            guard !name.isEmpty else { continue }
            let attributes = header.split(separator: ";").dropFirst().map { $0.trimmingCharacters(in: .whitespaces) }
            if attributes.contains(where: {
                guard $0.lowercased().hasPrefix("max-age=") else { return false }
                return (Int($0.split(separator: "=", maxSplits: 1).last ?? "") ?? 1) <= 0
            }) {
                values.removeValue(forKey: name)
                continue
            }
            let maxAge = attributes.first(where: { $0.lowercased().hasPrefix("max-age=") })
                .flatMap { Int($0.split(separator: "=", maxSplits: 1).last ?? "") }
            values[name] = StoredCookie(value: value, expiresAt: maxAge.map { Date().addingTimeInterval(TimeInterval($0)) })
        }
    }

    public func cookieHeader() -> String? {
        values = values.filter { $0.value.expiresAt.map { $0 > .now } ?? true }
        guard !values.isEmpty else { return nil }
        return values.keys.sorted().map { "\($0)=\(values[$0]?.value ?? "")" }.joined(separator: "; ")
    }

    public func value(named name: String) -> String? {
        guard let cookie = values[name], cookie.expiresAt.map({ $0 > .now }) ?? true else {
            values.removeValue(forKey: name)
            return nil
        }
        return cookie.value
    }
    public func clear() { values.removeAll() }
}

public actor MemoryServiceClient {
    public let profile: ConnectionProfile
    private let transport: any HTTPTransport
    private let secrets: any SecretStore
    private let cookies: MemoryCookieJar
    private var generation = 0

    public init(profile: ConnectionProfile, transport: any HTTPTransport = URLSessionTransport(),
                secrets: any SecretStore = KeychainSecretStore(), cookies: MemoryCookieJar = MemoryCookieJar()) {
        self.profile = profile
        self.transport = transport
        self.secrets = secrets
        self.cookies = cookies
    }

    public func get<T: Decodable & Sendable>(_ route: APIRoute<T>, context: RequestContext) async throws -> T {
        try await send(route, context: context)
    }

    public func send<T: Decodable & Sendable>(_ route: APIRoute<T>, context: RequestContext) async throws -> T {
        let startGeneration = generation
        let reply = try await perform(route, context: context)
        try Task.checkCancellation()
        guard startGeneration == generation else { throw MemoryServiceError.staleConnection }
        if profile.kind == .localSession, let cookieHeaders = reply.headers["set-cookie"] {
            await cookies.store(setCookieHeaders: cookieHeaders)
        }
        try Task.checkCancellation()
        guard startGeneration == generation else { throw MemoryServiceError.staleConnection }
        try validate(reply)
        do { return try JSONDecoder.memoryLayer.decode(T.self, from: reply.data) }
        catch { throw MemoryServiceError.httpStatus(reply.statusCode, "The service response did not match the expected data.") }
    }

    public func sendData(method: String, path: String, query: [URLQueryItem] = [],
                         headers: [String: String] = [:], body: Data? = nil,
                         context: RequestContext) async throws -> HTTPReply {
        let startGeneration = generation
        let reply = try await perform(method: method, path: path, query: query, headers: headers, body: body, context: context)
        try Task.checkCancellation()
        guard startGeneration == generation else { throw MemoryServiceError.staleConnection }
        if profile.kind == .localSession, let cookieHeaders = reply.headers["set-cookie"] {
            await cookies.store(setCookieHeaders: cookieHeaders)
        }
        try Task.checkCancellation()
        guard startGeneration == generation else { throw MemoryServiceError.staleConnection }
        try validate(reply)
        return reply
    }

    public func refreshIdentity(context: RequestContext) async throws -> AuthMeResponse {
        if profile.kind == .serviceToken, try secrets.read(profileID: profile.id) == nil {
            throw MemoryServiceError.unauthorized
        }
        let identity = try await send(APIRoute<AuthMeResponse>(path: "/v1/auth/me"), context: context)
        if profile.kind == .localSession, identity.mode != .singleUser {
            throw MemoryServiceError.localBootstrapUnavailable
        }
        return identity
    }

    public func bootstrapLocalSession(context: RequestContext) async throws -> AuthMeResponse {
        guard profile.kind == .localSession, profile.isLoopback else { throw MemoryServiceError.localBootstrapUnavailable }
        let current = try await refreshIdentity(context: context)
        guard current.mode == .singleUser else { throw MemoryServiceError.localBootstrapUnavailable }
        let _: BootstrapResponse = try await send(APIRoute(method: "POST", path: "/v1/auth/session/bootstrap"), context: context)
        return try await refreshIdentity(context: context)
    }

    public func cancelAll() async {
        generation += 1
        await cookies.clear()
        await transport.cancelAll()
    }

    public func hasStoredToken() throws -> Bool { try secrets.read(profileID: profile.id) != nil }
    public func storeToken(_ token: String) throws { try secrets.write(token, profileID: profile.id) }
    public func removeToken() throws { try secrets.remove(profileID: profile.id) }

    private func perform<T: Decodable & Sendable>(_ route: APIRoute<T>, context: RequestContext) async throws -> HTTPReply {
        guard context.profileID == profile.id else { throw MemoryServiceError.staleConnection }
        return try await perform(method: route.method, path: route.path, query: route.query,
                                 headers: route.headers, body: route.body, context: context)
    }

    private func perform(method: String, path: String, query: [URLQueryItem],
                         headers: [String: String], body: Data?, context: RequestContext) async throws -> HTTPReply {
        try Task.checkCancellation()
        guard context.profileID == profile.id else { throw MemoryServiceError.staleConnection }
        let url = try profile.endpoint(path: path, query: query)
        var request = URLRequest(url: url)
        request.httpMethod = method.uppercased()
        request.httpBody = body
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        for (name, value) in headers { request.setValue(value, forHTTPHeaderField: name) }
        if body != nil, request.value(forHTTPHeaderField: "Content-Type") == nil {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        if profile.kind == .serviceToken, let token = try secrets.read(profileID: profile.id) {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        if profile.kind == .localSession, let cookie = await cookies.cookieHeader() {
            request.setValue(cookie, forHTTPHeaderField: "Cookie")
        }
        let isWrite = !["GET", "HEAD", "OPTIONS"].contains(request.httpMethod ?? "GET")
        if profile.kind == .localSession, isWrite, let csrf = await cookies.value(named: "memory_csrf") {
            request.setValue(csrf, forHTTPHeaderField: "x-csrf-token")
            if let scheme = profile.baseURL.scheme, let host = profile.baseURL.host {
                let port = profile.baseURL.port.map { ":\($0)" } ?? ""
                request.setValue("\(scheme)://\(host)\(port)", forHTTPHeaderField: "Origin")
            }
        }
        try Task.checkCancellation()
        let reply = try await transport.send(request)
        try Task.checkCancellation()
        return reply
    }

    private func validate(_ reply: HTTPReply) throws {
        guard !(200..<300).contains(reply.statusCode) else { return }
        switch reply.statusCode {
        case 401: throw MemoryServiceError.unauthorized
        case 403: throw MemoryServiceError.forbidden
        default:
            let message = (try? JSONSerialization.jsonObject(with: reply.data) as? [String: Any])?["message"] as? String ?? ""
            throw MemoryServiceError.httpStatus(reply.statusCode, message)
        }
    }
}

private struct BootstrapResponse: Decodable, Sendable { let authenticated: Bool }
