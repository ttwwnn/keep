import AppKit
import SwiftUI

/// A window whose native tab bar lives in the titlebar row, so the chrome is
/// a single line: traffic lights, tabs, the new-tab button.
///
/// AppKit offers no supported way to do this. The approach — intercept the
/// tab bar accessory as it is added and constrain it into the toolbar row —
/// is adapted from Ghostty's `TitlebarTabsTahoeTerminalWindow` (MIT), which
/// carries the scars of every edge case. Private class names are looked up
/// by string; when they drift in a macOS release, the window degrades to a
/// normal tab bar below the titlebar rather than breaking.
final class KeepWindow: NSWindow, NSToolbarDelegate {
    /// The content-side view the tab bar's left edge hugs while the sidebar
    /// is expanded, so the sidebar owns the strip above itself.
    weak var contentAnchorView: NSView?

    private var tabBarObserver: NSObjectProtocol? {
        didSet {
            guard let oldValue else { return }
            NotificationCenter.default.removeObserver(oldValue)
        }
    }

    deinit {
        if let tabBarObserver {
            NotificationCenter.default.removeObserver(tabBarObserver)
        }
    }

    override func becomeMain() {
        super.becomeMain()
        // Only the main window of a tab group carries the real NSTabBar, and
        // AppKit moves it between windows as main changes; re-adopt it every
        // time we gain main. setupTabBar is idempotent.
        setupTabBar()
    }

    // AppKit adds the native tab bar through this. Detect it and change its
    // layout attribute before the call — after is an AppKit assertion.
    override func addTitlebarAccessoryViewController(
        _ childViewController: NSTitlebarAccessoryViewController
    ) {
        guard isTabBar(childViewController) else {
            super.addTitlebarAccessoryViewController(childViewController)
            return
        }
        tabBarObserver = nil
        childViewController.layoutAttribute = .right
        super.addTitlebarAccessoryViewController(childViewController)
        DispatchQueue.main.async { self.setupTabBar() }
    }

    override func removeTitlebarAccessoryViewController(at index: Int) {
        if let child = titlebarAccessoryViewControllers[safe: index], isTabBar(child) {
            tabBarObserver = nil
        }
        super.removeTitlebarAccessoryViewController(at: index)
    }

    private func isTabBar(_ child: NSTitlebarAccessoryViewController) -> Bool {
        guard child.identifier == nil else { return false }
        if child.view.contains(className: "NSTabBar") { return true }
        // When a window joins an existing group, AppKit first adds an empty
        // NSView and attaches the tab bar later.
        return child.layoutAttribute == .bottom
            && child.view.className == "NSView"
            && child.view.subviews.isEmpty
    }

    /// Constrain the tab bar's accessory into the toolbar row.
    private func setupTabBar() {
        guard tabBarObserver == nil else { return }
        guard
            let titlebarView,
            let tabBarView,
            let clipView = tabBarView.firstSuperview(withClassName: "NSTitlebarAccessoryClipView")
                ?? tabBarView.firstSuperview(withClassName: "NSTitlebarAccessoryContainerView"),
            let accessoryView = clipView.subviews[safe: 0],
            let toolbarView = titlebarView.firstDescendant(withClassName: "NSToolbarView")
        else { return }

        // Without this the bar stretches to the accessory row's height.
        if let newTabButton = titlebarView.firstDescendant(withClassName: "NSTabBarNewTabButton") {
            tabBarView.frame.size.height = newTabButton.frame.width
        }

        clipView.translatesAutoresizingMaskIntoConstraints = false
        accessoryView.translatesAutoresizingMaskIntoConstraints = false

        // The bar's left edge: at the sidebar divider while the sidebar is
        // expanded (the sidebar owns the strip above itself), but never left
        // of the traffic lights and the sidebar toggle, which is where it
        // lands when the sidebar collapses.
        let chromeClearance: CGFloat = 160
        var leftConstraints: [NSLayoutConstraint] = [
            clipView.leftAnchor.constraint(
                greaterThanOrEqualTo: toolbarView.leftAnchor, constant: chromeClearance)
        ]
        if let contentAnchorView {
            leftConstraints.append(
                clipView.leftAnchor.constraint(
                    greaterThanOrEqualTo: contentAnchorView.leftAnchor))
            let hug = clipView.leftAnchor.constraint(equalTo: contentAnchorView.leftAnchor)
            hug.priority = .defaultLow
            leftConstraints.append(hug)
        }

        NSLayoutConstraint.activate(leftConstraints + [
            clipView.rightAnchor.constraint(equalTo: toolbarView.rightAnchor),
            clipView.topAnchor.constraint(equalTo: toolbarView.topAnchor, constant: 2),
            clipView.heightAnchor.constraint(equalTo: toolbarView.heightAnchor),
            accessoryView.leftAnchor.constraint(equalTo: clipView.leftAnchor),
            accessoryView.rightAnchor.constraint(equalTo: clipView.rightAnchor),
            accessoryView.topAnchor.constraint(equalTo: clipView.topAnchor),
            accessoryView.heightAnchor.constraint(equalTo: clipView.heightAnchor),
        ])
        clipView.needsLayout = true
        accessoryView.needsLayout = true

        // Appearance changes resize the bar and wipe these constraints;
        // watch for that and re-apply.
        tabBarView.postsFrameChangedNotifications = true
        tabBarObserver = NotificationCenter.default.addObserver(
            forName: NSView.frameDidChangeNotification,
            object: tabBarView,
            queue: .main
        ) { [weak self] _ in
            guard let self else { return }
            self.tabBarObserver = nil
            DispatchQueue.main.async { self.setupTabBar() }
        }
    }
}

// MARK: - sidebar toggle accessory

extension KeepWindow {
    /// A sidebar toggle beside the traffic lights.
    ///
    /// Not an NSToolbarItem: the tab bar overlay makes the toolbar believe it
    /// has no room and shunts its items into the overflow chevron. A titlebar
    /// accessory with `.left` layout is positioned independently of toolbar
    /// layout, so it stays put in both sidebar states.
    func installSidebarToggle() {
        // SwiftUI, not NSButton: an unbordered template image inside the
        // transparent titlebar rendered nothing (though it stayed clickable),
        // while SwiftUI draws SF Symbols dependably anywhere.
        let view = NSHostingView(rootView: SidebarToggle())
        view.setFrameSize(view.fittingSize)

        let accessory = NSTitlebarAccessoryViewController()
        // The identifier keeps isTabBar from mistaking this for the tab bar.
        accessory.identifier = NSUserInterfaceItemIdentifier("keep-sidebar-toggle")
        accessory.view = view
        accessory.layoutAttribute = .left
        addTitlebarAccessoryViewController(accessory)
    }
}

private struct SidebarToggle: View {
    var body: some View {
        Button {
            NSApp.sendAction(
                #selector(NSSplitViewController.toggleSidebar(_:)), to: nil, from: nil)
        } label: {
            Image(systemName: "sidebar.left")
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(.secondary)
                .frame(width: 30, height: 26)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Toggle Sidebar")
        .padding(.leading, 6)
    }
}

// MARK: - private-view spelunking

extension NSWindow {
    /// The `NSTitlebarView` inside the theme frame. Private API by KVC.
    var titlebarView: NSView? {
        guard let frame = contentView?.superview else { return nil }
        guard frame.responds(to: Selector(("titlebarView"))) else { return nil }
        return frame.value(forKey: "titlebarView") as? NSView
    }

    var tabBarView: NSView? {
        titlebarView?.firstDescendant(withClassName: "NSTabBar")
    }
}

extension NSView {
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

extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
