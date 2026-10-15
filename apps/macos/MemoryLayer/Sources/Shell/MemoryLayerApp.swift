import SwiftUI

@main
struct MemoryLayerApp: App {
    @StateObject private var model = AppModel()

    var body: some Scene {
        WindowGroup {
            AppShellView(model: model)
                .frame(minWidth: 900, minHeight: 620)
        }
        .defaultSize(width: 1280, height: 800)
        .commands {
            SidebarCommands()
            CommandGroup(after: .sidebar) {
                Button("Connection Settings…") { NotificationCenter.default.post(name: .showConnectionSettings, object: nil) }
                    .keyboardShortcut(",", modifiers: .command)
                Button("Refresh") { NotificationCenter.default.post(name: .refreshMemoryLayer, object: nil) }
                    .keyboardShortcut("r", modifiers: .command)
                Button("Toggle Inspector") { NotificationCenter.default.post(name: .toggleMemoryInspector, object: nil) }
                    .keyboardShortcut("i", modifiers: [.command, .option])
            }
            CommandMenu("Navigate") {
                ForEach(Array(AppDestination.allCases.prefix(9).enumerated()), id: \.element.id) { index, destination in
                    Button(destination.rawValue) {
                        NotificationCenter.default.post(name: .selectDestination, object: destination)
                    }
                    .keyboardShortcut(KeyEquivalent(Character(String(index + 1))), modifiers: .command)
                }
            }
        }
    }
}

extension Notification.Name {
    static let showConnectionSettings = Notification.Name("MemoryLayer.showConnectionSettings")
    static let selectDestination = Notification.Name("MemoryLayer.selectDestination")
    static let refreshMemoryLayer = Notification.Name("MemoryLayer.refresh")
    static let toggleMemoryInspector = Notification.Name("MemoryLayer.toggleInspector")
}
