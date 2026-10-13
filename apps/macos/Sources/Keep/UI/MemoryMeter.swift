import SwiftUI
import AppKit
import Darwin

/// The computer's RAM in use, read the way Activity Monitor adds it up:
/// app memory (anonymous pages that are not purgeable), wired and compressed.
/// The whole machine's, not this app's.
struct MemoryReading: Equatable {
    var used: UInt64
    var total: UInt64

    var percent: Double {
        total == 0 ? 0 : min(100, Double(used) / Double(total) * 100)
    }

    /// nil when the kernel does not answer.
    static func now() -> MemoryReading? {
        var stats = vm_statistics64()
        var count = mach_msg_type_number_t(
            MemoryLayout<vm_statistics64>.stride / MemoryLayout<integer_t>.stride)
        let status = withUnsafeMutablePointer(to: &stats) { pointer in
            pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                host_statistics64(mach_host_self(), HOST_VM_INFO64, $0, &count)
            }
        }
        guard status == KERN_SUCCESS else { return nil }
        let page = UInt64(vm_kernel_page_size)
        let pages = UInt64(stats.internal_page_count)
            &- UInt64(stats.purgeable_count)
            &+ UInt64(stats.wire_count)
            &+ UInt64(stats.compressor_page_count)
        return MemoryReading(used: pages * page, total: ProcessInfo.processInfo.physicalMemory)
    }

    /// "11,2 GB", in the reader's language.
    static func gigabytes(_ bytes: UInt64) -> String {
        let formatter = NumberFormatter()
        formatter.locale = Locale(identifier: "pt_BR")
        formatter.minimumFractionDigits = 1
        formatter.maximumFractionDigits = 1
        let value = Double(bytes) / 1_073_741_824
        return (formatter.string(from: NSNumber(value: value)) ?? String(format: "%.1f", value)) + " GB"
    }
}

/// The RAM bar above the AI usage footer: how much of the computer's memory
/// is in use, written in gigabytes beside the bar. Quiet like the footer's
/// bars — only past 75% does it take amber, then red from 90.
struct MemoryMeter: View {
    @State private var zoom = GhosttyApp.shared.zoomLevel

    var body: some View {
        TimelineView(.periodic(from: .now, by: 3)) { _ in
            if let reading = MemoryReading.now() {
                content(reading)
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: GhosttyApp.textSizeDidChange)) { _ in
            zoom = GhosttyApp.shared.zoomLevel
        }
    }

    private func content(_ reading: MemoryReading) -> some View {
        let level = UsageText.level(reading.percent)
        let figures = "\(MemoryReading.gigabytes(reading.used)) / \(MemoryReading.gigabytes(reading.total))"
        return VStack(alignment: .leading, spacing: 0) {
            Rectangle()
                .fill(UsageInk.wash(0.08))
                .frame(height: 1)
            Text("Memória RAM")
                .font(Font(GhosttyApp.shared.tabNameFont))
                .foregroundStyle(UsageInk.inkFaint)
                .padding(.horizontal, 17)
                .padding(.top, 7)
                .padding(.bottom, 5)
            HStack(spacing: 6) {
                GeometryReader { space in
                    ZStack(alignment: .leading) {
                        Capsule().fill(UsageInk.wash(0.12))
                        Capsule()
                            .fill(UsageInk.fill(level))
                            .frame(width: max(2, space.size.width * reading.percent / 100))
                    }
                }
                .frame(height: 4)
                Text(figures)
                    .font(Font(GhosttyApp.shared.tabNameFont))
                    .monospacedDigit()
                    .foregroundStyle(level == .normal ? UsageInk.inkResting : UsageInk.fill(level))
                    .lineLimit(1)
                    .fixedSize()
            }
            .frame(height: 16)
            .padding(.horizontal, 17)
            .padding(.bottom, 8)
        }
        // The whole panel is the button, the gap round the bar included.
        .contentShape(Rectangle())
        .onTapGesture { ActivityMonitor.showMemory() }
        .help("Memória RAM do computador em uso (\(Int(reading.percent.rounded()))%). Clique para ver os processos.")
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Memória RAM em uso: \(figures)")
        .accessibilityAddTraits(.isButton)
        .accessibilityAction { ActivityMonitor.showMemory() }
    }
}

/// The system's own Activity Monitor, on its Memory tab: the processes that
/// are using the computer's RAM, most hungry first.
enum ActivityMonitor {
    private static let bundleID = "com.apple.ActivityMonitor"
    private static let memoryTab = 1

    @MainActor
    static func showMemory() {
        let running = NSRunningApplication.runningApplications(withBundleIdentifier: bundleID).first
        // Already on the Memory tab: bring it forward, nothing to restart.
        if let running, selectedTab() == memoryTab {
            running.activate()
            return
        }
        // The app reads its tab when it starts, and offers no way to be told
        // another from outside without Accessibility permission; so the tab is
        // written to its preferences and an open instance is closed first.
        CFPreferencesSetAppValue("SelectedTab" as CFString, memoryTab as CFNumber, bundleID as CFString)
        CFPreferencesAppSynchronize(bundleID as CFString)
        guard let running else { return launch() }
        running.terminate()
        Task { @MainActor in
            for _ in 0..<40 where !running.isTerminated {
                try? await Task.sleep(for: .milliseconds(50))
            }
            if running.isTerminated {
                // It writes its preferences on the way out; the tab is ours again.
                CFPreferencesSetAppValue("SelectedTab" as CFString, memoryTab as CFNumber, bundleID as CFString)
                CFPreferencesAppSynchronize(bundleID as CFString)
            }
            launch()
        }
    }

    private static func selectedTab() -> Int? {
        CFPreferencesAppSynchronize(bundleID as CFString)
        return (CFPreferencesCopyAppValue("SelectedTab" as CFString, bundleID as CFString) as? NSNumber)?.intValue
    }

    @MainActor
    private static func launch() {
        guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID) else { return }
        NSWorkspace.shared.openApplication(at: url, configuration: NSWorkspace.OpenConfiguration())
    }
}

extension GhosttyApp {
    /// The face and size of a tab's name in the sidebar (12.5 points, a quarter
    /// up and at the terminal's zoom), which the footers under the list write
    /// in as well, so everything in the sidebar's column reads as one text.
    var tabNameFont: NSFont { terminalFont(size: 12.5 * textScale) }

    /// The chrome's text is set a quarter above the size it was drawn at: at
    /// 100% it was too small to read (asked for 13/10/2026: "como ficam em 125%").
    static let textBoost = 1.25

    /// What the chrome's text is multiplied by: the zoom, and the boost.
    var textScale: Double { zoomLevel * Self.textBoost }

    /// What the chrome's icons are multiplied by: the zoom, held at 130% so
    /// that the zoom control's percentage still fits its capsule. The icons
    /// are as drawn at 100% (they were right there) and follow the zoom.
    var iconScale: Double { min(zoomLevel, 1.3) }
}
