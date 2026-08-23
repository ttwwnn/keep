import AppKit

/// AppKit at the top; SwiftUI only where it earns its keep (the sidebar).
///
/// Wiring, nothing else: the delegate builds the session, the one window,
/// and the poller, and translates menu items into intents. Every action in
/// the app funnels through `Session.dispatch`.
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let session = Session()
    private var controllers: [MainWindowController] = []
    private var poller: DaemonPoller?

    /// The window a menu item means.
    ///
    /// Matched by identity rather than trusting `NSWindow.windowController`,
    /// and falling back twice: to the main window, and then to the first one
    /// there is. A modal alert leaves `keyWindow` nil, and a menu item picked
    /// while one is up still has to mean something.
    private var focused: MainWindowController? {
        if let key = NSApp.keyWindow,
           let match = controllers.first(where: { $0.window === key }) { return match }
        if let main = NSApp.mainWindow,
           let match = controllers.first(where: { $0.window === main }) { return match }
        return controllers.first
    }

    /// Every menu action is an intent, and every intent now says where it
    /// came from.
    private func send(_ intent: Intent) {
        guard let controller = focused else { return }
        session.dispatch(intent, from: controller.windowID)
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        _ = GhosttyApp.shared
        buildMenu()

        let controller = MainWindowController(session: session, id: .first)
        controllers.append(controller)
        controller.showWindow(nil)
        controller.window?.makeKeyAndOrderFront(nil)

        // The first window carries everything that already exists. A window
        // opened later starts empty on purpose; this one starting empty would
        // just look like the app had lost the lot.
        session.start(firstWindow: controller.windowID, renderer: controller)
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

    @objc func newTab(_ sender: Any?) { send(.newTab(in: nil)) }
    @objc func goTo(_ sender: Any?) { send(.togglePicker) }
    @objc func find(_ sender: Any?) { send(.toggleSearch(global: false)) }
    @objc func findGlobal(_ sender: Any?) { send(.toggleSearch(global: true)) }
    @objc func closePane(_ sender: Any?) { send(.closePane(nil)) }
    @objc func closeTab(_ sender: Any?) { send(.closeTab(nil)) }
    @objc func splitRight(_ sender: Any?) { send(.split(1)) }
    @objc func splitDown(_ sender: Any?) { send(.split(2)) }
    @objc func nextTab(_ sender: Any?) { send(.nextTab) }
    @objc func previousTab(_ sender: Any?) { send(.previousTab) }
    @objc func nextWorkspace(_ sender: Any?) { send(.nextWorkspace) }
    @objc func previousWorkspace(_ sender: Any?) { send(.previousWorkspace) }

    @objc func showTab(_ sender: Any?) {
        guard let tag = (sender as? NSMenuItem)?.tag else { return }
        send(.activateTabIndex(tag == 9 ? -1 : tag - 1))
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
        send(.newWorkspace(named: field.stringValue))
    }

    @objc func toggleSidebar(_ sender: Any?) {
        (focused?.window as? KeepWindow)?.toggleSidebar(sender)
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

        // The standard editing commands, routed through the responder chain
        // to whichever terminal has focus. Without this menu, copy and paste
        // depended entirely on libghostty's own key table seeing the event —
        // a menu key equivalent is checked before the window and is the one
        // path a Mac user can count on.
        let editItem = NSMenuItem()
        let editMenu = NSMenu(title: "Edit")
        editMenu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        editMenu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        editMenu.addItem(.separator())
        editMenu.addItem(
            withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        editItem.submenu = editMenu
        main.addItem(editItem)

        let goItem = NSMenuItem()
        let goMenu = NSMenu(title: "Go")
        goMenu.addItem(withTitle: "Go To…", action: #selector(goTo(_:)), keyEquivalent: "p")
        goMenu.addItem(
            withTitle: "Find in Pane…", action: #selector(find(_:)), keyEquivalent: "f")
        let findAllItem = NSMenuItem(
            title: "Find Everywhere…", action: #selector(findGlobal(_:)), keyEquivalent: "f")
        findAllItem.keyEquivalentModifierMask = [.command, .shift]
        goMenu.addItem(findAllItem)
        goItem.submenu = goMenu
        main.addItem(goItem)

        let viewItem = NSMenuItem()
        let viewMenu = NSMenu(title: "View")
        let toggleSidebarItem = viewMenu.addItem(
            withTitle: "Toggle Sidebar",
            action: #selector(toggleSidebar(_:)), keyEquivalent: "b")
        toggleSidebarItem.target = self
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
        // A step out from ⌃Tab, because ⌃Tab is already the tabs' and a menu
        // key equivalent is matched before anything else can answer for it —
        // a second one here would simply never win.
        let nextSpaceItem = NSMenuItem(
            title: "Show Next Workspace",
            action: #selector(nextWorkspace(_:)),
            keyEquivalent: "\t")
        nextSpaceItem.keyEquivalentModifierMask = [.control, .option]
        windowMenu.addItem(nextSpaceItem)
        let previousSpaceItem = NSMenuItem(
            title: "Show Previous Workspace",
            action: #selector(previousWorkspace(_:)),
            keyEquivalent: "\t")
        previousSpaceItem.keyEquivalentModifierMask = [.control, .option, .shift]
        windowMenu.addItem(previousSpaceItem)
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
