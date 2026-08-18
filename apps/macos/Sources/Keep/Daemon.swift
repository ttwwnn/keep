import Foundation

/// Talks to keepd over its unix socket.
///
/// Mirrors `crates/keep-proto`: frames are `[tag: u8][len: u32 be][payload]`.
/// Kept deliberately small — the app only needs to enumerate and kill
/// sessions. Terminal I/O never comes through here; a surface runs the
/// `keep` client as its child process instead.
enum Daemon {
    struct Tab: Identifiable, Hashable {
        var id: UInt32
        var cols: UInt16
        var rows: UInt16
        var clients: UInt32
        var finished: Bool
        /// What the program inside called itself (OSC 0/2). Often empty.
        var title: String

        /// Shells retitle constantly and usually with the host and path,
        /// which says nothing useful in a list of tabs from one machine.
        var label: String {
            let trimmed = title.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty else { return "Tab \(id)" }
            if let tail = trimmed.split(separator: ":").last, trimmed.contains("@") {
                return String(tail)
            }
            return trimmed
        }
    }

    struct Session: Identifiable, Hashable {
        var name: String
        var tabs: [Tab]
        var id: String { name }

        var clients: UInt32 { tabs.reduce(0) { $0 + $1.clients } }
        var liveTabs: [Tab] { tabs.filter { !$0.finished } }

        var stateLabel: String {
            if liveTabs.isEmpty { return "empty" }
            return clients > 0 ? "attached" : "idle"
        }
    }

    enum Failure: Error, LocalizedError {
        case cannotConnect(String)
        case protocolError(String)

        var errorDescription: String? {
            switch self {
            case .cannotConnect(let p): return "cannot reach the keep daemon at \(p)"
            case .protocolError(let m): return "protocol error: \(m)"
            }
        }
    }

    private static let tagList: UInt8 = 0x01
    private static let tagNewTab: UInt8 = 0x03
    private static let tagKill: UInt8 = 0x06
    private static let tagCloseTab: UInt8 = 0x07
    private static let tagSessions: UInt8 = 0x81
    private static let tagError: UInt8 = 0x84
    private static let tagOk: UInt8 = 0x85
    private static let tagTabCreated: UInt8 = 0x88

    static var socketPath: String {
        if let override = ProcessInfo.processInfo.environment["KEEP_SOCKET"] {
            return override
        }
        let base = ProcessInfo.processInfo.environment["XDG_RUNTIME_DIR"]
            ?? ProcessInfo.processInfo.environment["TMPDIR"]
            ?? "/tmp"
        let who = ProcessInfo.processInfo.environment["USER"] ?? "default"
        return (base as NSString).appendingPathComponent("keep-\(who).sock")
    }

    /// Start keepd if nothing is listening yet.
    ///
    /// The daemon is not a child of this app: it has to outlive every client,
    /// including the window that happened to launch it.
    static func ensureRunning() throws {
        if let fd = try? connect() {
            close(fd)
            return
        }
        let candidates = [
            ProcessInfo.processInfo.environment["KEEPD_BIN"],
            Bundle.main.bundleURL.appendingPathComponent("Contents/Resources/keepd").path,
            "/usr/local/bin/keepd",
        ].compactMap { $0 }

        guard let exe = candidates.first(where: { FileManager.default.isExecutableFile(atPath: $0) })
        else { throw Failure.cannotConnect(socketPath) }

        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: exe)
        proc.standardOutput = FileHandle.nullDevice
        proc.standardError = FileHandle.nullDevice
        try proc.run()

        let deadline = Date().addingTimeInterval(5)
        while Date() < deadline {
            if let fd = try? connect() {
                close(fd)
                return
            }
            Thread.sleep(forTimeInterval: 0.05)
        }
        throw Failure.cannotConnect(socketPath)
    }

    static func list() throws -> [Session] {
        let sock = try connect()
        defer { close(sock) }
        try send(sock, tag: tagList, payload: Data())

        let (tag, payload) = try recv(sock)
        switch tag {
        case tagSessions:
            return try decodeSessions(payload)
        case tagError:
            var r = Reader(payload)
            throw Failure.protocolError(try r.string())
        default:
            throw Failure.protocolError("unexpected reply tag \(tag)")
        }
    }

    /// Open a new tab in a session, creating the session if needed.
    /// Returns the new tab's id.
    @discardableResult
    static func newTab(in session: String, cols: UInt16 = 80, rows: UInt16 = 24) throws -> UInt32 {
        let sock = try connect()
        defer { close(sock) }
        var w = Writer()
        w.string(session)
        w.string("")           // cwd: inherit the daemon's
        w.u16(cols)
        w.u16(rows)
        try send(sock, tag: tagNewTab, payload: w.data)

        let (tag, payload) = try recv(sock)
        var r = Reader(payload)
        switch tag {
        case tagTabCreated: return try r.u32()
        case tagError: throw Failure.protocolError(try r.string())
        default: throw Failure.protocolError("unexpected reply tag \(tag)")
        }
    }

    static func closeTab(_ tab: UInt32, in session: String) throws {
        let sock = try connect()
        defer { close(sock) }
        var w = Writer()
        w.string(session)
        w.u32(tab)
        try send(sock, tag: tagCloseTab, payload: w.data)

        let (tag, payload) = try recv(sock)
        if tag == tagError {
            var r = Reader(payload)
            throw Failure.protocolError(try r.string())
        }
    }

    static func kill(_ name: String) throws {
        let sock = try connect()
        defer { close(sock) }
        var w = Writer()
        w.string(name)
        try send(sock, tag: tagKill, payload: w.data)

        let (tag, payload) = try recv(sock)
        if tag == tagError {
            var r = Reader(payload)
            throw Failure.protocolError(try r.string())
        }
        guard tag == tagOk else {
            throw Failure.protocolError("unexpected reply tag \(tag)")
        }
    }

    // MARK: - socket

    private static func connect() throws -> Int32 {
        let path = socketPath
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw Failure.cannotConnect(path) }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(path.utf8)
        guard bytes.count < MemoryLayout.size(ofValue: addr.sun_path) else {
            close(fd)
            throw Failure.cannotConnect(path)
        }
        withUnsafeMutablePointer(to: &addr.sun_path) { raw in
            raw.withMemoryRebound(to: CChar.self, capacity: bytes.count + 1) { dst in
                for (i, b) in bytes.enumerated() { dst[i] = CChar(bitPattern: b) }
                dst[bytes.count] = 0
            }
        }

        let size = socklen_t(MemoryLayout<sockaddr_un>.size)
        let ok = withUnsafePointer(to: &addr) { raw in
            raw.withMemoryRebound(to: sockaddr.self, capacity: 1) { sa in
                Darwin.connect(fd, sa, size)
            }
        }
        guard ok == 0 else {
            close(fd)
            throw Failure.cannotConnect(path)
        }
        return fd
    }

    private static func send(_ fd: Int32, tag: UInt8, payload: Data) throws {
        var frame = Data([tag])
        frame.append(contentsOf: withUnsafeBytes(of: UInt32(payload.count).bigEndian) { Array($0) })
        frame.append(payload)
        try frame.withUnsafeBytes { buf in
            var sent = 0
            while sent < buf.count {
                let n = write(fd, buf.baseAddress!.advanced(by: sent), buf.count - sent)
                guard n > 0 else { throw Failure.protocolError("short write") }
                sent += n
            }
        }
    }

    private static func recv(_ fd: Int32) throws -> (UInt8, Data) {
        let head = try readExactly(fd, 5)
        let tag = head[0]
        let len = head.subdata(in: 1..<5).withUnsafeBytes {
            UInt32(bigEndian: $0.loadUnaligned(as: UInt32.self))
        }
        guard len <= 16 * 1024 * 1024 else { throw Failure.protocolError("frame too large") }
        let payload = len == 0 ? Data() : try readExactly(fd, Int(len))
        return (tag, payload)
    }

    private static func readExactly(_ fd: Int32, _ count: Int) throws -> Data {
        var out = Data(capacity: count)
        var buf = [UInt8](repeating: 0, count: count)
        var got = 0
        while got < count {
            let n = read(fd, &buf[got], count - got)
            guard n > 0 else { throw Failure.protocolError("truncated frame") }
            got += n
        }
        out.append(contentsOf: buf)
        return out
    }

    private static func decodeSessions(_ payload: Data) throws -> [Session] {
        var r = Reader(payload)
        let count = try r.u32()
        var out: [Session] = []
        out.reserveCapacity(Int(min(count, 4096)))
        for _ in 0..<count {
            let name = try r.string()
            let tabCount = try r.u32()
            var tabs: [Tab] = []
            tabs.reserveCapacity(Int(min(tabCount, 4096)))
            for _ in 0..<tabCount {
                tabs.append(Tab(
                    id: try r.u32(),
                    cols: try r.u16(),
                    rows: try r.u16(),
                    clients: try r.u32(),
                    finished: try r.u8() != 0,
                    title: try r.string()
                ))
            }
            out.append(Session(name: name, tabs: tabs))
        }
        return out
    }
}

// MARK: - wire helpers

private struct Writer {
    var data = Data()
    mutating func u16(_ v: UInt16) {
        data.append(contentsOf: withUnsafeBytes(of: v.bigEndian) { Array($0) })
    }
    mutating func u32(_ v: UInt32) {
        data.append(contentsOf: withUnsafeBytes(of: v.bigEndian) { Array($0) })
    }
    mutating func string(_ s: String) {
        let bytes = Array(s.utf8)
        u32(UInt32(bytes.count))
        data.append(contentsOf: bytes)
    }
}

private struct Reader {
    private let data: Data
    private var pos: Int
    init(_ data: Data) {
        self.data = data
        self.pos = data.startIndex
    }
    private mutating func take(_ n: Int) throws -> Data {
        guard pos + n <= data.endIndex else { throw Daemon.Failure.protocolError("truncated") }
        defer { pos += n }
        return data.subdata(in: pos..<(pos + n))
    }
    mutating func u8() throws -> UInt8 { try take(1)[0] }
    mutating func u16() throws -> UInt16 {
        try take(2).withUnsafeBytes { UInt16(bigEndian: $0.loadUnaligned(as: UInt16.self)) }
    }
    mutating func u32() throws -> UInt32 {
        try take(4).withUnsafeBytes { UInt32(bigEndian: $0.loadUnaligned(as: UInt32.self)) }
    }
    mutating func string() throws -> String {
        let n = Int(try u32())
        guard let s = String(data: try take(n), encoding: .utf8) else {
            throw Daemon.Failure.protocolError("invalid utf-8")
        }
        return s
    }
}
