import AppKit

/// A window whose titlebar carries the tab bar.
///
/// macOS puts native tabs in the titlebar row only when the window has a
/// toolbar in the compact unified style. Without the toolbar the tab bar
/// appears as a second strip below the titlebar, which is the look this
/// window exists to avoid.
final class TerminalWindow: NSWindow {
    override func awakeFromNib() {
        super.awakeFromNib()
        installToolbar()
    }

    func installToolbar() {
        guard toolbar == nil else { return }
        let toolbar = NSToolbar(identifier: "KeepTerminalToolbar")
        toolbar.showsBaselineSeparator = false
        self.toolbar = toolbar
        toolbarStyle = .unifiedCompact
        titlebarAppearsTransparent = true
        titleVisibility = .hidden
    }
}
