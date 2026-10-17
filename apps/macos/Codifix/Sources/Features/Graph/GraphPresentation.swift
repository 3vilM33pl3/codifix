import Foundation
import simd
import SwiftUI

struct GraphDisplayNode: Identifiable, Sendable {
    let reference: GraphNodeReference
    let label: String
    let grouping: String
    let layers: Set<GraphLayer>
    let code: CodeGraphNode?
    let memory: ProjectMemoryGraphNode?

    var id: String { reference.id }

    var primaryLayer: GraphLayer {
        if layers.contains(.code) { return .code }
        if layers.contains(.provenance) { return .provenance }
        return .memoryRelations
    }

    var detailType: String {
        if let code { return code.symbolKind ?? code.nodeKind }
        if let memory {
            return memory.nodeKind == .memory
                ? "Memory · \(memory.memoryType ?? "unknown")"
                : "Source · \(memory.sourceKind ?? "unknown")"
        }
        return "Graph node"
    }

    var accessibleSummary: String {
        if let code {
            return [detailType, code.filePath, code.startLine.map(String.init)]
                .compactMap { $0 }
                .joined(separator: ", ")
        }
        if let memory, memory.nodeKind == .memory {
            return [detailType, memory.summary, memory.tags.joined(separator: ", ")]
                .compactMap { $0 }
                .joined(separator: ", ")
        }
        if let memory {
            return [detailType, memory.filePath, memory.symbolName, memory.provenanceStatus]
                .compactMap { $0 }
                .joined(separator: ", ")
        }
        return detailType
    }

    var displaySize: Float {
        if let code { return 0.15 + min(0.15, Float(max(0, code.degree)) * 0.012) }
        if let memory, memory.nodeKind == .memory {
            return 0.2 + min(0.09, Float(max(0, memory.activation ?? 0)) * 0.008)
        }
        return 0.13
    }

    var color: Color {
        if let code {
            if code.seed { return Color(red: 0.22, green: 0.72, blue: 0.57) }
            return GraphPalette.group(code.group)
        }
        if let memory, memory.nodeKind == .source {
            return primaryLayer == .provenance
                ? Color(red: 0.37, green: 0.7, blue: 0.95)
                : Color(red: 0.55, green: 0.64, blue: 0.73)
        }
        return primaryLayer == .provenance
            ? Color(red: 0.67, green: 0.58, blue: 0.94)
            : Color(red: 0.88, green: 0.69, blue: 0.35)
    }
}

struct GraphDisplayEdge: Identifiable, Sendable {
    let id: String
    let source: GraphNodeReference
    let target: GraphNodeReference
    let label: String
    let layer: GraphLayer
    let code: CodeGraphEdge?
    let memory: ProjectMemoryGraphEdge?
    let isAttachment: Bool

    var color: Color {
        if let code {
            if code.edgeKind.localizedCaseInsensitiveContains("call") { return Color(red: 0.35, green: 0.78, blue: 0.67) }
            if code.edgeKind.localizedCaseInsensitiveContains("test") { return Color(red: 0.91, green: 0.69, blue: 0.38) }
            if code.edgeKind.localizedCaseInsensitiveContains("import") { return Color(red: 0.4, green: 0.61, blue: 0.9) }
            return Color(red: 0.55, green: 0.65, blue: 0.75)
        }
        guard let memory else { return .secondary }
        if memory.edgeKind == .provenance { return Color(red: 0.37, green: 0.7, blue: 0.95) }
        return switch memory.relationType {
        case "supports": Color(red: 0.42, green: 0.76, blue: 0.55)
        case "supersedes": Color(red: 0.91, green: 0.69, blue: 0.38)
        case "duplicates": Color(red: 0.86, green: 0.48, blue: 0.58)
        default: Color(red: 0.67, green: 0.58, blue: 0.94)
        }
    }
}

struct GraphDisplaySnapshot: Sendable {
    let nodes: [GraphDisplayNode]
    let edges: [GraphDisplayEdge]
    let positions: [GraphNodeReference: SIMD3<Float>]

    var nodesByID: [GraphNodeReference: GraphDisplayNode] {
        Dictionary(uniqueKeysWithValues: nodes.map { ($0.reference, $0) })
    }

    var sceneKey: String {
        let nodeKey = nodes.map { node in
            let position = positions[node.reference] ?? .zero
            let layers = node.layers.map(\.rawValue).sorted().joined(separator: "+")
            return "\(node.id)|\(node.label)|\(node.grouping)|\(node.displaySize)|\(layers)|\(position.x),\(position.y),\(position.z)"
        }.joined(separator: ";")
        let edgeKey = edges.map { "\($0.id)|\($0.source.id)|\($0.target.id)|\($0.layer.rawValue)" }.joined(separator: ";")
        return nodeKey + "#" + edgeKey
    }
}

enum GraphPresentation {
    static func make(
        from snapshot: GraphServiceSnapshot,
        visibleLayers: Set<GraphLayer>,
        selectedCodeNodeID: UUID?
    ) -> GraphDisplaySnapshot {
        var nodesByID: [GraphNodeReference: GraphDisplayNode] = [:]
        var edges: [GraphDisplayEdge] = []

        if visibleLayers.contains(.code) {
            for node in snapshot.codeGraph.nodes {
                let reference = GraphNodeReference.code(node.id)
                nodesByID[reference] = GraphDisplayNode(
                    reference: reference,
                    label: node.label,
                    grouping: node.group,
                    layers: [.code],
                    code: node,
                    memory: nil
                )
            }
            for edge in snapshot.codeGraph.edges {
                let source = GraphNodeReference.code(edge.source)
                let target = GraphNodeReference.code(edge.target)
                guard nodesByID[source] != nil, nodesByID[target] != nil else { continue }
                edges.append(GraphDisplayEdge(
                    id: "code-edge:\(edge.id.uuidString.lowercased())",
                    source: source,
                    target: target,
                    label: edge.edgeKind,
                    layer: .code,
                    code: edge,
                    memory: nil,
                    isAttachment: false
                ))
            }
        }

        let memoryNodes = Dictionary(uniqueKeysWithValues: snapshot.memoryGraph.nodes.map { ($0.id, $0) })
        let shownMemoryEdges = snapshot.memoryGraph.edges.filter { edge in
            edge.edgeKind == .provenance
                ? visibleLayers.contains(.provenance)
                : visibleLayers.contains(.memoryRelations)
        }
        var referencedMemoryNodeIDs = Set<String>()
        for edge in shownMemoryEdges {
            referencedMemoryNodeIDs.insert(edge.source)
            referencedMemoryNodeIDs.insert(edge.target)
            let layer: GraphLayer = edge.edgeKind == .provenance ? .provenance : .memoryRelations
            edges.append(GraphDisplayEdge(
                id: "memory-edge:\(edge.id)",
                source: memoryReference(edge.source, nodes: memoryNodes),
                target: memoryReference(edge.target, nodes: memoryNodes),
                label: edge.edgeKind == .provenance ? "Provenance · \(edge.sourceKind ?? "source")" : (edge.relationType ?? "memory relation"),
                layer: layer,
                code: nil,
                memory: edge,
                isAttachment: false
            ))
        }

        if visibleLayers.contains(.memoryRelations) {
            referencedMemoryNodeIDs.formUnion(
                memoryNodes.values.lazy.filter { $0.nodeKind == .memory }.map(\.id)
            )
        }

        var attachments: [(ProjectMemoryGraphNode, ProjectMemoryGraphEdge)] = []
        if visibleLayers.contains(.memoryRelations), let selectedCodeNodeID,
           let selectedCodeNode = snapshot.codeGraph.nodes.first(where: { $0.id == selectedCodeNodeID }) {
            for edge in snapshot.memoryGraph.edges where edge.edgeKind == .provenance {
                guard let source = sourceEndpoint(edge: edge, nodes: memoryNodes),
                      matches(source: source, code: selectedCodeNode),
                      let memoryEndpoint = memoryEndpoint(edge: edge, nodes: memoryNodes) else { continue }
                referencedMemoryNodeIDs.insert(source.id)
                referencedMemoryNodeIDs.insert(memoryEndpoint.id)
                attachments.append((source, edge))
            }
        }

        for nodeID in referencedMemoryNodeIDs {
            guard let node = memoryNodes[nodeID] else { continue }
            let reference = memoryReference(node.id, nodes: memoryNodes)
            var layers = Set(shownMemoryEdges.filter { $0.source == node.id || $0.target == node.id }.map {
                $0.edgeKind == .provenance ? GraphLayer.provenance : .memoryRelations
            })
            if attachments.contains(where: { $0.0.id == node.id }) { layers.insert(.memoryRelations) }
            nodesByID[reference] = GraphDisplayNode(
                reference: reference,
                label: node.label,
                grouping: node.nodeKind == .memory ? (node.memoryType ?? "memory") : (node.sourceKind ?? "source"),
                layers: layers,
                code: nil,
                memory: node
            )
        }

        if let selectedCodeNodeID {
            let codeReference = GraphNodeReference.code(selectedCodeNodeID)
            for (source, edge) in attachments {
                let sourceReference = memoryReference(source.id, nodes: memoryNodes)
                edges.append(GraphDisplayEdge(
                    id: "attachment:\(selectedCodeNodeID.uuidString.lowercased()):\(edge.id)",
                    source: codeReference,
                    target: sourceReference,
                    label: "Source attachment · \(edge.sourceKind ?? "source")",
                    layer: .memoryRelations,
                    code: nil,
                    memory: edge,
                    isAttachment: true
                ))
            }
        }

        let nodes = nodesByID.values.sorted { $0.id < $1.id }
        let seeds = nodes.map { GraphNodeSeed(id: $0.reference, grouping: $0.grouping) }
        return GraphDisplaySnapshot(nodes: nodes,
                                    edges: edges.sorted { $0.id < $1.id },
                                    positions: GraphSpatialLayout.positions(for: seeds))
    }

    private static func memoryReference(_ id: String, nodes: [String: ProjectMemoryGraphNode]) -> GraphNodeReference {
        if nodes[id]?.nodeKind == .source { return .source(id) }
        return .memory(id)
    }

    private static func memoryEndpoint(
        edge: ProjectMemoryGraphEdge,
        nodes: [String: ProjectMemoryGraphNode]
    ) -> ProjectMemoryGraphNode? {
        if let source = nodes[edge.source], source.nodeKind == .memory { return source }
        if let target = nodes[edge.target], target.nodeKind == .memory { return target }
        return nil
    }

    private static func sourceEndpoint(
        edge: ProjectMemoryGraphEdge,
        nodes: [String: ProjectMemoryGraphNode]
    ) -> ProjectMemoryGraphNode? {
        if let source = nodes[edge.source], source.nodeKind == .source { return source }
        if let target = nodes[edge.target], target.nodeKind == .source { return target }
        return nil
    }

    private static func matches(source: ProjectMemoryGraphNode, code: CodeGraphNode) -> Bool {
        let sourcePath = source.filePath.map(normalizedPath)
        let codePath = code.filePath.map(normalizedPath)
        guard let sourcePath, let codePath, sourcePath == codePath else { return false }
        guard let sourceSymbol = source.symbolName?.trimmingCharacters(in: .whitespacesAndNewlines),
              !sourceSymbol.isEmpty else { return true }
        return [code.name, code.qualifiedName, code.label].compactMap { $0 }.contains {
            $0 == sourceSymbol || $0.hasSuffix(".\(sourceSymbol)")
        }
    }

    private static func normalizedPath(_ path: String) -> String {
        path.split(separator: "/").joined(separator: "/")
    }
}

enum GraphPalette {
    private static let colors: [Color] = [
        Color(red: 0.48, green: 0.78, blue: 0.59),
        Color(red: 0.48, green: 0.65, blue: 0.9),
        Color(red: 0.75, green: 0.59, blue: 0.9),
        Color(red: 0.9, green: 0.65, blue: 0.47),
        Color(red: 0.88, green: 0.55, blue: 0.66),
        Color(red: 0.51, green: 0.75, blue: 0.72)
    ]

    static func group(_ name: String) -> Color {
        var hash: UInt64 = 14_695_981_039_346_656_037
        for byte in name.utf8 { hash = (hash ^ UInt64(byte)) &* 1_099_511_628_211 }
        return colors[Int(hash % UInt64(colors.count))]
    }
}
