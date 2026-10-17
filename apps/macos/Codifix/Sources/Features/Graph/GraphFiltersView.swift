import SwiftUI

struct GraphFiltersView: View {
    let current: GraphFilterDraft
    let focusSearch: Bool
    let onCancel: () -> Void
    let onApply: (GraphFilterDraft) -> Void
    @State private var draft: GraphFilterDraft
    @FocusState private var focusedField: Field?

    private enum Field: Hashable { case query }

    init(current: GraphFilterDraft, focusSearch: Bool = false,
         onCancel: @escaping () -> Void, onApply: @escaping (GraphFilterDraft) -> Void) {
        self.current = current
        self.focusSearch = focusSearch
        self.onCancel = onCancel
        self.onApply = onApply
        _draft = State(initialValue: current)
    }

    private var runIDIsValid: Bool {
        draft.runID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
            UUID(uuidString: draft.runID.trimmingCharacters(in: .whitespacesAndNewlines)) != nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Graph filters").font(.title2.weight(.semibold))
                Text("Bound the read-only code neighborhood.").font(.callout).foregroundStyle(.secondary)
            }

            Form {
                TextField("Text or symbol", text: $draft.query)
                    .focused($focusedField, equals: .query)
                    .accessibilityIdentifier("graph.filter.query")
                TextField("File path", text: $draft.filePath)
                    .accessibilityIdentifier("graph.filter.file")
                TextField("Symbol", text: $draft.symbol)
                    .accessibilityIdentifier("graph.filter.symbol")
                TextField("Edge kind", text: $draft.edgeKind)
                    .accessibilityIdentifier("graph.filter.edgeKind")
                TextField("Run ID (UUID)", text: $draft.runID)
                    .accessibilityIdentifier("graph.filter.runID")
                if !runIDIsValid {
                    Text("Run ID must be a valid UUID.").font(.caption).foregroundStyle(.red)
                }
                Picker("Depth", selection: $draft.depth) {
                    ForEach(0...2, id: \.self) { depth in
                        Text("\(depth) \(depth == 1 ? "hop" : "hops")").tag(depth)
                    }
                }
                Stepper("Maximum nodes · \(draft.limitNodes)", value: $draft.limitNodes, in: 1...1_000, step: 25)
                Stepper("Maximum edges · \(draft.limitEdges)", value: $draft.limitEdges, in: 1...2_000, step: 50)
            }
            .formStyle(.grouped)

            Text("Service limits are 1,000 nodes and 2,000 edges. Any service truncation is reported in the graph view.")
                .font(.caption).foregroundStyle(.secondary)

            HStack {
                Spacer()
                Button("Cancel", action: onCancel)
                    .keyboardShortcut(.cancelAction)
                Button("Apply filters") { onApply(draft) }
                    .buttonStyle(.borderedProminent)
                    .disabled(!runIDIsValid)
                    .keyboardShortcut(.defaultAction)
                    .accessibilityIdentifier("graph.filter.apply")
            }
        }
        .padding(20)
        .frame(minWidth: 460, idealWidth: 500, minHeight: 520)
        .accessibilityIdentifier("graph.filters.sheet")
        .task {
            if focusSearch { focusedField = .query }
        }
    }
}
