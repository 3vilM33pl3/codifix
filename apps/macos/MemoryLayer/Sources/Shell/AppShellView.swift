import SwiftUI

struct AppShellView: View {
    @ObservedObject var model: AppModel
    @State private var presentsConnectionSettings = false

    private var destinationsByGroup: [(MemoryFeatureGroup, [AppDestination])] {
        MemoryFeatureGroup.allCases.map { group in (group, AppDestination.allCases.filter { $0.group == group }) }
    }

    var body: some View {
        NavigationSplitView {
            List(selection: $model.selectedDestination) {
                ForEach(destinationsByGroup, id: \.0.id) { group, destinations in
                    Section(group.rawValue) {
                        ForEach(destinations) { destination in
                            Label(destination.rawValue, systemImage: destination.symbol)
                                .tag(destination)
                        }
                    }
                }
            }
            .listStyle(.sidebar)
            .navigationSplitViewColumnWidth(min: 175, ideal: 205, max: 260)
            .safeAreaInset(edge: .bottom, spacing: 0) {
                Button { presentsConnectionSettings = true } label: {
                    Label("Connection settings", systemImage: "slider.horizontal.3")
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .buttonStyle(.plain)
                .padding(.horizontal, 14)
                .padding(.vertical, 12)
                .background(.bar)
            }
        } detail: {
            Group {
                if let context = model.featureContext {
                    DestinationContentView(destination: model.selectedDestination, context: context)
                        .id(model.revision)
                } else {
                    ConnectionWelcomeView(state: model.connectionState) {
                        presentsConnectionSettings = true
                    }
                }
            }
            .navigationTitle(model.selectedDestination.rawValue)
            .toolbar {
                ToolbarItem(placement: .principal) {
                    HStack(spacing: 7) {
                        Text(model.activeProfile?.projectSlug ?? "No project")
                            .font(.system(size: 12, weight: .medium))
                        if let profile = model.activeProfile {
                            Text("·").foregroundStyle(.secondary)
                            Text(profile.name).font(.system(size: 12)).foregroundStyle(.secondary)
                        }
                    }
                    .lineLimit(1)
                }
                ToolbarItemGroup(placement: .primaryAction) {
                    connectionBadge
                    Button("Refresh", systemImage: "arrow.clockwise") {
                        Task { await reloadIdentity() }
                    }
                    .disabled(model.activeProfile == nil)
                    Button("Inspector", systemImage: "sidebar.right") {
                        model.isInspectorPresented.toggle()
                    }
                    .help("Show connection and permission details")
                }
            }
            .inspector(isPresented: $model.isInspectorPresented) {
                ConnectionInspector(context: model.featureContext, state: model.connectionState)
                    .inspectorColumnWidth(min: 250, ideal: 300, max: 420)
            }
        }
        .navigationSplitViewStyle(.balanced)
        .sheet(isPresented: $presentsConnectionSettings) {
            ConnectionSettingsView(model: model)
        }
        .onReceive(NotificationCenter.default.publisher(for: .showConnectionSettings)) { _ in
            presentsConnectionSettings = true
        }
        .onReceive(NotificationCenter.default.publisher(for: .selectDestination)) { notification in
            if let destination = notification.object as? AppDestination { model.selectedDestination = destination }
        }
        .alert("Memory Layer", isPresented: Binding(
            get: { model.alertMessage != nil },
            set: { if !$0 { model.alertMessage = nil } }
        )) {
            Button("OK") { model.alertMessage = nil }
        } message: {
            Text(model.alertMessage ?? "")
        }
        .task { await model.loadProfiles() }
    }

    @ViewBuilder private var connectionBadge: some View {
        switch model.connectionState {
        case .connected:
            Label("Connected", systemImage: "checkmark.circle.fill").labelStyle(.titleAndIcon).foregroundStyle(.green)
        case .connecting:
            Label("Connecting", systemImage: "arrow.triangle.2.circlepath").foregroundStyle(.secondary)
        case .failed:
            Label("Unavailable", systemImage: "exclamationmark.circle.fill").foregroundStyle(.orange)
        case .disconnected:
            Label("Disconnected", systemImage: "circle").foregroundStyle(.secondary)
        }
    }

    private func reloadIdentity() async {
        await model.refreshIdentity()
    }
}

struct DestinationContentView: View {
    let destination: AppDestination
    let context: FeatureContext

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: destination.symbol).font(.system(size: 30)).foregroundStyle(.secondary)
            Text(destination.rawValue).font(.title2.weight(.semibold))
            Text("This screen is being connected to the Memory Layer service.")
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement(children: .combine)
    }
}

private struct ConnectionWelcomeView: View {
    let state: ConnectionState
    let onConfigure: () -> Void

    var body: some View {
        ContentUnavailableView {
            Label("Choose a Memory Layer connection", systemImage: "externaldrive.connected.to.line.below")
        } description: {
            Text("Connect to an existing service and project. The app will show the selected service and project before loading its data.")
        } actions: {
            Button("Review connections…", action: onConfigure).buttonStyle(.borderedProminent)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct ConnectionInspector: View {
    let context: FeatureContext?
    let state: ConnectionState

    var body: some View {
        Form {
            Section("Service") {
                LabeledContent("Status", value: statusText)
                if let context {
                    LabeledContent("Connection", value: context.profile.name)
                    LabeledContent("Service URL", value: context.profile.baseURL.absoluteString)
                    LabeledContent("Project", value: context.profile.projectSlug)
                    LabeledContent("Repository", value: context.profile.repositoryRoot?.path ?? "Not set")
                } else {
                    Text("No connection selected").foregroundStyle(.secondary)
                }
            }
            if let context {
                Section("Access") {
                    LabeledContent("Principal", value: context.auth?.principal.displayName ?? "Unknown")
                    LabeledContent("Role", value: context.permissions.role.rawValue)
                    LabeledContent("Mode", value: context.auth?.mode.rawValue ?? "Unknown")
                    if context.permissions.readOnly { Label("Service is read-only", systemImage: "lock.fill") }
                }
            }
        }
        .formStyle(.grouped)
        .padding(.top, 8)
    }

    private var statusText: String {
        switch state {
        case .disconnected: "Disconnected"
        case .connecting: "Connecting…"
        case .connected: "Connected"
        case let .failed(message): message
        }
    }
}
