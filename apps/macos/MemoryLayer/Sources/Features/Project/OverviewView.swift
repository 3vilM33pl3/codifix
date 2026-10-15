import SwiftUI

public struct OverviewView: View {
    public let context: FeatureContext
    @State private var overview: ProjectOverviewResponse?
    @State private var activities: [ProjectActivity] = []
    @State private var loadError: String?
    @State private var loadErrorUnavailable = false
    @State private var loading = true
    @State private var refreshID = UUID()
    @State private var operation: String?
    @State private var operationError: String?
    @State private var operationResult: String?
    @State private var maintenancePresented = false
    @State private var archiveConfirmation = false
    @State private var archiveTargetContext: FeatureContext?

    public init(context: FeatureContext) { self.context = context }

    public var body: some View {
        ZStack {
            if !context.permissions.allows(.memoryRead) {
                ProjectLoadingState(context: context, title: "Access restricted", message: "This identity does not have permission to read the project overview.", isLoading: false)
            } else if overview == nil && loading {
                ProjectLoadingState(context: context, title: "Loading project overview…", message: "Fetching project health and memory metrics from the selected service.", isLoading: true)
            } else if overview == nil, let loadError {
                ProjectLoadingState(context: context, title: loadErrorUnavailable ? "Service unavailable" : "Could not load project overview", message: "The selected service did not return project overview data.", isLoading: false, error: loadError) { refreshID = UUID() }
            } else if let overview {
                content(overview)
            }
        }
        .task(id: context.revision) {
            overview = nil
            activities = []
            loadError = nil
            operation = nil
            operationError = nil
            operationResult = nil
            refreshID = UUID()
        }
        .task(id: refreshID) { await load() }
        .sheet(isPresented: $maintenancePresented) { maintenanceSheet }
        .confirmationDialog("Archive low-value memories?", isPresented: $archiveConfirmation, titleVisibility: .visible) {
            Button("Archive low-value memories", role: .destructive) {
                let snapshot = archiveTargetContext ?? context
                Task { await perform(.archive, using: snapshot) }
            }
            Button("Cancel", role: .cancel) { archiveTargetContext = nil }
        } message: {
            Text("Eligible low-value memories in \(archiveTargetContext?.profile.projectSlug ?? context.profile.projectSlug) will move to archived status. They can be inspected using the archived filter.")
        }
    }

    @ViewBuilder private func content(_ overview: ProjectOverviewResponse) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            ProjectPageHeader(title: "Project overview", subtitle: "\(context.profile.projectSlug) · \(context.profile.name) · \(overview.serviceStatus)") {
                HStack(spacing: 8) {
                    Button("Refresh") { refreshID = UUID() }.disabled(loading || operation != nil)
                    Button("Maintenance…") { maintenancePresented = true }
                        .disabled(context.permissions.readOnly || !context.permissions.allows(.memoryCurate))
                }
            }

            if let loadError {
                ProjectBanner(title: "Refresh failed", message: "Showing the last overview loaded. \(loadError)", tone: .failure) {
                    refreshID = UUID()
                }
            }
            if let operationResult {
                ProjectBanner(title: operation ?? "Maintenance completed", message: operationResult, tone: .success)
            }
            if let operationError {
                ProjectBanner(title: "Maintenance failed", message: operationError, tone: .failure)
            }

            LazyVGrid(columns: [GridItem(.adaptive(minimum: 156), spacing: 10)], alignment: .leading, spacing: 10) {
                ProjectMetricCard(value: "\(overview.activeMemories)", title: "Active memories", note: "\(overview.archivedMemories) archived")
                ProjectMetricCard(value: confidencePercent(overview), title: "Confidence", note: "\(overview.highConfidenceMemories) high-confidence memories")
                ProjectMetricCard(value: coveragePercent(overview), title: "Embedding coverage", note: "\(overview.freshEmbeddingChunks) of \(overview.embeddingChunksTotal) chunks in active space")
                ProjectMetricCard(value: "\(overview.pendingReplacementProposals)", title: "Pending review", note: "Replacement proposals")
            }

            HSplitView {
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        activityCard
                        memoryCompositionCard(overview)
                    }
                    .padding(.vertical, 1)
                }
                .frame(minWidth: 410, maxWidth: .infinity, maxHeight: .infinity)

                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        healthCard(overview)
                        maintenanceCard
                    }
                    .padding(.vertical, 1)
                }
                .frame(minWidth: 290, idealWidth: 360, maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    private var activityCard: some View {
        ProjectCard(title: "Recent activity") {
            if activities.isEmpty {
                Text("No recent project activity.").foregroundStyle(.secondary).font(.callout)
            } else {
                Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 0) {
                    GridRow {
                        Text("Event").foregroundStyle(.secondary)
                        Text("When").foregroundStyle(.secondary)
                    }
                    .font(.caption.weight(.medium))
                    ForEach(activities.prefix(6)) { activity in
                        GridRow(alignment: .top) {
                            Text(activity.summary).lineLimit(2)
                            Text(projectFormat(activity.recordedAt)).foregroundStyle(.secondary).lineLimit(1)
                        }
                        .font(.callout)
                        .padding(.vertical, 8)
                        .overlay(alignment: .bottom) { Divider() }
                    }
                }
            }
        }
    }

    private func memoryCompositionCard(_ overview: ProjectOverviewResponse) -> some View {
        ProjectCard(title: "Memory composition") {
            if overview.memoryTypeBreakdown.isEmpty {
                Text("No memory type data.").foregroundStyle(.secondary).font(.callout)
            } else {
                Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 0) {
                    GridRow {
                        Text("Type").foregroundStyle(.secondary)
                        Text("Count").foregroundStyle(.secondary)
                    }
                    .font(.caption.weight(.medium))
                    ForEach(overview.memoryTypeBreakdown) { item in
                        GridRow {
                            Text(item.name)
                            Text("\(item.count)").foregroundStyle(.secondary)
                        }
                        .font(.callout)
                        .padding(.vertical, 8)
                        .overlay(alignment: .bottom) { Divider() }
                    }
                }
            }
        }
    }

    private func healthCard(_ overview: ProjectOverviewResponse) -> some View {
        ProjectCard(title: "Service health") {
            healthRow("Service", value: overview.serviceStatus)
            healthRow("Database", value: overview.databaseStatus)
            healthRow("Watchers", value: "\(overview.watchers?.activeCount ?? 0) active · \(overview.watchers?.unhealthyCount ?? 0) unhealthy")
            healthRow("Captures", value: "\(overview.uncuratedRawCaptures) uncurated")
            healthRow("Backend", value: overview.activeEmbeddingModel.map { "\(overview.activeEmbeddingProvider ?? "Configured") · \($0)" } ?? "No active embedding backend")
            healthRow("Policy", value: overview.automation?.mode.capitalized ?? "Balanced")
        }
    }

    private func healthRow(_ label: String, value: String) -> some View {
        LabeledContent(label, value: value).font(.callout)
    }

    private var maintenanceCard: some View {
        ProjectCard(title: "Maintenance") {
            HStack(spacing: 8) {
                maintenanceButton(.curate)
                maintenanceButton(.reindex)
            }
            HStack(spacing: 8) {
                maintenanceButton(.reembed)
                Button("Archive low-value memories") {
                    archiveTargetContext = context
                    archiveConfirmation = true
                }
                .buttonStyle(.bordered).tint(.red)
                .disabled(!canProjectWrite(.memoryCurate) || operation != nil)
            }
            Text("Archive low-value affects selected low-value memories, not the project.")
                .font(.caption).foregroundStyle(.secondary)
            if let operation { ProgressView(operation).controlSize(.small) }
            if let operationError { Text(operationError).foregroundStyle(.red).font(.callout).textSelection(.enabled) }
        }
    }

    private func maintenanceButton(_ action: ProjectAction) -> some View {
        Button(action.title) { Task { await perform(action, using: context) } }
            .buttonStyle(.bordered)
            .disabled(!canProjectWrite(.memoryCurate) || operation != nil)
    }

    private var maintenanceSheet: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Project maintenance").font(.title2.weight(.bold))
            Text("Actions apply to memory on \(context.profile.projectSlug).")
                .foregroundStyle(.secondary)
            HStack {
                maintenanceButton(.curate)
                maintenanceButton(.reindex)
                maintenanceButton(.reembed)
            }
            Button("Archive low-value memories") {
                archiveTargetContext = context
                maintenancePresented = false
                archiveConfirmation = true
            }
            .buttonStyle(.bordered).tint(.red)
            .disabled(!canProjectWrite(.memoryCurate) || operation != nil)
            Text("Archive targets low-value memories; it does not archive the project.")
                .font(.callout).foregroundStyle(.secondary)
            if let operationResult {
                ProjectBanner(title: operation ?? "Maintenance completed", message: operationResult, tone: .success)
            }
            if let operationError {
                ProjectBanner(title: "Maintenance failed", message: operationError, tone: .failure)
            }
            HStack {
                Spacer()
                Button("Done") { maintenancePresented = false }.keyboardShortcut(.defaultAction)
            }
        }
        .padding(22)
        .frame(minWidth: 520, idealWidth: 560)
    }

    private func load() async {
        let token = refreshID
        let revision = context.revision
        loading = true
        loadError = nil
        loadErrorUnavailable = false
        do {
            async let overviewResponse = ProjectService(context: context).overview()
            async let activityResponse = ProjectService(context: context).activities(limit: 6)
            let (loadedOverview, loadedActivities) = try await (overviewResponse, activityResponse)
            try Task.checkCancellation()
            guard token == refreshID, revision == context.revision else { return }
            overview = loadedOverview
            activities = loadedActivities.items
        } catch is CancellationError {
            return
        } catch {
            guard token == refreshID, revision == context.revision else { return }
            loadError = error.localizedDescription
            loadErrorUnavailable = projectIsUnavailable(error)
        }
        if token == refreshID, revision == context.revision { loading = false }
    }

    private func perform(_ action: ProjectAction, using snapshot: FeatureContext) async {
        guard operation == nil else { return }
        operation = action.title
        operationError = nil
        operationResult = nil
        let token = refreshID
        do {
            let service = ProjectService(context: snapshot)
            let result: String
            switch action {
            case .curate:
                let response = try await service.curate()
                result = "Curated \(response.inputCount) capture(s), produced \(response.outputCount) memory update(s), and created \(response.proposalCount) proposal(s)."
            case .reindex:
                let response = try await service.reindex()
                result = "Reindexed \(response.reindexedEntries) memory entries."
            case .reembed:
                let response = try await service.reembed()
                result = "Created \(response.reembeddedChunks) embedding chunk(s)."
            case .archive:
                let response = try await service.archiveLowValueMemories()
                result = "Archived \(response.archivedCount) low-value memories."
                archiveTargetContext = nil
            }
            try Task.checkCancellation()
            guard token == refreshID, snapshot.revision == context.revision else { return }
            operationResult = result
            refreshID = UUID()
        } catch is CancellationError {
            return
        } catch {
            guard snapshot.revision == context.revision else { return }
            operationError = error.localizedDescription
        }
        if snapshot.revision == context.revision { operation = nil }
    }

    private func canProjectWrite(_ permission: MemoryPermission) -> Bool {
        !context.permissions.readOnly && context.permissions.allows(permission)
    }

    private func confidencePercent(_ overview: ProjectOverviewResponse) -> String {
        let total = overview.activeMemories
        guard total > 0 else { return "—" }
        return "\(Int((Double(overview.highConfidenceMemories) / Double(total) * 100).rounded()))%"
    }

    private func coveragePercent(_ overview: ProjectOverviewResponse) -> String {
        guard overview.embeddingChunksTotal > 0 else { return "—" }
        return "\(Int((Double(overview.freshEmbeddingChunks) / Double(overview.embeddingChunksTotal) * 100).rounded()))%"
    }
}
