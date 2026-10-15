import SwiftUI

public struct AccessView: View {
    private let context: FeatureContext
    @State private var section: AccessSection = .principals
    @State private var principals: [AccessPrincipal] = []
    @State private var tokens: [AccessToken] = []
    @State private var memberships: [AccessMembership] = []
    @State private var selectedPrincipalID: String?
    @State private var selectedTokenID: String?
    @State private var selectedMembershipID: String?
    @State private var loadError: String?
    @State private var writeError: String?
    @State private var loading = false
    @State private var writing = false
    @State private var requestGate = AccessRequestGate()
    @State private var writeGeneration = UUID()
    @State private var readTask: Task<Void, Never>?
    @State private var mutationTask: Task<Void, Never>?
    @State private var presentedSheet: AccessPresentedSheet?
    @State private var pendingOneTimeSecret: String?
    @State private var oneTimeSecret: String?
    @State private var tokenToRevoke: AccessToken?
    @State private var membershipToRevoke: AccessMembership?

    public init(context: FeatureContext) { self.context = context }

    private var service: AccessService { AccessService(context: context) }
    private var permitted: Bool {
        context.permissions.globalAdministrator && context.permissions.globalAllowed.contains(.authManage)
    }
    private var canMutate: Bool { permitted && !context.permissions.readOnly }
    private var selectedPrincipal: AccessPrincipal? { principals.first { $0.id == selectedPrincipalID } }
    private var selectedToken: AccessToken? { tokens.first { $0.id == selectedTokenID } }
    private var selectedMembership: AccessMembership? { memberships.first { $0.id == selectedMembershipID } }

    public var body: some View {
        VStack(spacing: 0) {
            if permitted {
                accessContent
            } else {
                ContentUnavailableView(
                    "Access restricted",
                    systemImage: "lock.shield",
                    description: Text("Access administration requires a global administrator with auth_manage permission.")
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .task(id: context.revision) { await load(generation: beginLoad()) }
        .onChange(of: context.revision) { _, _ in clearTransientState() }
        .onDisappear { clearTransientState() }
        .sheet(item: $presentedSheet, onDismiss: finishSheetDismissal) { sheet in
            switch sheet {
            case .createToken:
                AccessTokenCreateSheet(project: context.profile.projectSlug) { request in
                    try await createToken(request)
                }
                .frame(minWidth: 420, idealWidth: 480, minHeight: 390)
            case .grantMembership:
                AccessMembershipGrantSheet(
                    principals: principals,
                    project: context.profile.projectSlug,
                    isReadOnly: !canMutate
                ) { request in
                    try await grantMembership(request)
                }
                .frame(minWidth: 440, idealWidth: 500, minHeight: 400)
            case .oneTimeSecret:
                if let oneTimeSecret {
                    AccessOneTimeSecretSheet(secret: oneTimeSecret) { clearOneTimeSecret() }
                        .frame(minWidth: 440, idealWidth: 520, minHeight: 300)
                }
            }
        }
        .confirmationDialog(
            "Revoke service token?",
            isPresented: Binding(get: { tokenToRevoke != nil }, set: { if !$0 { tokenToRevoke = nil } }),
            titleVisibility: .visible,
            presenting: tokenToRevoke
        ) { token in
            Button("Revoke \(token.name)", role: .destructive) { startMutation { await revokeToken(token) } }
            Button("Cancel", role: .cancel) { tokenToRevoke = nil }
        } message: { token in
            Text("Clients using \(token.name) will lose access immediately. This cannot be undone.")
        }
        .confirmationDialog(
            "Revoke project membership?",
            isPresented: Binding(get: { membershipToRevoke != nil }, set: { if !$0 { membershipToRevoke = nil } }),
            titleVisibility: .visible,
            presenting: membershipToRevoke
        ) { membership in
            Button("Revoke access", role: .destructive) { startMutation { await revokeMembership(membership) } }
            Button("Cancel", role: .cancel) { membershipToRevoke = nil }
        } message: { membership in
            Text("This removes the \(membership.role.title) grant for \(principalName(membership.principalID)) on \(membership.project).")
        }
    }

    private var accessContent: some View {
        VStack(spacing: 0) {
            pageHeader
            if context.permissions.readOnly {
                Label("Read-only connection. Access details are available; changes are disabled.", systemImage: "eye")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal)
                    .padding(.vertical, 8)
                    .background(Color(nsColor: .controlBackgroundColor))
            }
            Picker("Access section", selection: $section) {
                ForEach(AccessSection.allCases) { section in Text(section.rawValue).tag(section) }
            }
            .pickerStyle(.segmented)
            .padding(.horizontal)
            .padding(.vertical, 10)

            if let loadMessage = loadError {
                AccessErrorBanner(title: "Could not load access", message: loadMessage) { reload() }
            }
            if let writeMessage = writeError {
                AccessErrorBanner(title: "Access change failed", message: writeMessage) {
                    self.writeError = nil
                    reload()
                }
            }

            Group {
                switch section {
                case .principals: principalBrowser
                case .tokens: tokenBrowser
                case .memberships: membershipBrowser
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .overlay {
                if loading && principals.isEmpty && tokens.isEmpty && memberships.isEmpty {
                    ProgressView("Loading access…")
                        .padding(24)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
                }
            }
        }
        .overlay(alignment: .bottom) {
            if writing { ProgressView("Applying access change…").padding(12) }
        }
    }

    private var pageHeader: some View {
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 3) {
                Text("Access").font(.title2.weight(.bold))
                Text("Global administrator · Project roles and service tokens")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 16)
            Button { reload() } label: { Label("Refresh", systemImage: "arrow.clockwise") }
                .disabled(loading || writing)
            Button { presentedSheet = .createToken } label: { Label("Create token…", systemImage: "plus") }
                .buttonStyle(.borderedProminent)
                .disabled(!canMutate || writing)
            Button { presentedSheet = .grantMembership } label: { Label("Grant access…", systemImage: "person.badge.plus") }
                .disabled(!canMutate || writing || principals.isEmpty)
        }
        .padding()
        .background(Color(nsColor: .windowBackgroundColor))
        .overlay(alignment: .bottom) { Divider() }
    }

    private var principalBrowser: some View {
        NavigationSplitView {
            List(selection: $selectedPrincipalID) {
                ForEach(principals) { principal in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(principal.displayName).lineLimit(1)
                        Text("\(displayKind(principal.kind)) · \(principal.globalRole?.title ?? "No global role")")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .tag(Optional(principal.id))
                }
            }
            .overlay {
                if principals.isEmpty && !loading && loadError == nil {
                    ContentUnavailableView("No principals", systemImage: "person.2")
                }
            }
            .navigationTitle("Principals")
            .navigationSplitViewColumnWidth(min: 230, ideal: 290, max: 380)
        } detail: {
            if let principal = selectedPrincipal {
                principalDetail(principal)
            } else {
                ContentUnavailableView("Select a principal", systemImage: "person.crop.circle")
            }
        }
    }

    private func principalDetail(_ principal: AccessPrincipal) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(principal.displayName).font(.title2.weight(.semibold))
                    Text(principal.email ?? principal.id).font(.callout).foregroundStyle(.secondary)
                    HStack(spacing: 8) {
                        AccessBadge(text: displayKind(principal.kind), tint: .blue)
                        AccessBadge(text: "Global · \(principal.globalRole?.title ?? "None")", tint: .gray)
                    }
                }
                AccessDetailSection(title: "Identity") {
                    AccessInfoRow(label: "Principal ID", value: principal.id, monospaced: true)
                    AccessInfoRow(label: "Issuer", value: principal.issuer ?? "—")
                    AccessInfoRow(label: "Subject", value: principal.subject ?? "—", monospaced: true)
                    AccessInfoRow(label: "Global permissions", value: principal.globalPermissions.isEmpty ? "—" : principal.globalPermissions.joined(separator: ", "))
                }
                AccessDetailSection(title: "Groups") {
                    if principal.groups.isEmpty { Text("No groups").foregroundStyle(.secondary) }
                    else { ForEach(principal.groups, id: \.self) { Text($0).font(.body) } }
                }
                AccessDetailSection(title: "Project access") {
                    if principal.projects.isEmpty { Text("No project roles").foregroundStyle(.secondary) }
                    else {
                        ForEach(principal.projects, id: \.project) { grant in
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(grant.project).font(.headline)
                                    Text(grant.source.isEmpty ? "Project role" : grant.source).font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                AccessBadge(text: grant.role.title, tint: .blue)
                            }
                            if !grant.permissions.isEmpty {
                                Text(grant.permissions.joined(separator: ", ")).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
                if hasTokens(for: principal) {
                    AccessDetailSection(title: "Service tokens") {
                        ForEach(tokensFor(principal)) { token in
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(token.name)
                                    Text(token.tokenPrefix).font(.caption.monospaced()).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Text(token.revokedAt == nil ? "Active" : "Revoked").font(.caption).foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
            .frame(maxWidth: 780, alignment: .leading)
            .padding(24)
            .frame(maxWidth: .infinity, alignment: .center)
        }
        .navigationTitle("Principal")
    }

    private var tokenBrowser: some View {
        NavigationSplitView {
            List(selection: $selectedTokenID) {
                ForEach(tokens) { token in
                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(token.name).lineLimit(1)
                            Spacer(minLength: 4)
                            Circle().fill(token.revokedAt == nil ? Color.green : Color.gray).frame(width: 7, height: 7)
                        }
                        Text("\(token.tokenPrefix) · \(token.revokedAt == nil ? "Active" : "Revoked")")
                            .font(.caption.monospaced())
                            .foregroundStyle(.secondary)
                    }
                    .tag(Optional(token.id))
                }
            }
            .overlay {
                if tokens.isEmpty && !loading && loadError == nil {
                    ContentUnavailableView("No service tokens", systemImage: "key")
                }
            }
            .navigationTitle("Service tokens")
            .navigationSplitViewColumnWidth(min: 230, ideal: 290, max: 380)
        } detail: {
            if let token = selectedToken {
                tokenDetail(token)
            } else {
                ContentUnavailableView("Select a service token", systemImage: "key.horizontal")
            }
        }
    }

    private func tokenDetail(_ token: AccessToken) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(token.name).font(.title2.weight(.semibold))
                    Text(token.tokenPrefix).font(.body.monospaced()).foregroundStyle(.secondary)
                    AccessBadge(text: token.revokedAt == nil ? "Active" : "Revoked", tint: token.revokedAt == nil ? .green : .gray)
                }
                AccessDetailSection(title: "Token") {
                    AccessInfoRow(label: "Token ID", value: token.id, monospaced: true)
                    AccessInfoRow(label: "Created", value: formatAccessDate(token.createdAt))
                    AccessInfoRow(label: "Last used", value: token.lastUsedAt.map(formatAccessDate) ?? "Never")
                    AccessInfoRow(label: "Expires", value: token.expiresAt.map(formatAccessDate) ?? "Never")
                    AccessInfoRow(label: "Revoked", value: token.revokedAt.map(formatAccessDate) ?? "—")
                }
                AccessDetailSection(title: "Project grants") {
                    if token.projects.isEmpty { Text("No project grants").foregroundStyle(.secondary) }
                    else {
                        ForEach(token.projects, id: \.project) { grant in
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(grant.project).font(.headline)
                                    Text(grant.source.isEmpty ? "Project grant" : grant.source).font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                AccessBadge(text: grant.role.title, tint: .blue)
                            }
                            if !grant.permissions.isEmpty { Text(grant.permissions.joined(separator: ", ")).font(.caption).foregroundStyle(.secondary) }
                        }
                    }
                }
                if token.revokedAt == nil {
                    Button(role: .destructive) { tokenToRevoke = token } label: {
                        Label("Revoke token…", systemImage: "key.slash")
                    }
                    .disabled(!canMutate || writing)
                }
            }
            .frame(maxWidth: 780, alignment: .leading)
            .padding(24)
            .frame(maxWidth: .infinity, alignment: .center)
        }
        .navigationTitle("Service token")
    }

    private var membershipBrowser: some View {
        NavigationSplitView {
            List(selection: $selectedMembershipID) {
                ForEach(memberships) { membership in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(principalName(membership.principalID)).lineLimit(1)
                        Text("\(membership.project) · \(membership.role.title)")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .tag(Optional(membership.id))
                }
            }
            .overlay {
                if memberships.isEmpty && !loading && loadError == nil {
                    ContentUnavailableView("No explicit memberships", systemImage: "person.2.badge.key")
                }
            }
            .navigationTitle("Memberships")
            .navigationSplitViewColumnWidth(min: 230, ideal: 290, max: 380)
        } detail: {
            if let membership = selectedMembership {
                membershipDetail(membership)
            } else {
                ContentUnavailableView("Select a membership", systemImage: "person.badge.key")
            }
        }
    }

    private func membershipDetail(_ membership: AccessMembership) -> some View {
        VStack(alignment: .leading, spacing: 20) {
            VStack(alignment: .leading, spacing: 6) {
                Text(principalName(membership.principalID)).font(.title2.weight(.semibold))
                Text(membership.project).font(.callout).foregroundStyle(.secondary)
                AccessBadge(text: membership.role.title, tint: .blue)
            }
            AccessDetailSection(title: "Membership") {
                AccessInfoRow(label: "Membership ID", value: membership.id, monospaced: true)
                AccessInfoRow(label: "Principal ID", value: membership.principalID, monospaced: true)
                AccessInfoRow(label: "Source", value: membership.source)
                AccessInfoRow(label: "Granted", value: formatAccessDate(membership.createdAt))
            }
            Button(role: .destructive) { membershipToRevoke = membership } label: {
                Label("Revoke membership…", systemImage: "person.badge.minus")
            }
            .disabled(!canMutate || writing)
            Spacer()
        }
        .frame(maxWidth: 780, alignment: .leading)
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .navigationTitle("Membership")
    }

    @MainActor
    private func load(generation: AccessRequestTicket) async {
        guard permitted else { return }
        loading = true
        loadError = nil
        defer { if requestGate.accepts(generation, revision: context.revision) { loading = false } }
        do {
            async let loadedPrincipals = service.principals()
            async let loadedTokens = service.tokens()
            async let loadedMemberships = service.memberships()
            let (newPrincipals, newTokens, newMemberships) = try await (
                loadedPrincipals, loadedTokens, loadedMemberships
            )
            try Task.checkCancellation()
            guard permitted,
                  context.revision == context.requestContext.revision,
                  requestGate.accepts(generation, revision: context.revision) else { return }
            principals = newPrincipals
            tokens = newTokens
            memberships = newMemberships
            if !newPrincipals.contains(where: { $0.id == selectedPrincipalID }) { selectedPrincipalID = newPrincipals.first?.id }
            if !newTokens.contains(where: { $0.id == selectedTokenID }) { selectedTokenID = newTokens.first?.id }
            if !newMemberships.contains(where: { $0.id == selectedMembershipID }) { selectedMembershipID = newMemberships.first?.id }
        } catch is CancellationError {
            return
        } catch {
            if requestGate.accepts(generation, revision: context.revision) { loadError = error.localizedDescription }
        }
    }

    @MainActor
    private func createToken(_ request: AccessTokenCreateRequest) async throws -> String {
        guard !writing else { throw AccessFeatureError.unknownWriteOutcome }
        guard canMutate else { throw AccessFeatureError.readOnly }
        let generation = writeGeneration
        writing = true
        writeError = nil
        defer { writing = false }
        do {
            let response = try await service.createToken(request)
            guard writeGeneration == generation,
                  context.revision == context.requestContext.revision else {
                throw AccessFeatureError.unknownWriteOutcome
            }
            guard let secret = response.token, !secret.isEmpty else {
                reload()
                throw AccessFeatureError.missingOneTimeSecret
            }
            pendingOneTimeSecret = secret
            reload()
            return secret
        } catch {
            throw accessWriteFailure(error)
        }
    }

    @MainActor
    private func grantMembership(_ request: AccessMembershipGrantRequest) async throws {
        guard !writing else { throw AccessFeatureError.unknownWriteOutcome }
        guard canMutate else { throw AccessFeatureError.readOnly }
        let generation = writeGeneration
        writing = true
        writeError = nil
        defer { writing = false }
        do {
            _ = try await service.grantMembership(request)
            guard writeGeneration == generation,
                  context.revision == context.requestContext.revision else {
                throw AccessFeatureError.unknownWriteOutcome
            }
            reload()
        } catch {
            throw accessWriteFailure(error)
        }
    }

    @MainActor
    private func revokeToken(_ token: AccessToken) async {
        tokenToRevoke = nil
        guard !writing, canMutate else { return }
        let generation = writeGeneration
        writing = true
        writeError = nil
        defer { writing = false }
        do {
            _ = try await service.revokeToken(id: token.id)
            guard writeGeneration == generation,
                  context.revision == context.requestContext.revision else { return }
            reload()
        } catch {
            if writeGeneration == generation { writeError = accessWriteFailure(error).localizedDescription }
        }
    }

    @MainActor
    private func revokeMembership(_ membership: AccessMembership) async {
        membershipToRevoke = nil
        guard !writing, canMutate else { return }
        let generation = writeGeneration
        writing = true
        writeError = nil
        defer { writing = false }
        do {
            _ = try await service.revokeMembership(id: membership.id)
            guard writeGeneration == generation,
                  context.revision == context.requestContext.revision else { return }
            reload()
        } catch {
            if writeGeneration == generation { writeError = accessWriteFailure(error).localizedDescription }
        }
    }

    private func principalName(_ id: String) -> String {
        principals.first { $0.id == id }?.displayName ?? id
    }

    private func hasTokens(for principal: AccessPrincipal) -> Bool {
        tokens.contains { $0.principalID == principal.id }
    }

    private func tokensFor(_ principal: AccessPrincipal) -> [AccessToken] {
        tokens.filter { $0.principalID == principal.id }
    }

    private func displayKind(_ kind: String) -> String {
        switch kind {
        case "human_oidc": "Human"
        case "service_token": "Service token"
        case "legacy_service_token": "Legacy service token"
        case "internal": "Internal"
        default: kind.replacingOccurrences(of: "_", with: " ").capitalized
        }
    }

    private func clearTransientState() {
        writeGeneration = UUID()
        requestGate.invalidate()
        readTask?.cancel()
        readTask = nil
        mutationTask?.cancel()
        mutationTask = nil
        oneTimeSecret = nil
        pendingOneTimeSecret = nil
        presentedSheet = nil
        tokenToRevoke = nil
        membershipToRevoke = nil
        writeError = nil
    }

    @MainActor private func beginLoad() -> AccessRequestTicket {
        requestGate.begin(revision: context.revision)
    }

    @MainActor private func reload() {
        let generation = beginLoad()
        readTask?.cancel()
        readTask = Task { await load(generation: generation) }
    }

    @MainActor private func startMutation(_ operation: @escaping @MainActor () async -> Void) {
        guard !writing, mutationTask == nil else { return }
        mutationTask = Task { await operation(); mutationTask = nil }
    }

    @MainActor private func finishSheetDismissal() {
        guard let pendingOneTimeSecret else { return }
        self.pendingOneTimeSecret = nil
        oneTimeSecret = pendingOneTimeSecret
        presentedSheet = .oneTimeSecret
    }

    @MainActor private func clearOneTimeSecret() {
        oneTimeSecret = nil
        pendingOneTimeSecret = nil
        presentedSheet = nil
    }

    private func accessWriteFailure(_ error: Error) -> Error {
        if let featureError = error as? AccessFeatureError { return featureError }
        if let serviceError = error as? MemoryServiceError,
           case let .httpStatus(status, _) = serviceError,
           (400..<500).contains(status) {
            return serviceError
        }
        return AccessFeatureError.unknownWriteOutcome
    }
}

private enum AccessPresentedSheet: String, Identifiable {
    case createToken
    case grantMembership
    case oneTimeSecret
    var id: String { rawValue }
}
