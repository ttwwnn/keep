import AppKit
import GhosttyKit
import QuartzCore

/// An `NSView` that libghostty renders a terminal into.
///
/// Drawing does not happen in SwiftUI: a character grid at frame rate needs a
/// real view with a Metal layer, which is exactly how Ghostty itself is built.
///
/// Surfaces are expensive — each owns a renderer and a `keep` client process —
/// so they are created once (see SurfacePool) and then mounted forever inside
/// the one window, mostly hidden. Visibility, not existence, is what changes
/// on a switch, and everything here is built around that:
///
/// - A hidden surface draws zero frames and defers PTY resizes; its Metal
///   layer keeps the last presented frame, so revealing it never shows a hole
///   even before the fresh draw lands.
/// - Drawing is on-demand where the runtime allows: the first
///   `GHOSTTY_ACTION_RENDER` a surface receives proves this build asks for
///   draws, and the free-running display link retires for good. Until that
///   proof, the link runs while visible — the conservative fallback.
final class TerminalSurfaceView: NSView {
    private var surface: ghostty_surface_t?
    private var displayLink: CVDisplayLink?
    private var occlusionObserver: NSObjectProtocol?
    private var drawCount = 0
    private var traceTimer: Timer?
    private let workspace: String
    let tab: UInt32

    /// Set once the runtime sends this surface a render request. From then on
    /// draws happen only when asked for, and the display link stays off.
    private var renderDriven = false

    /// The framebuffer size last actually sent, so layout passes that change
    /// nothing send nothing (they used to send everything twice).
    private var sentSize: CGSize?

    /// A resize that arrived while hidden. Only the visible tab's sessions
    /// re-wrap live during a window resize; the rest catch up in one call
    /// when revealed.
    private var pendingSize: CGSize?

    /// The active pane reports focus upward; the model owns the fact.
    var onFocusGained: (() -> Void)?

    init(workspace: String, tab: UInt32) {
        self.workspace = workspace
        self.tab = tab
        super.init(frame: .zero)
        wantsLayer = true
        autoresizingMask = [.width, .height]
        // Without this the view keeps its own backing store and libghostty
        // draws into something the window never composites.
        layerContentsRedrawPolicy = .duringViewResize
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    deinit {
        if let observer = occlusionObserver {
            NotificationCenter.default.removeObserver(observer)
        }
        traceTimer?.invalidate()
        if let link = displayLink { CVDisplayLinkStop(link) }
        if let surface {
            GhosttyApp.shared.unregister(surface: surface)
            ghostty_surface_free(surface)
        }
    }

    override var acceptsFirstResponder: Bool { true }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        observeOcclusion()
        guard window != nil, surface == nil else { return }
        createSurface()
    }

    private func createSurface() {
        guard let app = GhosttyApp.shared.app else { return }

        var config = ghostty_surface_config_new()
        config.platform_tag = GHOSTTY_PLATFORM_MACOS
        config.platform = ghostty_platform_u(
            macos: ghostty_platform_macos_s(nsview: Unmanaged.passUnretained(self).toOpaque())
        )
        config.scale_factor = Double(window?.backingScaleFactor ?? 2.0)

        // Which workspace and tab this surface should attach to travels
        // through the working directory: of the per-surface fields, that is
        // the only one this build of libghostty honours. `command`,
        // `env_vars` and `initial_input` are accepted by the API and then
        // ignored, which is why the client is fixed app-wide (GhosttyApp)
        // and the target is a file the client reads and deletes.
        guard let dir = Self.makeTargetDirectory(workspace: workspace, tab: tab) else { return }
        dir.withCString { wd in
            config.working_directory = wd
            surface = ghostty_surface_new(app, &config)
        }

        guard let surface else { return }
        GhosttyApp.shared.register(surface: surface, view: self)
        ghostty_surface_set_content_scale(surface, config.scale_factor, config.scale_factor)
        layer?.contentsScale = window?.backingScaleFactor ?? 2.0
        applyColorScheme()
        updateSize()
        startDisplayLink()
    }

    /// A private directory holding this surface's attach target.
    private static func makeTargetDirectory(workspace: String, tab: UInt32) -> String? {
        let base = FileManager.default.temporaryDirectory
            .appendingPathComponent("keep-attach", isDirectory: true)
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        do {
            try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
            try "\(workspace)\n\(tab)\n".write(
                to: base.appendingPathComponent(".keep-attach"),
                atomically: true,
                encoding: .utf8
            )
            return base.path
        } catch {
            return nil
        }
    }

    // MARK: - drawing

    /// The runtime asked for a frame (GHOSTTY_ACTION_RENDER). The first one
    /// is also the proof that this build drives its own drawing, which
    /// retires the free-running display link permanently.
    func runtimeRequestedDraw() {
        guard let surface else { return }
        if !renderDriven {
            renderDriven = true
            Trace.log("render", "\(workspace)/\(tab) render-driven; display link retired")
        }
        if let link = displayLink, CVDisplayLinkIsRunning(link) {
            CVDisplayLinkStop(link)
        }
        ghostty_surface_draw(surface)
        drawCount += 1
    }

    // MARK: - how often to draw

    /// Frames are cheap to ask for and expensive to make. Nothing else paints
    /// this view — the runtime never asks for a frame in this build, so the
    /// link cannot be retired — but drawing an idle terminal at the display's
    /// own rate costs a tenth of a core to show the same thing 120 times a
    /// second. So the link keeps running and the drawing follows activity:
    /// every frame while something is happening, ten a second when not, which
    /// is still prompt for output and still blinks a cursor.
    ///
    /// "Something is happening" means input from the keyboard or mouse, or the
    /// runtime reporting that the terminal's contents moved.
    private var busyUntil: CFTimeInterval = 0
    private var lastDraw: CFTimeInterval = 0
    /// How long an event keeps the surface at full rate.
    private static let busyFor: CFTimeInterval = 0.75
    /// The idle rate: ten frames a second.
    private static let idleInterval: CFTimeInterval = 0.1

    /// Draw at full rate for a moment, because something just happened.
    func noteActivity() {
        busyUntil = CACurrentMediaTime() + Self.busyFor
    }

    private func startDisplayLink() {
        var link: CVDisplayLink?
        CVDisplayLinkCreateWithActiveCGDisplays(&link)
        guard let link else { return }
        CVDisplayLinkSetOutputHandler(link) { [weak self] _, _, _, _, _ in
            DispatchQueue.main.async {
                guard let self, let surface = self.surface else { return }
                let now = CACurrentMediaTime()
                // Every frame while something is happening; a tenth of a
                // second apart when nothing is.
                guard now < self.busyUntil || now - self.lastDraw >= Self.idleInterval
                else { return }
                self.lastDraw = now
                ghostty_surface_draw(surface)
                self.drawCount += 1
            }
            return kCVReturnSuccess
        }
        displayLink = link
        syncDisplayLink()
        startTraceTimer()
    }

    /// Whether anyone can currently see this surface. With one window, being
    /// in a visible window is not enough — most mounted surfaces are hidden.
    private var isEffectivelyVisible: Bool {
        !isHiddenOrHasHiddenAncestor
            && (window?.occlusionState.contains(.visible) ?? false)
    }

    /// The passive backstop: keep the link's running state matched to
    /// visibility. The switch pipeline uses the explicit fast path below
    /// instead of waiting for notifications.
    private func syncDisplayLink() {
        guard let link = displayLink else { return }
        let shouldRun = isEffectivelyVisible && !renderDriven
        if shouldRun {
            if !CVDisplayLinkIsRunning(link) { CVDisplayLinkStart(link) }
        } else if CVDisplayLinkIsRunning(link) {
            CVDisplayLinkStop(link)
        }
    }

    /// Draw now, without waiting to be told the view is visible. The switch
    /// pipeline calls this while the view is still hidden, so its layer holds
    /// a fresh frame at final geometry before the reveal commits.
    func resumeDrawing() {
        guard let surface else { return }
        flushPendingSize()
        ghostty_surface_set_occlusion(surface, true)
        if !renderDriven, let link = displayLink, !CVDisplayLinkIsRunning(link) {
            CVDisplayLinkStart(link)
        }
        ghostty_surface_draw(surface)
        drawCount += 1
    }

    /// Stop drawing for a surface that is alive but off screen.
    func suspendDrawing() {
        if let surface { ghostty_surface_set_occlusion(surface, false) }
        guard let link = displayLink, CVDisplayLinkIsRunning(link) else { return }
        CVDisplayLinkStop(link)
    }

    /// AppKit calls these on every descendant when an ancestor's isHidden
    /// flips — the automatic half of the visibility policy. The pipeline's
    /// explicit resume/suspend still runs first; these make the invariant
    /// hold no matter who toggled what.
    override func viewDidHide() {
        super.viewDidHide()
        suspendDrawing()
    }

    override func viewDidUnhide() {
        super.viewDidUnhide()
        flushPendingSize()
        if let surface {
            ghostty_surface_set_occlusion(surface, true)
            ghostty_surface_draw(surface)
        }
        syncDisplayLink()
    }

    private func observeOcclusion() {
        if let observer = occlusionObserver {
            NotificationCenter.default.removeObserver(observer)
            occlusionObserver = nil
        }
        guard let window else {
            syncDisplayLink()
            return
        }
        occlusionObserver = NotificationCenter.default.addObserver(
            forName: NSWindow.didChangeOcclusionStateNotification,
            object: window,
            queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.syncDisplayLink() }
        }
        syncDisplayLink()
    }

    /// Report the draw rate once a second while tracing. Silence is the
    /// expected state for anything hidden.
    // MARK: - scrolling to a line

    /// Put a line of history on screen, counted back from the newest.
    ///
    /// Ghostty exposes scrolling as keybind actions rather than as calls, so
    /// this asks for the actions by the names a config would use.
    ///
    /// It asks twice. A pane opened by a search result is mounted by the same
    /// switch that asks for the scroll, and its history is still arriving over
    /// the socket — the snapshot lands the viewport back at the bottom after
    /// the first attempt. Asking again once it has settled costs nothing when
    /// the first attempt already worked, because the target is the same place.
    func scrollBack(lines: Int) {
        apply(scrollBack: lines)
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.7) { [weak self] in
            self?.apply(scrollBack: lines)
        }
    }

    private func apply(scrollBack lines: Int) {
        guard surface != nil else {
            Trace.log("scroll", "\(workspace)/\(tab) asked for \(lines) with no surface")
            return
        }
        // From the bottom, always: the oldest end of a history is where the
        // daemon's copy and this one disagree, because a terminal trims it.
        perform("scroll_to_bottom")
        // Four rows short of the line, so it arrives with what follows it
        // rather than pinned to the last row of the screen. A line already
        // near the end simply stays where the bottom is.
        let back = max(0, lines - 4)
        guard back > 0 else { return }
        perform("scroll_page_lines:-\(back)")
    }

    @discardableResult
    private func perform(_ action: String) -> Bool {
        guard let surface else { return false }
        let done = action.withCString {
            ghostty_surface_binding_action(surface, $0, UInt(action.utf8.count))
        }
        Trace.log("scroll", "\(workspace)/\(tab) \(action) done=\(done)")
        return done
    }

    private func startTraceTimer() {
        guard Trace.enabled else { return }
        traceTimer = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated {
                guard let self else { return }
                let drawn = self.drawCount
                self.drawCount = 0
                let visible = self.isEffectivelyVisible
                let running = self.displayLink.map { CVDisplayLinkIsRunning($0) } ?? false
                guard drawn > 0 || (visible && running) else { return }
                Trace.log("render", "\(self.workspace)/\(self.tab) fps=\(drawn) "
                    + "visible=\(visible) mode=\(self.renderDriven ? "on-demand" : "link")")
            }
        }
    }

    // MARK: - geometry

    /// Push the view's size to the session — deduplicated, and deferred
    /// entirely while hidden.
    private func updateSize() {
        guard let surface else { return }
        // libghostty wants the framebuffer size, so convert rather than
        // multiplying by a guessed scale.
        let backing = convertToBacking(bounds).size
        guard backing != sentSize || pendingSize != nil else { return }
        if isHiddenOrHasHiddenAncestor {
            pendingSize = backing
            return
        }
        pendingSize = nil
        sentSize = backing
        Trace.log("layout", "\(workspace)/\(tab) size \(Int(backing.width))x\(Int(backing.height))")
        ghostty_surface_set_size(
            surface,
            UInt32(max(1, backing.width)),
            UInt32(max(1, backing.height))
        )
    }

    private func flushPendingSize() {
        guard let surface, let pending = pendingSize else { return }
        pendingSize = nil
        sentSize = pending
        Trace.log("layout", "\(workspace)/\(tab) size \(Int(pending.width))x\(Int(pending.height)) (deferred)")
        ghostty_surface_set_size(
            surface,
            UInt32(max(1, pending.width)),
            UInt32(max(1, pending.height))
        )
    }

    /// Keep the layer from being rescaled by the compositor: we render at the
    /// display's density and Core Animation must know it.
    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        guard let window else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        layer?.contentsScale = window.backingScaleFactor
        CATransaction.commit()
        if let surface {
            let scale = window.backingScaleFactor
            ghostty_surface_set_content_scale(surface, scale, scale)
        }
        sentSize = nil   // scale changed: the same points are new pixels
        updateSize()
    }

    /// Tell libghostty whether we are dark or light, or a `dark:`/`light:`
    /// theme pair renders its wrong half.
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

    override func layout() {
        super.layout()
        // setFrameSize alone misses the first pass; the dedupe above makes
        // the overlap free.
        updateSize()
    }

    // MARK: - focus

    override func becomeFirstResponder() -> Bool {
        let accepted = super.becomeFirstResponder()
        if accepted {
            if let surface { ghostty_surface_set_focus(surface, true) }
            onFocusGained?()
        }
        return accepted
    }

    override func resignFirstResponder() -> Bool {
        let resigned = super.resignFirstResponder()
        if resigned, let surface { ghostty_surface_set_focus(surface, false) }
        return resigned
    }

    // MARK: - input

    /// Mouse-move reports go to the surface under the pointer, not to
    /// whichever surface last held the keyboard.
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas { removeTrackingArea(area) }
        addTrackingArea(NSTrackingArea(
            rect: .zero,
            options: [.mouseMoved, .activeInKeyWindow, .inVisibleRect],
            owner: self
        ))
    }

    override func keyDown(with event: NSEvent) {
        send(event, action: GHOSTTY_ACTION_PRESS)
    }

    override func keyUp(with event: NSEvent) {
        send(event, action: GHOSTTY_ACTION_RELEASE)
    }

    private func send(_ event: NSEvent, action: ghostty_input_action_e) {
        noteActivity()
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
        noteActivity()
        window?.makeFirstResponder(self)
        mouseButton(event, action: GHOSTTY_MOUSE_PRESS, button: GHOSTTY_MOUSE_LEFT)
    }

    override func mouseUp(with event: NSEvent) {
        noteActivity()
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

    /// Scrolling, which is also how the scrollback becomes reachable at all:
    /// the surface keeps the history, and until these events were forwarded
    /// there was simply no way to move the viewport into it.
    override func scrollWheel(with event: NSEvent) {
        noteActivity()
        guard let surface else { return }
        var x = event.scrollingDeltaX
        var y = event.scrollingDeltaY

        // `ghostty_input_scroll_mods_t` is a packed byte the header declines
        // to declare (see its comment at the typedef): bit 0 says the deltas
        // are precise — a trackpad reporting points rather than lines — and
        // bits 1-3 carry the momentum phase, which the renderer needs to tell
        // a flick from a drag.
        var mods: Int32 = 0
        if event.hasPreciseScrollingDeltas {
            mods = 1
            var momentum: ghostty_input_mouse_momentum_e
            switch event.momentumPhase {
            case .began: momentum = GHOSTTY_MOUSE_MOMENTUM_BEGAN
            case .stationary: momentum = GHOSTTY_MOUSE_MOMENTUM_STATIONARY
            case .changed: momentum = GHOSTTY_MOUSE_MOMENTUM_CHANGED
            case .ended: momentum = GHOSTTY_MOUSE_MOMENTUM_ENDED
            case .cancelled: momentum = GHOSTTY_MOUSE_MOMENTUM_CANCELLED
            case .mayBegin: momentum = GHOSTTY_MOUSE_MOMENTUM_MAY_BEGIN
            default: momentum = GHOSTTY_MOUSE_MOMENTUM_NONE
            }
            mods |= Int32(momentum.rawValue) << 1
        } else {
            // A wheel reports lines; the renderer works in points.
            x *= 10
            y *= 10
        }

        ghostty_surface_mouse_scroll(surface, x, y, mods)
    }
}
