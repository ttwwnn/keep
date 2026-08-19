import Foundation

/// Polls the daemon every two seconds and hands the listing to the session.
///
/// The socket work happens off the main thread: `Daemon.list()` is blocking
/// I/O, and a daemon busy in its parser must never become a UI hitch. Only
/// the reconciliation runs on the main actor.
@MainActor
final class DaemonPoller {
    private let session: Session
    private var timer: Timer?
    private var inFlight = false

    init(session: Session) {
        self.session = session
    }

    func start() {
        timer = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.poll() }
        }
    }

    private func poll() {
        guard !inFlight else { return }
        inFlight = true
        DispatchQueue.global(qos: .utility).async { [weak self] in
            let listing = try? Daemon.list()
            DispatchQueue.main.async {
                guard let self else { return }
                self.inFlight = false
                if let listing { self.session.reconcile(listing) }
            }
        }
    }
}
