import AppKit

/// AppKit rather than SwiftUI at the top level: native window tabbing lives on
/// `NSWindow`, and driving it through SwiftUI's window management fights the
/// framework more than it helps.
final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        _ = GhosttyApp.shared
        buildMenu()

        do {
            try Daemon.ensureRunning()
        } catch {
            presentFatal(error.localizedDescription)
            return
        }

        // Nothing to show on a fresh daemon, so start something.
        if ((try? Daemon.list()) ?? []).isEmpty {
            WindowManager.shared.newTab(in: defaultSessionName())
        }
        WindowManager.shared.startPolling()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        // Closing every window must not kill the daemon's work; the app is
        // just a viewer.
        true
    }

    private func defaultSessionName() -> String {
        FileManager.default.homeDirectoryForCurrentUser.lastPathComponent
    }

    @objc func newTab(_ sender: Any?) {
        guard let session = WindowManager.shared.currentSession else { return }
        WindowManager.shared.newTab(in: session)
    }

    @objc func newSession(_ sender: Any?) {
        let alert = NSAlert()
        alert.messageText = "New session"
        alert.informativeText = "Sessions keep running when their windows close."
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 240, height: 24))
        field.placeholderString = "name"
        alert.accessoryView = field
        alert.addButton(withTitle: "Create")
        alert.addButton(withTitle: "Cancel")
        guard alert.runModal() == .alertFirstButtonReturn else { return }

        let name = field.stringValue.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return }
        WindowManager.shared.newTab(in: name)
    }

    private func presentFatal(_ message: String) {
        let alert = NSAlert()
        alert.alertStyle = .critical
        alert.messageText = "Cannot reach the keep daemon"
        alert.informativeText = message
        alert.runModal()
        NSApp.terminate(nil)
    }

    private func buildMenu() {
        let main = NSMenu()

        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(withTitle: "Quit Keep", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        appItem.submenu = appMenu
        main.addItem(appItem)

        let fileItem = NSMenuItem()
        let fileMenu = NSMenu(title: "File")
        fileMenu.addItem(withTitle: "New Tab", action: #selector(newTab(_:)), keyEquivalent: "t")
        fileMenu.addItem(withTitle: "New Session…", action: #selector(newSession(_:)), keyEquivalent: "n")
        fileMenu.addItem(.separator())
        fileMenu.addItem(withTitle: "Close Tab", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        fileItem.submenu = fileMenu
        main.addItem(fileItem)

        // Gives us Show Next/Previous Tab and the tab overview for free.
        let windowItem = NSMenuItem()
        let windowMenu = NSMenu(title: "Window")
        windowItem.submenu = windowMenu
        main.addItem(windowItem)

        NSApp.mainMenu = main
        NSApp.windowsMenu = windowMenu
    }
}

let delegate = AppDelegate()
let app = NSApplication.shared
app.delegate = delegate
app.setActivationPolicy(.regular)
app.activate(ignoringOtherApps: true)
app.run()
