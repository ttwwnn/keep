import AppKit
import GhosttyKit
import SwiftUI

/// An `NSView` that libghostty renders a terminal into.
///
/// Drawing does not happen in SwiftUI: a character grid at frame rate needs a
/// real view with a Metal layer, which is exactly how Ghostty itself is built.
/// SwiftUI composes this through `NSViewRepresentable`.
final class TerminalSurfaceView: NSView {
    private var surface: ghostty_surface_t?
    private var displayLink: CVDisplayLink?
    private let command: String

    init(command: String) {
        self.command = command
        super.init(frame: .zero)
        wantsLayer = true
        // Without this the view keeps its own backing store and libghostty
        // draws into something the window never composites.
        layerContentsRedrawPolicy = .duringViewResize
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    deinit {
        if let link = displayLink { CVDisplayLinkStop(link) }
        if let surface { ghostty_surface_free(surface) }
    }

    override var acceptsFirstResponder: Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard window != nil, surface == nil else { return }
        createSurface()
        window?.makeFirstResponder(self)
    }

    private func createSurface() {
        guard let app = GhosttyApp.shared.app else { return }

        var config = ghostty_surface_config_new()
        config.platform_tag = GHOSTTY_PLATFORM_MACOS
        config.platform = ghostty_platform_u(
            macos: ghostty_platform_macos_s(nsview: Unmanaged.passUnretained(self).toOpaque())
        )
        config.scale_factor = Double(window?.backingScaleFactor ?? 2.0)

        command.withCString { cmd in
            config.command = cmd
            surface = ghostty_surface_new(app, &config)
        }

        guard let surface else { return }
        ghostty_surface_set_content_scale(surface, config.scale_factor, config.scale_factor)
        applyColorScheme()
        updateSize()
        startDisplayLink()
    }

    private func startDisplayLink() {
        var link: CVDisplayLink?
        CVDisplayLinkCreateWithActiveCGDisplays(&link)
        guard let link else { return }
        CVDisplayLinkSetOutputHandler(link) { [weak self] _, _, _, _, _ in
            DispatchQueue.main.async {
                guard let self, let surface = self.surface else { return }
                ghostty_surface_draw(surface)
            }
            return kCVReturnSuccess
        }
        CVDisplayLinkStart(link)
        displayLink = link
    }

    private func updateSize() {
        guard let surface else { return }
        let scale = window?.backingScaleFactor ?? 2.0
        ghostty_surface_set_size(
            surface,
            UInt32(max(1, bounds.width * scale)),
            UInt32(max(1, bounds.height * scale))
        )
    }

    /// Tell libghostty whether we are dark or light.
    ///
    /// Without this it assumes light, so a config with a `dark:`/`light:`
    /// theme pair renders the wrong half against a dark app.
    private func applyColorScheme() {
        guard let surface else { return }
        let dark = effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        ghostty_surface_set_color_scheme(
            surface,
            dark ? GHOSTTY_COLOR_SCHEME_DARK : GHOSTTY_COLOR_SCHEME_LIGHT
        )
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyColorScheme()
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        updateSize()
    }

    // MARK: - input

    override func keyDown(with event: NSEvent) {
        send(event, action: GHOSTTY_ACTION_PRESS)
    }

    override func keyUp(with event: NSEvent) {
        send(event, action: GHOSTTY_ACTION_RELEASE)
    }

    private func send(_ event: NSEvent, action: ghostty_input_action_e) {
        guard let surface else { return }
        let text = event.characters ?? ""
        var key = ghostty_input_key_s()
        key.action = action
        key.mods = Self.mods(from: event.modifierFlags)
        key.consumed_mods = ghostty_input_mods_e(0)
        key.keycode = UInt32(event.keyCode)
        key.unshifted_codepoint = event.charactersIgnoringModifiers?.unicodeScalars.first?.value ?? 0
        key.composing = false

        text.withCString { ptr in
            key.text = ptr
            _ = ghostty_surface_key(surface, key)
        }
    }

    private static func mods(from flags: NSEvent.ModifierFlags) -> ghostty_input_mods_e {
        var raw: UInt32 = 0
        if flags.contains(.shift) { raw |= GHOSTTY_MODS_SHIFT.rawValue }
        if flags.contains(.control) { raw |= GHOSTTY_MODS_CTRL.rawValue }
        if flags.contains(.option) { raw |= GHOSTTY_MODS_ALT.rawValue }
        if flags.contains(.command) { raw |= GHOSTTY_MODS_SUPER.rawValue }
        return ghostty_input_mods_e(raw)
    }

    override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        mouseButton(event, action: GHOSTTY_MOUSE_PRESS, button: GHOSTTY_MOUSE_LEFT)
    }

    override func mouseUp(with event: NSEvent) {
        mouseButton(event, action: GHOSTTY_MOUSE_RELEASE, button: GHOSTTY_MOUSE_LEFT)
    }

    private func mouseButton(
        _ event: NSEvent,
        action: ghostty_input_mouse_state_e,
        button: ghostty_input_mouse_button_e
    ) {
        guard let surface else { return }
        _ = ghostty_surface_mouse_button(surface, action, button, Self.mods(from: event.modifierFlags))
    }

    override func mouseMoved(with event: NSEvent) {
        guard let surface else { return }
        let p = convert(event.locationInWindow, from: nil)
        ghostty_surface_mouse_pos(surface, p.x, bounds.height - p.y, Self.mods(from: event.modifierFlags))
    }
}

/// Bridges the AppKit surface into SwiftUI.
struct TerminalSurface: NSViewRepresentable {
    let command: String

    func makeNSView(context: Context) -> TerminalSurfaceView {
        TerminalSurfaceView(command: command)
    }

    func updateNSView(_ nsView: TerminalSurfaceView, context: Context) {}
}
