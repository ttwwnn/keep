import Foundation

// What Daemon/KeepCLI.swift asks of the rest of the app and this test does
// not build: the app's state folder (Model/SidebarStateStore.swift), here a
// temporary one, so the restoration's pictures never touch the person's.
func stateDirectory() -> URL {
    URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("keep-usage-test-state")
}
