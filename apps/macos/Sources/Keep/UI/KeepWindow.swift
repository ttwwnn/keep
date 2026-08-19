import AppKit
import GhosttyKit

/// Native unified chrome with a ClearMic-style flat leading panel.
///
/// The tracking separator keeps the toolbar boundary aligned with the split
/// while the custom, content-owned sidebar resizes or collapses. The tab
/// strip is the app's own view living as a toolbar item — no private view
/// hierarchy is hunted, moved, or constrained anywhere in this file anymore:
/// the ~230 lines that relocated AppKit's NSTabBar died with native tabbing.
final class KeepWindow: NSWindow, NSToolbarDelegate {
    private static let toolbarIdentifier = NSToolbar.Identifier("keep-main-toolbar")
    private static let tabStripItemIdentifier = NSToolbarItem.Identifier("keep-tab-strip")
    private static let toggleSidebarAccessoryIdentifier = NSUserInterfaceItemIdentifier(
        "keep-toggle-sidebar"
    )

    /// The sidebar toggle is chrome, but what it toggles is model state —
    /// the active tab's sidebar. The controller wires this to an intent.
    var onToggleSidebar: (() -> Void)?

    private weak var sidebarSplitView: NSSplitView?
    private weak var tabStrip: TabStripView?
    private weak var chromeBackdropView: NSView?
    private weak var sidebarBackdropView: NSView?
    private var terminalBackgroundObserver: NSObjectProtocol?

    // The last appearance actually applied, so becomeMain and repeated
    // notifications reapply nothing. Reapplying invalidates the shadow and
    // pokes the WindowServer — visible against a transparent, blurred window.
    private var appliedAppearance: (color: NSColor?, opacity: Double, blur: Int16)?

    deinit {
        if let terminalBackgroundObserver {
            NotificationCenter.default.removeObserver(terminalBackgroundObserver)
        }
    }

    override func becomeKey() {
        super.becomeKey()
        Trace.log("focus", "becomeKey responder=\(Trace.describe(firstResponder))")
    }

    override func resignKey() {
        super.resignKey()
        Trace.log("focus", "resignKey")
    }

    override func makeFirstResponder(_ responder: NSResponder?) -> Bool {
        let ok = super.makeFirstResponder(responder)
        Trace.log("responder", "→ \(Trace.describe(responder)) ok=\(ok)")
        return ok
    }

    func installUnifiedToolbar(
        sidebarController: NSSplitViewController,
        tabStrip: TabStripView,
        chromeBackdrop: NSView,
        sidebarBackdrop: NSView
    ) {
        sidebarSplitView = sidebarController.splitView
        self.tabStrip = tabStrip
        chromeBackdropView = chromeBackdrop
        sidebarBackdropView = sidebarBackdrop

        let toolbar = NSToolbar(identifier: Self.toolbarIdentifier)
        toolbar.delegate = self
        toolbar.displayMode = .iconOnly
        toolbar.allowsUserCustomization = false
        toolbar.autosavesConfiguration = false

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
    @objc func toggleSidebar(_ sender: Any?) {
        onToggleSidebar?()
    }

    private func applyTerminalAppearance() {
        let ghostty = GhosttyApp.shared
        let next = (
            color: ghostty.terminalBackground,
            opacity: ghostty.terminalBackgroundOpacity,
            blur: ghostty.terminalBackgroundBlur
        )
        if let applied = appliedAppearance,
            applied.color?.isEqual(next.color) ?? (next.color == nil),
            applied.opacity == next.opacity,
            applied.blur == next.blur
        {
            return
        }
        appliedAppearance = next

        let isTransparent = next.opacity < 1
        if isTransparent {
            // The renderer already draws the configured background colour at
            // `background-opacity`. Keep the host window effectively clear so
            // that alpha reaches the WindowServer instead of being composited
            // over a second opaque copy of the terminal colour.
            isOpaque = false
            backgroundColor = NSColor.white.withAlphaComponent(0.001)

            // The Metal surface starts below the toolbar. Continue the exact
            // same colour and alpha through that clear strip so the titlebar
            // does not reveal an un-tinted desktop behind it.
            if let terminalBackground = next.color {
                let tint = terminalBackground
                    .withAlphaComponent(CGFloat(next.opacity))
                    .cgColor
                chromeBackdropView?.layer?.backgroundColor = tint
                sidebarBackdropView?.layer?.backgroundColor = tint
                chromeBackdropView?.isHidden = false
                sidebarBackdropView?.isHidden = false
            } else {
                chromeBackdropView?.isHidden = true
                sidebarBackdropView?.isHidden = true
            }

            // The same libghostty hook Ghostty's own macOS host uses: reads
            // `background-blur` from the app config and applies WindowServer
            // blur to this NSWindow.
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
            if let terminalBackground = next.color {
                backgroundColor = terminalBackground.withAlphaComponent(1)
            }
        }

        invalidateShadow()
    }

    /// The tracking separator follows the split divider all the way to x = 0
    /// when the sidebar collapses, which would push any toolbar item before
    /// it into the overflow menu. A leading titlebar accessory is laid out
    /// independently, so the toggle stays available in both states.
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
    }

    // MARK: - toolbar

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [.sidebarTrackingSeparator, Self.tabStripItemIdentifier]
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

        case Self.tabStripItemIdentifier:
            guard let tabStrip else { return nil }
            let item = NSToolbarItem(itemIdentifier: itemIdentifier)
            item.view = tabStrip
            // Absorb all free width: a huge preferred width at low priority,
            // clamped by the toolbar. The tracking separator glues the left
            // edge to the sidebar divider — the exact geometry the old
            // NSTabBar relocation fought AppKit for, now free.
            let width = tabStrip.widthAnchor.constraint(equalToConstant: 10_000)
            width.priority = NSLayoutConstraint.Priority(240)
            let minWidth = tabStrip.widthAnchor.constraint(greaterThanOrEqualToConstant: 120)
            let height = tabStrip.heightAnchor.constraint(equalToConstant: 28)
            NSLayoutConstraint.activate([width, minWidth, height])
            return item

        default:
            return nil
        }
    }
}
