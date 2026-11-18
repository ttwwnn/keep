// Another app's window for a drag to end on, as the Finder or Mail would be:
// it takes files as they take them (file URLs off the drag pasteboard) and
// writes down what it was handed.
//
//   drop-target <x> <y> <w> <h> <out> [seconds]
//
// Screen coordinates, top-left origin, as mousedrag's. On a drop, <out> gets
// one path per line and then `move=<yes|no>`: whether the source let the drop
// move the file away, which the Finder does on the same volume. `<out>.ready`
// appears once the window is on screen.
import Cocoa

let args = CommandLine.arguments
guard args.count >= 6, let x = Double(args[1]), let y = Double(args[2]),
      let w = Double(args[3]), let h = Double(args[4]) else {
    FileHandle.standardError.write("usage: drop-target <x> <y> <w> <h> <out> [seconds]\n".data(using: .utf8)!)
    exit(2)
}
let out = args[5]
let seconds = args.count > 6 ? Double(args[6]) ?? 30 : 30

final class Target: NSView {
    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        return sender.draggingSourceOperationMask.contains(.copy) ? .copy : []
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        let urls = sender.draggingPasteboard.readObjects(
            forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
        let mask = sender.draggingSourceOperationMask
        let moves = mask.contains(.move) || mask.contains(.generic)
        let text = urls.map(\.path).joined(separator: "\n") + "\nmove=\(moves ? "yes" : "no")\n"
        try? text.write(toFile: out, atomically: true, encoding: .utf8)
        return !urls.isEmpty
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let screenHeight = NSScreen.screens.first?.frame.height ?? 0
let window = NSWindow(
    contentRect: NSRect(x: x, y: screenHeight - y - h, width: w, height: h),
    styleMask: [.titled], backing: .buffered, defer: false)
window.title = "drop-target"
window.level = .floating
let target = Target()
target.registerForDraggedTypes([.fileURL])
window.contentView = target
window.orderFrontRegardless()
// Up and on screen: the drag may start.
FileManager.default.createFile(atPath: out + ".ready", contents: nil)
DispatchQueue.main.asyncAfter(deadline: .now() + seconds) { exit(0) }
app.run()
