import SwiftUI

@main
struct CodifixApp: App {
    @StateObject private var model: AppModel

    init() {
        #if DEBUG
        if ProcessInfo.processInfo.arguments.contains("--uitesting") {
            _model = StateObject(wrappedValue: AppModel(profileStore: UITestProfileStore()))
            return
        }
        #endif
        _model = StateObject(wrappedValue: AppModel())
    }

    var body: some Scene {
        WindowGroup {
            AppShellView(model: model)
                .frame(minWidth: 900, minHeight: 620)
                .preferredColorScheme(.light)
                #if DEBUG
                .background(ValidationWindowSize().frame(width: 0, height: 0))
                #endif
        }
        .defaultSize(width: 1280, height: 800)
        .commands {
            SidebarCommands()
            CommandGroup(after: .sidebar) {
                Button("Connection Settings…") { NotificationCenter.default.post(name: .showConnectionSettings, object: nil) }
                    .keyboardShortcut(",", modifiers: .command)
                Button("Refresh") { NotificationCenter.default.post(name: .refreshCodifix, object: nil) }
                    .keyboardShortcut("r", modifiers: .command)
                Button("Toggle Inspector") { NotificationCenter.default.post(name: .toggleMemoryInspector, object: nil) }
                    .keyboardShortcut("i", modifiers: [.command, .option])
            }
            CommandMenu("Navigate") {
                Button("Focus Search or Question") {
                    NotificationCenter.default.post(name: .focusMemorySearch, object: nil)
                }
                .keyboardShortcut("f", modifiers: .command)
                Divider()
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

#if DEBUG
/// UI tests must not alter the user's saved service connection history.
private actor UITestProfileStore: ConnectionProfileStore {
    private var profiles: [ConnectionProfile] = []
    func load() -> [ConnectionProfile] { profiles }
    func save(_ profiles: [ConnectionProfile]) { self.profiles = profiles }
}
#endif

extension Notification.Name {
    static let showConnectionSettings = Notification.Name("Codifix.showConnectionSettings")
    static let selectDestination = Notification.Name("Codifix.selectDestination")
    static let refreshCodifix = Notification.Name("Codifix.refresh")
    static let toggleMemoryInspector = Notification.Name("Codifix.toggleInspector")
    static let focusMemorySearch = Notification.Name("Codifix.focusSearch")
}
