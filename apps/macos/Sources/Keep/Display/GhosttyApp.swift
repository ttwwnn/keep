import AppKit
import Foundation
import GhosttyKit

/// The libghostty runtime.
///
/// libghostty owns terminal emulation, font handling and Metal rendering. This
/// app supplies only the platform shell: a window, a view to draw into, and
/// the handful of callbacks the runtime needs to reach macOS.
final class GhosttyApp {
    static let shared = GhosttyApp()

    private(set) var app: ghostty_app_t?
    private(set) var failure: String?

    /// Which view each surface renders into, so a runtime action addressed to
    /// a surface can reach its view. Weak: the view's deinit unregisters and
    /// frees the surface, so a stale pointer is never dereferenced.
    private var surfaceViews: [ghostty_surface_t: Weak<TerminalSurfaceView>] = [:]

    func register(surface: ghostty_surface_t, view: TerminalSurfaceView) {
        surfaceViews[surface] = Weak(view)
    }

    func unregister(surface: ghostty_surface_t) {
        surfaceViews[surface] = nil
    }

    func view(for surface: ghostty_surface_t) -> TerminalSurfaceView? {
        surfaceViews[surface]?.value
    }

    /// Posted when the resolved terminal background, opacity, or blur changes.
    static let backgroundDidChange = Notification.Name("keep.terminalBackgroundDidChange")

    /// The colour libghostty fills a surface with, so the strip above the
    /// terminal can match it instead of showing the window's own grey.
    ///
    /// Nil until libghostty says. The config finalized at startup cannot
    /// answer this: a `theme = dark:...,light:...` is still unresolved there
    /// and reading it yields the light colour in a dark window. The resolved
    /// value arrives as an action instead, which is also how a theme change
    /// mid-session reaches us.
    private(set) var terminalBackground: NSColor?
    private(set) var terminalBackgroundOpacity: Double = 1
    private(set) var terminalBackgroundBlur: Int16 = 0

    /// The font the terminal itself renders with.
    ///
    /// Anywhere the app shows terminal output outside a surface — the picker's
    /// preview, search results — has to use it, or the glyphs a Nerd Font
    /// supplies come out as boxes. Read from the resolved config rather than
    /// named here, so changing the terminal's font changes these too.
    fileprivate(set) var terminalFontFamily: String?
    fileprivate(set) var terminalFontSize: Double = 13

    fileprivate func adopt(background: NSColor, opacity: Double, blur: Int16) {
        let opacity = min(max(opacity, 0), 1)
        let backgroundChanged = terminalBackground?.isEqual(background) != true
        guard backgroundChanged
            || opacity != terminalBackgroundOpacity
            || blur != terminalBackgroundBlur
        else { return }

        terminalBackground = background
        terminalBackgroundOpacity = opacity
        terminalBackgroundBlur = blur
        NotificationCenter.default.post(name: Self.backgroundDidChange, object: nil)
    }

    /// Read `font-family` and `font-size` from the terminal's own config.
    ///
    /// Not from `ghostty_config_get`: it answers for typed scalars like the
    /// background colour, but font-family is a repeatable string and comes
    /// back empty, and font-size is not the width this call expects. Reading
    /// the file the terminal reads is less clever and actually works.
    /// Returns rather than assigns: this is called from `init`, and touching
    /// `shared` there re-enters the singleton's own initializer.
    fileprivate static func fontSettings() -> (family: String?, size: Double) {
        let home = NSHomeDirectory()
        // XDG first, then the macOS location; the later one wins if both set
        // it, matching how the terminal resolves them.
        let paths = [
            (home as NSString).appendingPathComponent(".config/ghostty/config"),
            (home as NSString)
                .appendingPathComponent("Library/Application Support/com.mitchellh.ghostty/config"),
        ]
        var family: String?
        var size: Double?
        // The first file that exists wins, whole. The terminal resolves one
        // config location — XDG for preference — rather than merging them,
        // and merging here read a stale macOS-location file over the one the
        // terminal was actually using.
        for path in paths {
            guard let text = try? String(contentsOfFile: path, encoding: .utf8) else { continue }
            for line in text.split(separator: "\n") {
                let trimmed = line.trimmingCharacters(in: .whitespaces)
                guard !trimmed.hasPrefix("#"), let equals = trimmed.firstIndex(of: "=")
                else { continue }
                let key = trimmed[..<equals].trimmingCharacters(in: .whitespaces)
                let value = trimmed[trimmed.index(after: equals)...]
                    .trimmingCharacters(in: .whitespaces)
                    .trimmingCharacters(in: CharacterSet(charactersIn: "\"'"))
                switch key {
                case "font-family" where !value.isEmpty: family = value
                case "font-size": size = Double(value)
                default: break
                }
            }
            if family != nil || size != nil { break }
        }
        return (family, size ?? 13)
    }

    /// The font to show terminal text in outside a surface, at `size` points.
    /// Falls back to the system's monospaced face when the configured family
    /// is not installed — a wrong-looking preview beats an empty one.
    func terminalFont(size: Double? = nil) -> NSFont {
        let points = size ?? terminalFontSize
        let base = terminalFontFamily.flatMap { NSFont(name: $0, size: points) }
            ?? .monospacedSystemFont(ofSize: points, weight: .regular)

        // Terminal output is full of glyphs no ordinary face has — the icons
        // a Nerd Font puts in the private use area, which is why previews
        // came out as boxes. Cascading to one covers them whatever the base
        // face turns out to be, including when the configured family cannot
        // be read at all.
        guard let fallback = Self.nerdFontFamily else { return base }
        let descriptor = base.fontDescriptor.addingAttributes([
            .cascadeList: [NSFontDescriptor(fontAttributes: [.family: fallback])]
        ])
        return NSFont(descriptor: descriptor, size: points) ?? base
    }

    /// Any installed Nerd Font, found once.
    private static let nerdFontFamily: String? = NSFontManager.shared
        .availableFontFamilies
        .first { $0.localizedCaseInsensitiveContains("nerd font") }

    /// Read the background out of a config libghostty handed back, which —
    /// unlike one we build ourselves — has `theme = dark:...,light:...`
    /// resolved for the scheme in effect.
    ///
    /// Called straight from the action callback on purpose: the config is
    /// borrowed for the length of that call, and reading it a hop later on
    /// the main queue yields a zeroed struct, which reads as black.
    fileprivate static func background(of config: ghostty_config_t?) -> NSColor? {
        guard let config else { return nil }
        var color = ghostty_config_color_s()
        let key = "background"
        let found = key.withCString {
            ghostty_config_get(config, &color, $0, UInt(key.utf8.count))
        }
        guard found else { return nil }
        return NSColor(color)
    }

    fileprivate static func backgroundOpacity(of config: ghostty_config_t?) -> Double? {
        guard let config else { return nil }
        var opacity: Double = 1
        let key = "background-opacity"
        let found = key.withCString {
            ghostty_config_get(config, &opacity, $0, UInt(key.utf8.count))
        }
        return found ? opacity : nil
    }

    fileprivate static func backgroundBlur(of config: ghostty_config_t?) -> Int16? {
        guard let config else { return nil }
        var blur: Int16 = 0
        let key = "background-blur"
        let found = key.withCString {
            ghostty_config_get(config, &blur, $0, UInt(key.utf8.count))
        }
        return found ? blur : nil
    }

    private init() {
        // libghostty wants the process argv before anything else.
        var argv: [UnsafeMutablePointer<CChar>?] = CommandLine.unsafeArgv[0].map { [$0] } ?? []
        if ghostty_init(UInt(argv.count), &argv) != 0 {
            failure = "ghostty_init failed"
            return
        }

        guard let config = Self.makeConfig() else {
            failure = "ghostty_config_new failed"
            return
        }
        // These values do not depend on light/dark theme resolution, so make
        // them available before the first window is constructed. The resolved
        // config-change action below will keep all three appearance values in
        // sync after startup and reloads.
        terminalBackgroundOpacity = Self.backgroundOpacity(of: config) ?? 1
        terminalBackgroundBlur = Self.backgroundBlur(of: config) ?? 0
        let font = Self.fontSettings()
        terminalFontFamily = font.family
        terminalFontSize = font.size
        Trace.log("config", "terminal font: \(font.family ?? "system") @\(font.size)pt")

        var runtime = ghostty_runtime_config_s()
        runtime.userdata = nil

        // The runtime asks to be ticked from the main thread.
        runtime.wakeup_cb = { _ in
            DispatchQueue.main.async {
                if let app = GhosttyApp.shared.app { ghostty_app_tick(app) }
            }
        }

        // Actions are requests to the platform shell (open a window, set a
        // title, ...). Returning false means "not handled", which is a safe
        // default for everything this app does not implement yet. The ones
        // handled here settle the terminal's background, which the titlebar
        // matches so chrome and content read as one surface.
        runtime.action_cb = { _, target, action in
            switch action.tag {
            case GHOSTTY_ACTION_RENDER:
                // The runtime asking for a frame is the signal the display
                // link only ever approximated: draw this surface, now, and
                // nothing else. Actions can arrive off the main thread; the
                // registry lookup on main guards against a surface freed in
                // between.
                guard target.tag == GHOSTTY_TARGET_SURFACE,
                    let surface = target.target.surface else { return false }
                DispatchQueue.main.async {
                    GhosttyApp.shared.view(for: surface)?.runtimeRequestedDraw()
                }
                return true
            case GHOSTTY_ACTION_PWD:
                // Where a new tab or a new pane should start: whatever the
                // shell you are in last said it was in.
                guard target.tag == GHOSTTY_TARGET_SURFACE,
                    let surface = target.target.surface,
                    let raw = action.action.pwd.pwd
                else { return false }
                let pwd = String(cString: raw)
                DispatchQueue.main.async {
                    GhosttyApp.shared.view(for: surface)?.noteDirectory(pwd)
                }
                return true

            case GHOSTTY_ACTION_GOTO_SPLIT:
                // The keybinds for this are the person's own, in their
                // ghostty config — `super+alt+h=goto_split:left` and the rest
                // of vim's four. libghostty reads them, resolves the chord and
                // hands the direction over; every one of them used to arrive
                // here, fall through to the default arm, and be traced as an
                // action nobody implemented — which from the keyboard is a
                // shortcut that does nothing at all.
                guard target.tag == GHOSTTY_TARGET_SURFACE,
                    let surface = target.target.surface
                else { return false }
                let towards: TabHostView.Direction?
                switch action.action.goto_split {
                case GHOSTTY_GOTO_SPLIT_LEFT: towards = .left
                case GHOSTTY_GOTO_SPLIT_RIGHT: towards = .right
                case GHOSTTY_GOTO_SPLIT_UP: towards = .up
                case GHOSTTY_GOTO_SPLIT_DOWN: towards = .down
                // Previous and next walk the tree's order rather than the
                // screen's, and nothing in this app asks for them yet.
                default: towards = nil
                }
                guard let towards else { return false }
                DispatchQueue.main.async {
                    GhosttyApp.shared.view(for: surface)?.tabHost?.moveFocus(towards)
                }
                return true

            case GHOSTTY_ACTION_RESIZE_SPLIT:
                // The other half of the person's own vim keybinds, alongside
                // goto_split: `super+ctrl+j=resize_split:down,20`. This is the
                // action that showed up in the trace as tag 33, arriving over
                // and over and being answered by nobody.
                guard target.tag == GHOSTTY_TARGET_SURFACE,
                    let surface = target.target.surface
                else { return false }
                let resize = action.action.resize_split
                let way: TabHostView.Direction?
                switch resize.direction {
                case GHOSTTY_RESIZE_SPLIT_LEFT: way = .left
                case GHOSTTY_RESIZE_SPLIT_RIGHT: way = .right
                case GHOSTTY_RESIZE_SPLIT_UP: way = .up
                case GHOSTTY_RESIZE_SPLIT_DOWN: way = .down
                default: way = nil
                }
                guard let way else { return false }
                let amount = CGFloat(resize.amount)
                DispatchQueue.main.async {
                    GhosttyApp.shared.view(for: surface)?.tabHost?.resizeSplit(way, by: amount)
                }
                return true

            case GHOSTTY_ACTION_SCROLLBAR:
                // Not handled as a scrollbar — this app draws none — but as
                // the one thing the runtime does say when a terminal's
                // contents move. It is what puts an idle surface back at full
                // rate the instant output arrives.
                guard target.tag == GHOSTTY_TARGET_SURFACE,
                    let surface = target.target.surface else { return false }
                DispatchQueue.main.async {
                    GhosttyApp.shared.view(for: surface)?.noteActivity()
                }
                return false

            case GHOSTTY_ACTION_CONFIG_CHANGE:
                let config = action.action.config_change.config
                guard
                    let color = GhosttyApp.background(of: config),
                    let opacity = GhosttyApp.backgroundOpacity(of: config),
                    let blur = GhosttyApp.backgroundBlur(of: config)
                else { return true }
                DispatchQueue.main.async {
                    GhosttyApp.shared.adopt(background: color, opacity: opacity, blur: blur)
                    let font = GhosttyApp.fontSettings()
                    GhosttyApp.shared.terminalFontFamily = font.family
                    GhosttyApp.shared.terminalFontSize = font.size
                }
                return true
            case GHOSTTY_ACTION_RELOAD_CONFIG:
                DispatchQueue.main.async { GhosttyApp.shared.reloadConfig() }
                return true
            case GHOSTTY_ACTION_COLOR_CHANGE:
                let change = action.action.color_change
                guard change.kind == GHOSTTY_ACTION_COLOR_KIND_BACKGROUND else { return false }
                let color = NSColor(change)
                DispatchQueue.main.async {
                    let app = GhosttyApp.shared
                    app.adopt(
                        background: color,
                        opacity: app.terminalBackgroundOpacity,
                        blur: app.terminalBackgroundBlur
                    )
                }
                return true
            default:
                Trace.log("action", "tag \(action.tag.rawValue) target \(target.tag.rawValue)")
                return false
            }
        }

        // Reading is asked for by a surface and answered by one: the pointer
        // is the view, put there as the surface's userdata when it was made.
        runtime.read_clipboard_cb = { userdata, location, state in
            guard let userdata else { return false }
            // macOS has no selection clipboard, and `supports_selection_
            // clipboard` stays false so nothing should ask for one.
            guard location == GHOSTTY_CLIPBOARD_STANDARD else { return false }
            let view = Unmanaged<TerminalSurfaceView>
                .fromOpaque(userdata).takeUnretainedValue()
            let text = NSPasteboard.general.string(forType: .string) ?? ""
            // Not confirmed: let the terminal decide whether this text is the
            // kind that runs itself, and ask below if it is.
            view.completeClipboardRequest(text, state: state, confirmed: false)
            return true
        }
        runtime.confirm_read_clipboard_cb = { userdata, string, state, _ in
            guard let userdata, let string else { return }
            let view = Unmanaged<TerminalSurfaceView>
                .fromOpaque(userdata).takeUnretainedValue()
            let text = String(cString: string)
            DispatchQueue.main.async { view.confirmPaste(text, state: state) }
        }
        runtime.write_clipboard_cb = { _, location, contents, count, _ in
            guard count > 0, let contents else { return }
            guard location == GHOSTTY_CLIPBOARD_STANDARD else { return }
            // Find something to write before touching the pasteboard.
            // Clearing first and then discovering there was nothing to put
            // back destroys whatever the person had copied — which is what
            // copy-on-select did every time a selection came back empty.
            var text: String?
            for i in 0..<Int(count) {
                guard let data = contents[i].data else { continue }
                let candidate = String(cString: data)
                if !candidate.isEmpty { text = candidate; break }
            }
            guard let text else { return }
            let board = NSPasteboard.general
            board.clearContents()
            board.setString(text, forType: .string)
        }
        runtime.close_surface_cb = { _, _ in }

        guard let app = ghostty_app_new(&runtime, config) else {
            failure = "ghostty_app_new failed"
            return
        }
        self.app = app

        // A `theme = dark:...,light:...` config is resolved per color scheme,
        // and libghostty assumes light until told otherwise. Setting it on the
        // surface alone is not enough: the choice is made app-wide.
        syncColorScheme()
        appearanceObserver = NSApp.observe(\.effectiveAppearance) { [weak self] _, _ in
            self?.syncColorScheme()
        }
    }

    private var appearanceObserver: NSKeyValueObservation?

    /// The keep client a surface runs.
    static var clientBinary: String {
        if let override = ProcessInfo.processInfo.environment["KEEP_BIN"] { return override }
        // Resources, not MacOS: the app executable is "Keep" and macOS
        // filesystems are case-insensitive, so a sibling named "keep" would
        // overwrite the app itself.
        let bundled = Bundle.main.bundleURL
            .appendingPathComponent("Contents/Resources/keep").path
        return FileManager.default.isExecutableFile(atPath: bundled) ? bundled : "keep"
    }

    /// A config built the way this app always builds one.
    ///
    /// Made fresh rather than reused: libghostty asks the platform shell to
    /// reload — it does not reload itself — and handing it a new config is
    /// how the theme gets resolved for the scheme now in effect.
    private static func makeConfig() -> ghostty_config_t? {
        guard let config = ghostty_config_new() else { return nil }
        // Honour the user's own Ghostty config: same fonts and theme they
        // already tuned, with no separate settings file to maintain.
        ghostty_config_load_default_files(config)

        // Every surface runs the keep client. The per-surface `command` field
        // is ignored by this build of libghostty, so the command is fixed here
        // at app level and each surface names its target through environment
        // variables instead, which are honoured.
        if let overridePath = writeCommandOverride() {
            overridePath.withCString { ghostty_config_load_file(config, $0) }
        }

        ghostty_config_finalize(config)
        return config
    }

    /// Rebuild the config and hand it back, which is what the reload action
    /// asks for — libghostty does not reload itself. The colours are not
    /// read here: this config still has the theme unresolved. libghostty
    /// applies the scheme and reports the settled result as a config change,
    /// which is where they come from.
    fileprivate func reloadConfig() {
        guard let app, let config = Self.makeConfig() else { return }
        ghostty_app_update_config(app, config)
    }

    /// A tiny Ghostty config that points `command` at our client.
    private static func writeCommandOverride() -> String? {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("keep-app", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let file = dir.appendingPathComponent("command.conf")
        // `alt+backspace` is spelled out because the encoder does not send it.
        //
        // Measured, not guessed: with the kitty keyboard protocol in force —
        // proven in the same capture, by asking the terminal for its flags
        // and catching the reply — a plain backspace writes 0x7f and an
        // option-backspace writes nothing at all. Every field of the event is
        // by then identical to the one libghostty's own test says must
        // produce `CSI 127;3u`, and unbinding the chord changes nothing, so
        // this is not a binding eating it. Naming the bytes is the one thing
        // that does get them sent, and a word-delete that does nothing is the
        // difference between this being a terminal somebody can work in and
        // not.
        //
        // Keep's native toolbar already provides the outer breathing room.
        // Use a tighter terminal-only inset than the user's standalone
        // Ghostty window so the first prompt sits closer to the chrome.
        let body = """
            command = \(clientBinary)
            window-padding-y = 0
            keybind = alt+backspace=text:\\x1b\\x7f

            """
        do {
            try body.write(to: file, atomically: true, encoding: .utf8)
            return file.path
        } catch {
            return nil
        }
    }

    /// Follow the system between light and dark.
    func syncColorScheme() {
        guard let app else { return }
        let dark = NSApp.effectiveAppearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua
        ghostty_app_set_color_scheme(app, dark ? GHOSTTY_COLOR_SCHEME_DARK : GHOSTTY_COLOR_SCHEME_LIGHT)
    }

    func tick() {
        guard let app else { return }
        ghostty_app_tick(app)
    }
}

extension NSColor {
    /// libghostty hands colours over as three bytes, in sRGB.
    fileprivate convenience init(_ color: ghostty_config_color_s) {
        self.init(
            srgbRed: CGFloat(color.r) / 255,
            green: CGFloat(color.g) / 255,
            blue: CGFloat(color.b) / 255,
            alpha: 1
        )
    }

    fileprivate convenience init(_ change: ghostty_action_color_change_s) {
        self.init(
            srgbRed: CGFloat(change.r) / 255,
            green: CGFloat(change.g) / 255,
            blue: CGFloat(change.b) / 255,
            alpha: 1
        )
    }
}


/// A weak reference that can live in a dictionary value.
private final class Weak<T: AnyObject> {
    weak var value: T?
    init(_ value: T) { self.value = value }
}
