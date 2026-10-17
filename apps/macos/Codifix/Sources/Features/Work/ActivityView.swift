import SwiftUI

struct ActivityView: View {
    let context: FeatureContext
    @State private var activities: [WorkActivityEvent] = []
    @State private var selectedActivityID: String?
    @State private var filter = "all"
    @State private var loadError: Error?
    @State private var briefing: WorkBriefingResponse?
    @State private var briefingError: Error?
    @State private var llmAudit: WorkLlmAuditStatus?
    @State private var auditError: Error?
    @State private var loading = false
    @State private var briefingLoading = false
    @State private var auditLoading = false
    @State private var auditChanging = false
    @State private var memoryToOpen: WorkMemorySelection?
    @State private var refreshID = 0
    @State private var briefingID = 0
    @State private var requestedBriefingMode: Bool?
    @State private var activityGate = WorkRequestGate()
    @State private var briefingGate = WorkRequestGate()
    @State private var auditGate = WorkRequestGate()

    private let filters = [
        ("all", "All activity"), ("checkpoint", "Checkpoints"), ("scan", "Scans"),
        ("plan", "Plans"), ("commit_sync", "Commit sync"), ("bundle_export", "Bundle exports"),
        ("bundle_import", "Bundle imports"), ("graph_extract", "Graph extraction"),
        ("query", "Queries"), ("query_error", "Query errors"), ("watcher_health", "Watcher health"),
        ("memory_replacement", "Memory replacement"), ("capture_task", "Task capture"),
        ("curate", "Curation"), ("reindex", "Reindex"), ("reembed", "Reembed"),
        ("archive", "Archive"), ("delete_memory", "Memory deletion"), ("briefing", "Briefings"),
        ("diagnostic", "Diagnostics"), ("llm_audit", "LLM audit")
    ]

    private var selectedActivity: WorkActivityEvent? { activities.first { $0.id == selectedActivityID } }
    private var auditCanChange: Bool { context.permissions.globalAdministrator && context.permissions.globalAllowed.contains(.systemAdmin) && !context.permissions.readOnly }

    var body: some View {
        VStack(spacing: 12) {
            HStack(alignment: .center) {
                VStack(alignment: .leading, spacing: 3) {
                    Text("Activity").font(.title2.weight(.semibold))
                    Text("Persisted events · \(context.profile.projectSlug)").font(.callout).foregroundStyle(.secondary)
                }
                Spacer()
                Picker("Kind", selection: $filter) {
                    ForEach(filters, id: \.0) { Text($0.1).tag($0.0) }
                }
                .frame(width: 155)
                .accessibilityIdentifier("work.activity.filter")
                Button {
                    refreshID += 1
                } label: { Label("Refresh", systemImage: "arrow.clockwise") }
                    .buttonStyle(.bordered)
                    .disabled(loading)
                    .accessibilityIdentifier("work.activity.refresh")
            }

            if loading { ProgressView().controlSize(.small).frame(maxWidth: .infinity, alignment: .leading) }
            if let loadError, !activities.isEmpty {
                WorkNotice(title: "Showing the last activity snapshot", message: loadError.localizedDescription, tone: .warning)
            }
            if let briefingError {
                WorkNotice(title: "Briefing unavailable", message: briefingError.localizedDescription, tone: .error)
            }

            HSplitView {
                activityList
                    .frame(minWidth: 230, idealWidth: 285, maxWidth: 360)
                detailPane
                    .frame(minWidth: 360, maxWidth: .infinity, maxHeight: .infinity)
            }
            .accessibilityIdentifier("work.activity.split")
        }
        .padding(16)
        .task(id: "\(context.revision)-\(filter)-\(refreshID)") { await loadActivities() }
        .task(id: context.revision) { await loadAuditStatus() }
        .task(id: "\(context.revision)-briefing-\(briefingID)") {
            guard let requestedBriefingMode else { return }
            await loadBriefing(includeLLM: requestedBriefingMode)
        }
        .sheet(item: $memoryToOpen) { selection in
            KnowledgeMemoryDetailView(context: context, memoryID: selection.id)
        }
        .accessibilityIdentifier("work.activity.screen")
    }

    private var activityList: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Recent events").font(.headline)
            if activities.isEmpty, loading || loadError != nil {
                WorkLoadingOrError(context: context, loading: loading, error: loadError, hasContent: false,
                                   emptyTitle: "No activity recorded", emptyMessage: "This project has not returned any activity events.",
                                   retry: { refreshID += 1 })
            } else if activities.isEmpty, filter != "all" {
                ContentUnavailableView {
                    Label("No matching activity", systemImage: "line.3.horizontal.decrease.circle")
                } description: {
                    Text("No events match the selected kind in \(context.profile.projectSlug).")
                } actions: {
                    Button("Show all activity") { filter = "all" }.buttonStyle(.bordered)
                }
            } else if activities.isEmpty {
                ContentUnavailableView("No activity recorded", systemImage: "waveform.path",
                                      description: Text("This project has not returned any activity events."))
            } else {
                List(selection: $selectedActivityID) {
                    ForEach(activities) { event in
                        ActivityRow(event: event)
                            .tag(event.id)
                    }
                }
                .listStyle(.inset)
            }
        }
        .padding(.trailing, 8)
    }

    @ViewBuilder private var detailPane: some View {
        if let selectedActivity {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    VStack(alignment: .leading, spacing: 7) {
                        HStack {
                            WorkBadge(title: selectedActivity.kind.replacingOccurrences(of: "_", with: " ").capitalized, tone: .blue)
                            if let recorded = selectedActivity.recordedAt { Text(workDate(recorded)).font(.caption).foregroundStyle(.secondary) }
                        }
                        Text(selectedActivity.summary).font(.title3.weight(.semibold)).textSelection(.enabled)
                        if let actor = selectedActivity.actorName { Text("Recorded by \(actor)").font(.callout).foregroundStyle(.secondary) }
                    }

                    KnowledgeCard {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("Event details").font(.headline)
                            LabeledContent("Project", value: selectedActivity.project ?? context.profile.projectSlug)
                            if let source = selectedActivity.source { LabeledContent("Source", value: source) }
                            if let operation = selectedActivity.operationID { LabeledContent("Operation", value: operation) }
                            if let duration = selectedActivity.durationMS { LabeledContent("Duration", value: "\(duration) ms") }
                            if let usage = selectedActivity.tokenUsage {
                                LabeledContent("Tokens", value: usage.totalTokens.map { $0.formatted() } ?? "Not reported")
                                if let input = usage.inputTokens { LabeledContent("Input", value: input.formatted()) }
                                if let output = usage.outputTokens { LabeledContent("Output", value: output.formatted()) }
                            }
                            if let provider = selectedActivity.provider { LabeledContent("Provider", value: provider) }
                            if let model = selectedActivity.model { LabeledContent("Model", value: model) }
                            if let details = selectedActivity.details {
                                Divider()
                                Text(details.displayText).font(.system(.callout, design: .monospaced)).textSelection(.enabled)
                            } else {
                                Text("The service did not include structured event details.").font(.callout).foregroundStyle(.secondary)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }

                    if let memoryID = selectedActivity.memoryID {
                        Button("Open referenced memory", systemImage: "arrow.up.right.square") {
                            memoryToOpen = WorkMemorySelection(id: memoryID)
                        }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("work.activity.open-memory")
                    }

                    briefingPanel
                    auditPanel
                }
                .frame(maxWidth: 820, alignment: .leading)
                .padding(.vertical, 4)
                .padding(.horizontal, 12)
            }
        } else {
            VStack(alignment: .leading, spacing: 14) {
                briefingPanel
                auditPanel
                ContentUnavailableView("Select an activity", systemImage: "waveform.path",
                                      description: Text("Choose a project event to inspect its details."))
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .padding(12)
        }
    }

    private var briefingPanel: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Get up to speed").font(.headline)
            if let briefing {
                WorkNotice(title: "\(briefing.project ?? context.profile.projectSlug) · \(briefing.generatedAt.map(workDate) ?? "Briefing")",
                           message: briefing.briefing, tone: .info)
                HStack(spacing: 14) {
                    Label("\(briefing.tokenUsage.totalTokens?.formatted() ?? "Not reported") tokens", systemImage: "number")
                    if let actions = briefing.tokenUsage.actionCount { Label("\(actions) actions", systemImage: "bolt") }
                }
                .font(.caption).foregroundStyle(.secondary)
                if !briefing.warnings.isEmpty {
                    ForEach(Array(briefing.warnings.enumerated()), id: \.offset) { _, warning in
                        Label(warning, systemImage: "exclamationmark.triangle.fill")
                            .font(.callout).foregroundStyle(Color(nsColor: .systemOrange))
                    }
                }
                if !briefing.currentFocus.isEmpty { WorkStringList(title: "Current focus", items: briefing.currentFocus) }
                if !briefing.blockers.isEmpty { WorkStringList(title: "Blockers", items: briefing.blockers) }
                if !briefing.nextActions.isEmpty {
                    WorkStringList(title: "Next actions", items: briefing.nextActions.map(\.title))
                }
                if !briefing.usefulMemories.isEmpty {
                    VStack(alignment: .leading, spacing: 5) {
                        Text("Useful memories").font(.subheadline.weight(.semibold))
                        ForEach(briefing.usefulMemories) { memory in
                            Button(memory.summary) { memoryToOpen = WorkMemorySelection(id: memory.id) }
                                .buttonStyle(.link).lineLimit(2).frame(maxWidth: .infinity, alignment: .leading)
                        }
                    }
                }
            } else {
                Text("Generate a deterministic briefing or an LLM summary from this project’s persisted activity.")
                    .font(.callout).foregroundStyle(.secondary)
            }
            HStack {
                Button("Deterministic briefing") { beginBriefing(includeLLM: false) }
                    .buttonStyle(.bordered)
                    .disabled(briefingLoading)
                    .accessibilityIdentifier("work.activity.briefing.deterministic")
                Button("LLM briefing") { beginBriefing(includeLLM: true) }
                    .buttonStyle(.borderedProminent)
                    .disabled(briefingLoading)
                    .accessibilityIdentifier("work.activity.briefing.llm")
                if briefingLoading { ProgressView().controlSize(.small) }
            }
        }
        .padding(13)
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 9))
    }

    private var auditPanel: some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack {
                Text("LLM audit").font(.subheadline.weight(.semibold))
                Spacer()
                if let llmAudit {
                    Toggle("LLM audit", isOn: Binding(
                        get: { llmAudit.enabled },
                        set: { enabled in beginAuditChange(enabled: enabled) }
                    ))
                    .labelsHidden()
                    .disabled(!auditCanChange || auditChanging)
                    .accessibilityIdentifier("work.activity.audit.toggle")
                } else if auditLoading {
                    ProgressView().controlSize(.small)
                }
            }
            if let llmAudit {
                Text("Profile: \(llmAudit.profile ?? "Default") · Redaction \(llmAudit.redacted ? "enabled" : "disabled")")
                    .font(.caption).foregroundStyle(.secondary)
                if !auditCanChange {
                    Text("View only · Changing this service setting requires a global administrator.")
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            if let auditError { Text(auditError.localizedDescription).font(.caption).foregroundStyle(.red) }
        }
        .padding(.top, 4)
    }

    @MainActor private func loadActivities() async {
        let kind = filter == "all" ? nil : filter
        let resource = "activities:\(kind ?? "all")"
        let ticket = activityGate.begin(revision: context.revision, resourceID: resource)
        loading = true
        loadError = nil
        do {
            let response = try await WorkService(context: context).activities(kind: kind)
            try Task.checkCancellation()
            guard activityGate.accepts(ticket, revision: context.revision, resourceID: resource) else { return }
            let previousSelection = selectedActivityID
            activities = response.items
            selectedActivityID = response.items.first(where: { $0.id == previousSelection })?.id ?? response.items.first?.id
        } catch is CancellationError {
            return
        } catch {
            guard activityGate.accepts(ticket, revision: context.revision, resourceID: resource) else { return }
            loadError = error
        }
        if activityGate.accepts(ticket, revision: context.revision, resourceID: resource) { loading = false }
    }

    @MainActor private func loadBriefing(includeLLM: Bool) async {
        let resource = "briefing:\(includeLLM)"
        let ticket = briefingGate.begin(revision: context.revision, resourceID: resource)
        briefingLoading = true
        briefingError = nil
        do {
            let response = try await WorkService(context: context).briefing(includeLLMSummary: includeLLM)
            try Task.checkCancellation()
            guard briefingGate.accepts(ticket, revision: context.revision, resourceID: resource) else { return }
            briefing = response
        } catch is CancellationError {
            return
        } catch {
            guard briefingGate.accepts(ticket, revision: context.revision, resourceID: resource) else { return }
            briefingError = error
        }
        if briefingGate.accepts(ticket, revision: context.revision, resourceID: resource) { briefingLoading = false }
    }

    @MainActor private func beginBriefing(includeLLM: Bool) {
        guard !briefingLoading else { return }
        briefingLoading = true
        requestedBriefingMode = includeLLM
        briefingID += 1
    }

    @MainActor private func loadAuditStatus() async {
        let resource = "llm-audit-status"
        let ticket = auditGate.begin(revision: context.revision, resourceID: resource)
        auditLoading = true
        auditError = nil
        do {
            let response = try await WorkService(context: context).llmAuditStatus()
            try Task.checkCancellation()
            guard auditGate.accepts(ticket, revision: context.revision, resourceID: resource) else { return }
            llmAudit = response
        } catch is CancellationError {
            return
        } catch {
            guard auditGate.accepts(ticket, revision: context.revision, resourceID: resource) else { return }
            auditError = error
        }
        if auditGate.accepts(ticket, revision: context.revision, resourceID: resource) { auditLoading = false }
    }

    @MainActor private func beginAuditChange(enabled: Bool) {
        guard auditCanChange, !auditChanging, llmAudit != nil else { return }
        auditChanging = true
        Task { await updateAudit(enabled: enabled) }
    }

    @MainActor private func updateAudit(enabled: Bool) async {
        let resource = "llm-audit-update"
        let ticket = auditGate.begin(revision: context.revision, resourceID: resource)
        defer { auditChanging = false }
        auditError = nil
        do {
            let response = try await WorkService(context: context).setLLMAudit(enabled: enabled)
            try Task.checkCancellation()
            guard auditGate.accepts(ticket, revision: context.revision, resourceID: resource) else { return }
            llmAudit = response
        } catch is CancellationError {
            return
        } catch {
            guard auditGate.accepts(ticket, revision: context.revision, resourceID: resource) else { return }
            auditError = error
        }
    }
}

private struct ActivityRow: View {
    let event: WorkActivityEvent

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(event.summary).font(.subheadline.weight(.semibold)).lineLimit(2)
            Text(event.kind.replacingOccurrences(of: "_", with: " ").capitalized)
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Text(workDate(event.recordedAt))
                if let tokens = event.tokenUsage?.totalTokens { Text("· \(tokens.formatted()) tokens") }
            }
            .font(.caption2).foregroundStyle(.secondary)
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
    }
}

struct WorkStringList: View {
    let title: String
    let items: [String]
    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(title).font(.subheadline.weight(.semibold))
            ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                Label(item, systemImage: "circle.fill")
                    .font(.callout)
                    .labelStyle(WorkBulletLabelStyle())
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

private struct WorkBulletLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 7) {
            configuration.icon.font(.system(size: 5)).foregroundStyle(.secondary)
            configuration.title
        }
    }
}
