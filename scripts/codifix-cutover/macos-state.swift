#!/usr/bin/env swift
// SPDX-License-Identifier: AGPL-3.0-or-later
// One-release, additive transfer. Run while both app/CLI generations are stopped.
// No deletion, network access, secret output, fallback, or replacement of conflicts.
import Foundation
import Security
import CryptoKit

struct Failure: Error { let message: String }
func require(_ condition: Bool, _ message: String) throws {
    if !condition { throw Failure(message: message) }
}
func query(_ service: String, _ account: String? = nil) -> [String: Any] {
    var result: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                               kSecAttrService as String: service]
    if let account { result[kSecAttrAccount as String] = account }
    return result
}
func accounts(_ service: String) throws -> [String] {
    var q = query(service)
    q[kSecMatchLimit as String] = kSecMatchLimitAll
    q[kSecReturnAttributes as String] = true
    var result: CFTypeRef?
    let status = SecItemCopyMatching(q as CFDictionary, &result)
    if status == errSecItemNotFound { return [] }
    try require(status == errSecSuccess, "Keychain metadata unavailable (\(status))")
    guard let rows = result as? [[String: Any]] else { throw Failure(message: "Invalid Keychain metadata") }
    let names = try rows.map { row -> String in
        guard let account = row[kSecAttrAccount as String] as? String, !account.isEmpty else {
            throw Failure(message: "Missing account binding")
        }
        return account
    }
    try require(Set(names).count == names.count, "Ambiguous account binding")
    return names.sorted()
}
func read(_ service: String, _ account: String) throws -> Data? {
    var q = query(service, account)
    q[kSecReturnData as String] = true
    q[kSecMatchLimit as String] = kSecMatchLimitOne
    var result: CFTypeRef?
    let status = SecItemCopyMatching(q as CFDictionary, &result)
    if status == errSecItemNotFound { return nil }
    try require(status == errSecSuccess, "Keychain read unavailable (\(status))")
    guard let data = result as? Data else { throw Failure(message: "Invalid credential data") }
    return data
}
func run() throws {
    let args = Array(CommandLine.arguments.dropFirst())
    try require(args.first == "--inspect" || args.first == "--apply", "Usage: macos-state.swift --inspect|--apply [--company-origin EXACT_ORIGIN]")
    try require(args.count == 1 || (args.count == 3 && args[1] == "--company-origin"), "Unexpected arguments")
    let apply = args[0] == "--apply"
    var services = [("app.memorylayer.macOS", "dev.codifix.macos")]
    if args.count == 3 {
        let origin = args[2]
        guard let url = URL(string: origin), let host = url.host else { throw Failure(message: "Invalid company origin") }
        try require(url.user == nil && url.password == nil && url.query == nil && url.fragment == nil && url.path.isEmpty,
                    "Use the exact canonical company origin without a trailing slash")
        try require(url.scheme == "https" || (url.scheme == "http" && ["localhost", "127.0.0.1", "[::1]"].contains(host)), "Company origin must be HTTPS or loopback HTTP")
        let hash = SHA256.hash(data: Data(origin.utf8)).map { String(format: "%02x", $0) }.joined()
        for kind in ["company-device", "company-agent", "agent-settings"] {
            services.append(("net.memory-layer.\(kind):\(hash)", "dev.codifix.\(kind):\(hash)"))
        }
    }
    let defaults = UserDefaults.standard
    let old = defaults.persistentDomain(forName: "app.memorylayer.macOS") ?? [:]
    var target = defaults.persistentDomain(forName: "dev.codifix.macos") ?? [:]
    let profiles = old["memory.layer.connections.v1"] as? Data
    if let profiles {
        guard let rows = try JSONSerialization.jsonObject(with: profiles) as? [[String: Any]],
              rows.allSatisfy({ ($0["id"] as? String).flatMap(UUID.init(uuidString:)) != nil }) else {
            throw Failure(message: "Invalid profile UUIDs")
        }
        if let existing = target["codifix.connections.v1"] {
            try require((existing as? Data) == profiles, "Conflicting destination profiles; no overwrite")
        }
        print("Profiles ready: \(rows.count); original UUIDs and bytes preserved")
    } else { print("No stored app profiles") }
    // Preflight all destination conflicts before adding any item. Values remain in memory.
    var pending: [(String, String, Data)] = []
    for (source, destination) in services {
        let names = try accounts(source)
        print("Scoped Keychain service: \(source.split(separator: ":")[0]); items: \(names.count)")
        if apply {
            for account in names {
                guard let bytes = try read(source, account) else { throw Failure(message: "Source changed during transfer") }
                if let existing = try read(destination, account) {
                    try require(existing == bytes, "Conflicting destination credential; no overwrite")
                } else { pending.append((destination, account, bytes)) }
            }
        }
    }
    if apply {
        for (service, account, bytes) in pending {
            var q = query(service, account)
            q[kSecValueData as String] = bytes
            q[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
            let status = SecItemAdd(q as CFDictionary, nil)
            try require(status == errSecSuccess || status == errSecDuplicateItem, "Keychain copy failed (\(status)); originals retained; retry is safe")
            try require(try read(service, account) == bytes, "Destination verification failed; originals retained")
        }
        if let profiles {
            target["codifix.connections.v1"] = profiles
            defaults.setPersistentDomain(target, forName: "dev.codifix.macos")
            try require(defaults.synchronize(), "Preferences synchronization failed")
            try require(defaults.persistentDomain(forName: "dev.codifix.macos")?["codifix.connections.v1"] as? Data == profiles, "Preferences verification failed")
        }
        print("Transfer verified; copied \(pending.count) credentials; originals retained. New signed clients may request Keychain access.")
    } else { print("Inspection only; no preferences or Keychain writes; credential values were not read") }
}
do { try run() } catch let failure as Failure {
    fputs("\(failure.message)\n", stderr); exit(1)
} catch {
    fputs("Transfer failed; originals retained; no secret diagnostics emitted\n", stderr); exit(1)
}
