// Post key events to one process, whether or not it has focus.
//
// AppleScript's `keystroke` goes to whatever is frontmost and carries
// whatever modifiers the system thinks are held, which under a tiling window
// manager is neither predictable nor ours to control: a test that types into
// the wrong window and then asserts about the right one reports nonsense. A
// CGEvent posted to a pid goes where it is sent.
//
//   sendkey <pid> text <string>
//   sendkey <pid> key <macos-key-code> [ctrl|alt|shift|cmd]...
//
// Key codes are the platform's own (kVK_ANSI_*): 36 return, 51 backspace,
// 53 escape, 123 left.

import CoreGraphics
import Foundation

let args = CommandLine.arguments
guard args.count >= 4, let pid = Int32(args[1]) else {
    FileHandle.standardError.write(Data("""
        usage: sendkey <pid> text <string>
               sendkey <pid> key <code> [ctrl|alt|shift|cmd]...

        """.utf8))
    exit(2)
}

let source = CGEventSource(stateID: .hidSystemState)

/// Between events, so the terminal is never handed two keys in one turn of
/// its loop and the order it sees is the order they were sent.
let gap: UInt32 = 12_000

func post(_ event: CGEvent?) {
    guard let event else { return }
    event.postToPid(pid)
    usleep(gap)
}

switch args[2] {
case "text":
    for scalar in Array(args[3].unicodeScalars) {
        var chars = [UniChar(scalar.value)]
        for down in [true, false] {
            let event = CGEvent(keyboardEventSource: source, virtualKey: 0, keyDown: down)
            event?.keyboardSetUnicodeString(stringLength: 1, unicodeString: &chars)
            // Cleared, always. CGEvent inherits whatever the system believes
            // is held, and a stray option turns "a" into "å" — which is how
            // a harness lies to the person reading its output.
            event?.flags = []
            post(event)
        }
    }

case "key":
    guard let code = UInt16(args[3]) else { exit(2) }
    var flags: CGEventFlags = []
    for name in args.dropFirst(4) {
        switch name {
        case "ctrl": flags.insert(.maskControl)
        case "alt": flags.insert(.maskAlternate)
        case "shift": flags.insert(.maskShift)
        case "cmd": flags.insert(.maskCommand)
        default:
            FileHandle.standardError.write(Data("unknown modifier: \(name)\n".utf8))
            exit(2)
        }
    }
    for down in [true, false] {
        let event = CGEvent(keyboardEventSource: source, virtualKey: CGKeyCode(code), keyDown: down)
        event?.flags = flags
        post(event)
    }

case "drag":
    // drag <x1> <y1> <x2> <y2> [modifiers...] in screen coordinates.
    guard args.count >= 7,
        let x1 = Double(args[3]), let y1 = Double(args[4]),
        let x2 = Double(args[5]), let y2 = Double(args[6])
    else { exit(2) }
    var flags: CGEventFlags = []
    for name in args.dropFirst(7) {
        switch name {
        case "ctrl": flags.insert(.maskControl)
        case "alt": flags.insert(.maskAlternate)
        case "shift": flags.insert(.maskShift)
        case "cmd": flags.insert(.maskCommand)
        default: exit(2)
        }
    }
    func mouse(_ type: CGEventType, _ point: CGPoint) {
        let event = CGEvent(
            mouseEventSource: source, mouseType: type,
            mouseCursorPosition: point, mouseButton: .left)
        event?.flags = flags
        post(event)
    }
    mouse(.leftMouseDown, CGPoint(x: x1, y: y1))
    // Several steps, because a selection is made by moving: one jump from
    // start to end is a press and a release with nothing in between, which is
    // what a click looks like.
    for step in 1...8 {
        let t = Double(step) / 8
        mouse(.leftMouseDragged, CGPoint(x: x1 + (x2 - x1) * t, y: y1 + (y2 - y1) * t))
    }
    mouse(.leftMouseUp, CGPoint(x: x2, y: y2))

default:
    exit(2)
}
