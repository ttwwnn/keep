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
                // The sidebar sits a step behind the terminal rather than
                // level with it. The chrome row above the content keeps the
                // content's own colour, because it is the content's row.
                sidebarBackdropView?.layer?.backgroundColor = Self
                    .recessed(terminalBackground)
                    .withAlphaComponent(CGFloat(next.opacity))
                    .cgColor
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
            if let terminalBackground = next.color {
                backgroundColor = terminalBackground.withAlphaComponent(1)
                // Opaque or not, the sidebar is a step behind: the window's
                // own colour paints the content, and the sidebar paints over
                // it with the recessed one.
                sidebarBackdropView?.layer?.backgroundColor = Self
                    .recessed(terminalBackground).cgColor
                sidebarBackdropView?.isHidden = false
            } else {
                sidebarBackdropView?.isHidden = true
            }
        }

        Trace.log(
            "chrome",
            "appearance opacity=\(next.opacity) sidebar=\(sidebarBackdropView?.isHidden == false ? "recessed" : "hidden")")
        invalidateShadow()
    }

    /// A panel's colour: the terminal's, a step further back.
    ///
    /// Taken toward black rather than toward grey, so a terminal with a warm
    /// or cool background keeps its cast instead of washing out — the sidebar
    /// should read as the same room with less light in it, not as a different
    /// surface stuck to the side.
    private static func recessed(_ color: NSColor) -> NSColor {
        guard let rgb = color.usingColorSpace(.sRGB) else { return color }
        let factor: CGFloat = 0.72
        return NSColor(
            srgbRed: rgb.redComponent * factor,
            green: rgb.greenComponent * factor,
            blue: rgb.blueComponent * factor,
            alpha: rgb.alphaComponent)
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
        button.toolTip = "Toggle Sidebar (⌘B)"
        button.contentTintColor = .secondaryLabelColor
        // The height of a tab's capsule, so the controls in the chrome are one
        // family. It used to be 28 because that is where the titlebar centred
        // the old bezelled button, and shrinking it moved the control down by
        // the difference — the host below keeps it centred now, so the size is
        // free to be chosen rather than inherited.
        let side: CGFloat = 26
        button.frame = NSRect(x: 0, y: 0, width: side, height: side)

        // The same ground the new-tab button stands on, so the two controls
        // in the chrome are made of one thing.
        let accessory = NSTitlebarAccessoryViewController()
        accessory.identifier = Self.toggleSidebarAccessoryIdentifier
        accessory.layoutAttribute = .left
        if let glass = Glass.lozenge(cornerRadius: side / 2) {
            // The titlebar stretches an accessory view to its full height,
            // which a bezelled button tolerated and a glass capsule does not:
            // it became a tall rounded slab. So the accessory is a host that
            // stretches, holding a fixed circle it keeps centred.
            // Untinted at rest, like the new-tab button: glass refracts
            // darker than a dark bar, which is the quiet state this wants.
            glass.addSubview(button)
            accessory.view = CenteringHost(child: glass, size: side, leading: 4)
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


/// Keeps one fixed-size child centred while itself being stretched.
///
/// The titlebar sizes an accessory view to the whole of its height, and a
/// control that is a shape — a circle of glass, say — has to stay that shape
/// rather than being stretched into a slab.
private final class CenteringHost: NSView {
    private let child: NSView
    private let side: CGFloat
    private let leading: CGFloat

    init(child: NSView, size: CGFloat, leading: CGFloat) {
        self.child = child
        self.side = size
        self.leading = leading
        super.init(frame: NSRect(x: 0, y: 0, width: size + leading * 2, height: size))
        addSubview(child)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    override var intrinsicContentSize: NSSize {
        NSSize(width: side + leading * 2, height: NSView.noIntrinsicMetric)
    }

    override func layout() {
        super.layout()
        child.frame = NSRect(
            x: leading,
            y: ((bounds.height - side) / 2).rounded(),
            width: side,
            height: side)
    }
}
