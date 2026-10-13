import AppKit
import SwiftUI

/// A visual continuation of the terminal behind chrome. It must never take
/// clicks away from the native titlebar controls or window drag.
final class TerminalTintBackdropView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}

/// The window's split, with a divider that still drags but draws nothing.
///
/// The sidebar's recessed backdrop already marks where it ends. On macOS 27
/// the default divider became a light rule down the full height of the
/// window, titlebar included — a second boundary on top of the first.
final class SeamlessSplitView: NSSplitView {
    override var dividerStyle: NSSplitView.DividerStyle {
        get { .thin }
        set {}
    }
    override var dividerColor: NSColor { .clear }
}

/// The rows, observable. Exists so `render` can update the list WITHOUT
/// replacing the hosting controller's rootView — a rootView swap rebuilds
/// the whole view tree, and the rebuilt outline view grabs the keyboard from
/// the terminal every time it happens.
@MainActor
final class SidebarRows: ObservableObject {
    @Published var rows: [SessionSnapshot.SidebarRow] = []
}

/// The flat, full-height sidebar host.
///
/// A default split item would get AppKit's Tahoe sidebar glass, inset and
/// rounded floating container; this one owns a flat backdrop that reaches
/// every window edge while the hosted SwiftUI list starts below the traffic
/// lights via the safe-area guide.
@MainActor
final class SidebarHost: NSViewController {
    let backdropView = TerminalTintBackdropView()
    private var listTop: NSLayoutConstraint!
    /// The battery, in the sidebar's top row while the window is in full
    /// screen: the menu bar that carries the system's is out of sight there.
    private let battery = BatteryIndicator()
    private var observers: [NSObjectProtocol] = []
    private let hosting: NSHostingController<WorkspaceSidebar>
    private let model = SidebarRows()

    init(dispatch: @escaping (Intent) -> Void) {
        hosting = NSHostingController(rootView: WorkspaceSidebar(model: model, dispatch: dispatch))
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    /// Equality-guarded: a quiet poll re-renders nothing.
    func render(_ newRows: [SessionSnapshot.SidebarRow]) {
        guard newRows != model.rows else { return }
        model.rows = newRows
    }

    override func loadView() {
        let container = NSView()
        backdropView.wantsLayer = true
        backdropView.translatesAutoresizingMaskIntoConstraints = false

        addChild(hosting)
        let content = hosting.view
        content.translatesAutoresizingMaskIntoConstraints = false

        container.addSubview(backdropView)
        container.addSubview(content)
        // Measured off the window rather than taken from the safe-area guide.
        // The guide gives this container nothing — the list began at the very
        // top of the sidebar and its first row sat behind the traffic lights
        // — and the titlebar is the only thing that knows how tall it is.
        listTop = content.topAnchor.constraint(equalTo: container.topAnchor, constant: 52)
        NSLayoutConstraint.activate([
            backdropView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            backdropView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            backdropView.topAnchor.constraint(equalTo: container.topAnchor),
            backdropView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            content.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            content.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            listTop,
            content.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])
        battery.isHidden = true
        container.addSubview(battery)
        view = container
        // Whether the menu bar is out of sight changes with full screen, the
        // screen, the space and the system's setting: ask again at each.
        let center = NotificationCenter.default
        let again: (Notification) -> Void = { [weak self] _ in
            MainActor.assumeIsolated { self?.view.needsLayout = true }
        }
        for name in [NSApplication.didChangeScreenParametersNotification,
                     NSWindow.didEnterFullScreenNotification, NSWindow.didExitFullScreenNotification,
                     NSWindow.didChangeScreenNotification, NSWindow.willEnterFullScreenNotification,
                     NSWindow.willExitFullScreenNotification] {
            observers.append(center.addObserver(forName: name, object: nil, queue: .main, using: again))
        }
        observers.append(NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.activeSpaceDidChangeNotification, object: nil, queue: .main, using: again))
    }

    /// The row the battery stands in, in full screen: the tab row's height.
    private static let batteryRow: CGFloat = 34
    /// Past the traffic lights, which come down over the row when the pointer
    /// reaches the top.
    private static let trafficLightsEnd: CGFloat = 84

    /// Whether the bar at the top of the screen, which carries the system's
    /// battery, is out of sight: full screen hides it, and so does the
    /// system's own setting to hide it (the screen then has no strip the
    /// windows must leave free).
    private func menuBarHidden(for window: NSWindow) -> Bool {
        if window.styleMask.contains(.fullScreen) || !NSMenu.menuBarVisible() { return true }
        guard let screen = window.screen ?? NSScreen.main else { return false }
        return screen.visibleFrame.maxY >= screen.frame.maxY - 1
    }

    /// Where the battery goes: between the traffic lights and the zoom, where
    /// the zoom stands when the titlebar row is shown (see `CenteringHost`:
    /// it rides the sidebar's edge, a divider's gap and the toggle away, then
    /// a gap more), centred on a row `rowHeight` tall at the top. Nil where
    /// there is no battery, or no room between the two.
    private func batteryFrame(rowHeight: CGFloat) -> NSRect? {
        guard battery.isPresent else { return nil }
        let toggle = max(4, view.bounds.width - 12 - 26)
        let zoomEnd = toggle - 6
        let zoomLeft = [false, true]
            .map { zoomEnd - ZoomControl.width(compact: $0) }
            .first { $0 >= 4 }
        let width = battery.fittingWidth
        let height = battery.fittingHeight
        let end = (zoomLeft ?? zoomEnd) - 10
        var left = end - width
        if left < Self.trafficLightsEnd {
            // No room between the two. In full screen the traffic lights are
            // away, and the battery takes the row's own corner instead.
            guard view.window?.styleMask.contains(.fullScreen) == true else { return nil }
            left = 12
        }
        return NSRect(
            x: left,
            y: view.bounds.height - rowHeight + ((rowHeight - height) / 2).rounded(),
            width: width, height: height)
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        guard let window = view.window else { return }
        // The titlebar's height, asked of the window: its frame less the part
        // it says is content. Plus a little, so the first row is under the
        // traffic lights rather than against them.
        let titlebar = window.frame.height - window.contentLayoutRect.height
        // Full screen has no titlebar and no traffic lights in the way, until
        // the pointer pulls them down over everything anyway: the list starts
        // at the top, level with the tab row beside it.
        let fullScreen = window.styleMask.contains(.fullScreen)
        let bar = titlebar > 1 ? titlebar : 52
        // In a window the titlebar's own row holds it; in full screen it gets
        // a row of its own, as tall as the tab row beside it.
        let batteryFrame = menuBarHidden(for: window)
            ? self.batteryFrame(rowHeight: fullScreen ? Self.batteryRow : bar) : nil
        if battery.isHidden != (batteryFrame == nil) { battery.isHidden = batteryFrame == nil }
        if let batteryFrame, battery.frame != batteryFrame { battery.frame = batteryFrame }
        let wanted = fullScreen
            ? (batteryFrame == nil ? 4 : Self.batteryRow + 4)
            : bar + 4
        if abs(listTop.constant - wanted) > 0.5 {
            listTop.constant = wanted
        }
    }
}
