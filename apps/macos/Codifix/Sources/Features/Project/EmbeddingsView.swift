import SwiftUI

public struct EmbeddingsView: View {
    public let context: FeatureContext
    @State private var response: ProjectEmbeddingBackendsResponse?
    @State private var overview: ProjectOverviewResponse?
    @State private var selectedBackendID: String?
    @State private var loading = true
    @State private var busy = false
    @State private var refreshID = UUID()
    @State private var loadError: String?
    @State private var operationError: String?
    @State private var operationResult: String?
    @State private var operationTitle: String?
    @State private var operationTicket = UUID()

    public init(context: FeatureContext) { self.context = context }

    private var selectedBackend: ProjectEmbeddingBackend? {
        response?.backends.first(where: { $0.name == selectedBackendID })
    }

    public var body: some View {
        ZStack {
            if !context.permissions.allows(.memoryRead) {
                ProjectLoadingState(context: context, title: "Access restricted", message: "This identity does not have permission to inspect embedding backend status.", isLoading: false)
            } else if response == nil && loading {
                ProjectLoadingState(context: context, title: "Loading embedding status…", message: "Checking configured providers, readiness, and coverage for this project.", isLoading: true)
            } else if response == nil, let loadError {
                ProjectLoadingState(context: context, title: "Could not load embeddings", message: "The service did not return backend readiness or coverage.", isLoading: false, error: loadError) { refreshID = UUID() }
            } else {
                contentView
            }
        }
        .task(id: context.revision) {
            response = nil
            overview = nil
            selectedBackendID = nil
            loadError = nil
            operationError = nil
            operationResult = nil
            operationTitle = nil
            busy = false
            refreshID = UUID()
        }
        .task(id: refreshID) { await load() }
    }

    private var contentView: some View {
        VStack(alignment: .leading, spacing: 14) {
            ProjectPageHeader(title: "Embeddings", subtitle: "Backend readiness, coverage and maintenance") {
                HStack(spacing: 8) {
                    Button("Refresh") { refreshID = UUID() }.disabled(loading || busy)
                    Button("Reindex all") { Task { await runProjectOperation(.reindex) } }
                        .disabled(!canProjectWrite || busy)
                    Button("Re-embed all") { Task { await runProjectOperation(.reembed) } }
                        .disabled(!canProjectWrite || busy || !hasReadyBackend)
                }
            }

            if let loadError {
                ProjectBanner(title: "Refresh failed", message: "Showing the last backend status. \(loadError)", tone: .failure) { refreshID = UUID() }
            }
            if let operationResult {
                ProjectBanner(title: operationTitle ?? "Embedding operation completed", message: operationResult, tone: .success)
            }
            if let operationError {
                ProjectBanner(title: operationTitle ?? "Embedding operation failed", message: operationError, tone: .failure)
            }

            if let response, response.backends.isEmpty {
                ContentUnavailableView("No embedding backends configured", systemImage: "square.3.layers.3d.slash", description: Text("No embedding providers are configured on this service."))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let response {
                HSplitView {
                    List(response.backends, selection: $selectedBackendID) { backend in
                        VStack(alignment: .leading, spacing: 5) {
                            HStack {
                                Text(backend.name).font(.subheadline.weight(.semibold))
                                Spacer()
                                if backend.active { Text("Active search").font(.caption).foregroundStyle(Color.accentColor) }
                            }
                            Text("\(backend.provider) · \(backend.model)").font(.caption).foregroundStyle(.secondary).lineLimit(1)
                            HStack(spacing: 8) {
                                Text(backend.ready ? "Ready" : "Not ready").foregroundStyle(backend.ready ? .green : .orange)
                                Text("·")
                                Text("\(backend.projectMemoryCount ?? 0) memories · \(backend.projectChunkCount ?? 0) chunks")
                            }
                            .font(.caption).foregroundStyle(.secondary)
                        }
                        .padding(.vertical, 5)
                        .tag(backend.id)
                    }
                    .listStyle(.plain)
                    .frame(minWidth: 265, idealWidth: 320, maxWidth: 370)

                    backendDetail(response: response)
                        .frame(minWidth: 400, maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                }
            }
            if busy, let operationTitle {
                ProgressView(operationTitle).controlSize(.small)
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    @ViewBuilder private func backendDetail(response: ProjectEmbeddingBackendsResponse) -> some View {
        if let backend = selectedBackend {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    VStack(alignment: .leading, spacing: 5) {
                        HStack(spacing: 8) {
                            readinessBadge(backend)
                            if backend.active { Text("Active search").font(.caption.weight(.medium)).padding(.horizontal, 8).padding(.vertical, 4).background(Color.accentColor.opacity(0.1), in: Capsule()).foregroundStyle(Color.accentColor) }
                        }
                        Text(backend.name).font(.title3.weight(.semibold))
                        Text("Configured backend · Settings are managed outside this view").font(.callout).foregroundStyle(.secondary)
                    }

                    ProjectCard(title: nil) {
                        LabeledContent("Provider", value: backend.provider)
                        LabeledContent("Model", value: backend.model.isEmpty ? "Not configured" : backend.model)
                        LabeledContent("Memory coverage", value: "\(backend.projectMemoryCount.map(String.init) ?? "—") / \(overview.map { String($0.activeMemories) } ?? "—")")
                        LabeledContent("Chunk coverage", value: "\(backend.projectChunkCount.map(String.init) ?? "—") / \(overview.map { String($0.embeddingChunksTotal) } ?? "—")")
                        LabeledContent("Base URL", value: backend.baseURL.isEmpty ? "Default provider endpoint" : backend.baseURL).textSelection(.enabled)
                    }

                    Toggle("Automatic embedding creation", isOn: Binding(
                        get: { backend.createEnabled },
                        set: { enabled in Task { await setCreation(backend, enabled: enabled) } }
                    ))
                    .disabled(!canManageGlobal || busy)

                    HStack(spacing: 8) {
                        Button(backend.active ? "Turn off search" : "Activate search") {
                            Task { await toggleSearch(backend) }
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(!canManageGlobal || busy || (!backend.active && !backend.ready))

                        Button("Create embeddings") { Task { await runProjectOperation(.reembed, backend: backend) } }
                            .buttonStyle(.bordered)
                            .disabled(!canProjectWrite || busy || !backend.ready)

                        Button("Reindex backend") { Task { await runProjectOperation(.reindex, backend: backend) } }
                            .buttonStyle(.bordered)
                            .disabled(!canProjectWrite || busy || !backend.ready)
                    }
                    if !backend.ready {
                        ProjectBanner(title: "Backend is not ready", message: "Search activation and embedding maintenance are unavailable until this provider is ready.", tone: .warning)
                        if canManageGlobal {
                            Text("Automatic creation can still be changed while this backend is unavailable.")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                    } else if !canManageGlobal {
                        Text(context.permissions.readOnly ? "This connection is read-only." : "Global embedding settings require the embeddings_manage permission.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    if !canProjectWrite {
                        Text(context.permissions.readOnly ? "Project maintenance is disabled for this read-only connection." : "Project embedding maintenance requires memory_curate permission.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    Text("Search activation, automatic creation, and coverage maintenance are separate controls.")
                        .font(.caption).foregroundStyle(.secondary)
                }
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        } else {
            ContentUnavailableView("Select a backend", systemImage: "square.3.layers.3d", description: Text("Choose a configured backend to review its readiness, coverage, and controls."))
        }
    }

    private func readinessBadge(_ backend: ProjectEmbeddingBackend) -> some View {
        Text(backend.ready ? "Ready" : "Not ready")
            .font(.caption.weight(.medium))
            .padding(.horizontal, 8).padding(.vertical, 4)
            .background((backend.ready ? Color.green : Color.orange).opacity(0.12), in: Capsule())
            .foregroundStyle(backend.ready ? .green : .orange)
    }

    private var canProjectWrite: Bool {
        !context.permissions.readOnly && context.permissions.allows(.memoryCurate)
    }

    private var canManageGlobal: Bool {
        !context.permissions.readOnly && context.permissions.globalAllowed.contains(.embeddingsManage)
    }

    private var hasReadyBackend: Bool { response?.backends.contains(where: \.ready) == true }

    private func load() async {
        let token = refreshID
        let revision = context.revision
        loading = true
        loadError = nil
        do {
            let result = try await ProjectService(context: context).embeddingBackends()
            try Task.checkCancellation()
            guard token == refreshID, revision == context.revision else { return }
            response = result
            if selectedBackendID == nil || !result.backends.contains(where: { $0.name == selectedBackendID }) {
                selectedBackendID = result.active ?? result.backends.first?.name
            }
            loading = false
            do {
                let loadedOverview = try await ProjectService(context: context).overview()
                try Task.checkCancellation()
                guard token == refreshID, revision == context.revision else { return }
                overview = loadedOverview
            } catch is CancellationError {
                return
            } catch {
                guard token == refreshID, revision == context.revision else { return }
                // Backend coverage remains useful when this optional denominator read is unavailable.
                loadError = error.localizedDescription
            }
        } catch is CancellationError {
            return
        } catch {
            guard token == refreshID, revision == context.revision else { return }
            loadError = error.localizedDescription
        }
        if token == refreshID, revision == context.revision { loading = false }
    }

    private func toggleSearch(_ backend: ProjectEmbeddingBackend) async {
        guard !busy, canManageGlobal, backend.active || backend.ready else { return }
        await runSettingsOperation(title: backend.active ? "Disabling search" : "Activating search") {
            if backend.active {
                return try await ProjectService(context: context).deactivateEmbeddingBackend()
            }
            return try await ProjectService(context: context).activateEmbeddingBackend(backend.name)
        }
    }

    private func setCreation(_ backend: ProjectEmbeddingBackend, enabled: Bool) async {
        guard !busy, canManageGlobal else { return }
        await runSettingsOperation(title: enabled ? "Enabling automatic creation" : "Disabling automatic creation") {
            try await ProjectService(context: context).setEmbeddingCreation(backend: backend.name, enabled: enabled)
        }
    }

    private func runSettingsOperation(
        title: String,
        operation body: @escaping @Sendable () async throws -> ProjectEmbeddingBackendsResponse
    ) async {
        guard !busy else { return }
        busy = true
        operationTicket = UUID()
        let ticket = operationTicket
        let revision = context.revision
        operationTitle = title
        operationError = nil
        operationResult = nil
        do {
            let result = try await body()
            try Task.checkCancellation()
            guard ticket == operationTicket, revision == context.revision else { return }
            response = result
            operationResult = "Backend settings were updated by the service."
        } catch is CancellationError {
            return
        } catch {
            guard ticket == operationTicket, revision == context.revision else { return }
            operationError = error.localizedDescription
        }
        if ticket == operationTicket, revision == context.revision { busy = false }
    }

    private func runProjectOperation(_ action: ProjectAction, backend: ProjectEmbeddingBackend? = nil) async {
        guard !busy, canProjectWrite else { return }
        if action == .reindex {
            if let backend, !backend.ready { return }
        } else if !(backend?.ready ?? hasReadyBackend) {
            return
        }
        busy = true
        operationTicket = UUID()
        let ticket = operationTicket
        let revision = context.revision
        operationTitle = action == .reindex ? "Reindexing" : "Creating embeddings"
        operationError = nil
        operationResult = nil
        do {
            let service = ProjectService(context: context)
            let result: String
            if action == .reindex {
                let response = try await service.reindex(backend: backend?.name)
                result = "Reindexed \(response.reindexedEntries) memory entries\(backend.map { " for \($0.name)" } ?? "")."
            } else {
                let response = try await service.reembed(backend: backend?.name)
                result = "Created \(response.reembeddedChunks) embedding chunk(s)\(backend.map { " for \($0.name)" } ?? "")."
            }
            try Task.checkCancellation()
            guard ticket == operationTicket, revision == context.revision else { return }
            operationResult = result
            refreshID = UUID()
        } catch is CancellationError {
            return
        } catch {
            guard ticket == operationTicket, revision == context.revision else { return }
            operationError = error.localizedDescription
        }
        if ticket == operationTicket, revision == context.revision { busy = false }
    }
}
