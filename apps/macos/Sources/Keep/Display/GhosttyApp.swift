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
        runtime.action_cb = { _, _, action in
            switch action.tag {
            case GHOSTTY_ACTION_CONFIG_CHANGE:
                let config = action.action.config_change.config
                guard
                    let color = GhosttyApp.background(of: config),
                    let opacity = GhosttyApp.backgroundOpacity(of: config),
                    let blur = GhosttyApp.backgroundBlur(of: config)
                else { return true }
                DispatchQueue.main.async {
                    GhosttyApp.shared.adopt(background: color, opacity: opacity, blur: blur)
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
                return false
            }
        }

        // TODO: paste. Completing a read needs the surface handle, which
        // arrives through surface userdata; wire that up with the surface
        // registry rather than guessing here.
        runtime.read_clipboard_cb = { _, _, _ in false }
        runtime.confirm_read_clipboard_cb = { _, _, _, _ in }
        runtime.write_clipboard_cb = { _, _, contents, count, _ in
            guard count > 0, let contents else { return }
            let board = NSPasteboard.general
            board.clearContents()
            if let data = contents.pointee.data {
                board.setString(String(cString: data), forType: .string)
            }
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
        // Keep's native toolbar already provides the outer breathing room.
        // Use a tighter terminal-only inset than the user's standalone
        // Ghostty window so the first prompt sits closer to the chrome.
        let body = """
            command = \(clientBinary)
            window-padding-y = 0

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
