import AppKit
import RealityKit
import SwiftUI

struct GraphView: View {
    let context: FeatureContext

    @State private var snapshot: GraphServiceSnapshot?
    @State private var loading = true
    @State private var errorMessage: String?
    @State private var filterDraft = GraphFilterDraft()
    @State private var appliedFilters = GraphFilterDraft()
    @State private var visibleLayers: Set<GraphLayer> = [.code]
    @State private var selection: GraphSelection?
    @State private var neighborhoodAnchorCodeNodeID: UUID?
    @State private var cameraState = GraphCameraState()
    @State private var renderer = GraphSceneController()
    @State private var requestGate = GraphRequestGate()
    @State private var reloadID = 0
    @State private var showingFilters = false
    @State private var filterFocusRequest = 0
    @State private var selectedMemory: MemoryDestination?

    private var display: GraphDisplaySnapshot? {
        guard let snapshot else { return nil }
        return GraphPresentation.make(from: snapshot,
                                      visibleLayers: visibleLayers,
                                      selectedCodeNodeID: neighborhoodAnchorCodeNodeID)
    }

    private var taskID: String { "\(context.revision)-\(reloadID)" }

    var body: some View {
        GeometryReader { geometry in
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    header
                    if let errorMessage {
                        errorState(errorMessage)
                    } else if loading && snapshot == nil {
                        loadingState
                    } else if let snapshot, let display {
                        graphContent(snapshot: snapshot, display: display, availableWidth: geometry.size.width)
                    } else {
                        loadingState
                    }
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .topLeading)
            }
        }
        .task(id: taskID) { await loadGraph() }
        .sheet(isPresented: $showingFilters) {
            GraphFiltersView(current: filterDraft,
                             focusSearch: filterFocusRequest > 0,
                             onCancel: { showingFilters = false },
                             onApply: applyFilters)
        }
        .sheet(item: $selectedMemory) { destination in
            KnowledgeMemoryDetailView(context: context, memoryID: destination.id)
        }
        .onReceive(NotificationCenter.default.publisher(for: Notification.Name("Codifix.focusSearch"))) { _ in
            filterFocusRequest += 1
            showingFilters = true
        }
        .accessibilityIdentifier("graph.feature")
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top, spacing: 16) {
                VStack(alignment: .leading, spacing: 3) {
                    Text("Graph").font(.largeTitle.weight(.semibold))
                    Text("Read-only code neighborhood · Bounded to \(appliedFilters.depth) hops")
                        .font(.callout).foregroundStyle(.secondary)
                }
                Spacer(minLength: 16)
                Button("Filters", systemImage: "line.3.horizontal.decrease") { showingFilters = true }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("graph.filters")
            }

            HStack(spacing: 8) {
                ForEach(GraphLayer.allCases) { layer in
                    let selected = visibleLayers.contains(layer)
                    Button {
                        toggle(layer)
                    } label: {
                        Label(layer.title, systemImage: layer.symbol)
                    }
                    .buttonStyle(.bordered)
                    .tint(selected ? .accentColor : .secondary)
                    .accessibilityAddTraits(selected ? .isSelected : [])
                    .accessibilityIdentifier("graph.layer.\(layer.rawValue)")
                }
                Button("Reset view", systemImage: "arrow.counterclockwise") { resetCamera() }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("graph.resetView")
                Spacer(minLength: 0)
                if loading { ProgressView().controlSize(.small).accessibilityLabel("Refreshing graph") }
            }

            if let snapshot {
                HStack(spacing: 7) {
                    Circle().fill(Color.green).frame(width: 7, height: 7)
                    Text("\(snapshot.status.project) · \(context.profile.name)")
                    Text("·").foregroundStyle(.tertiary)
                    Text("\(snapshot.codeGraph.nodes.count) code nodes")
                    Text("·").foregroundStyle(.tertiary)
                    Text("\(snapshot.codeGraph.edges.count) code edges")
                    if snapshot.status.gitHead != nil, let commit = snapshot.status.gitHead {
                        Text("·").foregroundStyle(.tertiary)
                        Text(String(commit.prefix(8))).monospaced()
                    }
                    Spacer(minLength: 0)
                    Button("Refresh", systemImage: "arrow.clockwise") { reloadID += 1 }
                        .buttonStyle(.borderless)
                        .disabled(loading)
                        .accessibilityIdentifier("graph.refresh")
                }
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)
            }
        }
    }

    @ViewBuilder
    private func graphContent(snapshot: GraphServiceSnapshot,
                              display: GraphDisplaySnapshot,
                              availableWidth: CGFloat) -> some View {
        if !snapshot.status.hasGraph && snapshot.memoryGraph.totalMemories == 0 {
            ContentUnavailableView {
                Label("No graph data for \(snapshot.status.project)", systemImage: "point.3.connected.trianglepath.dotted")
            } description: {
                Text("This service has no extracted code graph or active codifix graph for this project. If you expected existing data, review the selected connection and project.")
            } actions: {
                Button("Review connection settings", systemImage: "gearshape") {
                    NotificationCenter.default.post(name: .showConnectionSettings, object: nil)
                }
                .buttonStyle(.bordered)
                Button("Refresh", systemImage: "arrow.clockwise") { reloadID += 1 }
                    .buttonStyle(.borderedProminent)
            }
            .frame(maxWidth: .infinity, minHeight: 380)
        } else if display.nodes.isEmpty {
            noVisibleNodes(snapshot: snapshot)
        } else {
            workspace(snapshot: snapshot, display: display, availableWidth: availableWidth)
            graphFootnote(snapshot)
        }
    }

    @ViewBuilder
    private func workspace(snapshot: GraphServiceSnapshot,
                           display: GraphDisplaySnapshot,
                           availableWidth: CGFloat) -> some View {
        if availableWidth >= 1_060 {
            HSplitView {
                canvas(display: display, snapshot: snapshot)
                    .frame(minWidth: 580, maxWidth: .infinity, maxHeight: .infinity)
                GraphInspectorView(context: context,
                                   data: display,
                                   selection: selection,
                                   onSelectNode: selectNode,
                                   onSelectEdge: selectEdge,
                                   onOpenMemory: { selectedMemory = MemoryDestination(id: $0) })
                    .frame(minWidth: 238, idealWidth: 270, maxWidth: 340, maxHeight: .infinity)
            }
            .frame(height: 570)
        } else {
            VStack(alignment: .leading, spacing: 12) {
                canvas(display: display, snapshot: snapshot)
                    .frame(height: 440)
                GraphInspectorView(context: context,
                                   data: display,
                                   selection: selection,
                                   onSelectNode: selectNode,
                                   onSelectEdge: selectEdge,
                                   onOpenMemory: { selectedMemory = MemoryDestination(id: $0) })
                    .frame(maxWidth: .infinity, minHeight: 260, maxHeight: 360)
            }
        }
    }

    private func canvas(display: GraphDisplaySnapshot, snapshot: GraphServiceSnapshot) -> some View {
        VStack(spacing: 0) {
            HStack(spacing: 14) {
                if visibleLayers.contains(.code) {
                    Label("\(snapshot.codeGraph.stats.returnedNodes) code nodes", systemImage: "circle.grid.3x3.fill")
                }
                if visibleLayers.contains(.provenance) {
                    Label("\(snapshot.memoryGraph.edges.filter { $0.edgeKind == .provenance }.count) provenance", systemImage: "arrow.triangle.branch")
                }
                if visibleLayers.contains(.memoryRelations) {
                    Label("\(snapshot.memoryGraph.edges.filter { $0.edgeKind == .memoryRelation }.count) memory relations", systemImage: "point.3.connected.trianglepath.dotted")
                }
                Spacer()
                cameraControls
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)

            GraphRealityCanvas(snapshot: display,
                               selection: selectedNodeReference,
                               camera: $cameraState,
                               renderer: renderer,
                               onSelectNode: selectNode)

            HStack {
                Text("Drag to orbit · Shift-drag to pan · Pinch to zoom · Select a node to inspect")
                Spacer()
                Button("Zoom out", systemImage: "minus.magnifyingglass") { cameraState.zoom(in: false) }
                    .labelStyle(.iconOnly)
                Button("Zoom in", systemImage: "plus.magnifyingglass") { cameraState.zoom(in: true) }
                    .labelStyle(.iconOnly)
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
        }
        .background(Color(nsColor: .controlBackgroundColor), in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color(nsColor: .separatorColor), lineWidth: 0.5))
        .accessibilityIdentifier("graph.canvas")
    }

    private var cameraControls: some View {
        HStack(spacing: 2) {
            cameraButton("Rotate left", symbol: "rotate.left") { cameraState.orbit(horizontal: -18, vertical: 0) }
            cameraButton("Rotate right", symbol: "rotate.right") { cameraState.orbit(horizontal: 18, vertical: 0) }
            cameraButton("Pan left", symbol: "arrow.left") { cameraState.panBy(horizontal: 24, vertical: 0) }
            cameraButton("Pan up", symbol: "arrow.up") { cameraState.panBy(horizontal: 0, vertical: 24) }
            cameraButton("Pan down", symbol: "arrow.down") { cameraState.panBy(horizontal: 0, vertical: -24) }
            cameraButton("Pan right", symbol: "arrow.right") { cameraState.panBy(horizontal: -24, vertical: 0) }
        }
    }

    private func cameraButton(_ label: String, symbol: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).frame(width: 24, height: 20)
        }
        .buttonStyle(.borderless)
        .help(label)
        .accessibilityLabel(label)
    }

    private func noVisibleNodes(snapshot: GraphServiceSnapshot) -> some View {
        VStack(spacing: 10) {
            ContentUnavailableView {
                Label(noMatchesTitle(snapshot), systemImage: "line.3.horizontal.decrease.circle")
            } description: {
                Text(noMatchesDescription(snapshot))
            } actions: {
                if appliedFilters.isActive {
                    Button("Clear filters") { applyFilters(GraphFilterDraft()) }
                        .buttonStyle(.borderedProminent)
                }
                if !visibleLayers.contains(.code) || !visibleLayers.contains(.provenance) || !visibleLayers.contains(.memoryRelations) {
                    Button("Show all layers", systemImage: "square.3.layers.3d") {
                        visibleLayers = Set(GraphLayer.allCases)
                        if neighborhoodAnchorCodeNodeID == nil, let first = snapshot.codeGraph.nodes.first {
                            neighborhoodAnchorCodeNodeID = first.id
                            if selection == nil { selection = .node(.code(first.id)) }
                        }
                    }
                    .buttonStyle(.bordered)
                }
                if !snapshot.status.hasGraph {
                    Button("Review connection settings", systemImage: "gearshape") {
                        NotificationCenter.default.post(name: .showConnectionSettings, object: nil)
                    }.buttonStyle(.bordered)
                }
            }
            .frame(maxWidth: .infinity, minHeight: 300)
        }
    }

    private func noMatchesTitle(_ snapshot: GraphServiceSnapshot) -> String {
        if visibleLayers.isEmpty { return "No graph layers selected" }
        if appliedFilters.isActive { return "No nodes match these filters" }
        if !snapshot.status.hasGraph { return "No extracted code graph" }
        return "No nodes in the selected layers"
    }

    private func noMatchesDescription(_ snapshot: GraphServiceSnapshot) -> String {
        if visibleLayers.isEmpty { return "Enable Code, Provenance, or Memory relationships to display available project data." }
        if appliedFilters.isActive {
            let fullCount = snapshot.codeGraph.stats.totalNodes
            return "The service reports \(fullCount) total code nodes. Clear the filters or enable another graph layer."
        }
        if !snapshot.status.hasGraph {
            return "No code graph is stored for \(snapshot.status.project) on \(context.profile.name). Review the connection and project if you expected data."
        }
        return "Enable Code, Provenance, or Memory relationships to display available data."
    }

    @ViewBuilder
    private func graphFootnote(_ snapshot: GraphServiceSnapshot) -> some View {
        if snapshot.codeGraph.truncated || snapshot.memoryGraph.totalMemories > snapshot.memoryGraph.returnedMemories {
            VStack(alignment: .leading, spacing: 5) {
                if snapshot.codeGraph.truncated {
                    Label(snapshot.codeGraph.truncationReason ?? "The service limited the returned code graph.", systemImage: "exclamationmark.triangle")
                }
                if snapshot.memoryGraph.totalMemories > snapshot.memoryGraph.returnedMemories {
                    Label("Showing \(snapshot.memoryGraph.returnedMemories) of \(snapshot.memoryGraph.totalMemories) active memories; the codifix graph endpoint is limited to 250.", systemImage: "exclamationmark.triangle")
                }
            }
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 3)
        }
    }

    private var loadingState: some View {
        VStack(alignment: .leading, spacing: 14) {
            ProgressView("Loading project graph…")
                .frame(maxWidth: .infinity, alignment: .leading)
            ForEach(0..<3) { index in
                RoundedRectangle(cornerRadius: 8)
                    .fill(Color(nsColor: .quaternarySystemFill))
                    .overlay(alignment: .leading) {
                        Capsule().fill(Color(nsColor: .separatorColor).opacity(0.7))
                            .frame(width: CGFloat(290 + index * 65), height: 8).padding(14)
                    }
                    .frame(height: 54)
            }
            Text("Connecting to the selected service and retrieving project graph data.")
                .font(.caption).foregroundStyle(.secondary)
        }
        .accessibilityIdentifier("graph.loading")
    }

    private func errorState(_ message: String) -> some View {
        let title: String
        let symbol: String
        if message == MemoryServiceError.unauthorized.localizedDescription {
            title = "Sign in to continue"
            symbol = "person.crop.circle.badge.exclamationmark"
        } else if message == MemoryServiceError.forbidden.localizedDescription {
            title = "Project access required"
            symbol = "lock.fill"
        } else {
            title = "Graph unavailable"
            symbol = "exclamationmark.triangle"
        }
        return VStack(alignment: .leading, spacing: 12) {
            ContentUnavailableView(title, systemImage: symbol, description: Text(message))
            HStack {
                Button("Retry connection", systemImage: "arrow.clockwise") { errorMessage = nil; reloadID += 1 }
                    .buttonStyle(.borderedProminent)
                Button("Connection settings", systemImage: "gearshape") {
                    NotificationCenter.default.post(name: .showConnectionSettings, object: nil)
                }
                .buttonStyle(.bordered)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .systemRed).opacity(0.08), in: RoundedRectangle(cornerRadius: 9))
        .accessibilityIdentifier("graph.error")
    }

    private var selectedNodeReference: GraphNodeReference? {
        guard case let .node(reference) = selection else { return nil }
        return reference
    }

    private func toggle(_ layer: GraphLayer) {
        if visibleLayers.contains(layer) {
            visibleLayers.remove(layer)
            if layer == .code {
                if case .node(.code) = selection { selection = nil }
                neighborhoodAnchorCodeNodeID = nil
            }
        } else {
            visibleLayers.insert(layer)
        }
    }

    private func selectNode(_ reference: GraphNodeReference?) {
        selection = reference.map(GraphSelection.node)
        if case let .code(id)? = reference { neighborhoodAnchorCodeNodeID = id }
    }

    private func selectEdge(_ id: String?) {
        selection = id.map(GraphSelection.edge)
    }

    private func applyFilters(_ filters: GraphFilterDraft) {
        filterDraft = filters
        appliedFilters = filters
        selection = nil
        neighborhoodAnchorCodeNodeID = nil
        cameraState = GraphCameraState()
        showingFilters = false
        reloadID += 1
    }

    private func resetCamera() {
        withAnimation(.easeInOut(duration: 0.2)) { cameraState = GraphCameraState() }
        if neighborhoodAnchorCodeNodeID == nil, let first = snapshot?.codeGraph.nodes.first {
            neighborhoodAnchorCodeNodeID = first.id
            if selection == nil { selection = .node(.code(first.id)) }
        }
    }

    @MainActor
    private func loadGraph() async {
        let key = "\(context.profile.projectSlug)|\(reloadID)|\(context.revision)"
        let request = requestGate.begin(revision: context.revision, key: key)
        loading = true
        errorMessage = nil

        guard context.permissions.allows(.memoryRead) else {
            errorMessage = MemoryServiceError.forbidden.localizedDescription
            loading = false
            return
        }

        do {
            let result = try await GraphService(context: context).load(filters: appliedFilters)
            try Task.checkCancellation()
            guard requestGate.accepts(request, revision: context.revision, key: key) else { return }
            snapshot = result
            if let anchor = neighborhoodAnchorCodeNodeID,
               result.codeGraph.nodes.contains(where: { $0.id == anchor }) {
                // Preserve the code neighborhood while the inspector focuses
                // on an attached source node or edge.
            } else if let preferred = result.codeGraph.nodes.first(where: \.seed) ?? result.codeGraph.nodes.first {
                neighborhoodAnchorCodeNodeID = preferred.id
                if selection == nil { selection = .node(.code(preferred.id)) }
            }
        } catch is CancellationError {
            return
        } catch {
            guard !Task.isCancelled, requestGate.accepts(request, revision: context.revision, key: key) else { return }
            errorMessage = error.localizedDescription
        }

        guard !Task.isCancelled, requestGate.accepts(request, revision: context.revision, key: key) else { return }
        loading = false
    }
}

private struct MemoryDestination: Identifiable {
    let id: String
}

private struct GraphRealityCanvas: View {
    let snapshot: GraphDisplaySnapshot
    let selection: GraphNodeReference?
    @Binding var camera: GraphCameraState
    let renderer: GraphSceneController
    let onSelectNode: (GraphNodeReference?) -> Void

    @State private var startingMagnification: Float?
    @State private var dragStartCamera: GraphCameraState?

    private var projectedNodes: [(node: GraphDisplayNode, point: CGPoint)] {
        let projected = snapshot.nodes.compactMap { node -> (GraphDisplayNode, CGPoint, CGFloat)? in
            guard let position = snapshot.positions[node.reference],
                  let point = camera.project(position, in: sizeForProjection) else { return nil }
            let distance = hypot(point.x - sizeForProjection.width / 2, point.y - sizeForProjection.height / 2)
            return (node, point, distance)
        }
        let limited = projected.sorted { $0.2 < $1.2 }.prefix(260)
        return limited.map { ($0.0, $0.1) }
    }

    @State private var sizeForProjection: CGSize = .zero

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                RealityView { content in
                    renderer.prepare(in: &content, snapshot: snapshot, cameraState: camera, selection: selection)
                } update: { _ in
                    renderer.update(snapshot: snapshot, cameraState: camera, selection: selection)
                }
                .allowsHitTesting(false)
                .background(Color(red: 0.96, green: 0.97, blue: 0.99))

                ForEach(projectedNodes, id: \.node.id) { item in
                    nodeTarget(item.node, at: item.point)
                }
            }
            .clipped()
            .contentShape(Rectangle())
            .onAppear { sizeForProjection = geometry.size }
            .onChange(of: geometry.size) { _, size in sizeForProjection = size }
            .simultaneousGesture(
                DragGesture(minimumDistance: 4)
                    .onChanged { value in
                        if dragStartCamera == nil { dragStartCamera = camera }
                        var next = dragStartCamera ?? camera
                        if NSEvent.modifierFlags.contains(.shift) {
                            next.panBy(horizontal: value.translation.width, vertical: value.translation.height)
                        } else {
                            next.orbit(horizontal: value.translation.width, vertical: value.translation.height)
                        }
                        camera = next
                    }
                    .onEnded { _ in dragStartCamera = nil }
            )
            .simultaneousGesture(
                MagnifyGesture()
                    .onChanged { value in
                        if startingMagnification == nil { startingMagnification = Float(camera.distance) }
                        camera.distance = min(90, max(4, (startingMagnification ?? camera.distance) / Float(value.magnification)))
                    }
                    .onEnded { _ in startingMagnification = nil }
            )
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Three-dimensional project graph")
        .accessibilityHint("Drag to orbit. Hold Shift while dragging to pan. Pinch to zoom. Select graph nodes or use the inspector lists.")
    }

    @ViewBuilder
    private func nodeTarget(_ node: GraphDisplayNode, at point: CGPoint) -> some View {
        let selected = selection == node.reference
        Button {
            onSelectNode(node.reference)
        } label: {
            Circle()
                .fill(node.color)
                .overlay(Circle().strokeBorder(selected ? Color.primary : .white, lineWidth: selected ? 2 : 1))
                .frame(width: selected ? 23 : 14, height: selected ? 23 : 14)
                .overlay(alignment: .top) {
                    if selected || snapshot.nodes.count <= 16 {
                        Text(node.label)
                            .font(.system(size: 10, design: .monospaced))
                            .lineLimit(1)
                            .fixedSize(horizontal: true, vertical: false)
                            .padding(.horizontal, 5)
                            .padding(.vertical, 2)
                            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 4))
                            .offset(y: 17)
                    }
                }
                .contentShape(Rectangle().inset(by: -10))
        }
        .buttonStyle(.plain)
        .position(point)
        .accessibilityLabel(node.label)
        .accessibilityHint(node.accessibleSummary)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}
