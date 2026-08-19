import AppKit
import GhosttyKit

/// Native unified chrome with a ClearMic-style flat leading panel.
///
/// The tracking separator keeps the toolbar boundary aligned with the split
/// while the custom, content-owned sidebar resizes or collapses. The tab
/// strip is NOT a toolbar item — toolbar sizing pushed a flexible custom view
/// into the overflow menu — it is plain content, constrained into the chrome
/// row by the window controller. No private view hierarchy is hunted, moved,
/// or constrained anywhere in this file: the ~230 lines that relocated
/// AppKit's NSTabBar died with native tabbing.
final class KeepWindow: NSWindow, NSToolbarDelegate {
    private static let toolbarIdentifier = NSToolbar.Identifier("keep-main-toolbar")
    private static let toggleSidebarAccessoryIdentifier = NSUserInterfaceItemIdentifier(
        "keep-toggle-sidebar"
    )

    /// The sidebar toggle is chrome, but what it toggles is model state —
    /// the active tab's sidebar. The controller wires this to an intent.
    var onToggleSidebar: (() -> Void)?

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

    func installUnifiedToolbar(chromeBackdrop: NSView, sidebarBackdrop: NSView) {
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
        button.isBordered = false
        button.controlSize = .small
        button.imagePosition = .imageOnly
        button.imageScaling = .scaleProportionallyDown
        button.toolTip = "Toggle Sidebar"
        button.contentTintColor = .secondaryLabelColor
        let side: CGFloat = 24
        button.frame = NSRect(x: 0, y: 0, width: side, height: side)

        // The same ground the new-tab button stands on, so the two controls
        // in the chrome are made of one thing.
        let accessory = NSTitlebarAccessoryViewController()
        accessory.identifier = Self.toggleSidebarAccessoryIdentifier
        accessory.layoutAttribute = .left
        if let glass = Glass.lozenge(cornerRadius: side / 2) {
            Glass.tint(glass, NSColor.white.withAlphaComponent(0.22))
            glass.frame = NSRect(x: 0, y: 0, width: side, height: side)
            glass.addSubview(button)
            let host = NSView(frame: NSRect(x: 0, y: 0, width: side + 8, height: side))
            glass.frame.origin.x = 4
            host.addSubview(glass)
            accessory.view = host
        } else {
            button.bezelStyle = .circular
            button.isBordered = true
            button.setFrameSize(NSSize(width: 28, height: 28))
            accessory.view = button
        }
        addTitlebarAccessoryViewController(accessory)
    }

    // MARK: - toolbar

    /// No items at all.
    ///
    /// The toolbar earns its place by giving the titlebar its unified height
    /// and letting content run under it; it holds nothing. A tracking
    /// separator used to live here to keep the toolbar's boundary on the
    /// split divider, back when the tab bar was a toolbar item — all it does
    /// now is draw a vertical line down the left of the tab strip, which is a
    /// boundary this chrome does not want.
    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        []
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        toolbarDefaultItemIdentifiers(toolbar)
    }

    func toolbar(
        _ toolbar: NSToolbar,
        itemForItemIdentifier itemIdentifier: NSToolbarItem.Identifier,
        willBeInsertedIntoToolbar flag: Bool
    ) -> NSToolbarItem? {
        nil
    }
}
