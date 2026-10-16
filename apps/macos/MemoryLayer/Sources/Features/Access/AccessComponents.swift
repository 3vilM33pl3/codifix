import AppKit
import SwiftUI

struct AccessDetailSection<Content: View>: View {
    let title: String
    let content: Content

    init(title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title).font(.headline)
            VStack(alignment: .leading, spacing: 10) { content }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(14)
                .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 10))
        }
    }
}

struct AccessInfoRow: View {
    let label: String
    let value: String
    var monospaced = false

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(label).foregroundStyle(.secondary).frame(width: 145, alignment: .leading)
            Text(value)
                .font(monospaced ? .body.monospaced() : .body)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

struct AccessBadge: View {
    enum Tint { case blue, green, gray }
    let text: String
    let tint: Tint

    var body: some View {
        Text(text)
            .font(.caption.weight(.medium))
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .foregroundStyle(foreground)
            .background(background, in: RoundedRectangle(cornerRadius: 6))
    }

    private var foreground: Color {
        switch tint {
        case .blue: .blue
        case .green: .green
        case .gray: .secondary
        }
    }

    private var background: Color {
        switch tint {
        case .blue: Color.blue.opacity(0.10)
        case .green: Color.green.opacity(0.10)
        case .gray: Color.gray.opacity(0.12)
        }
    }
}

struct AccessErrorBanner: View {
    let title: String
    let message: String
    let retry: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.headline)
                Text(message).font(.callout).textSelection(.enabled)
            }
            Spacer()
            Button("Refresh", action: retry)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: 8))
        .padding(.horizontal)
        .padding(.top, 8)
    }
}

struct AccessTokenCreateSheet: View {
    @Environment(\.dismiss) private var dismiss
    let project: String
    let create: (AccessTokenCreateRequest) async throws -> String
    @State private var name = ""
    @State private var targetProject = ""
    @State private var role: AccessRole = .reader
    @State private var expiration: TokenExpiration = .never
    @State private var submitting = false
    @State private var errorMessage: String?
    @State private var submissionTask: Task<Void, Never>?
    @State private var operationGeneration = UUID()

    init(project: String, create: @escaping (AccessTokenCreateRequest) async throws -> String) {
        self.project = project
        self.create = create
        _targetProject = State(initialValue: project)
    }

    private var valid: Bool { !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !targetProject.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Create service token").font(.title2.weight(.semibold))
                    Text("The raw secret is shown once and cannot be recovered later.").foregroundStyle(.secondary)
                }
                Spacer()
                Button { dismiss() } label: { Image(systemName: "xmark") }
                    .buttonStyle(.borderless)
                    .accessibilityLabel("Close")
                    .disabled(submitting)
            }
            Form {
                TextField("Name", text: $name)
                    .accessibilityIdentifier("access.tokenName")
                TextField("Project", text: $targetProject)
                    .accessibilityIdentifier("access.tokenProject")
                Picker("Role", selection: $role) {
                    ForEach(AccessRole.serviceTokenRoles) { Text($0.title).tag($0) }
                }
                Picker("Expiration", selection: $expiration) {
                    ForEach(TokenExpiration.allCases) { Text($0.title).tag($0) }
                }
            }
            .disabled(submitting)
            if let errorMessage { Text(errorMessage).font(.callout).foregroundStyle(.red).textSelection(.enabled) }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction).disabled(submitting)
                Button(submitting ? "Creating…" : "Create token") { submissionTask = Task { await submit() } }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!valid || submitting)
                    .accessibilityIdentifier("access.submitToken")
            }
        }
        .padding(22)
        .interactiveDismissDisabled(submitting)
        .onDisappear {
            operationGeneration = UUID()
            submissionTask?.cancel()
            submissionTask = nil
        }
    }

    @MainActor private func submit() async {
        guard valid, !submitting else { return }
        let generation = operationGeneration
        submitting = true
        errorMessage = nil
        defer { submitting = false }
        let request = AccessTokenCreateRequest(
            name: name.trimmingCharacters(in: .whitespacesAndNewlines),
            project: targetProject.trimmingCharacters(in: .whitespacesAndNewlines),
            role: role,
            expiresInSeconds: expiration.seconds
        )
        do {
            _ = try await create(request)
            guard generation == operationGeneration else { return }
            dismiss()
        } catch {
            if generation == operationGeneration { errorMessage = error.localizedDescription }
        }
    }
}

private enum TokenExpiration: String, CaseIterable, Identifiable {
    case never, oneDay, sevenDays, thirtyDays, ninetyDays
    var id: String { rawValue }
    var title: String {
        switch self {
        case .never: "Never"
        case .oneDay: "1 day"
        case .sevenDays: "7 days"
        case .thirtyDays: "30 days"
        case .ninetyDays: "90 days"
        }
    }
    var seconds: Int? {
        switch self {
        case .never: nil
        case .oneDay: 86_400
        case .sevenDays: 604_800
        case .thirtyDays: 2_592_000
        case .ninetyDays: 7_776_000
        }
    }
}

struct AccessMembershipGrantSheet: View {
    @Environment(\.dismiss) private var dismiss
    let principals: [AccessPrincipal]
    let project: String
    let isReadOnly: Bool
    let grant: (AccessMembershipGrantRequest) async throws -> Void
    @State private var principalID = ""
    @State private var targetProject = ""
    @State private var role: AccessRole = .reader
    @State private var submitting = false
    @State private var errorMessage: String?
    @State private var submissionTask: Task<Void, Never>?
    @State private var operationGeneration = UUID()

    init(principals: [AccessPrincipal], project: String, isReadOnly: Bool,
         grant: @escaping (AccessMembershipGrantRequest) async throws -> Void) {
        self.principals = principals
        self.project = project
        self.isReadOnly = isReadOnly
        self.grant = grant
        _principalID = State(initialValue: principals.first?.id ?? "")
        _targetProject = State(initialValue: project)
    }

    private var selectedPrincipal: AccessPrincipal? { principals.first { $0.id == principalID } }
    private var roles: [AccessRole] {
        selectedPrincipal?.kind == "service_token"
            ? AccessRole.serviceTokenRoles : AccessRole.allCases
    }
    private var valid: Bool { !principalID.isEmpty && !targetProject.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Grant project access").font(.title2.weight(.semibold))
                    Text("Explicit grants combine with configured identity group mappings.").foregroundStyle(.secondary)
                }
                Spacer()
                Button { dismiss() } label: { Image(systemName: "xmark") }
                    .buttonStyle(.borderless)
                    .accessibilityLabel("Close")
                    .disabled(submitting)
            }
            Form {
                Picker("Principal", selection: $principalID) {
                    ForEach(principals) { Text($0.displayName).tag($0.id) }
                }
                .accessibilityIdentifier("access.membershipPrincipal")
                TextField("Project", text: $targetProject)
                    .accessibilityIdentifier("access.membershipProject")
                Picker("Role", selection: $role) {
                    ForEach(roles) { Text($0.title).tag($0) }
                }
            }
            .disabled(submitting)
            if let errorMessage { Text(errorMessage).font(.callout).foregroundStyle(.red).textSelection(.enabled) }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction).disabled(submitting)
                Button(submitting ? "Granting…" : "Grant access") { submissionTask = Task { await submit() } }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!valid || isReadOnly || submitting)
                    .accessibilityIdentifier("access.submitMembership")
            }
        }
        .padding(22)
        .onChange(of: principalID) { _, _ in if !roles.contains(role) { role = .operatorRole } }
        .onDisappear {
            operationGeneration = UUID()
            submissionTask?.cancel()
            submissionTask = nil
        }
    }

    @MainActor private func submit() async {
        guard valid, !isReadOnly, !submitting else { return }
        let generation = operationGeneration
        submitting = true
        errorMessage = nil
        defer { submitting = false }
        do {
            try await grant(AccessMembershipGrantRequest(
                principalID: principalID,
                project: targetProject.trimmingCharacters(in: .whitespacesAndNewlines),
                role: role
            ))
            guard generation == operationGeneration else { return }
            dismiss()
        } catch {
            if generation == operationGeneration { errorMessage = error.localizedDescription }
        }
    }
}

struct AccessOneTimeSecretSheet: View {
    @Environment(\.dismiss) private var dismiss
    let secret: String
    let onDismiss: () -> Void
    @State private var copied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            Label("Service token created", systemImage: "checkmark.circle.fill")
                .font(.title2.weight(.semibold))
                .foregroundStyle(.green)
            Text("Copy this secret to a secure place now. It will not be shown again after you close this window.")
                .foregroundStyle(.secondary)
            Text(secret)
                .font(.body.monospaced())
                .textSelection(.enabled)
                .accessibilityIdentifier("access.oneTimeSecret")
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
            HStack {
                if copied { Label("Copied", systemImage: "checkmark").foregroundStyle(.green) }
                Spacer()
                Button("Dismiss") { close() }.keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier("access.dismissSecret")
                Button(copied ? "Copy again" : "Copy token") { copy() }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .accessibilityIdentifier("access.copyToken")
            }
        }
        .padding(22)
        .onDisappear(perform: onDismiss)
    }

    private func copy() {
        NSPasteboard.general.clearContents()
        copied = NSPasteboard.general.setString(secret, forType: .string)
    }

    private func close() {
        onDismiss()
        dismiss()
    }
}

func formatAccessDate(_ date: Date) -> String {
    date.formatted(date: .abbreviated, time: .shortened)
}
