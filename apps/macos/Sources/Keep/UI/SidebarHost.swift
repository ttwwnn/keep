import AppKit
import SwiftUI

/// A visual continuation of the terminal behind chrome. It must never take
/// clicks away from the native titlebar controls or window drag.
class TerminalTintBackdropView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
}

/// A side panel drawn as the Finder draws its sidebar on macOS 26: a card with
/// rounded corners, held off the window's edges, the room's own colour in the
/// margin around it.
final class FloatingPanelBackdrop: TerminalTintBackdropView {
    /// The margin between the card and the window's edges, and the card's corner: the two the Finder uses.
    static let inset: CGFloat = 8
    static let radius: CGFloat = 18
    private let card = CALayer()
    /// The margin: the window's colour, the card cut out of it so the two never stack.
    private let floor = CAShapeLayer()

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
        floor.fillRule = .evenOdd
        card.cornerRadius = Self.radius
        card.cornerCurve = .continuous
        card.borderWidth = 1
        layer?.addSublayer(floor)
        layer?.addSublayer(card)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    /// `floor` nil where the window already paints the margin (an opaque window); `dark` picks the rim.
    func paint(card color: CGColor?, floor floorColor: CGColor?, dark: Bool) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        card.backgroundColor = color
        card.borderColor = (dark ? NSColor.white.withAlphaComponent(0.08) : NSColor.black.withAlphaComponent(0.07)).cgColor
        floor.fillColor = floorColor
        CATransaction.commit()
    }

    func paint(like other: FloatingPanelBackdrop) {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        card.backgroundColor = other.card.backgroundColor
        card.borderColor = other.card.borderColor
        floor.fillColor = other.floor.fillColor
        CATransaction.commit()
        isHidden = other.isHidden
    }

    var cardColor: CGColor? { card.backgroundColor }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        place()
    }

    override func layout() {
        super.layout()
        place()
    }

    private func place() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        let inner = bounds.insetBy(dx: Self.inset, dy: Self.inset)
        card.frame = inner.width > 0 && inner.height > 0 ? inner : .zero
        floor.frame = bounds
        let path = CGMutablePath()
        path.addRect(bounds)
        if card.frame != .zero {
            path.addRoundedRect(in: inner, cornerWidth: Self.radius, cornerHeight: Self.radius)
        }
        floor.path = path
        CATransaction.commit()
    }
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
    /// Moves when the footer's account list does: the tabs' "(IA·n)" is read
    /// from it, so the list is drawn again.
    @Published var accountListRevision = 0
    /// Whether the window is the one in use (key). Out of use, the states are
    /// drawn in the resting tones (see `Palette.Resting`).
    @Published var inUse = true
}

/// The full-height sidebar host.
///
/// A default split item would get AppKit's Tahoe sidebar glass; this one owns
/// its backdrop, a card inset and rounded as the Finder's sidebar is but in the
/// terminal's own colours, while the hosted SwiftUI list starts below the
/// traffic lights.
@MainActor
final class SidebarHost: NSViewController {
    let backdropView = FloatingPanelBackdrop()
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
    func render(_ newRows: [SessionSnapshot.SidebarRow], accountListRevision: Int) {
        if accountListRevision != model.accountListRevision { model.accountListRevision = accountListRevision }
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
            // Inside the card, so the rows' pills and the footer stay off its rim.
            content.leadingAnchor.constraint(equalTo: container.leadingAnchor, constant: FloatingPanelBackdrop.inset),
            content.trailingAnchor.constraint(equalTo: container.trailingAnchor, constant: -FloatingPanelBackdrop.inset),
            listTop,
            content.bottomAnchor.constraint(equalTo: container.bottomAnchor, constant: -FloatingPanelBackdrop.inset),
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
        let keyChanged: (Notification) -> Void = { [weak self] note in
            MainActor.assumeIsolated {
                guard let self, let window = self.view.window,
                      (note.object as? NSWindow) === window else { return }
                self.syncInUse()
            }
        }
        for name in [NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification] {
            observers.append(center.addObserver(forName: name, object: nil, queue: .main, using: keyChanged))
        }
    }

    private func syncInUse() {
        let now = view.window?.isKeyWindow ?? true
        if model.inUse != now { model.inUse = now }
    }

    /// The row the battery stands in, in full screen: the tab row's height.
    private static let batteryRow: CGFloat = 34
    /// Past the traffic lights, which come down over the row when the pointer
    /// reaches the top.
    private static let trafficLightsEnd: CGFloat = 84

    /// Where the battery goes: between the zoom, right after the traffic lights
    /// (see `CenteringHost.zoomPlace`), and the sidebar toggle, centred on a row
    /// `rowHeight` tall at the top. Nil where there is no battery, or no room
    /// between the two.
    private func batteryFrame(rowHeight: CGFloat) -> NSRect? {
        guard battery.isPresent else { return nil }
        let toggle = max(4, view.bounds.width - 12 - 26)
        // The zoom right after the traffic lights (CenteringHost.zoomPlace, the same rule), the battery after it.
        let zoomStart = Self.trafficLightsEnd
        let zoomRight = [false, true]
            .map { zoomStart + ZoomControl.width(compact: $0) }
            .first { $0 <= toggle - 6 }
        let width = battery.fittingWidth
        let height = battery.fittingHeight
        var left = (zoomRight ?? zoomStart) + 10
        if left + width > toggle - 10 {
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
        syncInUse()
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
        let batteryFrame = BatteryIndicator.menuBarHidden(for: window)
            ? self.batteryFrame(rowHeight: fullScreen ? Self.batteryRow : bar) : nil
        if battery.isHidden != (batteryFrame == nil) { battery.isHidden = batteryFrame == nil }
        if let batteryFrame, battery.frame != batteryFrame { battery.frame = batteryFrame }
        let wanted = fullScreen
            ? (batteryFrame == nil ? 4 : Self.batteryRow + 4)
            : bar + 4
        if abs(listTop.constant - wanted) > 0.5 {
            listTop.constant = wanted
        }
        holdMinimum(showsBattery: BatteryIndicator.menuBarHidden(for: window) && battery.isPresent)
    }

    /// The sidebar never gets narrower than its top row needs (pedido de 17/11/2026): the traffic lights, the whole
    /// zoom, the battery and the time where they show, and the toggle, by the room rules of `batteryFrame`.
    private func holdMinimum(showsBattery: Bool) {
        guard let item = (parent as? NSSplitViewController)?.splitViewItem(for: self) else { return }
        let zoomEnd = Self.trafficLightsEnd + ZoomControl.width(compact: false)
        let batteryRoom = showsBattery ? 10 + battery.fittingWidth : 0
        let minimum = max(200, ceil(zoomEnd + batteryRoom + 10 + 26 + 12))
        guard item.minimumThickness != minimum else { return }
        item.minimumThickness = minimum
        if item.maximumThickness != NSSplitViewItem.unspecifiedDimension, item.maximumThickness < minimum {
            item.maximumThickness = minimum
        }
    }
}
