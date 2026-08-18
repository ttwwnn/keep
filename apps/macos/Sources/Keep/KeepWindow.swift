import AppKit

/// Native unified chrome, matching the structure used by Finder and ClearMic.
///
/// The tracking separator tells AppKit that the toolbar has a section owned by
/// the sidebar. AppKit can then extend the sidebar material behind the traffic
/// lights and keep the boundary aligned while the split resizes or collapses.
final class KeepWindow: NSWindow, NSToolbarDelegate {
    private static let toolbarIdentifier = NSToolbar.Identifier("keep-main-toolbar")

    private weak var sidebarSplitView: NSSplitView?

    func installUnifiedToolbar(tracking splitView: NSSplitView) {
        sidebarSplitView = splitView

        let toolbar = NSToolbar(identifier: Self.toolbarIdentifier)
        toolbar.delegate = self
        toolbar.displayMode = .iconOnly
        toolbar.allowsUserCustomization = false
        toolbar.autosavesConfiguration = false

        toolbarStyle = .unified
        titleVisibility = .visible
        titlebarSeparatorStyle = .none
        self.toolbar = toolbar
    }

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [.toggleSidebar, .sidebarTrackingSeparator]
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
        case .toggleSidebar:
            let item = NSToolbarItem(itemIdentifier: itemIdentifier)
            item.label = "Toggle Sidebar"
            item.paletteLabel = item.label
            item.toolTip = item.label
            item.image = NSImage(
                systemSymbolName: "sidebar.left",
                accessibilityDescription: item.label
            )
            item.target = nil
            item.action = #selector(NSSplitViewController.toggleSidebar(_:))
            return item

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
