import Foundation

/// The directories you actually go to, from zoxide.
///
/// The picker's other half is "somewhere to start something new", and the
/// honest source for that is the same one the shell uses — not a list this
/// app invents. Absent zoxide, that half is simply empty rather than wrong.
enum Zoxide {
    /// Paths in zoxide's own order, most-used first. Blocking; call it off
    /// the main thread.
    static func directories(limit: Int = 200) -> [String] {
        guard let binary = executable else { return [] }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: binary)
        process.arguments = ["query", "-l"]
        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = FileHandle.nullDevice
        do {
            try process.run()
        } catch {
            return []
        }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard process.terminationStatus == 0,
              let text = String(data: data, encoding: .utf8)
        else { return [] }
        return text
            .split(separator: "\n")
            .map(String.init)
            .filter { !$0.isEmpty }
            .prefix(limit)
            .map { $0 }
    }

    /// A GUI app inherits none of the shell's PATH, so the usual places are
    /// checked by hand.
    private static var executable: String? {
        let candidates = [
            "/opt/homebrew/bin/zoxide",
            "/usr/local/bin/zoxide",
            "/usr/bin/zoxide",
            (NSHomeDirectory() as NSString).appendingPathComponent(".local/bin/zoxide"),
        ]
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0) }
    }
}
