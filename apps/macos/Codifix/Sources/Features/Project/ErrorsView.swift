import SwiftUI

private struct ProjectErrorItem: Identifiable, Sendable {
    let id: String
    let date: Date?
    let severity: String
    let title: String
    let source: String
    let component: String
    let operation: String
    let detail: String
    let rawError: String?
    let fixHint: String?
    let doctorHint: String?
    let commandHint: String?
}

public struct ErrorsView: View {
    public let context: FeatureContext
    @State private var overview: ProjectOverviewResponse?
    @State private var items: [ProjectErrorItem] = []
    @State private var selectedID: String?
    @State private var loading = true
    @State private var refreshID = UUID()
    @State private var loadError: String?

    public init(context: FeatureContext) { self.context = context }

    public var body: some View {
        ZStack {
            if !context.permissions.allows(.memoryRead) {
                ProjectLoadingState(context: context, title: "Access restricted", message: "This identity does not have permission to inspect project diagnostics.", isLoading: false)
            } else if overview == nil && loading {
                ProjectLoadingState(context: context, title: "Loading diagnostics…", message: "Inspecting project coverage and recorded health events.", isLoading: true)
            } else if overview == nil, let loadError {
                ProjectLoadingState(context: context, title: "Could not load diagnostics", message: "The service did not return project health or activity records.", isLoading: false, error: loadError) { refreshID = UUID() }
            } else {
                content
            }
        }
        .task(id: context.revision) {
            overview = nil
            items = []
            selectedID = nil
            loadError = nil
            refreshID = UUID()
        }
        .task(id: refreshID) { await load() }
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: 14) {
            ProjectPageHeader(title: "Errors", subtitle: "\(items.count) diagnostic\(items.count == 1 ? "" : "s") · Service is \(overview?.serviceStatus ?? "connected")") {
                Button("Refresh") { refreshID = UUID() }.disabled(loading)
            }
            if let loadError {
                ProjectBanner(title: "Refresh failed", message: "Showing diagnostics from the last successful read. \(loadError)", tone: .failure)
            }
            if items.isEmpty {
                ContentUnavailableView("No project diagnostics", systemImage: "checkmark.circle", description: Text("No warnings or errors are recorded in the current project health and activity data."))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                HSplitView {
                    List(items, selection: $selectedID) { item in
                        VStack(alignment: .leading, spacing: 5) {
                            HStack(alignment: .firstTextBaseline) {
                                Text(item.title).font(.subheadline.weight(.semibold)).lineLimit(2)
                                Spacer(minLength: 4)
                                ProjectSeverityBadge(severity: item.severity)
                            }
                            Text("\(item.severity.capitalized) · \(item.component)")
                                .font(.caption).foregroundStyle(.secondary)
                            if let date = item.date {
                                Text(projectFormat(date)).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                        .padding(.vertical, 5)
                        .tag(item.id)
                    }
                    .listStyle(.plain)
                    .frame(minWidth: 245, idealWidth: 310, maxWidth: 360)

                    if let selected = items.first(where: { $0.id == selectedID }) ?? items.first {
                        issueDetail(selected)
                            .frame(minWidth: 350, maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                    }
                }
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private func issueDetail(_ item: ProjectErrorItem) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                ProjectSeverityBadge(severity: item.severity)
                Text(item.title).font(.title3.weight(.semibold)).textSelection(.enabled)
                Text("\(item.source) · \(item.component) · \(item.operation) · \(item.date.map(projectFormat) ?? "Current health")")
                    .font(.callout).foregroundStyle(.secondary).textSelection(.enabled)
                ProjectCard(title: "Diagnostic detail") {
                    Text(item.detail).font(.callout).textSelection(.enabled)
                }
                if let rawError = item.rawError {
                    ProjectCard(title: "Raw technical error") {
                        Text(rawError).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                    }
                }
                if let fixHint = item.fixHint {
                    ProjectBanner(title: "Suggested next step", message: fixHint, tone: .warning)
                }
                VStack(alignment: .leading, spacing: 8) {
                    Text("Technical detail").font(.headline)
                    Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 5) {
                        GridRow { Text("Component").foregroundStyle(.secondary); Text(item.component) }
                        GridRow { Text("Operation").foregroundStyle(.secondary); Text(item.operation) }
                        GridRow { Text("Severity").foregroundStyle(.secondary); Text(item.severity) }
                        GridRow { Text("Source").foregroundStyle(.secondary); Text(item.source) }
                    }
                    .font(.system(.caption, design: .monospaced))
                }
                if let commandHint = item.commandHint ?? item.doctorHint {
                    ProjectCard(title: "Diagnostic reference") {
                        Text(commandHint).font(.system(.callout, design: .monospaced)).textSelection(.enabled)
                        Text("Reference only. This view does not execute commands or dismiss stored diagnostics.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func load() async {
        let token = refreshID
        let revision = context.revision
        loading = true
        loadError = nil
        do {
            async let overviewResponse = ProjectService(context: context).overview()
            async let activityResponse = ProjectService(context: context).activities()
            let (loadedOverview, activityList) = try await (overviewResponse, activityResponse)
            try Task.checkCancellation()
            guard token == refreshID, revision == context.revision else { return }
            overview = loadedOverview
            let result = projectErrors(overview: loadedOverview, activities: activityList.items)
            items = result
            if selectedID.flatMap({ id in result.first(where: { $0.id == id }) }) == nil {
                selectedID = result.first?.id
            }
        } catch is CancellationError {
            return
        } catch {
            guard token == refreshID, revision == context.revision else { return }
            loadError = error.localizedDescription
        }
        if token == refreshID, revision == context.revision { loading = false }
    }
}

private struct ProjectSeverityBadge: View {
    let severity: String
    var body: some View {
        Text(severity.capitalized)
            .font(.caption.weight(.medium))
            .padding(.horizontal, 8).padding(.vertical, 3)
            .background(severity == "error" ? Color.red.opacity(0.12) : Color.orange.opacity(0.14), in: Capsule())
            .foregroundStyle(severity == "error" ? .red : .orange)
    }
}

private func projectErrors(overview: ProjectOverviewResponse, activities: [ProjectActivity]) -> [ProjectErrorItem] {
    var result: [ProjectErrorItem] = []
    if overview.missingEmbeddingChunks > 0 {
        result.append(ProjectErrorItem(
            id: "embedding-coverage-\(overview.project)", date: nil, severity: "warning",
            title: "Embedding coverage is incomplete", source: "Embeddings", component: "semantic retrieval",
            operation: "coverage inspection",
            detail: "\(overview.missingEmbeddingChunks) active memory chunk(s) do not have a current embedding. Keyword and evidence inspection remain available.",
            rawError: nil,
            fixHint: "Inspect backend coverage, then create embeddings for uncovered memories.",
            doctorHint: nil, commandHint: nil
        ))
    }
    for event in activities {
        guard let details = event.details, let type = details["type"]?.stringValue else { continue }
        if type == "diagnostic", let raw = details["diagnostic"]?.objectValue {
            result.append(ProjectErrorItem(
                id: event.id, date: event.recordedAt,
                severity: raw["severity"]?.stringValue ?? "warning",
                title: raw["message"]?.stringValue ?? event.summary,
                source: raw["source"]?.stringValue ?? event.source ?? "service",
                component: raw["component"]?.stringValue ?? "service",
                operation: raw["operation"]?.stringValue ?? "recorded diagnostic",
                detail: raw["explanation"]?.stringValue ?? event.summary,
                rawError: raw["raw_error"]?.stringValue,
                fixHint: raw["fix_hint"]?.stringValue,
                doctorHint: raw["doctor_hint"]?.stringValue,
                commandHint: raw["command_hint"]?.stringValue
            ))
        } else if type == "query", let error = details["error"]?.stringValue {
            result.append(ProjectErrorItem(
                id: event.id, date: event.recordedAt, severity: "error", title: error,
                source: event.source ?? "service", component: "query", operation: "query",
                detail: "A persisted project query failed.", rawError: error,
                fixHint: "Inspect Query or the activity detail if this repeats.",
                doctorHint: "codifix doctor", commandHint: "codifix doctor"
            ))
        } else if type == "watcher_health",
                  let health = details["health"]?.stringValue,
                  ["stale", "restarting", "failed"].contains(health) {
            result.append(ProjectErrorItem(
                id: event.id, date: event.recordedAt, severity: health == "failed" ? "error" : "warning",
                title: details["message"]?.stringValue ?? event.summary,
                source: event.source ?? "watcher", component: "watcher", operation: "heartbeat",
                detail: "Watcher reported \(health) health.", rawError: details["message"]?.stringValue,
                fixHint: "Inspect the read-only watcher presence and run codifix doctor if the issue persists.",
                doctorHint: "codifix doctor", commandHint: "codifix watcher status"
            ))
        }
    }
    return result.sorted { ($0.date ?? .distantFuture) > ($1.date ?? .distantFuture) }
}
