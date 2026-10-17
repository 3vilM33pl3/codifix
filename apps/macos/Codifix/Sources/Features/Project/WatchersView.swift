import SwiftUI

public struct WatchersView: View {
    public let context: FeatureContext
    @State private var overview: ProjectOverviewResponse?
    @State private var loading = true
    @State private var refreshID = UUID()
    @State private var loadError: String?
    @State private var selectedWatcherID: String?

    public init(context: FeatureContext) { self.context = context }

    public var body: some View {
        ZStack {
            if !context.permissions.allows(.memoryRead) {
                ProjectLoadingState(context: context, title: "Access restricted", message: "This identity does not have permission to read watcher status for this project.", isLoading: false)
            } else if overview == nil && loading {
                ProjectLoadingState(context: context, title: "Loading watcher presence…", message: "Reading the latest project watcher heartbeat summary.", isLoading: true)
            } else if overview == nil, let loadError {
                ProjectLoadingState(context: context, title: "Could not load watcher presence", message: "The service did not return project health data.", isLoading: false, error: loadError) { refreshID = UUID() }
            } else if let overview {
                content(overview)
            }
        }
        .task(id: context.revision) {
            overview = nil
            selectedWatcherID = nil
            loadError = nil
            refreshID = UUID()
        }
        .task(id: refreshID) { await load() }
    }

    private func content(_ overview: ProjectOverviewResponse) -> some View {
        let summary = overview.watchers
        let watchers = summary?.watchers ?? []
        let selected = watchers.first(where: { $0.id == selectedWatcherID }) ?? watchers.first
        return ScrollView(.vertical) {
          VStack(alignment: .leading, spacing: 14) {
            ProjectPageHeader(title: "Watchers", subtitle: "Repository presence and heartbeat monitoring") {
                Button("Refresh") { refreshID = UUID() }.disabled(loading)
            }
            if let loadError {
                ProjectBanner(title: "Refresh failed", message: "Showing the last watcher snapshot. \(loadError)", tone: .failure)
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 150), spacing: 10)], alignment: .leading, spacing: 10) {
                ProjectMetricCard(value: "\(summary?.activeCount ?? 0)", title: "Active", note: "Managed watcher presence")
                ProjectMetricCard(value: "\(summary?.unhealthyCount ?? 0)", title: "Unhealthy", note: summary?.unhealthyCount == 0 ? "No recovery needed" : "Review watcher details")
                ProjectMetricCard(value: summary?.lastHeartbeatAt.map(relativeHeartbeat) ?? "—", title: "Last heartbeat", note: "Project presence summary")
                ProjectMetricCard(value: "\(summary?.staleAfterSeconds ?? 0)s", title: "Stale threshold", note: "Current service policy")
            }

            if watchers.isEmpty {
                ContentUnavailableView("No active watchers", systemImage: "eye.slash", description: Text("The service reports no watcher presence for this project."))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                Table(watchers, selection: $selectedWatcherID) {
                    TableColumn("Repository") { watcher in Text(URL(fileURLWithPath: watcher.repoRoot).lastPathComponent.isEmpty ? watcher.repoRoot : URL(fileURLWithPath: watcher.repoRoot).lastPathComponent).lineLimit(1) }
                    TableColumn("Host") { watcher in Text(watcher.hostname).lineLimit(1) }
                    TableColumn("Mode") { watcher in Text(watcher.mode.capitalized) }
                    TableColumn("Health") { watcher in Label(watcher.health.capitalized, systemImage: healthSymbol(watcher.health)).foregroundStyle(healthColor(watcher.health)) }
                    TableColumn("Ownership") { watcher in Text(watcher.managedByService ? "Managed" : "External") }
                }
                .frame(minHeight: 150, idealHeight: 220, maxHeight: 260)

                if let selected {
                    watcherDetail(selected)
                }
                ProjectBanner(title: "Read-only presence", message: "Start and manage watchers through the Codifix CLI.", tone: .info)
            }
          }
          .padding(16)
          .frame(maxWidth: .infinity, alignment: .topLeading)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private func watcherDetail(_ watcher: ProjectWatcher) -> some View {
        ProjectCard(title: "\(watcher.project) · \(watcher.hostname)") {
            Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 8) {
                GridRow { Text("Heartbeat").foregroundStyle(.secondary); Text("\(relativeHeartbeat(watcher.lastHeartbeatAt)) ago") }
                GridRow { Text("Started").foregroundStyle(.secondary); Text(projectFormat(watcher.startedAt)) }
                GridRow { Text("Host service").foregroundStyle(.secondary); Text(watcher.hostServiceID) }
                GridRow { Text("Associated agent").foregroundStyle(.secondary); Text(watcher.agentCLI ?? watcher.agentSessionID ?? "None") }
                GridRow { Text("Restarts").foregroundStyle(.secondary); Text("\(watcher.restartAttemptCount)") }
                GridRow { Text("Repository").foregroundStyle(.secondary); Text(watcher.repoRoot).textSelection(.enabled) }
                GridRow { Text("Process identifier").foregroundStyle(.secondary); Text("\(watcher.pid)") }
                if let agentPID = watcher.agentPID {
                    GridRow { Text("Agent process").foregroundStyle(.secondary); Text("\(agentPID)") }
                }
                if let lastAttempt = watcher.lastRestartAttemptAt {
                    GridRow { Text("Last restart attempt").foregroundStyle(.secondary); Text(projectFormat(lastAttempt)) }
                }
            }
            .font(.callout)
        }
    }

    private func load() async {
        let token = refreshID
        let revision = context.revision
        loading = true
        loadError = nil
        do {
            let response = try await ProjectService(context: context).overview()
            try Task.checkCancellation()
            guard token == refreshID, revision == context.revision else { return }
            overview = response
            if selectedWatcherID.flatMap({ id in response.watchers?.watchers.first(where: { $0.id == id }) }) == nil {
                selectedWatcherID = response.watchers?.watchers.first?.id
            }
        } catch is CancellationError {
            return
        } catch {
            guard token == refreshID, revision == context.revision else { return }
            loadError = error.localizedDescription
        }
        if token == refreshID, revision == context.revision { loading = false }
    }

    private func relativeHeartbeat(_ date: Date) -> String {
        let seconds = max(0, Int(Date.now.timeIntervalSince(date)))
        if seconds < 60 { return "\(seconds)s" }
        if seconds < 3_600 { return "\(seconds / 60)m" }
        return "\(seconds / 3_600)h"
    }

    private func healthSymbol(_ health: String) -> String {
        switch health {
        case "healthy": "checkmark.circle.fill"
        case "restarting": "arrow.clockwise.circle.fill"
        default: "exclamationmark.triangle.fill"
        }
    }

    private func healthColor(_ health: String) -> Color {
        switch health {
        case "healthy": .green
        case "restarting": .orange
        default: .red
        }
    }
}
