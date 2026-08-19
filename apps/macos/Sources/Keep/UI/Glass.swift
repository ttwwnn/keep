import AppKit

/// Liquid Glass, where the system has it.
///
/// `NSGlassEffectView` arrived in macOS 26 and this app deploys to 14, so
/// every use of it is behind a check and every check has a fallback that
/// looked acceptable before glass existed. The fallbacks are not placeholders
/// — they are what the app looked like yesterday.
///
/// Glass belongs on things that float over content: the picker's card, the
/// selected tab's lozenge, a field. It deliberately does not go on the
/// sidebar or the chrome strip, which are tinted with the terminal's own
/// background so that chrome and content read as one surface — refracting
/// there would undo the thing that tinting is for.
enum Glass {
    /// Whether the system can actually draw glass.
    static var isAvailable: Bool {
        if #available(macOS 26.0, *) { return true }
        return false
    }

    /// A view that hosts `content` on glass, or on a plain rounded material
    /// where glass does not exist.
    static func panel(_ content: NSView, cornerRadius: CGFloat) -> NSView {
        if #available(macOS 26.0, *) {
            let glass = NSGlassEffectView()
            glass.cornerRadius = cornerRadius
            glass.style = .regular
            glass.contentView = content
            return glass
        }
        let effect = NSVisualEffectView()
        effect.material = .hudWindow
        effect.blendingMode = .withinWindow
        effect.state = .active
        effect.wantsLayer = true
        effect.layer?.cornerRadius = cornerRadius
        effect.layer?.cornerCurve = .continuous
        effect.layer?.borderWidth = 1
        effect.layer?.borderColor = NSColor.white.withAlphaComponent(0.12).cgColor
        content.translatesAutoresizingMaskIntoConstraints = false
        effect.addSubview(content)
        NSLayoutConstraint.activate([
            content.leadingAnchor.constraint(equalTo: effect.leadingAnchor),
            content.trailingAnchor.constraint(equalTo: effect.trailingAnchor),
            content.topAnchor.constraint(equalTo: effect.topAnchor),
            content.bottomAnchor.constraint(equalTo: effect.bottomAnchor),
        ])
        return effect
    }

    /// A small glass lozenge with nothing in it — a highlight behind
    /// something else, like the selected tab.
    ///
    /// Returns nil where glass is unavailable, so callers keep painting the
    /// flat fill they already had rather than being handed a stand-in.
    static func lozenge(cornerRadius: CGFloat) -> NSView? {
        guard #available(macOS 26.0, *) else { return nil }
        let glass = NSGlassEffectView()
        glass.cornerRadius = cornerRadius
        glass.style = .regular
        return glass
    }

    /// Groups nearby glass so the system can blend them into one another
    /// instead of stacking separate panes of it.
    static func container(spacing: CGFloat, content: NSView) -> NSView {
        guard #available(macOS 26.0, *) else { return content }
        let container = NSGlassEffectContainerView()
        container.spacing = spacing
        container.contentView = content
        return container
    }

    /// Set a glass view's corner radius, if it is one.
    static func setCornerRadius(_ view: NSView, _ radius: CGFloat) {
        guard #available(macOS 26.0, *), let glass = view as? NSGlassEffectView else { return }
        glass.cornerRadius = radius
    }

    /// Tint a glass view, if it is one.
    static func tint(_ view: NSView, _ color: NSColor?) {
        guard #available(macOS 26.0, *), let glass = view as? NSGlassEffectView else { return }
        glass.tintColor = color
    }
}
