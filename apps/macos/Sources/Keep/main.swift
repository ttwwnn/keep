import AppKit

/// AppKit at the top; SwiftUI only where it earns its keep (the sidebar).
///
/// Wiring, nothing else: the delegate builds the session, the one window,
/// and the poller, and translates menu items into intents. Every action in
/// the app funnels through `Session.dispatch`.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let session = Session()
    private var windowController: MainWindowController?
    private var poller: DaemonPoller?

    func applicationDidFinishLaunching(_ notification: Notification) {
        _ = GhosttyApp.shared
        buildMenu()

        let controller = MainWindowController(session: session)
        windowController = controller
        session.renderer = controller
        controller.showWindow(nil)
        controller.window?.makeKeyAndOrderFront(nil)

        session.start()
        let poller = DaemonPoller(session: session)
        poller.start()
        self.poller = poller
    }

    /// The app is a viewer; the daemon keeps the work. Closing the window is
    /// quitting — no code path closes daemon tabs on the way out, so quit
    /// trivially leaves everything running.
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    func applicationWillTerminate(_ notification: Notification) {
        session.flush()
    }

    // MARK: - menu actions (each one is an intent)

    @objc func newTab(_ sender: Any?) { session.dispatch(.newTab(in: nil)) }
    @objc func closePane(_ sender: Any?) { session.dispatch(.closePane(nil)) }
    @objc func closeTab(_ sender: Any?) { session.dispatch(.closeTab(nil)) }
    @objc func splitRight(_ sender: Any?) { session.dispatch(.split(1)) }
    @objc func splitDown(_ sender: Any?) { session.dispatch(.split(2)) }
    @objc func nextTab(_ sender: Any?) { session.dispatch(.nextTab) }
    @objc func previousTab(_ sender: Any?) { session.dispatch(.previousTab) }

    @objc func showTab(_ sender: Any?) {
        guard let tag = (sender as? NSMenuItem)?.tag else { return }
        session.dispatch(.activateTabIndex(tag == 9 ? -1 : tag - 1))
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
        session.dispatch(.newWorkspace(named: field.stringValue))
    }

    @objc func toggleSidebar(_ sender: Any?) {
        (windowController?.window as? KeepWindow)?.toggleSidebar(sender)
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
            withTitle: "Split Right", action: #selector(splitRight(_:)), keyEquivalent: "d")
        let splitDownItem = NSMenuItem(
            title: "Split Down", action: #selector(splitDown(_:)), keyEquivalent: "d")
        splitDownItem.keyEquivalentModifierMask = [.command, .shift]
        fileMenu.addItem(splitDownItem)
        fileMenu.addItem(.separator())
        // ⌘W closes what you are looking at — the pane. Only a tab with no
        // splits makes the two the same thing.
        fileMenu.addItem(
            withTitle: "Close Pane", action: #selector(closePane(_:)), keyEquivalent: "w")
        let closeTabItem = NSMenuItem(
            title: "Close Tab", action: #selector(closeTab(_:)), keyEquivalent: "w")
        closeTabItem.keyEquivalentModifierMask = [.command, .shift]
        fileMenu.addItem(closeTabItem)
        fileItem.submenu = fileMenu
        main.addItem(fileItem)

        let viewItem = NSMenuItem()
        let viewMenu = NSMenu(title: "View")
        let toggleSidebarItem = viewMenu.addItem(
            withTitle: "Toggle Sidebar",
            action: #selector(toggleSidebar(_:)), keyEquivalent: "s")
        toggleSidebarItem.target = self
        toggleSidebarItem.keyEquivalentModifierMask = [.command, .control]
        viewItem.submenu = viewMenu
        main.addItem(viewItem)

        let windowItem = NSMenuItem()
        let windowMenu = NSMenu(title: "Window")
        let nextItem = NSMenuItem(
            title: "Show Next Tab", action: #selector(nextTab(_:)), keyEquivalent: "\t")
        nextItem.keyEquivalentModifierMask = [.control]
        windowMenu.addItem(nextItem)
        let previousItem = NSMenuItem(
            title: "Show Previous Tab", action: #selector(previousTab(_:)), keyEquivalent: "\t")
        previousItem.keyEquivalentModifierMask = [.control, .shift]
        windowMenu.addItem(previousItem)
        windowMenu.addItem(.separator())
        // ⌘1–⌘8 select by position; ⌘9 is the last tab, per macOS convention.
        for n in 1...9 {
            let item = NSMenuItem(
                title: n == 9 ? "Show Last Tab" : "Show Tab \(n)",
                action: #selector(showTab(_:)),
                keyEquivalent: String(n))
            item.tag = n
            windowMenu.addItem(item)
        }
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
