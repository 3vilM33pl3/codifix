import SwiftUI

struct MemoriesView: View {
    let context: FeatureContext
    @FocusState private var searchFocused: Bool

    @State private var response: ProjectMemoriesResponse?
    @State private var selectedMemoryID: String?
    @State private var selectedMemory: MemoryEntry?
    @State private var history: MemoryHistoryResponse?
    @State private var searchText = ""
    @State private var tagText = ""
    @State private var statusFilter = "active"
    @State private var typeFilter = "all"
    @State private var detailTab: DetailTab = .content
    @State private var isLoading = true
    @State private var isDetailLoading = false
    @State private var isDeleting = false
    @State private var errorMessage: String?
    @State private var detailError: String?
    @State private var deleteError: String?
    @State private var presentsDeleteConfirmation = false
    @State private var listGate = KnowledgeRequestGate()
    @State private var detailGate = KnowledgeRequestGate()
    @State private var pendingDelete: (id: String, title: String)?
    @State private var refreshID = 0

    private enum DetailTab: String, CaseIterable, Identifiable {
        case content = "Content"
        case provenance = "Provenance"
        case history = "History"
        var id: String { rawValue }
    }

    private var service: KnowledgeService { KnowledgeService(context: context) }

    private var filteredItems: [MemoryListItem] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let tag = tagText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return (response?.items ?? []).filter { item in
            guard statusFilter == "all" || item.status == statusFilter else { return false }
            guard typeFilter == "all" || item.memoryType == typeFilter else { return false }
            guard query.isEmpty || "\(item.summary) \(item.preview)".lowercased().contains(query) else { return false }
            guard tag.isEmpty || item.tags.contains(where: { $0.localizedCaseInsensitiveContains(tag) }) else { return false }
            return true
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            KnowledgePageHeader(
                title: "Memories",
                subtitle: response.map { "\($0.total) memories · \($0.project)" } ?? "Canonical records and their evidence",
                actionTitle: "Refresh"
            ) {
                refreshID += 1
            }

            if !context.permissions.allows(.memoryRead) {
                KnowledgeStateView(kind: .restricted, context: context, retry: nil)
            } else if isLoading && response == nil {
                KnowledgeStateView(kind: .loading, context: context, retry: nil)
            } else if let errorMessage, response == nil {
                KnowledgeStateView(kind: stateKind(for: errorMessage), context: context, retry: { refreshID += 1 })
            } else if let response, response.items.isEmpty {
                KnowledgeStateView(kind: .empty, context: context, retry: { refreshID += 1 })
            } else {
                if let errorMessage {
                    Label("Refresh failed: \(errorMessage)", systemImage: "exclamationmark.triangle.fill")
                        .font(.callout).foregroundStyle(.orange)
                        .padding(10)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Color.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: 8))
                }
                GeometryReader { geometry in
                    if geometry.size.width < 720 {
                        VStack(spacing: 12) {
                            memoryList
                                .frame(height: min(350, max(250, geometry.size.height * 0.42)))
                            memoryDetail
                        }
                    } else {
                        HSplitView {
                            memoryList.frame(minWidth: 255, idealWidth: min(330, geometry.size.width * 0.31), maxWidth: 390)
                            memoryDetail.frame(minWidth: 420, maxWidth: .infinity, maxHeight: .infinity)
                        }
                    }
                }
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task(id: "\(context.revision)-\(refreshID)") { await loadMemories() }
        .task(id: "\(selectedMemoryID ?? "none")-\(detailTab.rawValue)") {
            guard let id = selectedMemoryID else {
                detailGate.invalidate()
                selectedMemory = nil
                history = nil
                return
            }
            await loadSelectedMemory(id: id)
        }
        .onChange(of: selectedMemoryID) { _, _ in
            detailGate.invalidate()
            selectedMemory = nil
            history = nil
            detailError = nil
        }
        .onChange(of: searchText) { reconcileSelection() }
        .onChange(of: tagText) { reconcileSelection() }
        .onChange(of: statusFilter) { reconcileSelection() }
        .onChange(of: typeFilter) { reconcileSelection() }
        .onReceive(NotificationCenter.default.publisher(for: .focusMemorySearch)) { _ in
            searchFocused = true
        }
        .alert("Delete memory?", isPresented: $presentsDeleteConfirmation) {
            Button("Cancel", role: .cancel) {}
            Button("Delete memory", role: .destructive) { Task { await deletePendingMemory() } }
        } message: {
            Text("This deletes \(pendingDelete?.title ?? "the selected memory") from \(context.profile.projectSlug). Its version history remains available as a tombstone.")
        }
        .alert("Memory could not be deleted", isPresented: Binding(
            get: { deleteError != nil }, set: { if !$0 { deleteError = nil } }
        )) {
            Button("OK") { deleteError = nil }
        } message: { Text(deleteError ?? "") }
    }

    private var memoryList: some View {
        VStack(alignment: .leading, spacing: 10) {
            TextField("Search summaries and content", text: $searchText)
                .focused($searchFocused)
                .textFieldStyle(.roundedBorder)
                .accessibilityIdentifier("memories.search")
            HStack(spacing: 8) {
                TextField("Filter tags", text: $tagText)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("memories.tags")
                Picker("Status", selection: $statusFilter) {
                    Text("Active").tag("active")
                    Text("Archived").tag("archived")
                    Text("All").tag("all")
                }
                .labelsHidden()
                .frame(width: 112)
                .accessibilityIdentifier("memories.status")
            }
            Picker("Type", selection: $typeFilter) {
                Text("All types").tag("all")
                ForEach(Array(Set((response?.items ?? []).map(\.memoryType))).sorted(), id: \.self) { type in
                    Text(type.replacingOccurrences(of: "_", with: " ").capitalized).tag(type)
                }
            }
            .pickerStyle(.menu)
            .accessibilityIdentifier("memories.type")

            if filteredItems.isEmpty {
                KnowledgeStateView(kind: .noMatches, context: context, retry: nil) {
                    searchText = ""
                    tagText = ""
                    statusFilter = "all"
                    typeFilter = "all"
                }
                .frame(minHeight: 190)
            } else {
                List(selection: $selectedMemoryID) {
                    ForEach(filteredItems) { item in
                        Button {
                            selectedMemoryID = item.id
                            detailTab = .content
                        } label: {
                            MemoryListRow(item: item, isSelected: item.id == selectedMemoryID)
                        }
                        .buttonStyle(.plain)
                        .tag(item.id)
                        .listRowSeparator(.hidden)
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets(top: 4, leading: 4, bottom: 4, trailing: 4))
                        .accessibilityIdentifier("memories.row.\(item.id)")
                    }
                }
                .listStyle(.plain)
                .scrollContentBackground(.hidden)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(Color(nsColor: .underPageBackgroundColor), in: RoundedRectangle(cornerRadius: 10))
    }

    @ViewBuilder private var memoryDetail: some View {
        if let selectedMemory {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    HStack(alignment: .top, spacing: 12) {
                        VStack(alignment: .leading, spacing: 8) {
                            HStack(spacing: 8) {
                                KnowledgeTag(text: selectedMemory.memoryType.capitalized, tone: .blue)
                                KnowledgeTag(text: selectedMemory.status.capitalized, tone: selectedMemory.status == "active" ? .green : .neutral)
                                Text("Version \(selectedMemory.versionNumber)").font(.caption).foregroundStyle(.secondary)
                            }
                            Text(selectedMemory.summary).font(.title2.weight(.semibold)).fixedSize(horizontal: false, vertical: true)
                            Text("Updated \(knowledgeDate(selectedMemory.updatedAt)) · \(selectedMemory.tags.joined(separator: " · "))")
                                .font(.callout).foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 8)
                        Button("Delete memory…", role: .destructive) {
                            pendingDelete = (selectedMemory.id, selectedMemory.summary)
                            presentsDeleteConfirmation = true
                        }
                            .disabled(!context.permissions.allows(.memoryDelete) || context.permissions.readOnly || isDeleting)
                            .accessibilityIdentifier("memories.delete")
                    }
                    Picker("Memory detail", selection: $detailTab) {
                        ForEach(DetailTab.allCases) { tab in Text(tab.rawValue).tag(tab) }
                    }
                    .pickerStyle(.segmented)

                    if let detailError {
                        Label(detailError, systemImage: "exclamationmark.triangle.fill")
                            .foregroundStyle(.orange)
                    }
                    if isDetailLoading {
                        ProgressView("Loading memory…").frame(maxWidth: .infinity, alignment: .leading)
                    }
                    switch detailTab {
                    case .content: canonicalContent
                    case .provenance: provenanceContent
                    case .history: historyContent
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .accessibilityIdentifier("memories.detail")
        } else if isLoading {
            ProgressView("Loading memories…").frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ContentUnavailableView("Select a memory", systemImage: "square.stack.3d.up", description: Text("Choose a canonical record to inspect its content and evidence."))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private var canonicalContent: some View {
        VStack(alignment: .leading, spacing: 14) {
            Divider()
            Text("Canonical memory").font(.headline)
            Text(selectedMemory?.canonicalText ?? "").font(.body).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
            KnowledgeCard {
                VStack(alignment: .leading, spacing: 10) {
                    LabeledContent("Confidence", value: selectedMemory.map { "\($0.confidence.formatted(.number.precision(.fractionLength(2))))" } ?? "—")
                    LabeledContent("Importance", value: selectedMemory.map { $0.importance.formatted(.number.precision(.fractionLength(2))) } ?? "—")
                    LabeledContent("Sources", value: "\(selectedMemory?.sources.count ?? 0) references")
                    if let embedding = selectedMemory?.embeddingSpaces.first {
                        LabeledContent("Embedding", value: "\(embedding.provider) · \(embedding.model)")
                    }
                }
                .font(.callout)
            }
            if let selectedMemory, !selectedMemory.relatedMemories.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Related context").font(.headline)
                    ForEach(selectedMemory.relatedMemories) { related in
                        Button {
                            selectedMemoryID = related.memoryID
                            detailTab = .content
                        } label: {
                            HStack(spacing: 6) {
                                Text(related.summary).lineLimit(2)
                                Text("· \(related.relationType)").foregroundStyle(.secondary)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .buttonStyle(.link)
                        .accessibilityIdentifier("memories.related.\(related.memoryID)")
                    }
                }
            }
        }
    }

    private var provenanceContent: some View {
        VStack(alignment: .leading, spacing: 12) {
            Divider()
            Text("Source evidence").font(.headline)
            if let selectedMemory, selectedMemory.sources.isEmpty {
                ContentUnavailableView("No source records", systemImage: "doc.text.magnifyingglass", description: Text("This canonical memory does not have attached source evidence."))
            } else {
                ForEach(Array((selectedMemory?.sources ?? []).enumerated()), id: \.offset) { _, source in
                    KnowledgeCard {
                        VStack(alignment: .leading, spacing: 9) {
                            HStack {
                                Text(source.displayName).font(.headline).lineLimit(2)
                                Spacer()
                                KnowledgeTag(text: source.provenance?.status.capitalized ?? "Unverified", tone: source.provenance?.status == "verified" ? .green : .amber)
                            }
                            if let kind = source.sourceKind { LabeledContent("Evidence", value: kind.replacingOccurrences(of: "_", with: " ").capitalized) }
                            if let symbol = source.symbolName { LabeledContent("Symbol", value: symbol) }
                            if let commit = source.gitCommit { LabeledContent("Commit", value: commit).font(.system(.callout, design: .monospaced)) }
                            if let provenance = source.provenance, let reason = provenance.reason, !reason.isEmpty {
                                LabeledContent("Verification", value: reason)
                            }
                            if let excerpt = source.excerpt, !excerpt.isEmpty {
                                Text(excerpt).font(.callout).foregroundStyle(.secondary).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                            }
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder private var historyContent: some View {
        VStack(alignment: .leading, spacing: 12) {
            Divider()
            HStack {
                Text("Version history").font(.headline)
                Spacer()
                if let history { Text("\(history.versions.count) versions").foregroundStyle(.secondary) }
            }
            if isDetailLoading && history == nil {
                ProgressView("Loading history…")
            } else if let history {
                ForEach(history.versions.sorted(by: { $0.versionNumber > $1.versionNumber })) { version in
                    KnowledgeCard {
                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                Text("Version \(version.versionNumber)").font(.headline)
                                if version.id == selectedMemoryID { KnowledgeTag(text: "Current", tone: .blue) }
                                Spacer()
                                Text(knowledgeDate(version.updatedAt)).font(.caption).foregroundStyle(.secondary)
                            }
                            Text(version.summary).font(.subheadline.weight(.semibold))
                            Text(version.isTombstone ? "Memory deleted at this version." : version.canonicalText)
                                .font(.callout).textSelection(.enabled)
                        }
                    }
                }
                Text("Earlier versions are read-only.").font(.caption).foregroundStyle(.secondary)
            } else if let detailError {
                ContentUnavailableView("History unavailable", systemImage: "clock.arrow.circlepath", description: Text(detailError))
            }
        }
    }

    private func loadMemories() async {
        let request = listGate.begin(revision: context.revision)
        isLoading = true
        do {
            let payload = try await service.memories()
            guard !Task.isCancelled, listGate.accepts(request, revision: context.revision) else { return }
            response = payload
            errorMessage = nil
            if !payload.items.contains(where: { $0.id == selectedMemoryID }) {
                selectedMemoryID = payload.items.first(where: { $0.status == statusFilter })?.id ?? payload.items.first?.id
            }
            if payload.items.isEmpty { selectedMemoryID = nil }
        } catch is CancellationError {
            return
        } catch {
            guard !Task.isCancelled, listGate.accepts(request, revision: context.revision) else { return }
            errorMessage = error.localizedDescription
        }
        guard listGate.accepts(request, revision: context.revision) else { return }
        isLoading = false
    }

    private func loadSelectedMemory(id: String) async {
        let request = detailGate.begin(revision: context.revision, resourceID: id)
        detailError = nil
        if detailTab == .history { history = nil }
        isDetailLoading = true
        do {
            if detailTab == .history {
                let payload = try await service.history(memoryID: id)
                guard !Task.isCancelled, selectedMemoryID == id, detailGate.accepts(request, revision: context.revision, resourceID: id) else { return }
                history = payload
            } else {
                let payload = try await service.memory(id: id)
                guard !Task.isCancelled, selectedMemoryID == id, detailGate.accepts(request, revision: context.revision, resourceID: id) else { return }
                selectedMemory = payload
            }
        } catch is CancellationError {
            return
        } catch {
            guard !Task.isCancelled, selectedMemoryID == id, detailGate.accepts(request, revision: context.revision, resourceID: id) else { return }
            detailError = error.localizedDescription
        }
        guard !Task.isCancelled, selectedMemoryID == id, detailGate.accepts(request, revision: context.revision, resourceID: id) else { return }
        isDetailLoading = false
    }

    private func deletePendingMemory() async {
        guard !isDeleting, let target = pendingDelete,
              context.permissions.allows(.memoryDelete), !context.permissions.readOnly else { return }
        isDeleting = true
        do {
            let result = try await service.delete(memoryID: target.id)
            guard result.deleted else { throw MemoryServiceError.httpStatus(409, "The service did not delete this memory.") }
            if let response {
                self.response = ProjectMemoriesResponse(
                    project: response.project,
                    total: max(response.total - 1, 0),
                    items: response.items.filter { $0.id != target.id }
                )
            }
            if selectedMemoryID == target.id {
                selectedMemoryID = filteredItems.first?.id
                selectedMemory = nil
                history = nil
            }
            deleteError = nil
            if response?.items.isEmpty == true { errorMessage = nil }
        } catch {
            deleteError = error.localizedDescription
        }
        pendingDelete = nil
        isDeleting = false
    }

    private func reconcileSelection() {
        guard let selectedMemoryID else { return }
        if !filteredItems.contains(where: { $0.id == selectedMemoryID }) {
            self.selectedMemoryID = filteredItems.first?.id
            selectedMemory = nil
            history = nil
        }
    }

    private func stateKind(for message: String) -> KnowledgeStateView.Kind {
        if message.localizedCaseInsensitiveContains("401") || message.localizedCaseInsensitiveContains("credentials") { return .authRequired }
        if message.localizedCaseInsensitiveContains("403") || message.localizedCaseInsensitiveContains("permission") { return .restricted }
        if message.localizedCaseInsensitiveContains("offline") || message.localizedCaseInsensitiveContains("connect") { return .unavailable }
        return .failed(message)
    }
}

private struct MemoryListRow: View {
    let item: MemoryListItem
    let isSelected: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(item.summary).font(.subheadline.weight(.semibold)).lineLimit(2)
            Text(item.preview).font(.callout).foregroundStyle(.secondary).lineLimit(2)
            HStack(spacing: 6) {
                Text(item.memoryType.replacingOccurrences(of: "_", with: " ").capitalized)
                Text("·")
                Text("\(item.tags.joined(separator: " · ")) · v\(item.versionNumber)").lineLimit(1)
            }
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .background(isSelected ? Color.accentColor.opacity(0.12) : Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 9))
        .contentShape(RoundedRectangle(cornerRadius: 9))
    }
}

struct KnowledgeMemoryDetailView: View {
    let context: FeatureContext
    let memoryID: String
    @Environment(\.dismiss) private var dismiss
    @State private var memory: MemoryEntry?
    @State private var loading = true
    @State private var error: String?
    @State private var requestGate = KnowledgeRequestGate()

    var body: some View {
        NavigationStack {
          Group {
            if loading && memory == nil {
                ProgressView("Loading memory…").frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let error, memory == nil {
                ContentUnavailableView("Memory unavailable", systemImage: "exclamationmark.triangle", description: Text(error))
            } else if let memory {
                ScrollView {
                    VStack(alignment: .leading, spacing: 14) {
                        HStack {
                            KnowledgeTag(text: memory.memoryType.capitalized, tone: .blue)
                            KnowledgeTag(text: memory.status.capitalized, tone: .green)
                            Text("Version \(memory.versionNumber)").font(.caption).foregroundStyle(.secondary)
                        }
                        Text(memory.summary).font(.title2.weight(.semibold))
                        Text(memory.canonicalText).font(.body).textSelection(.enabled)
                        KnowledgeCard {
                            VStack(alignment: .leading, spacing: 8) {
                                Text("Sources · \(memory.sources.count)").font(.headline)
                                ForEach(Array(memory.sources.enumerated()), id: \.offset) { _, source in
                                    VStack(alignment: .leading, spacing: 4) {
                                        Text(source.displayName).font(.subheadline.weight(.medium))
                                        if let excerpt = source.excerpt { Text(excerpt).font(.callout).foregroundStyle(.secondary).textSelection(.enabled) }
                                    }
                                    Divider()
                                }
                            }
                        }
                    }
                    .padding(18)
                    .frame(maxWidth: 760, alignment: .leading)
                    .frame(maxWidth: .infinity, alignment: .topLeading)
                }
            }
        }
          .toolbar {
              ToolbarItem(placement: .confirmationAction) {
                  Button("Close") { dismiss() }
                      .keyboardShortcut(.cancelAction)
                      .accessibilityIdentifier("knowledge.memory.close")
              }
          }
        }
        .task(id: "\(context.revision)-\(memoryID)") { await load() }
        .frame(minWidth: 520, minHeight: 420)
        .accessibilityIdentifier("knowledge.memory.detail.sheet")
    }

    private func load() async {
        let request = requestGate.begin(revision: context.revision, resourceID: memoryID)
        loading = true
        error = nil
        do {
            let payload = try await KnowledgeService(context: context).memory(id: memoryID)
            guard !Task.isCancelled, requestGate.accepts(request, revision: context.revision, resourceID: memoryID) else { return }
            memory = payload
        } catch is CancellationError {
            return
        } catch {
            guard !Task.isCancelled, requestGate.accepts(request, revision: context.revision, resourceID: memoryID) else { return }
            self.error = error.localizedDescription
        }
        guard !Task.isCancelled, requestGate.accepts(request, revision: context.revision, resourceID: memoryID) else { return }
        loading = false
    }
}

struct KnowledgeTag: View {
    enum Tone { case blue, green, amber, neutral }
    let text: String
    var tone: Tone = .neutral

    var body: some View {
        Text(text)
            .font(.caption.weight(.medium))
            .padding(.horizontal, 8)
            .padding(.vertical, 4)
            .foregroundStyle(foreground)
            .background(background, in: RoundedRectangle(cornerRadius: 6))
    }

    private var foreground: Color {
        switch tone {
        case .blue: .blue
        case .green: .green
        case .amber: .orange
        case .neutral: .secondary
        }
    }
    private var background: Color {
        switch tone {
        case .blue: .blue.opacity(0.1)
        case .green: .green.opacity(0.1)
        case .amber: .orange.opacity(0.12)
        case .neutral: Color.secondary.opacity(0.1)
        }
    }
}
