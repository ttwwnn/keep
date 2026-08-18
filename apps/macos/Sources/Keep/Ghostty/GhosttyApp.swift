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

    private init() {
        // libghostty wants the process argv before anything else.
        var argv: [UnsafeMutablePointer<CChar>?] = CommandLine.unsafeArgv[0].map { [$0] } ?? []
        if ghostty_init(UInt(argv.count), &argv) != 0 {
            failure = "ghostty_init failed"
            return
        }

        guard let config = ghostty_config_new() else {
            failure = "ghostty_config_new failed"
            return
        }
        // Honour the user's own Ghostty config: same fonts and theme they
        // already tuned, with no separate settings file to maintain.
        ghostty_config_load_default_files(config)

        // Every surface runs the keep client. The per-surface `command` field
        // is ignored by this build of libghostty, so the command is fixed here
        // at app level and each surface names its target through environment
        // variables instead, which are honoured.
        if let overridePath = Self.writeCommandOverride() {
            overridePath.withCString { ghostty_config_load_file(config, $0) }
        }

        ghostty_config_finalize(config)

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
        // default for everything this app does not implement yet.
        runtime.action_cb = { _, _, _ in false }

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

    /// A tiny Ghostty config that points `command` at our client.
    private static func writeCommandOverride() -> String? {
        let dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("keep-app", isDirectory: true)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let file = dir.appendingPathComponent("command.conf")
        let body = "command = \(clientBinary)\n"
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
