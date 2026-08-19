import AppKit
import GhosttyKit

/// Native unified chrome with a ClearMic-style flat leading panel.
///
/// The tracking separator keeps the toolbar boundary aligned with the split
/// while the custom, content-owned sidebar resizes or collapses.
final class KeepWindow: NSWindow, NSToolbarDelegate {
    private static let toolbarIdentifier = NSToolbar.Identifier("keep-main-toolbar")
    private static let toggleSidebarAccessoryIdentifier = NSUserInterfaceItemIdentifier(
        "keep-toggle-sidebar"
    )

    private weak var sidebarItem: NSSplitViewItem?
    private weak var sidebarSplitView: NSSplitView?
    private weak var contentAnchorView: NSView?
    private weak var chromeBackdropView: NSView?
    private weak var sidebarBackdropView: NSView?
    private weak var sidebarToggleButton: NSButton?
    private weak var configuredTabBarView: NSView?
    private weak var configuredTabClipView: NSView?
    private var tabBarConstraints: [NSLayoutConstraint] = []
    private var terminalBackgroundObserver: NSObjectProtocol?
    private var tabBarObserver: NSObjectProtocol? {
        didSet {
            guard let oldValue else { return }
            NotificationCenter.default.removeObserver(oldValue)
        }
    }

    deinit {
        if let terminalBackgroundObserver {
            NotificationCenter.default.removeObserver(terminalBackgroundObserver)
        }
        if let tabBarObserver {
            NotificationCenter.default.removeObserver(tabBarObserver)
        }
        NSLayoutConstraint.deactivate(tabBarConstraints)
    }

    override func becomeKey() {
        super.becomeKey()
        // Who holds the keyboard matters more than which window is key: if the
        // sidebar list has it, navigation keys move the selection, and moving
        // the selection is what switching workspace *is*.
        Trace.log("focus", "becomeKey responder=\(Trace.describe(firstResponder))")
    }

    override func makeFirstResponder(_ responder: NSResponder?) -> Bool {
        let ok = super.makeFirstResponder(responder)
        Trace.log("responder", "→ \(Trace.describe(responder)) ok=\(ok)")
        return ok
    }

    override func resignKey() {
        super.resignKey()
        Trace.log("focus", "resignKey \(title)")
    }

    override func becomeMain() {
        super.becomeMain()
        Trace.log("focus", "becomeMain \(title)")
        applyTerminalAppearance()
        // AppKit moves the one real tab bar between the windows in a group.
        // Re-adopt it whenever this window becomes the selected tab.
        setupTabBar()
    }

    func installUnifiedToolbar(
        sidebarController: NSSplitViewController,
        sidebarItem: NSSplitViewItem,
        contentAnchor: NSView,
        chromeBackdrop: NSView,
        sidebarBackdrop: NSView
    ) {
        self.sidebarItem = sidebarItem
        let splitView = sidebarController.splitView
        sidebarSplitView = splitView
        contentAnchorView = contentAnchor
        chromeBackdropView = chromeBackdrop
        sidebarBackdropView = sidebarBackdrop

        let toolbar = NSToolbar(identifier: Self.toolbarIdentifier)
        toolbar.delegate = self
        toolbar.displayMode = .iconOnly
        toolbar.allowsUserCustomization = false
        toolbar.autosavesConfiguration = false

        // Keep the full-height leading section. The tab bar itself is moved
        // upward below; `.unifiedCompact` would inset the whole panel again.
        toolbarStyle = .unified
        titleVisibility = .hidden
        titlebarAppearsTransparent = true
        titlebarSeparatorStyle = .none
        self.toolbar = toolbar
        installSidebarToggleAccessory()

        applyTerminalAppearance()
        terminalBackgroundObserver = NotificationCenter.default.addObserver(
            forName: GhosttyApp.backgroundDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.applyTerminalAppearance()
        }
    }

    /// The flat split item deliberately does not have AppKit's `.sidebar`
    /// behavior, so `NSSplitViewController.toggleSidebar` would be a no-op.
    /// Keeping the standard selector on the window also makes the View menu
    /// resolve against the currently selected native tab instead of another
    /// backing window in the tab group.
    @objc func toggleSidebar(_ sender: Any?) {
        // AppKit moves one toolbar between the backing windows in a native tab
        // group, but a custom button keeps the target it was created with. The
        // visible button can therefore belong to an unselected KeepWindow.
        // Always mutate the window that is actually selected in the group.
        let activeWindow = (tabGroup?.selectedWindow as? KeepWindow) ?? self
        guard
            let sidebarItem = activeWindow.sidebarItem,
            sidebarItem.canCollapse
        else { return }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.2
            sidebarItem.animator().isCollapsed = !sidebarItem.isCollapsed
        }
    }

    private func applyTerminalAppearance() {
        let ghostty = GhosttyApp.shared
        let isTransparent = ghostty.terminalBackgroundOpacity < 1

        if isTransparent {
            // The renderer already draws the configured background colour at
            // `background-opacity`. Keep the host window effectively clear so
            // that alpha reaches the WindowServer instead of being composited
            // over a second opaque copy of the terminal colour.
            isOpaque = false
            backgroundColor = NSColor.white.withAlphaComponent(0.001)

            // The Metal surface starts below the toolbar. Continue the exact
            // same colour and alpha through that clear strip so the titlebar
            // does not reveal an un-tinted (often bright) desktop behind it.
            // This view lives only in the terminal split item. The flat
            // leading panel has its own matching regional backdrop.
            if let terminalBackground = ghostty.terminalBackground {
                let tint = terminalBackground
                    .withAlphaComponent(CGFloat(ghostty.terminalBackgroundOpacity))
                    .cgColor
                chromeBackdropView?.layer?.backgroundColor = tint
                sidebarBackdropView?.layer?.backgroundColor = tint
                chromeBackdropView?.isHidden = false
                sidebarBackdropView?.isHidden = false
            } else {
                chromeBackdropView?.isHidden = true
                sidebarBackdropView?.isHidden = true
            }

            // This is the same libghostty hook used by Ghostty's macOS host.
            // It reads `background-blur` from the active app config and applies
            // the requested WindowServer blur to this NSWindow.
            if let app = ghostty.app {
                ghostty_set_window_background_blur(
                    app,
                    Unmanaged.passUnretained(self).toOpaque()
                )
            }
        } else {
            isOpaque = true
            chromeBackdropView?.isHidden = true
            sidebarBackdropView?.isHidden = true
            if let terminalBackground = ghostty.terminalBackground {
                backgroundColor = terminalBackground.withAlphaComponent(1)
            }
        }

        invalidateShadow()
    }

    // AppKit creates its tab bar as a titlebar accessory below the toolbar.
    // Move only that system-owned accessory into the toolbar row. If Apple's
    // private view hierarchy changes, the guard in `setupTabBar` simply leaves
    // the normal tab row in place instead of breaking the window.
    override func addTitlebarAccessoryViewController(
        _ childViewController: NSTitlebarAccessoryViewController
    ) {
        guard isTabBar(childViewController) else {
            super.addTitlebarAccessoryViewController(childViewController)
            return
        }
        clearTabBarLayout()
        childViewController.layoutAttribute = .right
        super.addTitlebarAccessoryViewController(childViewController)
        DispatchQueue.main.async { [weak self] in self?.setupTabBar() }
    }

    override func removeTitlebarAccessoryViewController(at index: Int) {
        if let child = titlebarAccessoryViewControllers[safe: index], isTabBar(child) {
            clearTabBarLayout()
        }
        super.removeTitlebarAccessoryViewController(at: index)
    }

    private func isTabBar(_ child: NSTitlebarAccessoryViewController) -> Bool {
        guard child.identifier == nil else { return false }
        if child.view.contains(className: "NSTabBar") { return true }
        // A window joining an existing group receives the accessory before
        // AppKit attaches NSTabBar to its initially empty view.
        return child.layoutAttribute == .bottom
            && child.view.className == "NSView"
            && child.view.subviews.isEmpty
    }

    /// Re-place the tab bar after the group's membership changed.
    ///
    /// AppKit keeps one real tab bar and moves it between the windows of a
    /// group, so only the selected one has it to find. When the group grows or
    /// shrinks, the window that ends up holding it may never have run the
    /// placement — which leaves the bar sitting in AppKit's own row instead of
    /// in the toolbar line the rest of this chrome lives on.
    func refreshTabBar() {
        (tabGroup?.selectedWindow as? KeepWindow ?? self).setupTabBar()
    }

    private func setupTabBar() {
        Trace.log("tabbar", "setup: titlebar=\(titlebarView != nil) tabBar=\(tabBarView != nil) "
            + "groupWindows=\(tabGroup?.windows.count ?? 0) barVisible=\(tabGroup?.isTabBarVisible ?? false)")
        guard
            let titlebarView,
            let tabBarView,
            let clipView = tabBarView.firstSuperview(withClassName: "NSTitlebarAccessoryClipView")
                ?? tabBarView.firstSuperview(withClassName: "NSTitlebarAccessoryContainerView"),
            let accessoryView = clipView.subviews[safe: 0],
            let toolbarView = titlebarView.firstDescendant(withClassName: "NSToolbarView")
        else { return }

        if configuredTabBarView === tabBarView,
            configuredTabClipView === clipView,
            !tabBarConstraints.isEmpty,
            tabBarConstraints.allSatisfy(\.isActive)
        {
            Trace.log("tabbar", "setup: already configured, skipped")
            return
        }
        Trace.log("tabbar", "setup: relocating into the toolbar row")
        clearTabBarLayout()

        // Preserve AppKit's native 28 pt tab size. The clip is shifted below
        // so these controls share the toolbar items' vertical center.
        if let newTabButton = titlebarView.firstDescendant(withClassName: "NSTabBarNewTabButton") {
            tabBarView.frame.size.height = newTabButton.frame.width
        }

        clipView.translatesAutoresizingMaskIntoConstraints = false
        accessoryView.translatesAutoresizingMaskIntoConstraints = false
        titlebarView.layoutSubtreeIfNeeded()

        // Expanded: the content edge wins, leaving the whole leading section
        // to the sidebar. Collapsed: the tabs begin just after the real toggle.
        var leftConstraints: [NSLayoutConstraint] = []
        if let sidebarToggleButton {
            // Titlebar accessories and the tab clip use separate Auto Layout
            // engines. Convert the button edge to a toolbar-local constant
            // instead of creating an illegal cross-engine constraint.
            let buttonFrame = sidebarToggleButton.convert(
                sidebarToggleButton.bounds,
                to: toolbarView
            )
            let clearance = buttonFrame.maxX > 0
                && buttonFrame.maxX < toolbarView.bounds.width
                ? buttonFrame.maxX + 8
                : 140
            leftConstraints.append(
                clipView.leftAnchor.constraint(
                    greaterThanOrEqualTo: toolbarView.leftAnchor,
                    constant: clearance
                )
            )
        } else {
            leftConstraints.append(
                clipView.leftAnchor.constraint(
                    greaterThanOrEqualTo: toolbarView.leftAnchor,
                    constant: 140
                )
            )
        }
        if let contentAnchorView {
            leftConstraints.append(
                clipView.leftAnchor.constraint(
                    greaterThanOrEqualTo: contentAnchorView.leftAnchor
                )
            )
            let hugContentEdge = clipView.leftAnchor.constraint(
                equalTo: contentAnchorView.leftAnchor
            )
            hugContentEdge.priority = .defaultLow
            leftConstraints.append(hugContentEdge)
        }

        tabBarConstraints = leftConstraints + [
            clipView.rightAnchor.constraint(equalTo: toolbarView.rightAnchor),
            // AppKit otherwise leaves 16 pt above and 8 pt below its 28 pt
            // tab bar. Raising the accessory by 4 pt centers it at 12/12.
            clipView.topAnchor.constraint(
                equalTo: titlebarView.topAnchor,
                constant: -4
            ),
            clipView.heightAnchor.constraint(equalTo: toolbarView.heightAnchor),
            accessoryView.leftAnchor.constraint(equalTo: clipView.leftAnchor),
            accessoryView.rightAnchor.constraint(equalTo: clipView.rightAnchor),
            accessoryView.topAnchor.constraint(equalTo: clipView.topAnchor),
            accessoryView.heightAnchor.constraint(equalTo: clipView.heightAnchor),
        ]
        configuredTabBarView = tabBarView
        configuredTabClipView = clipView
        NSLayoutConstraint.activate(tabBarConstraints)

        tabBarView.postsFrameChangedNotifications = true
        tabBarObserver = NotificationCenter.default.addObserver(
            forName: NSView.frameDidChangeNotification,
            object: tabBarView,
            queue: .main
        ) { [weak self] _ in
            DispatchQueue.main.async { [weak self] in self?.setupTabBar() }
        }
    }

    private func clearTabBarLayout() {
        tabBarObserver = nil
        NSLayoutConstraint.deactivate(tabBarConstraints)
        tabBarConstraints.removeAll()
        configuredTabBarView = nil
        configuredTabClipView = nil
    }

    /// The tracking separator follows the split divider all the way to x = 0
    /// when the sidebar collapses. Any toolbar item before it is therefore
    /// moved into the overflow menu. A leading titlebar accessory is laid out
    /// independently, so the real toggle remains available in both states.
    private func installSidebarToggleAccessory() {
        let button = NSButton(
            image: NSImage(
                systemSymbolName: "sidebar.left",
                accessibilityDescription: "Toggle Sidebar"
            ) ?? NSImage(),
            target: self,
            action: #selector(toggleSidebar(_:))
        )
        button.bezelStyle = .circular
        button.controlSize = .small
        button.imagePosition = .imageOnly
        button.imageScaling = .scaleProportionallyDown
        button.toolTip = "Toggle Sidebar"
        button.setFrameSize(NSSize(width: 28, height: 28))

        let accessory = NSTitlebarAccessoryViewController()
        accessory.identifier = Self.toggleSidebarAccessoryIdentifier
        accessory.layoutAttribute = .left
        accessory.view = button
        addTitlebarAccessoryViewController(accessory)
        sidebarToggleButton = button
    }

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [.sidebarTrackingSeparator]
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        toolbarDefaultItemIdentifiers(toolbar)
    }

    func toolbar(
        _ toolbar: NSToolbar,
        itemForItemIdentifier itemIdentifier: NSToolbarItem.Identifier,
        willBeInsertedIntoToolbar flag: Bool
    ) -> NSToolbarItem? {
        switch itemIdentifier {
        case .sidebarTrackingSeparator:
            guard let sidebarSplitView else { return nil }
            return NSTrackingSeparatorToolbarItem(
                identifier: itemIdentifier,
                splitView: sidebarSplitView,
                dividerIndex: 0
            )

        default:
            return nil
        }
    }
}

// MARK: - Native tab bar placement

private extension NSWindow {
    var titlebarView: NSView? {
        guard let frame = contentView?.superview else { return nil }
        guard frame.responds(to: Selector(("titlebarView"))) else { return nil }
        return frame.value(forKey: "titlebarView") as? NSView
    }

    var tabBarView: NSView? {
        titlebarView?.firstDescendant(withClassName: "NSTabBar")
    }
}

private extension NSView {
    func firstSuperview(withClassName name: String) -> NSView? {
        guard let superview else { return nil }
        if String(describing: type(of: superview)) == name { return superview }
        return superview.firstSuperview(withClassName: name)
    }

    func firstDescendant(withClassName name: String) -> NSView? {
        for subview in subviews {
            if String(describing: type(of: subview)) == name { return subview }
            if let found = subview.firstDescendant(withClassName: name) { return found }
        }
        return nil
    }

    func contains(className name: String) -> Bool {
        if String(describing: type(of: self)) == name { return true }
        return subviews.contains { $0.contains(className: name) }
    }
}

private extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
