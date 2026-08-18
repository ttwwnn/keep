import AppKit

/// AppKit at the top: native tabbing lives on `NSWindow`, and driving it
/// through SwiftUI's window management fights the framework. SwiftUI is still
/// used where it earns its keep — the sidebar is a hosted SwiftUI view.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let store = Store()

    func applicationDidFinishLaunching(_ notification: Notification) {
        _ = GhosttyApp.shared
        buildMenu()
        store.start()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        // The app is a viewer; the daemon keeps the work.
        true
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        // Quit closes windows as a side effect; that must not close tabs.
        WindowManager.shared.isQuitting = true
        return .terminateNow
    }

    @objc func newTab(_ sender: Any?) {
        store.newTabInFront()
    }

    @objc func newWorkspace(_ sender: Any?) {
        let alert = NSAlert()
        alert.messageText = "New workspace"
        alert.informativeText = "Workspaces keep running after their windows close."
        let field = NSTextField(frame: NSRect(x: 0, y: 0, width: 240, height: 24))
        field.placeholderString = "name"
        alert.accessoryView = field
        alert.window.initialFirstResponder = field
        alert.addButton(withTitle: "Create")
        alert.addButton(withTitle: "Cancel")
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        store.createWorkspace(named: field.stringValue)
    }

    private func buildMenu() {
        let main = NSMenu()

        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(
            withTitle: "Quit Keep", action: #selector(NSApplication.terminate(_:)),
            keyEquivalent: "q")
        appItem.submenu = appMenu
        main.addItem(appItem)

        let fileItem = NSMenuItem()
        let fileMenu = NSMenu(title: "File")
        fileMenu.addItem(withTitle: "New Tab", action: #selector(newTab(_:)), keyEquivalent: "t")
        fileMenu.addItem(
            withTitle: "New Workspace…", action: #selector(newWorkspace(_:)), keyEquivalent: "n")
        fileMenu.addItem(.separator())
        fileMenu.addItem(
            withTitle: "Close Tab", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        fileItem.submenu = fileMenu
        main.addItem(fileItem)

        let viewItem = NSMenuItem()
        let viewMenu = NSMenu(title: "View")
        viewMenu.addItem(
            withTitle: "Toggle Sidebar",
            action: #selector(NSSplitViewController.toggleSidebar(_:)), keyEquivalent: "s")
        viewMenu.items.last?.keyEquivalentModifierMask = [.command, .control]
        viewItem.submenu = viewMenu
        main.addItem(viewItem)

        // Native tabs put Show Next/Previous Tab and the overview here.
        let windowItem = NSMenuItem()
        let windowMenu = NSMenu(title: "Window")
        windowItem.submenu = windowMenu
        main.addItem(windowItem)

        NSApp.mainMenu = main
        NSApp.windowsMenu = windowMenu
    }
}

// Top-level code runs on the main actor when the entry point awaits it.
MainActor.assumeIsolated {
    let delegate = AppDelegate()
    let app = NSApplication.shared
    app.delegate = delegate
    app.setActivationPolicy(.regular)
    app.activate(ignoringOtherApps: true)
    // Keep the delegate alive for the app's lifetime.
    objc_setAssociatedObject(app, "keep.delegate", delegate, .OBJC_ASSOCIATION_RETAIN)
    app.run()
}
