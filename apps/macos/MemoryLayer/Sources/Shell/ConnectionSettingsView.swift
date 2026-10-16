import SwiftUI

struct ConnectionSettingsView: View {
    @ObservedObject var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var name = "Local Memory Layer"
    @State private var baseURL = Self.suggestedURL
    @State private var project = ""
    @State private var repositoryRoot = ""
    @State private var kind: ConnectionKind = .localSession
    @State private var token = ""
    @State private var formError: String?
    @State private var pendingRemoval: ConnectionProfile?

    #if DEBUG
    private static let suggestedURL = "http://127.0.0.1:4250"
    #else
    private static let suggestedURL = "http://127.0.0.1:4040"
    #endif

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 3) {
                    Text("Connection settings").font(.title2.weight(.semibold))
                    Text("Select an existing service and project.").foregroundStyle(.secondary)
                }
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
            .padding(20)
            Divider()
            Form {
                Section("New connection") {
                    TextField("Name", text: $name)
                    TextField("Service URL", text: $baseURL)
                        .textContentType(.URL)
                        .accessibilityIdentifier("connection.serviceURL")
                    TextField("Project slug", text: $project)
                        .help("Use an existing project slug from this service.")
                        .accessibilityIdentifier("connection.projectSlug")
                    TextField("Repository root (optional)", text: $repositoryRoot)
                        .help("Local path used by repository-aware project screens.")
                    Picker("Authentication", selection: $kind) {
                        Text("Local owner session").tag(ConnectionKind.localSession)
                        Text("Remote service token").tag(ConnectionKind.serviceToken)
                    }
                    if kind == .serviceToken {
                        SecureField("Service token", text: $token)
                            .textContentType(.password)
                    } else {
                        Text("Local owner sessions are only available on loopback single-user services.")
                            .font(.callout).foregroundStyle(.secondary)
                    }
                }
                if !model.profiles.isEmpty {
                    Section("Saved connections") {
                        ForEach(model.profiles) { profile in
                            HStack(spacing: 12) {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(profile.name).fontWeight(.medium)
                                    Text("\(profile.projectSlug) · \(profile.baseURL.absoluteString)")
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Button("Connect") {
                                    Task {
                                        await model.connect(profile)
                                        if case .connected = model.connectionState { dismiss() }
                                    }
                                }
                                    .disabled(model.connectionState == .connecting)
                                Button(role: .destructive) {
                                    pendingRemoval = profile
                                } label: { Image(systemName: "trash") }
                                .help("Remove saved connection")
                            }
                        }
                    }
                }
                if let formError { Text(formError).foregroundStyle(.red) }
            }
            .formStyle(.grouped)
            .padding(.horizontal, 10)
            Divider()
            HStack {
                Spacer()
                Button("Connect") { Task { await connect() } }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.defaultAction)
                    .disabled(model.connectionState == .connecting)
            }
            .padding(16)
        }
        .frame(width: 600, height: 600)
        .alert("Remove saved connection?", isPresented: Binding(
            get: { pendingRemoval != nil },
            set: { if !$0 { pendingRemoval = nil } }
        ), presenting: pendingRemoval) { profile in
            Button("Remove connection", role: .destructive) {
                Task { await model.removeProfile(profile) }
                pendingRemoval = nil
            }
            Button("Cancel", role: .cancel) { pendingRemoval = nil }
        } message: { profile in
            Text("Remove \(profile.name) and its saved credentials from this Mac? Memories remain on the service.")
        }
        .onAppear {
            if let profile = model.activeProfile {
                name = profile.name
                baseURL = profile.baseURL.absoluteString
                project = profile.projectSlug
                repositoryRoot = profile.repositoryRoot?.path ?? ""
                kind = profile.kind
            }
        }
    }

    private func connect() async {
        guard let url = URL(string: baseURL.trimmingCharacters(in: .whitespacesAndNewlines)),
              let scheme = url.scheme?.lowercased(), ["http", "https"].contains(scheme), url.host != nil else {
            formError = "Enter a full service URL beginning with http:// or https://."
            return
        }
        guard !project.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            formError = "Enter the slug of an existing project."
            return
        }
        let candidate = ConnectionProfile(name: name, baseURL: url, projectSlug: project, kind: kind)
        if kind == .localSession && !candidate.isLoopback {
            formError = "Local owner sessions require a loopback service URL."
            return
        }
        if kind == .serviceToken && token.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            formError = "Enter a service token. It will be saved in Keychain for this connection."
            return
        }
        let profile = ConnectionProfile(
            name: name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "Memory Layer" : name,
            baseURL: url,
            projectSlug: project.trimmingCharacters(in: .whitespacesAndNewlines),
            repositoryRoot: repositoryRoot.isEmpty ? nil : URL(fileURLWithPath: repositoryRoot),
            kind: kind
        )
        await model.connect(profile, token: kind == .serviceToken ? token : nil)
        if case .connected = model.connectionState { dismiss() }
    }
}
