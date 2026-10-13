// The dividers between a running app's panes, read and moved through the
// accessibility tree — no mouse, nothing the person at the machine would feel.
//
//   axsplit <app name> list                 one line per divider between panes:
//                                           "<index> <position> <x y w h>", in the
//                                           order the tree lists them
//   axsplit <app name> set <index> <points> put a divider at a position, in points
//                                           from its split's leading edge
//
// A divider between panes is a splitter whose split view says "pane-split"
// (TabHostView): the sidebar's own divider is not one of them. Only what is
// on screen is in the tree, so these are the dividers of the tab on view.
// Written for tools/hibernate-test.sh, which needs a divider somewhere other
// than the middle to see whether it comes back where it was left. Exits 1 when
// there is no such divider, 4 when the app did not answer.
import AppKit
import ApplicationServices

let arguments = CommandLine.arguments
guard arguments.count >= 3 else {
    FileHandle.standardError.write("usage: axsplit <app> list | set <index> <points>\n".data(using: .utf8)!)
    exit(2)
}
guard let app = NSWorkspace.shared.runningApplications.first(where: { $0.localizedName == arguments[1] })
else { exit(1) }
guard AXIsProcessTrusted() else {
    FileHandle.standardError.write("axsplit: not trusted for accessibility\n".data(using: .utf8)!)
    exit(3)
}
AXUIElementSetMessagingTimeout(AXUIElementCreateSystemWide(), 4)
var silent = false

func attribute(_ element: AXUIElement, _ key: String) -> AnyObject? {
    var value: AnyObject?
    let result = AXUIElementCopyAttributeValue(element, key as CFString, &value)
    if result == .cannotComplete { silent = true }
    return result == .success ? value : nil
}

func frame(_ element: AXUIElement) -> String {
    var origin = CGPoint.zero
    var size = CGSize.zero
    if let value = attribute(element, kAXPositionAttribute) { AXValueGetValue(value as! AXValue, .cgPoint, &origin) }
    if let value = attribute(element, kAXSizeAttribute) { AXValueGetValue(value as! AXValue, .cgSize, &size) }
    return "\(Int(origin.x)) \(Int(origin.y)) \(Int(size.width)) \(Int(size.height))"
}

var dividers: [AXUIElement] = []
func walk(_ element: AXUIElement, depth: Int) {
    guard depth < 48 else { return }
    if (attribute(element, kAXRoleAttribute) as? String) == "AXSplitter",
       let parent = attribute(element, kAXParentAttribute),
       (attribute(parent as! AXUIElement, kAXIdentifierAttribute) as? String) == "pane-split"
    {
        dividers.append(element)
    }
    for child in (attribute(element, kAXChildrenAttribute) as? [AXUIElement]) ?? [] {
        walk(child, depth: depth + 1)
    }
}
let root = AXUIElementCreateApplication(app.processIdentifier)
for window in (attribute(root, kAXWindowsAttribute) as? [AXUIElement]) ?? [] { walk(window, depth: 0) }

switch arguments[2] {
case "list":
    guard !dividers.isEmpty else { exit(silent ? 4 : 1) }
    for (index, divider) in dividers.enumerated() {
        let position = (attribute(divider, kAXValueAttribute) as? NSNumber)?.intValue ?? -1
        print("\(index)\t\(position)\t\(frame(divider))")
    }
case "set" where arguments.count >= 5:
    guard let index = Int(arguments[3]), dividers.indices.contains(index),
          let points = Double(arguments[4])
    else { exit(silent ? 4 : 1) }
    let result = AXUIElementSetAttributeValue(
        dividers[index], kAXValueAttribute as CFString, NSNumber(value: points))
    exit(result == .success ? 0 : 1)
default:
    exit(2)
}
