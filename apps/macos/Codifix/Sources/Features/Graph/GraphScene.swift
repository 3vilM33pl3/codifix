import AppKit
import RealityKit
import SwiftUI
import simd

struct GraphCameraState: Equatable, Sendable {
    var yaw: Float = -0.2
    var pitch: Float = 0.18
    var distance: Float = 18
    var pan = SIMD3<Float>(repeating: 0)

    mutating func orbit(horizontal: CGFloat, vertical: CGFloat) {
        yaw += Float(horizontal) * 0.008
        pitch = min(1.35, max(-1.35, pitch + Float(vertical) * 0.008))
    }

    mutating func panBy(horizontal: CGFloat, vertical: CGFloat) {
        let scale = max(0.005, distance / 2_000)
        pan.x -= Float(horizontal) * scale
        pan.y += Float(vertical) * scale
    }

    mutating func zoom(by magnification: CGFloat) {
        distance = min(90, max(4, distance / Float(max(0.25, 1 + magnification))))
    }

    mutating func zoom(in isZoomIn: Bool) {
        distance = min(90, max(4, distance * (isZoomIn ? 0.86 : 1.16)))
    }

    func project(_ point: SIMD3<Float>, in size: CGSize) -> CGPoint? {
        let relative = point - pan
        let sinYaw = sin(yaw)
        let cosYaw = cos(yaw)
        let sinPitch = sin(pitch)
        let cosPitch = cos(pitch)
        let right = SIMD3<Float>(cosYaw, 0, -sinYaw)
        let up = SIMD3<Float>(-sinPitch * sinYaw, cosPitch, -sinPitch * cosYaw)
        let cameraOffset = SIMD3<Float>(sinYaw * cosPitch, sinPitch, cosYaw * cosPitch)
        let depth = distance - simd_dot(relative, cameraOffset)
        guard depth > 0.15 else { return nil }
        // The RealityKit camera below uses a 60° vertical field of view.
        let focalLength = Float(size.height * 0.5) / tan(Float.pi / 6)
        let x = Float(size.width / 2) + simd_dot(relative, right) * focalLength / depth
        let y = Float(size.height / 2) - simd_dot(relative, up) * focalLength / depth
        let margin: Float = 36
        guard x >= -margin, x <= Float(size.width) + margin,
              y >= -margin, y <= Float(size.height) + margin else { return nil }
        return CGPoint(x: CGFloat(x), y: CGFloat(y))
    }
}

@MainActor
final class GraphSceneController {
    private let root = Entity()
    private let camera: PerspectiveCamera
    private var sceneKey: String?

    init() {
        let camera = PerspectiveCamera()
        camera.camera.fieldOfViewInDegrees = 60
        self.camera = camera
    }

    func prepare(
        in content: inout RealityViewCameraContent,
        snapshot: GraphDisplaySnapshot,
        cameraState: GraphCameraState,
        selection: GraphNodeReference?
    ) {
        // RealityView runs make once per scene. The controller can outlive that
        // view when the responsive layout switches between compact and wide.
        content.add(root)
        content.add(camera)
        update(snapshot: snapshot, cameraState: cameraState, selection: selection)
    }

    func update(
        snapshot: GraphDisplaySnapshot,
        cameraState: GraphCameraState,
        selection: GraphNodeReference?
    ) {
        let key = snapshot.sceneKey + "|" + (selection?.id ?? "")
        if key != sceneKey {
            rebuild(snapshot: snapshot, selection: selection)
            sceneKey = key
        }
        let offset = cameraState.distance
        let yaw = cameraState.yaw
        let pitch = cameraState.pitch
        let cameraPosition = cameraState.pan + SIMD3<Float>(
            sin(yaw) * cos(pitch) * offset,
            sin(pitch) * offset,
            cos(yaw) * cos(pitch) * offset
        )
        camera.position = cameraPosition
        camera.look(at: cameraState.pan, from: cameraPosition, relativeTo: nil)
    }

    private func rebuild(snapshot: GraphDisplaySnapshot, selection: GraphNodeReference?) {
        root.children.removeAll()
        for edge in snapshot.edges {
            guard let start = snapshot.positions[edge.source], let end = snapshot.positions[edge.target] else { continue }
            let vector = end - start
            let length = simd_length(vector)
            guard length > 0.001 else { continue }
            let entity = ModelEntity(
                mesh: .generateCylinder(height: length, radius: edge.isAttachment ? 0.014 : 0.01),
                materials: [SimpleMaterial(color: NSColor(edge.color), isMetallic: false)]
            )
            entity.position = (start + end) / 2
            entity.orientation = simd_quatf(from: SIMD3<Float>(0, 1, 0), to: simd_normalize(vector))
            root.addChild(entity)
        }

        for node in snapshot.nodes {
            guard let position = snapshot.positions[node.reference] else { continue }
            let selected = selection == node.reference
            let radius = node.displaySize * (selected ? 1.35 : 1)
            let entity = ModelEntity(
                mesh: .generateSphere(radius: radius),
                materials: [SimpleMaterial(color: NSColor(selected ? Color.accentColor : node.color), isMetallic: false)]
            )
            entity.position = position
            root.addChild(entity)
        }
    }
}
