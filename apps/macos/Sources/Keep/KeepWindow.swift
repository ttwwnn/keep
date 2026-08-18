import AppKit
import SwiftUI

/// The window chrome, arranged the way Finder arranges it: the sidebar runs
/// to the very top with the traffic lights and toggle floating over it, and
/// the native tab bar sits in its own row on the content side.
final class KeepWindow: NSWindow {
    /// The content-side view marking where the sidebar ends.
    weak var contentAnchorView: NSView?

    private var sidebarPatch: NSVisualEffectView?

    override func becomeMain() {
        super.becomeMain()
        installSidebarMaterialPatch()
    }

    /// Let the sidebar's material show through the titlebar strip above it.
    ///
    /// The theme frame paints an opaque band across the titlebar no matter
    /// what — with the toolbar gone and the titlebar transparent it still
    /// does — cutting the sidebar off. This view sits inside the titlebar
    /// and uses `.withinWindow` blending, so it reproduces whatever the
    /// window renders beneath it: the sidebar's actual material, which
    /// already extends to the top thanks to fullSizeContentView. Because it
    /// samples this window rather than the desktop, it cannot drift from the
    /// real sidebar the way a second `.behindWindow` material did. Its
    /// trailing edge is tied to the divider, so it collapses with the
    /// sidebar.
    private func installSidebarMaterialPatch() {
        guard sidebarPatch == nil,
            let themeFrame = contentView?.superview,
            let titlebar = themeFrame.subviews.first(where: {
                String(describing: type(of: $0)) == "NSTitlebarContainerView"
            }),
            let contentAnchorView
        else { return }

        let patch = NSVisualEffectView()
        patch.material = .sidebar
        patch.blendingMode = .withinWindow
        patch.state = .followsWindowActiveState
        patch.translatesAutoresizingMaskIntoConstraints = false
        titlebar.addSubview(patch, positioned: .below, relativeTo: titlebar.subviews.first)
        NSLayoutConstraint.activate([
            patch.leadingAnchor.constraint(equalTo: titlebar.leadingAnchor),
            patch.topAnchor.constraint(equalTo: titlebar.topAnchor),
            patch.bottomAnchor.constraint(equalTo: titlebar.bottomAnchor),
            patch.trailingAnchor.constraint(equalTo: contentAnchorView.leadingAnchor),
        ])
        sidebarPatch = patch
    }

    /// A sidebar toggle beside the traffic lights, floating over the sidebar.
    ///
    /// SwiftUI in a titlebar accessory: toolbar items need a toolbar (whose
    /// backdrop is the very band being fought), and an unbordered NSButton
    /// with a template image drew nothing inside the transparent titlebar.
    func installSidebarToggle() {
        let view = NSHostingView(rootView: SidebarToggle())
        view.setFrameSize(view.fittingSize)

        let accessory = NSTitlebarAccessoryViewController()
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
