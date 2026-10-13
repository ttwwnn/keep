import SwiftUI
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
            .help("Memória RAM do computador em uso (\(Int(reading.percent.rounded()))%)")
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Memória RAM em uso: \(figures)")
        }
    }
}

extension GhosttyApp {
    /// The face and size of a tab's name in the sidebar (12.5 points at the
    /// terminal's zoom), which the footers under the list write in as well, so
    /// everything in the sidebar's column reads as one text.
    var tabNameFont: NSFont { terminalFont(size: 12.5 * zoomLevel) }
}
