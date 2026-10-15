import SwiftUI

private struct MemorySheetRequest: Identifiable {
    let id: String
}

struct QueryView: View {
    let context: FeatureContext
    @ObservedObject var sessionStore: KnowledgeSessionStore

    @State private var question = ""
    @State private var includeStale = false
    @State private var response: QueryResponse?
    @State private var selectedResultID: String?
    @State private var isLoading = false
    @State private var errorMessage: String?
    @State private var errorKind: KnowledgeStateView.Kind?
    @State private var requestGate = KnowledgeRequestGate()
    @State private var showsHistory = false
    @State private var memorySheet: MemorySheetRequest?

    private var service: KnowledgeService { KnowledgeService(context: context) }
    private var queryHistory: [QueryHistoryEntry] {
        sessionStore.queryHistory(profileID: context.profile.id, project: context.profile.projectSlug)
    }
    private var selectedResult: QueryResult? { response?.results.first(where: { $0.id == selectedResultID }) ?? response?.results.first }

    init(context: FeatureContext, sessionStore: KnowledgeSessionStore) {
        self.context = context
        self.sessionStore = sessionStore
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            KnowledgePageHeader(title: "Query", subtitle: "Ask a project question and inspect the evidence")

            if !context.permissions.allows(.memoryRead) {
                KnowledgeStateView(kind: .restricted, context: context, retry: nil)
            } else {
                queryForm
                if let errorMessage, let errorKind {
                    KnowledgeStateView(kind: errorKind,
                        context: context, retry: nil)
                        .frame(minHeight: 140, maxHeight: 200)
                }
                if let response {
                    if response.insufficientEvidence {
                        evidenceWarning
                    }
                    resultContent(response)
                } else if !isLoading && errorMessage == nil {
                    ContentUnavailableView("Ask this project", systemImage: "text.magnifyingglass", description: Text("Answers are grounded in the selected project's memories and cited evidence."))
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .task(id: context.revision) {
            requestGate.invalidate()
            isLoading = false
            response = nil
            selectedResultID = nil
            errorMessage = nil
            errorKind = nil
        }
        .sheet(isPresented: $showsHistory) { historySheet }
        .sheet(item: $memorySheet) { request in
            KnowledgeMemoryDetailView(context: context, memoryID: request.id)
        }
    }

    private var queryForm: some View {
        KnowledgeCard {
            VStack(alignment: .leading, spacing: 12) {
                Text("Question").font(.caption).foregroundStyle(.secondary)
                TextField("Ask a question about this project…", text: $question, axis: .vertical)
                    .lineLimit(1...3)
                    .textFieldStyle(.roundedBorder)
                    .accessibilityIdentifier("query.question")
                    .onSubmit { Task { await ask() } }
                HStack(spacing: 10) {
                    Button {
                        Task { await ask() }
                    } label: {
                        if isLoading { Label("Searching…", systemImage: "hourglass") }
                        else { Label("Ask question", systemImage: "arrow.up.right") }
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(isLoading || question.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    .accessibilityIdentifier("query.submit")
                    Button("History") { showsHistory = true }
                        .buttonStyle(.bordered)
                        .disabled(queryHistory.isEmpty)
                        .accessibilityIdentifier("query.history")
                    Spacer()
                    Toggle("Include stale results in ranking", isOn: $includeStale)
                        .toggleStyle(.checkbox)
                        .font(.callout)
                        .accessibilityIdentifier("query.includeStale")
                }
            }
        }
    }

    @ViewBuilder private func resultContent(_ result: QueryResponse) -> some View {
        GeometryReader { geometry in
            if geometry.size.width < 720 {
                VStack(spacing: 12) {
                    rankedEvidence(result)
                        .frame(height: min(320, max(220, geometry.size.height * 0.4)))
                    answerAndSource(result)
                }
            } else {
                HSplitView {
                    rankedEvidence(result).frame(minWidth: 245, idealWidth: min(310, geometry.size.width * 0.31), maxWidth: 390)
                    answerAndSource(result).frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
        }
    }

    private func rankedEvidence(_ result: QueryResponse) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .firstTextBaseline) {
                Text("Ranked evidence").font(.headline)
                Spacer()
                Text("\(result.results.count) memories").font(.caption).foregroundStyle(.secondary)
            }
            Text("Ranked by relevance").font(.callout).foregroundStyle(.secondary)
            if result.results.isEmpty {
                ContentUnavailableView("No evidence returned", systemImage: "doc.text.magnifyingglass", description: Text("The answer has no memory records to inspect."))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List(selection: $selectedResultID) {
                    ForEach(Array(result.results.enumerated()), id: \.element.id) { index, item in
                        Button {
                            selectedResultID = item.id
                        } label: {
                            VStack(alignment: .leading, spacing: 6) {
                                Text("\(index + 1). \(item.summary)").font(.subheadline.weight(.semibold)).lineLimit(2)
                                Text(item.snippet).font(.callout).foregroundStyle(.secondary).lineLimit(3)
                                HStack {
                                    Text("\(item.matchKind.capitalized) · \(item.score.formatted(.number.precision(.fractionLength(2))))")
                                    if result.answerCitations.contains(where: { $0.memoryID == item.memoryID }) { Text("· Cited") }
                                    Spacer()
                                    Text(item.memoryType.replacingOccurrences(of: "_", with: " ").capitalized)
                                }
                                .font(.caption).foregroundStyle(.secondary)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(10)
                            .background(item.id == selectedResultID ? Color.accentColor.opacity(0.12) : Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 9))
                            .contentShape(RoundedRectangle(cornerRadius: 9))
                        }
                        .buttonStyle(.plain)
                        .tag(item.id)
                        .listRowSeparator(.hidden)
                        .listRowBackground(Color.clear)
                        .listRowInsets(EdgeInsets(top: 4, leading: 4, bottom: 4, trailing: 4))
                        .accessibilityIdentifier("query.result.\(index + 1)")
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

    private func answerAndSource(_ result: QueryResponse) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                HStack(spacing: 8) {
                    KnowledgeTag(text: result.insufficientEvidence ? "Evidence insufficient" : "Evidence sufficient", tone: result.insufficientEvidence ? .amber : .green)
                    KnowledgeTag(text: "Confidence \(result.confidence.formatted(.percent.precision(.fractionLength(0))))", tone: .blue)
                }
                Text("Answer").font(.headline)
                Text(result.answer.isEmpty ? "No answer was returned for this question." : result.answer)
                    .font(.body).fixedSize(horizontal: false, vertical: true).textSelection(.enabled)

                if !result.answerCitations.isEmpty {
                    FlowLayout(spacing: 8) {
                        ForEach(Array(result.answerCitations.enumerated()), id: \.offset) { _, citation in
                            Button {
                                selectedResultID = citation.memoryID
                                memorySheet = MemorySheetRequest(id: citation.memoryID)
                            } label: {
                                Label("[\(citation.resultNumber)] \(citation.summary)", systemImage: "doc.text.magnifyingglass")
                                    .lineLimit(1)
                            }
                            .buttonStyle(.bordered)
                            .accessibilityIdentifier("query.citation.\(citation.resultNumber)")
                        }
                    }
                }

                HStack(spacing: 8) {
                    Button("Inspect source", systemImage: "sidebar.right") {
                        if let selectedResult { memorySheet = MemorySheetRequest(id: selectedResult.memoryID) }
                    }
                    .buttonStyle(.bordered)
                    .disabled(selectedResult == nil)
                    .accessibilityIdentifier("query.inspectSource")
                    Button("Open memory", systemImage: "arrow.up.right.square") {
                        if let selectedResult { memorySheet = MemorySheetRequest(id: selectedResult.memoryID) }
                    }
                    .buttonStyle(.bordered)
                    .disabled(selectedResult == nil)
                    .accessibilityIdentifier("query.openMemory")
                }

                Divider()
                if let selectedResult {
                    selectedSource(result: selectedResult)
                } else {
                    ContentUnavailableView("Select a source", systemImage: "doc.text", description: Text("Choose a ranked memory to inspect its provenance."))
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .accessibilityIdentifier("query.answer")
    }

    private func selectedSource(result: QueryResult) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Selected source").font(.headline)
            Text(result.summary).font(.title3.weight(.semibold))
            Text("Score \(result.score.formatted(.number.precision(.fractionLength(2)))) · \(result.matchKind.capitalized) · \(result.memoryType.replacingOccurrences(of: "_", with: " "))")
                .font(.callout).foregroundStyle(.secondary)
            Text(result.snippet).font(.body).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
            if !result.scoreExplanation.isEmpty {
                KnowledgeCard {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Why this result").font(.subheadline.weight(.semibold))
                        ForEach(Array(result.scoreExplanation.enumerated()), id: \.offset) { _, line in
                            Label(line, systemImage: "arrow.up.right")
                                .font(.callout).foregroundStyle(.secondary)
                        }
                    }
                }
            }
            if !result.sources.isEmpty {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Provenance").font(.subheadline.weight(.semibold))
                    ForEach(Array(result.sources.enumerated()), id: \.offset) { _, source in
                        HStack(alignment: .top, spacing: 8) {
                            Image(systemName: "doc.text").foregroundStyle(.secondary)
                            VStack(alignment: .leading, spacing: 3) {
                                Text(source.displayName).font(.callout.weight(.medium))
                                if let excerpt = source.excerpt { Text(excerpt).font(.caption).foregroundStyle(.secondary).lineLimit(4) }
                            }
                        }
                    }
                }
            }
        }
    }

    private var evidenceWarning: some View {
        KnowledgeCard {
            Label("This answer may be incomplete because the project returned insufficient evidence.", systemImage: "exclamationmark.triangle.fill")
                .foregroundStyle(.orange)
                .font(.callout)
        }
    }

    private var historySheet: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Recent questions").font(.title2.weight(.semibold))
            Text("History is kept only for this app session.").font(.callout).foregroundStyle(.secondary)
            if queryHistory.isEmpty {
                ContentUnavailableView("No recent questions", systemImage: "clock", description: Text("Questions you ask in this session appear here."))
                    .frame(minHeight: 220)
            } else {
                List(queryHistory.reversed()) { entry in
                    Button {
                        question = entry.question
                        response = entry.response
                        selectedResultID = entry.response.results.first?.id
                        errorMessage = nil
                        errorKind = nil
                        showsHistory = false
                    } label: {
                        VStack(alignment: .leading, spacing: 5) {
                            Text(entry.question).font(.subheadline.weight(.semibold)).lineLimit(2)
                            Text("\(entry.response.answerCitations.count) cited memories · \(entry.response.insufficientEvidence ? "Insufficient" : "Sufficient") evidence")
                                .font(.callout).foregroundStyle(.secondary)
                            Text(knowledgeDate(entry.askedAt)).font(.caption).foregroundStyle(.tertiary)
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.vertical, 4)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("query.history.\(entry.id.uuidString)")
                }
                .listStyle(.inset)
            }
            HStack {
                Spacer()
                Button("Close") { showsHistory = false }.keyboardShortcut(.cancelAction)
            }
        }
        .padding(20)
        .frame(minWidth: 480, minHeight: 380)
    }

    private func ask() async {
        guard !isLoading else { return }
        let trimmed = question.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, context.permissions.allows(.memoryRead) else { return }
        let ticket = requestGate.begin(revision: context.revision, resourceID: trimmed)
        isLoading = true
        errorMessage = nil
        errorKind = nil
        let started = ContinuousClock.now
        do {
            let payload = try await service.query(QueryRequest(
                project: context.profile.projectSlug,
                query: trimmed,
                filters: [:],
                topK: 8,
                minConfidence: nil,
                includeStale: includeStale,
                history: false
            ))
            guard !Task.isCancelled, requestGate.accepts(ticket, revision: context.revision, resourceID: trimmed) else { return }
            let duration = started.duration(to: .now)
            response = payload
            selectedResultID = payload.results.first?.id
            sessionStore.recordQuery(
                QueryHistoryEntry(question: trimmed, response: payload, duration: duration, askedAt: .now),
                profileID: context.profile.id,
                project: context.profile.projectSlug
            )
        } catch is CancellationError {
            return
        } catch {
            guard !Task.isCancelled, requestGate.accepts(ticket, revision: context.revision, resourceID: trimmed) else { return }
            errorMessage = error.localizedDescription
            errorKind = knowledgeErrorKind(error, fallbackMessage: error.localizedDescription)
        }
        guard !Task.isCancelled, requestGate.accepts(ticket, revision: context.revision, resourceID: trimmed) else { return }
        isLoading = false
    }
}

/// A compact wrapping layout for citation buttons that adapts to the available pane width.
private struct FlowLayout: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        var x: CGFloat = 0
        var y: CGFloat = 0
        var rowHeight: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x > 0 && x + size.width > width {
                x = 0
                y += rowHeight + spacing
                rowHeight = 0
            }
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
        }
        return CGSize(width: min(width, max(0, x - spacing)), height: y + rowHeight)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX
        var y = bounds.minY
        var rowHeight: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x > bounds.minX && x + size.width > bounds.maxX {
                x = bounds.minX
                y += rowHeight + spacing
                rowHeight = 0
            }
            subview.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
        }
    }
}
