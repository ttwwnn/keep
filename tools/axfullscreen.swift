// Put a running app's main window in full screen, or take it out, through
// the accessibility tree: no mouse, no keys.
//
//   axfullscreen <app name> on|off|state
//
// The window is the largest one the app has (a second, tiny one can exist).
// `state` prints 1 or 0. Exits 1 when the app or a window is not there.
import AppKit
import ApplicationServices

let arguments = CommandLine.arguments
guard arguments.count >= 3,
      let app = NSWorkspace.shared.runningApplications.first(where: { $0.localizedName == arguments[1] })
else { exit(1) }
AXUIElementSetMessagingTimeout(AXUIElementCreateSystemWide(), 4)
let element = AXUIElementCreateApplication(app.processIdentifier)
var windows: AnyObject?
AXUIElementCopyAttributeValue(element, kAXWindowsAttribute as CFString, &windows)
func area(_ window: AXUIElement) -> CGFloat {
    var value: AnyObject?
    AXUIElementCopyAttributeValue(window, kAXSizeAttribute as CFString, &value)
    var size = CGSize.zero
    if let value { AXValueGetValue(value as! AXValue, .cgSize, &size) }
    return size.width * size.height
}
guard let window = (windows as? [AXUIElement])?.max(by: { area($0) < area($1) }) else { exit(1) }
switch arguments[2] {
case "state":
    var value: AnyObject?
    AXUIElementCopyAttributeValue(window, "AXFullScreen" as CFString, &value)
    print((value as? Bool) == true ? 1 : 0)
case "on", "off":
    let result = AXUIElementSetAttributeValue(window, "AXFullScreen" as CFString, (arguments[2] == "on") as CFBoolean)
    exit(result == .success ? 0 : 1)
default:
    exit(2)
}
