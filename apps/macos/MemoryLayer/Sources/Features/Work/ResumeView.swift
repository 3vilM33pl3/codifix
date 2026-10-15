import SwiftUI

struct ResumeView: View {
    let context: FeatureContext
    @State private var response: WorkResumeResponse?
    @State private var loading = false
    @State private var includeLLMSummary = false
    @State private var loadError: Error?
    @State private var requestGate = WorkRequestGate()
    @State private var memoryToOpen: WorkMemorySelection?
    @State private var refreshID = 0

    var body: some View {
        VStack(alignment: .leading, spacing: 13) {
            HStack(alignment: .center) {
                VStack(alignment: .leading, spacing: 3) {
                    Text("Resume").font(.title2.weight(.semibold))
                    Text("A briefing to carry project work across sessions · \(context.profile.projectSlug)")
                        .font(.callout).foregroundStyle(.secondary)
                }
                Spacer()
                Picker("Summary", selection: $includeLLMSummary) {
                    Text("Deterministic").tag(false)
                    Text("LLM summary").tag(true)
                }
                .pickerStyle(.segmented)
                .frame(width: 190)
                .accessibilityIdentifier("work.resume.summary-mode")
                Button {
                    refreshID += 1
                } label: {
                    if loading { ProgressView().controlSize(.small) }
                    else { Text("Load resume") }
                }
                .buttonStyle(.borderedProminent)
                .disabled(loading)
                .accessibilityIdentifier("work.resume.load")
            }

            if let loadError, let response {
                WorkNotice(title: "Showing the previous resume", message: loadError.localizedDescription, tone: .warning)
            }

            if let response {
                HSplitView {
                    mainBriefing(response)
                        .frame(minWidth: 360, idealWidth: 570, maxWidth: .infinity, maxHeight: .infinity)
                    checkpointInspector(response)
                        .frame(minWidth: 270, idealWidth: 330, maxWidth: 430, maxHeight: .infinity)
                }
                .accessibilityIdentifier("work.resume.split")
            } else if loading {
                ProgressView("Building a project resume from the connected service…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if let loadError {
                WorkLoadingOrError(context: context, loading: false, error: loadError, hasContent: false,
                                   emptyTitle: "Resume unavailable", emptyMessage: "") {
                    beginResume()
                }
            } else {
                ContentUnavailableView {
                    Label("Load a project resume", systemImage: "arrow.clockwise")
                } description: {
                    Text("Choose a deterministic briefing or an LLM summary. The service supplies checkpoints, next steps, warnings, and referenced memories.")
                } actions: {
                    Button("Load deterministic resume") { beginResume(includeLLM: false) }
                        .buttonStyle(.borderedProminent)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .padding(16)
        .task(id: "\(context.revision)-\(includeLLMSummary)-\(refreshID)") {
            guard refreshID > 0 else { return }
            await loadResume()
        }
        .sheet(item: $memoryToOpen) { selection in
            KnowledgeMemoryDetailView(context: context, memoryID: selection.id)
        }
        .accessibilityIdentifier("work.resume.screen")
    }

    private func mainBriefing(_ response: WorkResumeResponse) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 13) {
                WorkNotice(title: response.checkpoint?.note ?? "Current project work",
                           message: response.briefing, tone: .info)

                if !response.warnings.isEmpty {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Warnings").font(.headline)
                        ForEach(Array(response.warnings.enumerated()), id: \.offset) { _, warning in
                            Label(warning, systemImage: "exclamationmark.triangle.fill")
                                .font(.callout).fixedSize(horizontal: false, vertical: true)
                                .foregroundStyle(Color(nsColor: .systemOrange))
                        }
                    }
                }

                if !response.nextSteps.isEmpty {
                    KnowledgeCard {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("Next steps").font(.headline)
                            ForEach(Array(response.nextSteps.enumerated()), id: \.element.id) { index, action in
                                HStack(alignment: .top, spacing: 9) {
                                    Text("\(index + 1).").font(.callout.weight(.medium)).foregroundStyle(.secondary)
                                    VStack(alignment: .leading, spacing: 3) {
                                        Text(action.title).font(.callout.weight(.medium))
                                        if let rationale = action.rationale, !rationale.isEmpty {
                                            Text(rationale).font(.caption).foregroundStyle(.secondary)
                                        }
                                        if let command = action.commandHint, !command.isEmpty {
                                            Text(command)
                                                .font(.system(.caption, design: .monospaced))
                                                .textSelection(.enabled)
                                                .padding(.top, 2)
                                            Text("Reference only · Commands are not executed here.")
                                                .font(.caption2).foregroundStyle(.secondary)
                                        }
                                    }
                                }
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }

                if !response.changeSummary.isEmpty || !response.attentionItems.isEmpty {
                    KnowledgeCard {
                        VStack(alignment: .leading, spacing: 9) {
                            if !response.changeSummary.isEmpty { WorkStringList(title: "Changes since checkpoint", items: response.changeSummary) }
                            if !response.attentionItems.isEmpty { WorkStringList(title: "Attention", items: response.attentionItems) }
                        }
                    }
                }

                if !response.memories.isEmpty {
                    VStack(alignment: .leading, spacing: 7) {
                        Text("Useful memories").font(.headline)
                        ForEach(response.memories) { memory in
                            Button {
                                memoryToOpen = WorkMemorySelection(id: memory.id)
                            } label: {
                                HStack(alignment: .top, spacing: 10) {
                                    Image(systemName: "square.stack.3d.up").foregroundStyle(.secondary)
                                    VStack(alignment: .leading, spacing: 3) {
                                        Text(memory.summary).font(.subheadline.weight(.semibold)).foregroundStyle(.primary)
                                        if let preview = memory.preview, !preview.isEmpty {
                                            Text(preview).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                                        }
                                        Text([memory.memoryType?.capitalized, memory.status?.capitalized].compactMap { $0 }.joined(separator: " · "))
                                            .font(.caption2).foregroundStyle(.secondary)
                                    }
                                    Spacer()
                                    Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(.tertiary)
                                }
                                .padding(10)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
                            }
                            .buttonStyle(.plain)
                            .accessibilityIdentifier("work.resume.memory.\(memory.id)")
                        }
                    }
                }

                if !response.timeline.isEmpty {
                    KnowledgeCard {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Recent work").font(.headline)
                            ForEach(response.timeline.prefix(8)) { event in
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(event.summary).font(.callout)
                                    Text("\(event.kind.replacingOccurrences(of: "_", with: " ").capitalized) · \(workDate(event.recordedAt))")
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                Divider()
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }

                if !response.commits.isEmpty {
                    KnowledgeCard {
                        VStack(alignment: .leading, spacing: 8) {
                            Text("Recent commits").font(.headline)
                            ForEach(response.commits.prefix(8)) { commit in
                                HStack(alignment: .top, spacing: 9) {
                                    Text(commit.shortHash).font(.system(.caption, design: .monospaced)).foregroundStyle(.secondary)
                                    VStack(alignment: .leading, spacing: 3) {
                                        Text(commit.subject).font(.callout)
                                        Text(workDate(commit.committedAt)).font(.caption).foregroundStyle(.secondary)
                                    }
                                    Spacer(minLength: 4)
                                }
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
            }
            .frame(maxWidth: 760, alignment: .leading)
            .padding(.vertical, 4)
            .padding(.trailing, 12)
        }
    }

    private func checkpointInspector(_ response: WorkResumeResponse) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 13) {
                KnowledgeCard {
                    VStack(alignment: .leading, spacing: 9) {
                        Text("Checkpoint").font(.headline)
                        LabeledContent("Project", value: response.checkpoint?.project ?? response.project ?? context.profile.projectSlug)
                        LabeledContent("Branch", value: response.checkpoint?.gitBranch ?? "Not reported")
                        LabeledContent("Marked", value: workDate(response.checkpoint?.markedAt))
                        LabeledContent("Repository", value: response.checkpoint?.repositoryRoot ?? context.profile.repositoryRoot?.path ?? "Not set")
                        if let head = response.checkpoint?.gitHead { LabeledContent("Git head", value: String(head.prefix(12))) }
                        if let thread = response.currentThread { LabeledContent("Current thread", value: thread) }
                    }
                }

                if !response.attentionItems.isEmpty {
                    VStack(alignment: .leading, spacing: 7) {
                        Text("Attention").font(.headline)
                        ForEach(Array(response.attentionItems.enumerated()), id: \.offset) { _, item in
                            WorkNotice(title: "Review", message: item, tone: .warning)
                        }
                    }
                }

                Text("The resume is read from the selected service and project. Repository context is limited to the configured root.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            .padding(.vertical, 4)
            .padding(.leading, 12)
        }
    }

    @MainActor private func beginResume(includeLLM: Bool? = nil) {
        if let includeLLM { includeLLMSummary = includeLLM }
        refreshID += 1
    }

    @MainActor private func loadResume() async {
        let selectedMode = includeLLMSummary
        let resource = "resume:\(selectedMode)"
        let ticket = requestGate.begin(revision: context.revision, resourceID: resource)
        loading = true
        loadError = nil
        do {
            let result = try await WorkService(context: context).resume(includeLLMSummary: selectedMode)
            try Task.checkCancellation()
            guard requestGate.accepts(ticket, revision: context.revision, resourceID: resource) else { return }
            response = result
        } catch is CancellationError {
            return
        } catch {
            guard requestGate.accepts(ticket, revision: context.revision, resourceID: resource) else { return }
            loadError = error
        }
        if requestGate.accepts(ticket, revision: context.revision, resourceID: resource) { loading = false }
    }
}
