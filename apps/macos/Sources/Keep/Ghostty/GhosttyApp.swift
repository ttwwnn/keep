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
    }

    func tick() {
        guard let app else { return }
        ghostty_app_tick(app)
    }
}
