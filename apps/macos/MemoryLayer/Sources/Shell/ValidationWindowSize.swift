#if DEBUG
import AppKit
import SwiftUI

/// Explicit sizing for screenshot/UI-test acceptance; normal launches restore
/// the user's standard window state without this helper changing it.
struct ValidationWindowSize: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        ValidationSizingView(frame: .zero)
    }

    func updateNSView(_ nsView: NSView, context: Context) {}
}

private final class ValidationSizingView: NSView {
    private var applied = false

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard !applied, let window,
              let argument = ProcessInfo.processInfo.arguments.first(where: { $0.hasPrefix("--ui-window-size=") }) else { return }
        let value = String(argument.dropFirst("--ui-window-size=".count))
        let size: NSSize
        switch value {
        case "1440x960": size = NSSize(width: 1440, height: 960)
        case "1024x768": size = NSSize(width: 1024, height: 768)
        default: return
        }
        applied = true
        // AppKit restores saved window geometry after the content view attaches.
        // Apply the explicit acceptance-test size after that initial restoration.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak window] in
            guard let window else { return }
            let origin = window.screen?.visibleFrame.origin ?? window.frame.origin
            window.setFrame(NSRect(origin: origin, size: size), display: true)
        }
    }
}
#endif
