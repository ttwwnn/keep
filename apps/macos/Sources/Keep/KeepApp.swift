import SwiftUI

@main
struct KeepApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
                .frame(minWidth: 700, minHeight: 420)
        }
        .windowToolbarStyle(.unified)
    }
}
