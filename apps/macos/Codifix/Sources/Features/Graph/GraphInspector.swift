import SwiftUI

struct GraphInspectorView: View {
    let context: FeatureContext
    let data: GraphDisplaySnapshot
    let selection: GraphSelection?
    let onSelectNode: (GraphNodeReference?) -> Void
    let onSelectEdge: (String?) -> Void
    let onOpenMemory: (String) -> Void

    private var nodesByID: [GraphNodeReference: GraphDisplayNode] { data.nodesByID }
    private var selectedNode: GraphDisplayNode? {
        guard case let .node(reference) = selection else { return nil }
        return nodesByID[reference]
    }
    private var selectedEdge: GraphDisplayEdge? {
        guard case let .edge(id) = selection else { return nil }
        return data.edges.first { $0.id == id }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                if let selectedNode {
                    nodeDetails(selectedNode)
                } else if let selectedEdge {
                    edgeDetails(selectedEdge)
                } else {
                    overview
                }

                DisclosureGroup("Graph nodes · \(data.nodes.count)") {
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 2) {
                            ForEach(data.nodes) { node in
                                Button {
                                    onSelectNode(node.reference)
                                } label: {
                                    HStack(spacing: 8) {
                                        Circle().fill(node.color).frame(width: 8, height: 8)
                                        VStack(alignment: .leading, spacing: 2) {
                                            Text(node.label).lineLimit(1)
                                            Text(node.accessibleSummary).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                        }
                                        Spacer(minLength: 0)
                                    }
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .padding(.vertical, 4)
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(.plain)
                                .accessibilityLabel(node.label)
                                .accessibilityHint(node.accessibleSummary)
                            }
                        }
                        .padding(.top, 7)
                    }
                    .frame(maxHeight: 240)
                }

                DisclosureGroup("Connections · \(data.edges.count)") {
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 4) {
                            ForEach(data.edges) { edge in
                                Button {
                                    onSelectEdge(edge.id)
                                } label: {
                                    VStack(alignment: .leading, spacing: 3) {
                                        Text(edge.label).font(.subheadline.weight(.medium)).lineLimit(1)
                                        Text("\(nodesByID[edge.source]?.label ?? "Unknown") → \(nodesByID[edge.target]?.label ?? "Unknown")")
                                            .font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                    }
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .padding(.vertical, 5)
                                    .contentShape(Rectangle())
                                }
                                .buttonStyle(.plain)
                            }
                        }
                        .padding(.top, 7)
                    }
                    .frame(maxHeight: 240)
                }
            }
            .padding(15)
        }
        .frame(minWidth: 238, idealWidth: 270, maxWidth: 340, maxHeight: .infinity)
        .background(Color(nsColor: .controlBackgroundColor))
        .accessibilityIdentifier("graph.inspector")
    }

    private var overview: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Graph overview").font(.headline)
            LabeledContent("Project", value: context.profile.projectSlug)
            LabeledContent("Service", value: context.profile.name)
            Divider()
            layerLegend
            Text("Select a node or connection to inspect the service data behind it.")
                .font(.callout).foregroundStyle(.secondary)
        }
    }

    private func nodeDetails(_ node: GraphDisplayNode) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(node.label).font(.headline).textSelection(.enabled)
            Text(node.detailType).font(.subheadline).foregroundStyle(.secondary)
            if let code = node.code {
                LabeledContent("Layer", value: "Code")
                LabeledContent("Group", value: code.group)
                if let language = code.language { LabeledContent("Language", value: language) }
                if let path = code.filePath {
                    LabeledContent("File", value: path).textSelection(.enabled)
                }
                if let line = code.startLine {
                    LabeledContent("Line", value: code.endLine.map { "\(line)–\($0)" } ?? String(line))
                }
                LabeledContent("Visible neighbors", value: String(visibleDegree(of: node.reference)))
                LabeledContent("Seed node", value: code.seed ? "Yes" : "No")
            }
            if let memory = node.memory, memory.nodeKind == .memory {
                LabeledContent("Layer", value: node.primaryLayer.title)
                LabeledContent("Type", value: memory.memoryType ?? "Unknown")
                if let confidence = memory.confidence { LabeledContent("Confidence", value: confidence.formatted(.number.precision(.fractionLength(2)))) }
                if let activation = memory.activation { LabeledContent("Activation", value: activation.formatted(.number.precision(.fractionLength(2)))) }
                if let importance = memory.importance { LabeledContent("Importance", value: String(importance)) }
                if !memory.tags.isEmpty { LabeledContent("Tags", value: memory.tags.joined(separator: ", ")) }
                if let summary = memory.summary { Text(summary).font(.callout).foregroundStyle(.secondary).textSelection(.enabled) }
                if let memoryID = memory.memoryID, context.permissions.allows(.memoryRead) {
                    Button("Open memory", systemImage: "arrow.up.right.square") { onOpenMemory(memoryID) }
                        .buttonStyle(.borderedProminent)
                        .accessibilityIdentifier("graph.openMemory")
                }
            }
            if let memory = node.memory, memory.nodeKind == .source {
                LabeledContent("Layer", value: node.primaryLayer.title)
                if let kind = memory.sourceKind { LabeledContent("Source kind", value: kind) }
                if let path = memory.filePath { LabeledContent("File", value: path).textSelection(.enabled) }
                if let symbol = memory.symbolName { LabeledContent("Symbol", value: symbol) }
                if let commit = memory.gitCommit { LabeledContent("Commit", value: commit) }
                if let status = memory.provenanceStatus { LabeledContent("Verification", value: status) }
                if let id = memory.sourceID { LabeledContent("Source ID", value: id).textSelection(.enabled) }
            }
            Divider()
            layerLegend
        }
    }

    private func edgeDetails(_ edge: GraphDisplayEdge) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(edge.label).font(.headline).textSelection(.enabled)
            LabeledContent("Layer", value: edge.layer.title)
            LabeledContent("From", value: nodesByID[edge.source]?.label ?? "Unknown")
            LabeledContent("To", value: nodesByID[edge.target]?.label ?? "Unknown")
            if let code = edge.code {
                LabeledContent("Resolution", value: code.resolutionStatus)
                LabeledContent("Confidence", value: code.confidence.formatted(.number.precision(.fractionLength(2))))
                if let path = code.filePath { LabeledContent("File", value: path).textSelection(.enabled) }
                if let line = code.startLine { LabeledContent("Line", value: code.endLine.map { "\(line)–\($0)" } ?? String(line)) }
            }
            if let memory = edge.memory, let relation = memory.relationType {
                LabeledContent("Relation", value: relation)
            }
            if edge.isAttachment {
                Text("This link is derived from matching source path and symbol metadata.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Divider()
            layerLegend
        }
    }

    private var layerLegend: some View {
        VStack(alignment: .leading, spacing: 7) {
            Text("Layers").font(.subheadline.weight(.semibold))
            ForEach(GraphLayer.allCases) { layer in
                Label(layer.title, systemImage: layer.symbol)
                    .font(.caption)
                    .foregroundStyle(layer == .code ? Color.accentColor : Color.secondary)
            }
        }
    }

    private func visibleDegree(of reference: GraphNodeReference) -> Int {
        data.edges.reduce(into: 0) { count, edge in
            if edge.source == reference || edge.target == reference { count += 1 }
        }
    }
}

enum GraphSelection: Equatable {
    case node(GraphNodeReference)
    case edge(String)
}
