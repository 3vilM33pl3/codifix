import SwiftUI

public struct AutomationsView: View {
    public let context: FeatureContext
    @State private var cards: [AutomationCardState] = []
    @State private var selectedLoopID: String?
    @State private var globalState: LoopGlobalState?
    @State private var recentRuns: [LoopRunSummary] = []
    @State private var selectedRun: LoopRunDetail?
    @State private var approvals: [LoopApproval] = []
    @State private var memoryProposals: [LoopMemoryProposal] = []
    @State private var queueLoading = false
    @State private var queueError: String?
    @State private var loading = true
    @State private var busy = false
    @State private var refreshID = UUID()
    @State private var loadError: String?
    @State private var actionError: String?
    @State private var actionResult: String?
    @State private var operation: String?
    @State private var actionTask: Task<Void, Never>?
    @State private var actionTaskID = UUID()
    @State private var loadRequestID = UUID()
    @State private var selectedReadID = UUID()
    @State private var queueRequestID = UUID()
    @State private var inspectRequest: InspectRunRequest?
    @State private var selectedRunLoading = false
    @State private var approvalQueuePresented = false
    @State private var approvalEdits: [String: String] = [:]
    @State private var proposalEdits: [String: String] = [:]
    @State private var invalidJSON: String?
    @State private var rejectTarget: AutomationDecisionTarget?
    @State private var stopTarget: GlobalStopTarget?

    public init(context: FeatureContext) { self.context = context }

    private var service: AutomationService { AutomationService(context: context) }
    private var selectedCard: AutomationCardState? { cards.first { $0.definition.loopID == selectedLoopID } }
    private var inspectTaskKey: InspectRunTaskKey { InspectRunTaskKey(loopID: selectedLoopID, request: inspectRequest) }
    private var canConfigure: Bool { !context.permissions.readOnly && context.permissions.allows(.loopsConfigure) }
    private var canRun: Bool { !context.permissions.readOnly && context.permissions.allows(.loopsRun) }
    private var canGlobalStop: Bool { !context.permissions.readOnly && context.permissions.globalAllowed.contains(.loopsConfigure) }

    public var body: some View {
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 14) {
                ProjectPageHeader(title: "Automations", subtitle: "Registered loops, dry-run previews and approvals · \(context.profile.projectSlug)") {
                    HStack(spacing: 8) {
                        Button("Approval queue · \(approvals.count + memoryProposals.count)") { approvalQueuePresented = true }
                            .disabled(!context.permissions.allows(.memoryRead))
                        Button("Refresh") { refreshID = UUID() }.disabled(loading || busy)
                        Button(globalState?.killSwitchEnabled == true ? "Resume all loops" : "Stop all loops…") {
                            stopTarget = GlobalStopTarget(context: context, stopping: globalState?.killSwitchEnabled != true)
                        }
                        .disabled(!canGlobalStop || busy || globalState == nil)
                        .tint(globalState?.killSwitchEnabled == true ? .accentColor : .red)
                    }
                }
                if let loadError { ProjectBanner(title: "Refresh failed", message: "Showing the last data loaded. \(loadError)", tone: .failure) { refreshID = UUID() } }
                if let actionError { ProjectBanner(title: "Action failed", message: actionError, tone: .failure) }
                if let actionResult { ProjectBanner(title: operation ?? "Completed", message: actionResult, tone: .success) }
                if let globalState, globalState.killSwitchEnabled {
                    ProjectBanner(title: "Global loop stop is active", message: globalState.reason ?? "The service is blocking loop execution.", tone: .warning)
                }
            }
            .padding(.horizontal, 20).padding(.top, 18).padding(.bottom, 14)

            if !context.permissions.allows(.memoryRead) {
                ProjectLoadingState(context: context, title: "Access restricted", message: "This identity cannot read loop definitions, runs, or approvals.", isLoading: false)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if cards.isEmpty && loading {
                ProjectLoadingState(context: context, title: "Loading automations…", message: "Reading loop definitions and effective settings from the selected service.", isLoading: true)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if cards.isEmpty && loadError != nil {
                ProjectLoadingState(context: context, title: "Could not load automations", message: "The service did not return automation data for this project.", isLoading: false, error: loadError) { refreshID = UUID() }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                HSplitView {
                    loopList.frame(minWidth: 245, idealWidth: 290, maxWidth: 340, maxHeight: .infinity)
                    detail.frame(minWidth: 460, maxWidth: .infinity, maxHeight: .infinity)
                }
                .padding(.horizontal, 20).padding(.bottom, 18)
            }
        }
        .task(id: context.revision) {
            actionTask?.cancel()
            loadRequestID = UUID(); selectedReadID = UUID(); queueRequestID = UUID(); inspectRequest = nil
            selectedRunLoading = false
            cards = []; selectedLoopID = nil; globalState = nil; recentRuns = []; selectedRun = nil
            approvals = []; memoryProposals = []; loadError = nil; actionError = nil; actionResult = nil
            refreshID = UUID()
        }
        .task(id: refreshID) { await load() }
        .task(id: selectedLoopID) { selectedRunLoading = false; await loadSelected() }
        .task(id: inspectTaskKey) {
            let key = inspectTaskKey
            guard let request = key.request, request.loopID == key.loopID else { return }
            await inspectRun(request)
        }
        .onDisappear {
            actionTask?.cancel()
            loadRequestID = UUID(); selectedReadID = UUID(); queueRequestID = UUID(); inspectRequest = nil
            loading = false; selectedRunLoading = false; queueLoading = false
        }
        .sheet(isPresented: $approvalQueuePresented, onDismiss: { invalidJSON = nil }) { approvalQueueSheet }
        .confirmationDialog(stopTarget?.stopping == true ? "Stop all loops?" : "Resume all loops?", isPresented: Binding(get: { stopTarget != nil }, set: { if !$0 { stopTarget = nil } }), titleVisibility: .visible) {
            if stopTarget?.stopping == true {
                Button("Stop all loops", role: .destructive) {
                    guard let target = stopTarget else { return }; stopTarget = nil
                    launchAction { await setGlobalStop(true, using: target.context) }
                }
            } else {
                Button("Resume all loops") {
                    guard let target = stopTarget else { return }; stopTarget = nil
                    launchAction { await setGlobalStop(false, using: target.context) }
                }
            }
            Button("Cancel", role: .cancel) { stopTarget = nil }
        } message: {
            Text("This changes the service-wide loop kill switch for the selected project connection, \(stopTarget?.context.profile.projectSlug ?? context.profile.projectSlug). Global control requires global loops_configure permission.")
        }
    }

    private var loopList: some View {
        ScrollView {
            LazyVStack(spacing: 8) {
                ForEach(cards, id: \.definition.loopID) { card in
                    let selected = selectedLoopID == card.definition.loopID
                    Button { selectedLoopID = card.definition.loopID } label: {
                        VStack(alignment: .leading, spacing: 7) {
                            HStack { Text(card.definition.name).font(.subheadline.weight(.semibold)); Spacer(minLength: 4); Text(card.definition.riskLevel.capitalized).font(.caption2).foregroundStyle(.secondary) }
                            Text(card.definition.description).font(.caption).foregroundStyle(.secondary).lineLimit(2).frame(maxWidth: .infinity, alignment: .leading)
                            HStack(spacing: 5) {
                                Circle().fill(card.effectiveSettings?.enabled == true ? Color.green : Color.gray).frame(width: 6, height: 6)
                                Text(stateLabel(card)).font(.caption2)
                                Text("·").foregroundStyle(.tertiary)
                                Text(card.effectiveSettings?.mode.label ?? card.definition.defaultMode.label).font(.caption2)
                            }.foregroundStyle(.secondary)
                        }
                        .padding(12).frame(maxWidth: .infinity, alignment: .leading)
                        .background(selected ? Color.accentColor.opacity(0.09) : Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
                        .overlay { RoundedRectangle(cornerRadius: 8).stroke(selected ? Color.accentColor.opacity(0.35) : Color(nsColor: .separatorColor).opacity(0.45), lineWidth: 0.7) }
                        .contentShape(Rectangle())
                    }.buttonStyle(.plain)
                }
            }.padding(10)
        }
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 9))
        .overlay(alignment: .topLeading) {
            if cards.isEmpty && !loading { ContentUnavailableView("No loops registered", systemImage: "bolt.slash", description: Text("This service has no loop definitions." )).padding(16) }
        }
    }

    @ViewBuilder private var detail: some View {
        if let card = selectedCard {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    HStack(spacing: 8) {
                        Text(stateLabel(card)).font(.caption.weight(.semibold)).padding(.horizontal, 9).padding(.vertical, 5).background(Color.accentColor.opacity(0.09), in: Capsule())
                        Text("\(card.definition.riskLevel.capitalized) risk").font(.caption.weight(.medium)).padding(.horizontal, 9).padding(.vertical, 5).background(Color(nsColor: .controlBackgroundColor), in: Capsule())
                        Spacer()
                        Text("v\(card.definition.version)").font(.caption).foregroundStyle(.secondary)
                    }
                    Text(card.definition.name).font(.title3.weight(.semibold))
                    Text(card.definition.description).font(.callout).foregroundStyle(.secondary)
                    ProjectCard(title: "Loop settings") {
                        settingLine("Scope", value: card.effectiveSettings.map { $0.scopeType.rawValue + " · " + $0.scopeID } ?? "Definition default")
                        settingLine("Next trigger", value: triggerLabel(card.definition))
                        settingLine("Budget", value: budgetLabel(card.effectiveSettings?.budgets))
                        settingLine("Last run", value: card.lastRun.map { "\($0.status.capitalized) · \($0.startedAt.formatted(date: .abbreviated, time: .shortened))" } ?? "Never")
                        if let until = card.effectiveSettings?.pausedUntil { settingLine("Paused until", value: until.formatted(date: .abbreviated, time: .shortened)) }
                        if let until = card.effectiveSettings?.snoozedUntil { settingLine("Snoozed until", value: until.formatted(date: .abbreviated, time: .shortened)) }
                        Picker("Effective mode", selection: Binding(get: { LoopMode.configurationValue(effective: card.effectiveSettings?.mode, default: card.definition.defaultMode) }, set: { mode in launchAction { await perform("Set mode to \(mode.label)") { try await service.update(card.definition.loopID, action: "enable", mode: mode) } } })) {
                            ForEach(LoopMode.configurable) { mode in Text(mode.label).tag(mode) }
                        }.disabled(!canConfigure || busy).padding(.top, 4)
                    }
                    actionButtons(card)
                    ProjectBanner(title: "Preview run is a dry run", message: "Preview gathers context and evaluates policy without applying proposed actions. Every run request from this screen sets dry_run to true.", tone: .info)
                    if !recentRuns.isEmpty { runsCard }
                    if let selectedRun { runCard(selectedRun) }
                }
                .padding(14).frame(maxWidth: .infinity, alignment: .leading)
            }
        } else {
            ContentUnavailableView("Select an automation", systemImage: "bolt", description: Text("Choose a registered loop to inspect its scope, settings and runs."))
        }
    }

    @ViewBuilder private func actionButtons(_ card: AutomationCardState) -> some View {
        VStack(alignment: .leading, spacing: 9) {
            HStack(spacing: 8) {
                Button("Preview run") { launchAction { await preview(card.definition.loopID) } }.buttonStyle(.borderedProminent).disabled(!canRun || busy || selectedRunLoading || globalState?.killSwitchEnabled == true)
                Button("Inspect last run") { if let id = card.lastRun?.id { inspectRequest = InspectRunRequest(id: id, loopID: card.definition.loopID, token: UUID()) } }.disabled(card.lastRun == nil || busy || selectedRunLoading)
                if let operation { Text("\(operation)…").font(.caption).foregroundStyle(.secondary) }
            }
            HStack(spacing: 8) {
                Button("Enable") { launchAction { await perform("Enable loop") { try await service.update(card.definition.loopID, action: "enable", mode: LoopMode.configurationValue(effective: card.effectiveSettings?.mode, default: card.definition.defaultMode)) } } }.disabled(!canConfigure || busy || selectedRunLoading)
                Button("Disable") { launchAction { await perform("Disable loop") { try await service.update(card.definition.loopID, action: "disable") } } }.disabled(!canConfigure || busy || selectedRunLoading)
                Button("Pause 1 hour") { launchAction { await perform("Pause loop") { try await service.update(card.definition.loopID, action: "pause") } } }.disabled(!canConfigure || busy || selectedRunLoading)
                Button("Snooze 1 day") { launchAction { await perform("Snooze loop") { try await service.update(card.definition.loopID, action: "snooze") } } }.disabled(!canConfigure || busy || selectedRunLoading)
            }
            if let reasons = card.effectiveSettings?.blockedReasons, !reasons.isEmpty {
                Text(reasons.joined(separator: " · ")).font(.caption).foregroundStyle(.orange)
            }
        }
    }

    private var runsCard: some View {
        ProjectCard(title: "Recent runs") {
            ForEach(recentRuns) { run in
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("\(run.status.capitalized) · \(run.mode.label)").font(.subheadline.weight(.medium))
                        Text(run.startedAt.formatted(date: .abbreviated, time: .shortened) + " · \(run.traceCount) traces").font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button("Inspect") { if let loopID = selectedLoopID { inspectRequest = InspectRunRequest(id: run.id, loopID: loopID, token: UUID()) } }.controlSize(.small).disabled(busy || selectedRunLoading)
                }
                if run.id != recentRuns.last?.id { Divider() }
            }
        }
    }

    private func runCard(_ run: LoopRunDetail) -> some View {
        ProjectCard(title: "Run inspector · \(run.summary.status.capitalized)") {
            settingLine("Started", value: run.summary.startedAt.formatted(date: .abbreviated, time: .shortened))
            settingLine("Finished", value: run.summary.finishedAt?.formatted(date: .abbreviated, time: .shortened) ?? "In progress")
            settingLine("Run reason", value: run.runReason ?? "—")
            if let summary = run.summary.outputSummary, !summary.isEmpty { Text(summary).font(.callout).textSelection(.enabled) }
            DisclosureGroup("Output and policy details") {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Output").font(.caption.weight(.semibold)); Text(run.output.prettyPrinted()).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                    Text("Policy decisions").font(.caption.weight(.semibold)); Text(run.policyDecisions.prettyPrinted()).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                }.frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 6)
            }
            if !run.traces.isEmpty {
                DisclosureGroup("Trace · \(run.traces.count)") {
                    ForEach(run.traces) { trace in VStack(alignment: .leading, spacing: 4) { Text(trace.title).font(.subheadline.weight(.medium)); Text(trace.traceType).font(.caption).foregroundStyle(.secondary); Text(trace.payload.prettyPrinted()).font(.system(.caption2, design: .monospaced)).textSelection(.enabled) }.frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 5) }
                }
            }
            if !run.memoryProposals.isEmpty { Text("\(run.memoryProposals.count) memory proposal(s) associated with this run. Review them in the automation proposal queue.").font(.caption).foregroundStyle(.secondary) }
        }
    }

    private var approvalQueueSheet: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    Text("Loop approvals and memory proposals from \(context.profile.projectSlug) are shown here. Knowledge replacement proposals have their own Review screen.").font(.callout).foregroundStyle(.secondary)
                    if let invalidJSON { ProjectBanner(title: "Invalid JSON", message: invalidJSON, tone: .failure) }
                    if let queueError { ProjectBanner(title: "Approval queue refresh failed", message: "Showing the last items loaded. \(queueError)", tone: .failure) }
                    if queueLoading { ProgressView("Loading automation approvals…").frame(maxWidth: .infinity, alignment: .leading) }
                    if !queueLoading && queueError == nil && approvals.isEmpty && memoryProposals.isEmpty {
                        ContentUnavailableView("No pending automation items", systemImage: "checkmark.circle", description: Text("This project's loop approval and memory proposal queues are empty."))
                    }
                    if !approvals.isEmpty {
                        ProjectCard(title: "Loop action approvals · \(approvals.count)") {
                            ForEach(approvals) { approval in approvalRow(approval) }
                        }
                    }
                    if !memoryProposals.isEmpty {
                        ProjectCard(title: "Loop memory proposals · \(memoryProposals.count)") {
                            ForEach(memoryProposals) { proposal in proposalRow(proposal) }
                        }
                    }
                }.padding(18)
            }
            .navigationTitle("Automation approval queue")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Close") { approvalQueuePresented = false } }
                ToolbarItem(placement: .primaryAction) { Button("Refresh") { launchAction { await loadApprovals() } }.disabled(busy || queueLoading) }
            }
            .alert("Reject this loop item?", isPresented: Binding(get: { rejectTarget != nil }, set: { if !$0 { rejectTarget = nil } })) {
                Button("Reject item", role: .destructive) {
                    guard let target = rejectTarget else { return }; rejectTarget = nil
                    launchAction { await resolve(target, action: "reject") }
                }
                Button("Cancel", role: .cancel) { rejectTarget = nil }
            } message: {
                Text("This records a rejection for the selected loop approval or memory proposal.")
            }
        }
        .frame(minWidth: 620, minHeight: 540)
    }

    private func approvalRow(_ approval: LoopApproval) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack { Text(approval.actionType.replacingOccurrences(of: "_", with: " ").capitalized).font(.subheadline.weight(.semibold)); Spacer(); Text(approval.createdAt.formatted(date: .abbreviated, time: .shortened)).font(.caption).foregroundStyle(.secondary) }
            Text("\(approval.loopID) · \(approval.riskReason)").font(.caption).foregroundStyle(.secondary)
            TextEditor(text: Binding(get: { approvalEdits[approval.id] ?? AutomationJSON.object(approval.proposedAction).prettyPrinted() }, set: { approvalEdits[approval.id] = $0 }))
                .font(.system(.caption, design: .monospaced)).frame(minHeight: 90).overlay { RoundedRectangle(cornerRadius: 6).stroke(Color(nsColor: .separatorColor).opacity(0.6)) }
                .disabled(!canRun || busy || approval.status != "pending")
            HStack {
                Button("Approve") { launchAction { await resolve(.approval(context, approval), action: "approve") } }.disabled(!canRun || busy || approval.status != "pending")
                Button("Save edited action") { launchAction { await resolveEditedApproval(approval) } }.disabled(!canRun || busy || approval.status != "pending")
                Button("Reject…", role: .destructive) { rejectTarget = .approval(context, approval) }.disabled(!canRun || busy || approval.status != "pending")
            }.controlSize(.small)
        }.padding(.vertical, 4)
    }

    private func proposalRow(_ proposal: LoopMemoryProposal) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack { Text(proposal.proposalType.replacingOccurrences(of: "_", with: " ").capitalized).font(.subheadline.weight(.semibold)); Spacer(); Text("\(Int(proposal.confidence * 100))% confidence").font(.caption).foregroundStyle(.secondary) }
            Text("\(proposal.loopID) · \(proposal.riskNotes ?? "No risk note")").font(.caption).foregroundStyle(.secondary)
            TextEditor(text: Binding(get: { proposalEdits[proposal.id] ?? AutomationJSON.object(proposal.candidate).prettyPrinted() }, set: { proposalEdits[proposal.id] = $0 }))
                .font(.system(.caption, design: .monospaced)).frame(minHeight: 90).overlay { RoundedRectangle(cornerRadius: 6).stroke(Color(nsColor: .separatorColor).opacity(0.6)) }
                .disabled(!canRun || busy || proposal.status != "pending")
            DisclosureGroup("Evidence") { Text(proposal.evidence.prettyPrinted()).font(.system(.caption, design: .monospaced)).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading) }
            HStack {
                Button("Approve") { launchAction { await resolve(.proposal(context, proposal), action: "approve") } }.disabled(!canRun || busy || proposal.status != "pending")
                Button("Save edited candidate") { launchAction { await resolveEditedProposal(proposal) } }.disabled(!canRun || busy || proposal.status != "pending")
                Button("Reject…", role: .destructive) { rejectTarget = .proposal(context, proposal) }.disabled(!canRun || busy || proposal.status != "pending")
            }.controlSize(.small)
        }.padding(.vertical, 4)
    }

    private func settingLine(_ title: String, value: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text(title).font(.caption).foregroundStyle(.secondary).frame(width: 112, alignment: .leading)
            Text(value).font(.callout).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    private func stateLabel(_ card: AutomationCardState) -> String {
        if globalState?.killSwitchEnabled == true || card.effectiveSettings?.globalKillSwitch == true { return "Globally stopped" }
        guard let settings = card.effectiveSettings else { return "Definition default" }
        if !settings.enabled { return "Disabled" }
        if !settings.blockedReasons.isEmpty { return "Blocked" }
        return settings.mode.label
    }
    private func triggerLabel(_ definition: LoopDefinition) -> String {
        if case let .array(values)? = definition.triggerSpec["supported"]?.value {
            let supported = values.compactMap { value -> String? in if case let .string(x) = value.value { return x }; return nil }
            if let trigger = supported.first(where: { $0 != "manual" }) ?? supported.first { return trigger.replacingOccurrences(of: "_", with: " ").capitalized }
        }
        return "Manual"
    }
    private func budgetLabel(_ budgets: [String: AutomationJSON]?) -> String {
        guard let budgets, !budgets.isEmpty else { return "Inherited configured limit" }
        return AutomationJSON.object(budgets).prettyPrinted()
    }

    private func load() async {
        let token = UUID(); loadRequestID = token
        let revision = context.revision
        loading = true; loadError = nil
        defer { if loadRequestID == token { loading = false } }
        do {
            async let definitionsRequest = service.definitions()
            async let stateRequest = service.globalState()
            async let approvalsRequest = service.pendingApprovals()
            async let proposalsRequest = service.pendingMemoryProposals()
            let (definitions, state, pendingApprovals, pendingProposals) = try await (definitionsRequest, stateRequest, approvalsRequest, proposalsRequest)
            try Task.checkCancellation()
            guard loadRequestID == token, context.revision == revision else { return }
            var next: [AutomationCardState] = []
            for definition in definitions {
                async let detailRequest = service.definition(definition.loopID)
                async let runsRequest = service.recentRuns(loopID: definition.loopID, limit: 1)
                let (detail, runs) = try await (detailRequest, runsRequest)
                guard loadRequestID == token, context.revision == revision else { return }
                next.append(AutomationCardState(definition: detail.definition, effectiveSettings: detail.effectiveSettings, lastRun: runs.first))
            }
            try Task.checkCancellation()
            guard loadRequestID == token, context.revision == revision else { return }
            cards = next; globalState = state; approvals = pendingApprovals; memoryProposals = pendingProposals
            if selectedLoopID == nil || !next.contains(where: { $0.definition.loopID == selectedLoopID }) { selectedLoopID = next.first?.definition.loopID }
        } catch is CancellationError { }
        catch { if loadRequestID == token { loadError = error.localizedDescription } }
    }

    private func loadSelected() async {
        let token = UUID(); selectedReadID = token
        guard let id = selectedLoopID else { recentRuns = []; selectedRun = nil; return }
        do {
            async let runsRequest = service.recentRuns(loopID: id)
            let runs = try await runsRequest
            try Task.checkCancellation()
            guard selectedReadID == token, selectedLoopID == id else { return }
            recentRuns = runs
            selectedRun = nil
        } catch is CancellationError { }
        catch { if selectedReadID == token && selectedLoopID == id { actionError = error.localizedDescription } }
    }
    private func loadApprovals() async {
        let token = UUID(); queueRequestID = token
        queueLoading = true; queueError = nil
        defer { if queueRequestID == token { queueLoading = false } }
        do {
            async let a = service.pendingApprovals()
            async let p = service.pendingMemoryProposals()
            let (newA, newP) = try await (a, p); try Task.checkCancellation()
            guard queueRequestID == token else { return }
            approvals = newA; memoryProposals = newP
        } catch is CancellationError { }
        catch { if queueRequestID == token { queueError = error.localizedDescription; actionError = error.localizedDescription } }
    }

    private func perform<T: Sendable>(_ label: String, action: () async throws -> T) async {
        guard !busy else { return }
        busy = true; operation = label; actionError = nil; actionResult = nil
        do { _ = try await action(); try Task.checkCancellation(); actionResult = "\(label) completed."; await load() }
        catch is CancellationError { actionError = "\(label) was interrupted; the service outcome may be unknown. Refresh before retrying." }
        catch { actionError = error.localizedDescription }
        operation = nil; busy = false
    }
    private func setGlobalStop(_ stopped: Bool, using target: FeatureContext) async {
        guard !busy else { return }; busy = true; operation = stopped ? "Stopping all loops" : "Resuming all loops"; actionError = nil
        do { let result = try await AutomationService(context: target).setGlobalStop(stopped); try Task.checkCancellation(); globalState = result; actionResult = stopped ? "The service-wide loop kill switch is on." : "The service-wide loop kill switch is off."; await load() }
        catch is CancellationError { actionError = "The global control request was interrupted; its service outcome may be unknown. Refresh before retrying." }
        catch { actionError = error.localizedDescription }
        operation = nil; busy = false
    }
    private func preview(_ id: String) async {
        guard !busy, !selectedRunLoading else { return }
        let revision = context.revision
        busy = true; operation = "Previewing loop"; actionError = nil; actionResult = nil
        defer { operation = nil; busy = false }
        do {
            let run = try await service.previewRun(id)
            try Task.checkCancellation()
            actionResult = "Dry-run preview completed for \(id)."
            if context.revision == revision, selectedLoopID == id { selectedRun = run }
            await loadApprovals()
            let runs = try await service.recentRuns(loopID: id)
            try Task.checkCancellation()
            if context.revision == revision, selectedLoopID == id { recentRuns = runs }
        } catch is CancellationError { actionError = "The dry-run preview was interrupted; the service may have recorded a run. Refresh before retrying." }
        catch { actionError = error.localizedDescription }
    }
    private func inspectRun(_ request: InspectRunRequest) async {
        guard !busy, actionTask == nil else { return }
        let revision = context.revision
        selectedRunLoading = true; operation = "Loading run"; actionError = nil
        defer {
            if inspectRequest == request && selectedLoopID == request.loopID {
                operation = nil; selectedRunLoading = false
            }
        }
        do {
            let detail = try await service.run(id: request.id)
            try Task.checkCancellation()
            guard inspectRequest == request, selectedLoopID == request.loopID, context.revision == revision else { return }
            selectedRun = detail
        } catch is CancellationError { }
        catch { if inspectRequest == request && selectedLoopID == request.loopID { actionError = error.localizedDescription } }
    }
    private func resolve(_ target: AutomationDecisionTarget, action: String, edited: AutomationJSON? = nil) async {
        guard !busy else { return }; busy = true; operation = action.capitalized; actionError = nil; actionResult = nil
        do {
            switch target {
            case let .approval(snapshot, approval): _ = try await AutomationService(context: snapshot).decide(approval, action: action, edited: edited)
            case let .proposal(snapshot, proposal): _ = try await AutomationService(context: snapshot).decide(proposal, action: action, edited: edited)
            }
            try Task.checkCancellation()
            actionResult = "The loop item was \(action)d."; await loadApprovals()
        } catch is CancellationError { actionError = "The \(action) request was interrupted; the service outcome may be unknown. Refresh the queue before retrying." }
        catch { actionError = error.localizedDescription }
        operation = nil; busy = false
    }
    private func resolveEditedApproval(_ approval: LoopApproval) async {
        do { let value = try parseJSON(approvalEdits[approval.id] ?? AutomationJSON.object(approval.proposedAction).prettyPrinted()); invalidJSON = nil; await resolve(.approval(context, approval), action: "edit", edited: value) }
        catch { invalidJSON = error.localizedDescription }
    }
    private func resolveEditedProposal(_ proposal: LoopMemoryProposal) async {
        do { let value = try parseJSON(proposalEdits[proposal.id] ?? AutomationJSON.object(proposal.candidate).prettyPrinted()); invalidJSON = nil; await resolve(.proposal(context, proposal), action: "edit", edited: value) }
        catch { invalidJSON = error.localizedDescription }
    }
    private func parseJSON(_ text: String) throws -> AutomationJSON {
        guard let data = text.data(using: .utf8) else { throw JSONEditError.invalid("Text could not be encoded as UTF-8.") }
        do { return try JSONDecoder().decode(AutomationJSON.self, from: data) }
        catch { throw JSONEditError.invalid(error.localizedDescription) }
    }

    private func launchAction(_ action: @escaping () async -> Void) {
        guard actionTask == nil, !busy, !selectedRunLoading else { return }
        let token = UUID(); actionTaskID = token
        actionTask = Task {
            await action()
            guard actionTaskID == token else { return }
            actionTask = nil
        }
    }
}

private struct AutomationCardState {
    let definition: LoopDefinition
    let effectiveSettings: EffectiveLoopSettings?
    let lastRun: LoopRunSummary?
}
private enum AutomationDecisionTarget {
    case approval(FeatureContext, LoopApproval)
    case proposal(FeatureContext, LoopMemoryProposal)
}
private enum JSONEditError: LocalizedError { case invalid(String); var errorDescription: String? { if case let .invalid(value) = self { return "Edited JSON is invalid: \(value)" }; return nil } }
private struct GlobalStopTarget { let context: FeatureContext; let stopping: Bool }
private struct InspectRunRequest: Hashable { let id: String; let loopID: String; let token: UUID }
private struct InspectRunTaskKey: Hashable { let loopID: String?; let request: InspectRunRequest? }
