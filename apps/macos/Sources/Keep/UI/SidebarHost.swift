import AppKit
import SwiftUI

/// A visual continuation of the terminal behind chrome. It must never take
/// clicks away from the native titlebar controls or window drag.
final class TerminalTintBackdropView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
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
        NSLayoutConstraint.activate([
            backdropView.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            backdropView.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            backdropView.topAnchor.constraint(equalTo: container.topAnchor),
            backdropView.bottomAnchor.constraint(equalTo: container.bottomAnchor),
            content.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            content.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            content.topAnchor.constraint(equalTo: container.safeAreaLayoutGuide.topAnchor),
            content.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])
        view = container
    }
}
