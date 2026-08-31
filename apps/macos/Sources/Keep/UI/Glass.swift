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

    /// Where a panel's material is allowed to look for what to refract.
    enum Backdrop {
        /// The window server's, which is what Liquid Glass samples and is
        /// not ours to redirect. Under an opaque window that is the window;
        /// under a see-through one it is the desktop, wallpaper and all.
        case screen
        /// This window's own content, and nothing past it. The pre-glass
        /// material can be told this; `NSGlassEffectView` cannot, which is
        /// the whole reason the distinction is spelled out here.
        case window
    }

    /// A view that hosts `content` on glass, or on a plain rounded material
    /// where glass does not exist — or where glass would reach past the
    /// window for something to refract and come back with the wallpaper.
    static func panel(
        _ content: NSView, cornerRadius: CGFloat, sampling: Backdrop = .screen
    ) -> NSView {
        if #available(macOS 26.0, *), sampling == .screen {
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

/// A selection lozenge that is glass on a dark ground and a flat wash on a
/// light one.
///
/// Tahoe's glass draws its own depth — an edge and a shadow — and over a
/// pale surface that depth reads as a pill floating off the list with a drop
/// shadow under it. The dark appearance keeps the glass; the light one gets
/// a plain rounded fill, which is what selection looks like everywhere else
/// on a light macOS.
final class AdaptiveLozengeView: NSView {
    private let glass: NSView?
    private var radius: CGFloat
    private var lightFill: NSColor

    init(cornerRadius: CGFloat, lightFill: NSColor) {
        self.radius = cornerRadius
        self.lightFill = lightFill
        self.glass = Glass.lozenge(cornerRadius: cornerRadius)
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerCurve = .continuous
        if let glass {
            glass.autoresizingMask = [.width, .height]
            addSubview(glass)
        }
        apply()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    func set(cornerRadius: CGFloat, tint: NSColor?, lightFill: NSColor) {
        radius = cornerRadius
        self.lightFill = lightFill
        if let glass {
            Glass.setCornerRadius(glass, cornerRadius)
            Glass.tint(glass, tint)
        }
        apply()
    }

    override func layout() {
        super.layout()
        glass?.frame = bounds
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        apply()
    }

    /// Again on arrival: `init` runs before the view knows its window, and
    /// `effectiveAppearance` answers for the system then, not for the window
    /// this will live in. Decided too early, the first paint wore the wrong
    /// appearance until something — a manual theme flip, say — forced every
    /// view to re-decide.
    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        apply()
    }

    private func apply() {
        let dark = effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        layer?.cornerRadius = radius
        if dark && glass != nil {
            glass?.isHidden = false
            layer?.backgroundColor = nil
        } else {
            glass?.isHidden = true
            layer?.backgroundColor = lightFill.cgColor
        }
    }
}
