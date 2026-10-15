import SwiftUI

struct AgentsView: View {
    let context: FeatureContext
    @State private var snapshot: WorkAgentSnapshot?
    @State private var selectedSessionID: String?
    @State private var loading = false
    @State private var refreshError: Error?
    @State private var requestGate = WorkRequestGate()
    @State private var refreshID = 0

    private var sessions: [WorkAgentSession] {
        (snapshot?.sessions ?? []).sorted { left, right in
            let leftCurrent = isCurrentProject(left)
            let rightCurrent = isCurrentProject(right)
            if leftCurrent != rightCurrent { return leftCurrent }
            return (left.startedAt ?? 0) > (right.startedAt ?? 0)
        }
    }

    private var selectedSession: WorkAgentSession? { sessions.first { $0.id == selectedSessionID } }
    var body: some View {
        VStack(alignment: .leading, spacing: 13) {
            HStack(alignment: .center) {
                VStack(alignment: .leading, spacing: 3) {
                    Text("Agents").font(.title2.weight(.semibold))
                    Text("Project: \(context.profile.projectSlug) · Service: \(context.profile.name)")
                        .font(.callout).foregroundStyle(.secondary)
                }
                Spacer()
                Button {
                    refreshID += 1
                } label: { Label("Refresh snapshot", systemImage: "arrow.clockwise") }
                    .buttonStyle(.bordered)
                    .disabled(loading)
                    .accessibilityIdentifier("work.agents.refresh")
            }

            TimelineView(.periodic(from: .now, by: 30)) { timeline in
                VStack(alignment: .leading, spacing: 8) {
                    if let collectedAt = snapshot?.collectedAt {
                        Text("Updated \(workRelativeDate(collectedAt))").font(.caption).foregroundStyle(.secondary)
                        if let refreshError {
                            WorkNotice(title: "Agent snapshot is stale", message: "\(refreshError.localizedDescription) The displayed session state is from the previous successful snapshot.", tone: .warning)
                        } else if timeline.date.timeIntervalSince(collectedAt) > 120 {
                            WorkNotice(title: "Agent snapshot is stale", message: "The last session snapshot is more than two minutes old. Displayed status may have changed.", tone: .warning)
                        }
                    }
                }
            }

            if loading && snapshot == nil {
                WorkLoadingOrError(context: context, loading: true, error: nil, hasContent: false,
                                   emptyTitle: "No sessions", emptyMessage: "")
            } else if let refreshError, snapshot == nil {
                WorkLoadingOrError(context: context, loading: false, error: refreshError, hasContent: false,
                                   emptyTitle: "No sessions", emptyMessage: "") {
                    refreshID += 1
                }
            } else if snapshot != nil {
                HSplitView {
                    sessionList.frame(minWidth: 230, idealWidth: 290, maxWidth: 360)
                    sessionDetail.frame(minWidth: 380, maxWidth: .infinity, maxHeight: .infinity)
                }
                .accessibilityIdentifier("work.agents.split")
            } else {
                WorkLoadingOrError(context: context, loading: true, error: nil, hasContent: false,
                                   emptyTitle: "Loading agents", emptyMessage: "")
            }
        }
        .padding(16)
        .task(id: "\(context.revision)-\(refreshID)") { await refresh() }
        .accessibilityIdentifier("work.agents.screen")
    }

    private var sessionList: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Coding sessions · current project first").font(.headline)
            if sessions.isEmpty {
                ContentUnavailableView("No active coding sessions", systemImage: "person.crop.rectangle",
                                      description: Text("The connected service did not report any coding sessions."))
            } else {
                List(selection: $selectedSessionID) {
                    ForEach(sessions) { session in
                        VStack(alignment: .leading, spacing: 4) {
                            Text(sessionTitle(session)).font(.subheadline.weight(.semibold)).lineLimit(1)
                            Text(session.currentTasks.first ?? session.projectName ?? session.agentCLI ?? "Coding session")
                                .font(.caption).foregroundStyle(.secondary).lineLimit(2)
                            HStack(spacing: 6) {
                                WorkBadge(title: session.status?.capitalized ?? "Unknown", tone: session.status == "working" ? .green : .secondary)
                                WorkBadge(title: isCurrentProject(session) ? "Current project" : (session.projectName ?? "Other project"),
                                          tone: isCurrentProject(session) ? .blue : .secondary)
                            }
                        }
                        .padding(.vertical, 5)
                        .tag(session.id)
                        .accessibilityIdentifier("work.agents.session.\(session.id)")
                    }
                }
                .listStyle(.inset)
            }
        }
        .padding(.trailing, 8)
    }

    @ViewBuilder private var sessionDetail: some View {
        if let selectedSession {
            ScrollView {
                VStack(alignment: .leading, spacing: 13) {
                    HStack {
                        WorkBadge(title: selectedSession.status?.capitalized ?? "Unknown", tone: .green)
                        WorkBadge(title: isCurrentProject(selectedSession) ? "Current project" : "Other project",
                                  tone: isCurrentProject(selectedSession) ? .blue : .secondary)
                    }
                    Text(sessionTitle(selectedSession)).font(.title3.weight(.semibold))
                    if let task = selectedSession.currentTasks.first {
                        Text(task).font(.callout).foregroundStyle(.secondary)
                    }

                    KnowledgeCard {
                        VStack(alignment: .leading, spacing: 9) {
                            Text("Session metrics").font(.headline)
                            workMetric("Task", selectedSession.currentTasks.first ?? "Not reported")
                            workMetric("Model", selectedSession.model ?? "Not reported")
                            workMetric("Agent CLI", selectedSession.agentCLI ?? "Not reported")
                            workMetric("Session", selectedSession.sessionID)
                            workMetric("Elapsed", elapsed(startedAt: selectedSession.startedAt))
                            workMetric("Context", selectedSession.contextPercent.map { "\(Int($0.rounded()))% used" } ?? "Not reported")
                            workMetric("Tokens", selectedSession.totalTokens > 0 ? selectedSession.totalTokens.formatted() : "Not reported")
                            workMetric("Cache read", selectedSession.totalCacheRead.map { $0.formatted() } ?? "Not reported")
                            workMetric("Cache write", selectedSession.totalCacheCreate.map { $0.formatted() } ?? "Not reported")
                            workMetric("Turns", selectedSession.turnCount.map(String.init) ?? "Not reported")
                            workMetric("Memory", selectedSession.memoryMB.map { String(format: "%.0f MB", $0) } ?? "Not reported")
                            workMetric("Memory files", selectedSession.memFileCount.map(String.init) ?? "Not reported")
                            workMetric("Memory lines", selectedSession.memLineCount.map(String.init) ?? "Not reported")
                            workMetric("Git", gitSummary(selectedSession))
                            if let cwd = selectedSession.cwd {
                                Text(cwd).font(.system(.caption, design: .monospaced)).foregroundStyle(.secondary).textSelection(.enabled)
                            }
                            if let prompt = selectedSession.firstAssistantText, !prompt.isEmpty {
                                Divider()
                                Text("First assistant response").font(.subheadline.weight(.semibold))
                                Text(prompt).font(.callout).textSelection(.enabled)
                            } else if let prompt = selectedSession.initialPrompt, !prompt.isEmpty {
                                Divider()
                                Text("Session prompt").font(.subheadline.weight(.semibold))
                                Text(prompt).font(.callout).textSelection(.enabled)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }

                    KnowledgeCard {
                        VStack(alignment: .leading, spacing: 9) {
                            Text("Subagents and resources").font(.headline)
                            if selectedSession.subagents.isEmpty {
                                Text("No child agents were reported for this session.").font(.callout).foregroundStyle(.secondary)
                            } else {
                                Table(selectedSession.subagents) {
                                    TableColumn("Session", value: \.name)
                                    TableColumn("State", value: \.status)
                                    TableColumn("Tokens") { Text($0.tokens.formatted()) }
                                }
                                .frame(minHeight: CGFloat(min(selectedSession.subagents.count, 6) * 34 + 30))
                            }
                            if !selectedSession.children.isEmpty {
                                Text("Child processes").font(.subheadline.weight(.semibold)).padding(.top, 5)
                                ForEach(selectedSession.children) { child in
                                    HStack {
                                        Text(child.command).lineLimit(1)
                                        Spacer()
                                        Text("PID \(child.pid)").foregroundStyle(.secondary)
                                        if let port = child.port { Text("Port \(port)").foregroundStyle(.secondary) }
                                    }
                                    .font(.caption)
                                }
                            }
                            if let snapshot {
                                Divider()
                                HStack {
                                    resourceMetric("Child processes", "\(selectedSession.children.count)")
                                    resourceMetric("Orphan ports", "\(snapshot.orphanPorts.count)")
                                    resourceMetric("Rate limits", snapshot.rateLimits.isEmpty ? "Unavailable" : "Available")
                                }
                                if !snapshot.rateLimits.isEmpty {
                                    ForEach(snapshot.rateLimits) { rate in
                                        VStack(alignment: .leading, spacing: 4) {
                                            Text(rate.source).font(.caption.weight(.semibold))
                                            if let pct = rate.fiveHourPercent {
                                                ProgressView(value: pct, total: 100).accessibilityLabel("Five hour limit \(Int(pct)) percent used")
                                            }
                                            Text(rateDescription(rate)).font(.caption).foregroundStyle(.secondary)
                                        }
                                    }
                                }
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }

                    if let snapshot, !snapshot.orphanPorts.isEmpty {
                        KnowledgeCard {
                            VStack(alignment: .leading, spacing: 6) {
                                Text("Orphan ports · observed only").font(.headline)
                                ForEach(snapshot.orphanPorts) { port in
                                    Text("Port \(port.port) · PID \(port.pid.map(String.init) ?? "Unknown") · \(port.projectName ?? "Project unknown")")
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                            }
                        }
                    }

                    WorkNotice(title: "Monitoring only", message: "Session controls remain in the coding tool that owns the work. This screen cannot send commands or change session state.")
                }
                .frame(maxWidth: 820, alignment: .leading)
                .padding(.vertical, 4)
                .padding(.horizontal, 12)
            }
        } else {
            ContentUnavailableView("Select a coding session", systemImage: "person.crop.rectangle",
                                  description: Text("Choose a session to inspect its resources and last reported status."))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    @MainActor private func refresh() async {
        let ticket = requestGate.begin(revision: context.revision, resourceID: "agents")
        loading = true
        refreshError = nil
        do {
            let response = try await WorkService(context: context).agentSnapshot()
            try Task.checkCancellation()
            guard requestGate.accepts(ticket, revision: context.revision, resourceID: "agents") else { return }
            snapshot = response
            if !sessions.contains(where: { $0.id == selectedSessionID }) { selectedSessionID = sessions.first?.id }
        } catch is CancellationError {
            return
        } catch {
            guard requestGate.accepts(ticket, revision: context.revision, resourceID: "agents") else { return }
            refreshError = error
        }
        if requestGate.accepts(ticket, revision: context.revision, resourceID: "agents") { loading = false }
    }

    private func isCurrentProject(_ session: WorkAgentSession) -> Bool {
        if session.projectName == context.profile.projectSlug { return true }
        guard let expected = context.profile.repositoryRoot?.standardizedFileURL.path,
              let cwd = session.cwd?.replacingOccurrences(of: "\\", with: "/") else { return false }
        return URL(fileURLWithPath: cwd).standardizedFileURL.path == expected
    }

    private func sessionTitle(_ session: WorkAgentSession) -> String {
        let name = session.currentTasks.first ?? session.projectName ?? session.agentCLI ?? "Coding session"
        if isCurrentProject(session), let cli = session.agentCLI { return "\(context.profile.projectSlug) · \(cli)" }
        return name
    }

    private func elapsed(startedAt: TimeInterval?) -> String {
        guard let startedAt else { return "Not reported" }
        let seconds = max(0, Int(Date.now.timeIntervalSince1970 - startedAt))
        if seconds < 60 { return "\(seconds) seconds" }
        if seconds < 3600 { return "\(seconds / 60) minutes" }
        return "\(seconds / 3600) hours \((seconds % 3600) / 60) minutes"
    }

    private func gitSummary(_ session: WorkAgentSession) -> String {
        let branch = session.gitBranch ?? "Branch not reported"
        guard let added = session.gitAdded, let modified = session.gitModified else { return branch }
        return "\(branch) · \(added) added · \(modified) modified"
    }

    private func rateDescription(_ rate: WorkRateLimit) -> String {
        var parts: [String] = []
        if let value = rate.fiveHourPercent { parts.append("5-hour \(Int(value.rounded()))% used") }
        if let value = rate.sevenDayPercent { parts.append("7-day \(Int(value.rounded()))% used") }
        return parts.isEmpty ? "Usage limits not reported" : parts.joined(separator: " · ")
    }
}

private func workMetric(_ title: String, _ value: String) -> some View {
    LabeledContent(title, value: value)
}

private func resourceMetric(_ title: String, _ value: String) -> some View {
    VStack(alignment: .leading, spacing: 3) {
        Text(title).font(.caption).foregroundStyle(.secondary)
        Text(value).font(.callout)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
}
