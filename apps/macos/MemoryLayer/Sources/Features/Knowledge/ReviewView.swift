import SwiftUI

private struct ReviewMemorySheet: Identifiable {
    let id: String
}

struct ReviewView: View {
    let context: FeatureContext

    @State private var proposals: [ReplacementProposal] = []
    @State private var selectedProposalID: String?
    @State private var policy: ReplacementPolicyResponse?
    @State private var isLoading = true
    @State private var isResolving = false
    @State private var isSavingPolicy = false
    @State private var errorMessage: String?
    @State private var requestGate = KnowledgeRequestGate()
    @State private var policyError: String?
    @State private var policySheet = false
    @State private var policySelection = "balanced"
    @State private var memorySheet: ReviewMemorySheet?
    @State private var refreshID = 0
    @State private var actionTask: Task<Void, Never>?
    @State private var resultMessage: String?
    @State private var pendingDecision: (proposal: ReplacementProposal, approve: Bool)?

    private var service: KnowledgeService { KnowledgeService(context: context) }
    private var activeProposal: ReplacementProposal? { proposals.first(where: { $0.id == selectedProposalID }) ?? proposals.first }
    private var repositoryRoot: URL? {
        guard let value = policy?.repositoryRoot ?? context.profile.repositoryRoot?.path else { return nil }
        return URL(fileURLWithPath: value)
    }
    private var canCurate: Bool { context.permissions.allows(.memoryCurate) && !context.permissions.readOnly }
    private var canEditPolicy: Bool { canCurate && policy?.writable == true && repositoryRoot != nil }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            KnowledgePageHeader(
                title: "Review",
                subtitle: "\(proposals.count) replacement proposal\(proposals.count == 1 ? "" : "s") need a decision",
                actionTitle: policy.map { "Policy: \($0.replacementPolicy.capitalized)" } ?? "Policy"
            ) {
                policySelection = policy?.replacementPolicy ?? "balanced"
                policySheet = true
            }

            if let resultMessage {
                Label(resultMessage, systemImage: "checkmark.circle.fill")
                    .foregroundStyle(.green).accessibilityIdentifier("review.result")
            }

            if !context.permissions.allows(.memoryRead) {
                KnowledgeStateView(kind: .restricted, context: context, retry: nil)
            } else if isLoading && policy == nil && proposals.isEmpty {
                KnowledgeStateView(kind: .loading, context: context, retry: nil)
            } else if let errorMessage, policy == nil && proposals.isEmpty {
                KnowledgeStateView(kind: stateKind(for: errorMessage), context: context, retry: { refreshID += 1 })
            } else if proposals.isEmpty {
                if let errorMessage {
                    Label(errorMessage, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                }
                if let policyError {
                    Label("Policy unavailable: \(policyError)", systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                }
                GeometryReader { geometry in
                    if geometry.size.width < 720 {
                        VStack(spacing: 14) { proposalQueue; proposalDetail }
                    } else {
                        HSplitView {
                            proposalQueue.frame(minWidth: 255, idealWidth: min(330, geometry.size.width * 0.31), maxWidth: 390)
                            proposalDetail.frame(minWidth: 420, maxWidth: .infinity, maxHeight: .infinity)
                        }
                    }
                }
            } else {
                if let errorMessage { Label(errorMessage, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange) }
                if let policyError { Label("Policy unavailable: \(policyError)", systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange) }
                GeometryReader { geometry in
                    if geometry.size.width < 720 {
                        VStack(spacing: 14) { proposalQueue.frame(height: min(310, max(220, geometry.size.height * 0.38))); proposalDetail }
                    } else {
                        HSplitView {
                            proposalQueue.frame(minWidth: 255, idealWidth: min(330, geometry.size.width * 0.31), maxWidth: 390)
                            proposalDetail.frame(minWidth: 420, maxWidth: .infinity, maxHeight: .infinity)
                        }
                    }
                }
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task(id: "\(context.revision)-\(refreshID)") { await refresh() }
        .onDisappear { actionTask?.cancel(); requestGate.invalidate() }
        .confirmationDialog(pendingDecision?.approve == true ? "Approve replacement?" : "Reject proposal?",
            isPresented: Binding(get: { pendingDecision != nil }, set: { if !$0 { pendingDecision = nil } }),
            titleVisibility: .visible) {
            if let decision = pendingDecision {
                Button(decision.approve ? "Approve replacement" : "Reject proposal", role: .destructive) {
                    pendingDecision = nil
                    launchAction { await resolve(decision.proposal, approve: decision.approve) }
                }
            }
            Button("Cancel", role: .cancel) { pendingDecision = nil }
        } message: {
            Text(pendingDecision?.approve == true
                ? "Replace the canonical content for this memory with the proposed version? Its version history is retained."
                : "Remove this proposal from the pending review queue?")
        }
        .sheet(isPresented: $policySheet) { policyEditor }
        .sheet(item: $memorySheet) { request in
            KnowledgeMemoryDetailView(context: context, memoryID: request.id)
        }
        .alert("Policy could not be saved", isPresented: Binding(
            get: { policyError != nil }, set: { if !$0 { policyError = nil } }
        )) {
            Button("OK") { policyError = nil }
        } message: { Text(policyError ?? "") }
    }

    private var proposalQueue: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Pending proposals").font(.headline)
                Spacer()
                Button("Refresh", systemImage: "arrow.clockwise") { refreshID += 1 }
                    .disabled(isResolving || isSavingPolicy)
                    .labelStyle(.iconOnly)
                    .help("Refresh proposals")
                    .accessibilityIdentifier("review.refresh")
            }
            if proposals.isEmpty {
                ContentUnavailableView("No pending proposals", systemImage: "checkmark.circle", description: Text("Replacement candidates will appear here when the service queues them for review."))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List(selection: $selectedProposalID) {
                    ForEach(proposals) { proposal in
                        Button {
                            selectedProposalID = proposal.id
                        } label: {
                            VStack(alignment: .leading, spacing: 6) {
                                Text(proposal.targetSummary).font(.subheadline.weight(.semibold)).lineLimit(2)
                                Text(proposal.candidateSummary).font(.callout).foregroundStyle(.secondary).lineLimit(2)
                                HStack {
                                    Text("Replacement · \(proposal.candidateMemoryType.replacingOccurrences(of: "_", with: " ").capitalized)")
                                    Spacer()
                                    Text("Score \(proposal.score.formatted(.number.precision(.fractionLength(2))))")
                                }
                                .font(.caption).foregroundStyle(.secondary)
                            }
                            .padding(10)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(proposal.id == (selectedProposalID ?? proposals.first?.id) ? Color.accentColor.opacity(0.12) : Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 9))
                            .contentShape(RoundedRectangle(cornerRadius: 9))
                        }
                        .buttonStyle(.plain)
                        .tag(proposal.id)
                        .listRowSeparator(.hidden)
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets(top: 4, leading: 4, bottom: 4, trailing: 4))
                        .accessibilityIdentifier("review.proposal.\(proposal.id)")
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

    @ViewBuilder private var proposalDetail: some View {
        if let proposal = activeProposal {
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    HStack(alignment: .top) {
                        VStack(alignment: .leading, spacing: 7) {
                            Text(proposal.targetSummary).font(.title2.weight(.semibold)).fixedSize(horizontal: false, vertical: true)
                            HStack {
                                KnowledgeTag(text: "Pending", tone: .amber)
                                KnowledgeTag(text: "Score \(proposal.score.formatted(.number.precision(.fractionLength(2))))", tone: .blue)
                                Text("Proposed replacement · \(proposal.policy.capitalized) policy").font(.callout).foregroundStyle(.secondary)
                            }
                        }
                        Spacer(minLength: 10)
                        Button("Open existing memory") {
                            memorySheet = ReviewMemorySheet(id: proposal.targetMemoryID)
                        }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("review.openExisting")
                    }

                    HStack(alignment: .top, spacing: 12) {
                        proposalCard(title: "Current summary", text: proposal.targetSummary, detail: "The existing canonical memory")
                        proposalCard(title: "Proposed replacement", text: proposal.candidateSummary, detail: proposal.candidateCanonicalText, emphasized: true)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)

                    VStack(alignment: .leading, spacing: 8) {
                        Text("Why it was proposed").font(.headline)
                        if proposal.reasons.isEmpty {
                            Text("The service did not include proposal reasons.").foregroundStyle(.secondary)
                        } else {
                            ForEach(Array(proposal.reasons.enumerated()), id: \.offset) { _, reason in
                                Label(reason, systemImage: "checkmark.circle").font(.callout)
                            }
                        }
                    }
                    KnowledgeCard {
                        VStack(alignment: .leading, spacing: 8) {
                            LabeledContent("Memory type", value: proposal.candidateMemoryType.replacingOccurrences(of: "_", with: " ").capitalized)
                            LabeledContent("Policy", value: proposal.policy.capitalized)
                            LabeledContent("Score", value: proposal.score.formatted(.number.precision(.fractionLength(2))))
                            LabeledContent("Queued", value: knowledgeDate(proposal.createdAt))
                        }
                        .font(.callout)
                    }

                    if let errorMessage { Label(errorMessage, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange) }
                    HStack(spacing: 8) {
                        Button("Approve replacement", systemImage: "checkmark") { pendingDecision = (proposal, true) }
                            .buttonStyle(.borderedProminent)
                            .disabled(!canCurate || isResolving)
                            .accessibilityIdentifier("review.approve")
                        Button("Reject proposal", systemImage: "xmark") { pendingDecision = (proposal, false) }
                            .buttonStyle(.bordered)
                            .disabled(!canCurate || isResolving)
                            .accessibilityIdentifier("review.reject")
                        if isResolving { ProgressView().controlSize(.small) }
                    }
                    if !canCurate {
                        Label("This connection is read-only or does not have the curation permission.", systemImage: "lock.fill")
                            .font(.caption).foregroundStyle(.secondary)
                    } else {
                        Text("Approval updates the canonical memory. Rejection keeps the current version.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .accessibilityIdentifier("review.detail")
        } else if isLoading {
            ProgressView("Loading proposals…").frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            ContentUnavailableView("Choose a proposal", systemImage: "checkmark.square", description: Text("Select a pending replacement to compare the current and proposed content."))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func proposalCard(title: String, text: String, detail: String, emphasized: Bool = false) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title).font(.caption).foregroundStyle(emphasized ? Color.accentColor : Color.secondary)
            Text(text).font(.headline).fixedSize(horizontal: false, vertical: true)
            if detail != text { Text(detail).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true) }
        }
        .frame(maxWidth: .infinity, minHeight: 122, alignment: .topLeading)
        .padding(12)
        .background(emphasized ? Color.accentColor.opacity(0.09) : Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 9))
        .overlay { RoundedRectangle(cornerRadius: 9).stroke(Color(nsColor: .separatorColor).opacity(0.65), lineWidth: 0.7) }
    }

    private var policyEditor: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Replacement policy").font(.title2.weight(.semibold))
            Text("Applies to the repository associated with this project.").font(.callout).foregroundStyle(.secondary)
            if let root = repositoryRoot {
                LabeledContent("Repository", value: root.path).font(.caption).foregroundStyle(.secondary)
            } else {
                Label("Set a repository path in the connection before editing policy.", systemImage: "exclamationmark.triangle")
                    .font(.callout).foregroundStyle(.orange)
            }
            if let policyError {
                Label(policyError, systemImage: "exclamationmark.triangle.fill")
                    .font(.callout).foregroundStyle(.orange)
            }
            Picker("Policy", selection: $policySelection) {
                Text("Conservative").tag("conservative")
                Text("Balanced").tag("balanced")
                Text("Aggressive").tag("aggressive")
            }
            .pickerStyle(.menu)
            .disabled(!canEditPolicy || isSavingPolicy)
            VStack(alignment: .leading, spacing: 5) {
                Text("Conservative · Prefer retaining existing memory")
                Text("Balanced · Weigh evidence and confidence")
                Text("Aggressive · Prefer stronger new evidence")
            }
            .font(.callout)
            Text("Proposal decisions remain available when policy editing is disabled.")
                .font(.caption).foregroundStyle(.secondary)
            if !canCurate {
                Label("This connection can inspect policy but does not have curation permission.", systemImage: "lock.fill")
                    .font(.caption).foregroundStyle(.secondary)
            } else if !canEditPolicy {
                Label("Policy editing is unavailable for this repository connection.", systemImage: "lock.fill")
                    .font(.caption).foregroundStyle(.secondary)
            }
            HStack {
                Button("Cancel") { policySheet = false }.keyboardShortcut(.cancelAction).disabled(isSavingPolicy)
                Spacer()
                if isSavingPolicy { ProgressView().controlSize(.small) }
                Button("Save policy") { launchAction { await savePolicy() } }
                    .buttonStyle(.borderedProminent)
                    .disabled(!canEditPolicy || isSavingPolicy || policySelection == policy?.replacementPolicy)
                    .accessibilityIdentifier("review.savePolicy")
            }
        }
        .padding(20)
        .frame(minWidth: 480, maxWidth: 560)
        .accessibilityIdentifier("review.policySheet")
        .interactiveDismissDisabled(isSavingPolicy)
    }

    private func refresh() async {
        let current = requestGate.begin(revision: context.revision)
        isLoading = true
        errorMessage = nil
        policyError = nil
        do {
            let proposalResult = try await service.proposals()
            guard !Task.isCancelled, requestGate.accepts(current, revision: context.revision) else { return }
            proposals = proposalResult.proposals
            if !proposals.contains(where: { $0.id == selectedProposalID }) { selectedProposalID = proposals.first?.id }
        } catch is CancellationError {
            return
        } catch {
            guard !Task.isCancelled, requestGate.accepts(current, revision: context.revision) else { return }
            errorMessage = error.localizedDescription
        }
        guard !Task.isCancelled, requestGate.accepts(current, revision: context.revision) else { return }
        do {
            let loadedPolicy = try await service.replacementPolicy()
            guard !Task.isCancelled, requestGate.accepts(current, revision: context.revision) else { return }
            policy = loadedPolicy
        } catch is CancellationError {
            return
        } catch {
            guard !Task.isCancelled, requestGate.accepts(current, revision: context.revision) else { return }
            policyError = error.localizedDescription
        }
        guard !Task.isCancelled, requestGate.accepts(current, revision: context.revision) else { return }
        isLoading = false
    }

    private func resolve(_ proposal: ReplacementProposal, approve: Bool) async {
        guard canCurate, !isResolving else { return }
        isResolving = true
        errorMessage = nil
        resultMessage = nil
        do {
            let result = try await service.resolveProposal(id: proposal.id, approve: approve)
            try Task.checkCancellation()
            requestGate.invalidate()
            resultMessage = "Proposal \(result.status): \(proposal.candidateSummary)"
            proposals.removeAll { $0.id == proposal.id }
            selectedProposalID = proposals.first?.id
            refreshID += 1
        } catch is CancellationError {
            errorMessage = "The decision was interrupted. Its outcome is unknown; refresh before trying again."
        } catch {
            errorMessage = error.localizedDescription
        }
        isResolving = false
    }

    private func savePolicy() async {
        guard canEditPolicy, !isSavingPolicy, let repositoryRoot else { return }
        isSavingPolicy = true
        do {
            let updated = try await service.saveReplacementPolicy(policySelection, repositoryRoot: repositoryRoot)
            try Task.checkCancellation()
            policy = updated
            policySheet = false
        } catch {
            policyError = error.localizedDescription
        }
        isSavingPolicy = false
    }

    private func launchAction(_ action: @escaping () async -> Void) {
        guard actionTask == nil else { return }
        actionTask = Task { await action(); actionTask = nil }
    }

    private func stateKind(for message: String) -> KnowledgeStateView.Kind {
        if message.localizedCaseInsensitiveContains("401") || message.localizedCaseInsensitiveContains("credentials") { return .authRequired }
        if message.localizedCaseInsensitiveContains("403") || message.localizedCaseInsensitiveContains("permission") { return .restricted }
        if message.localizedCaseInsensitiveContains("connect") || message.localizedCaseInsensitiveContains("offline") { return .unavailable }
        return .failed(message)
    }
}
