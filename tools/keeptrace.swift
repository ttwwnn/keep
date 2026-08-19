import Cocoa

// Observador externo do Keep: entrada do usuário, geometria de janela e foco,
// numa linha de tempo única.
//
// Nunca registra conteúdo de tecla. Este é um terminal: caracteres seriam
// senhas e comandos. Teclas aparecem como contagem e modificadores, só.

let target = "Keep"
let out = FileHandle.standardOutput
let started = Date()
let fmt: DateFormatter = {
    let f = DateFormatter()
    f.dateFormat = "HH:mm:ss.SSS"
    return f
}()

var clicks = 0
var keys = 0
var scrolls = 0

func emit(_ kind: String, _ detail: String) {
    let now = Date()
    let rel = String(format: "%7.3f", now.timeIntervalSince(started))
    let line = "\(fmt.string(from: now)) +\(rel)  \(kind.padding(toLength: 9, withPad: " ", startingAt: 0)) \(detail)\n"
    out.write(line.data(using: .utf8)!)
}

/// O que está debaixo do cursor, pela árvore de acessibilidade.
func elementAt(_ p: CGPoint) -> String {
    let system = AXUIElementCreateSystemWide()
    var element: AXUIElement?
    guard AXUIElementCopyElementAtPosition(system, Float(p.x), Float(p.y), &element) == .success,
          let element else { return "?" }

    func attr(_ el: AXUIElement, _ name: String) -> String? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(el, name as CFString, &value) == .success else { return nil }
        if let s = value as? String, !s.isEmpty { return s }
        return nil
    }

    // Sobe até achar algo com rótulo útil.
    var current: AXUIElement? = element
    var parts: [String] = []
    var hops = 0
    while let el = current, hops < 5 {
        let role = attr(el, kAXRoleAttribute as String) ?? "?"
        let label = attr(el, kAXTitleAttribute as String)
            ?? attr(el, kAXValueAttribute as String)
            ?? attr(el, kAXDescriptionAttribute as String)
        parts.append(label.map { "\(role)(\($0))" } ?? role)
        var parent: CFTypeRef?
        AXUIElementCopyAttributeValue(el, kAXParentAttribute as CFString, &parent)
        current = (parent as! AXUIElement?)
        hops += 1
    }
    return parts.joined(separator: " < ")
}

// --- amostragem de janelas e foco, numa thread própria
Thread.detachNewThread {
    var lastWindows = ""
    var lastFront = ""
    while true {
        let front = NSWorkspace.shared.frontmostApplication?.localizedName ?? "?"
        if front != lastFront {
            emit("focus", "frontmost=\(front)")
            lastFront = front
        }

        if let list = CGWindowListCopyWindowInfo(
            [.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID
        ) as? [[String: Any]] {
            let mine = list.filter {
                ($0[kCGWindowOwnerName as String] as? String) == target
                && (($0[kCGWindowBounds as String] as? [String: Any])?["Height"] as? Double ?? 0) > 120
            }.compactMap { info -> String? in
                guard let id = info[kCGWindowNumber as String] as? Int,
                      let b = info[kCGWindowBounds as String] as? [String: Any],
                      let w = b["Width"] as? Double, let h = b["Height"] as? Double
                else { return nil }
                let a = (info[kCGWindowAlpha as String] as? Double) ?? 1
                let x = (b["X"] as? Double) ?? 0, y = (b["Y"] as? Double) ?? 0
                return "#\(id) \(Int(x)),\(Int(y)) \(Int(w))x\(Int(h)) a=\(String(format: "%.2f", a))"
            }
            let now = "n=\(mine.count) \(mine.joined(separator: " | "))"
            if now != lastWindows {
                emit("window", now)
                lastWindows = now
            }
        }
        usleep(8000)
    }
}

// --- entrada do usuário
func mods(_ f: NSEvent.ModifierFlags) -> String {
    var s = ""
    if f.contains(.command) { s += "⌘" }
    if f.contains(.option) { s += "⌥" }
    if f.contains(.control) { s += "⌃" }
    if f.contains(.shift) { s += "⇧" }
    return s.isEmpty ? "-" : s
}

// CGEventTap em vez de NSEvent.addGlobalMonitorForEvents: o monitor global
// falha em silêncio quando lhe falta permissão, e foi o que cegou as
// gravações anteriores. Aqui a criação do tap devolve nil e a gente sabe.
let mask = (1 << CGEventType.leftMouseDown.rawValue)
    | (1 << CGEventType.rightMouseDown.rawValue)
    | (1 << CGEventType.keyDown.rawValue)
    | (1 << CGEventType.scrollWheel.rawValue)

let callback: CGEventTapCallBack = { _, type, event, _ in
    switch type {
    case .leftMouseDown, .rightMouseDown:
        clicks += 1
        let p = event.location
        let button = type == .rightMouseDown ? "right" : "left"
        let n = event.getIntegerValueField(.mouseEventClickState)
        emit("click", "#\(clicks) \(button) clickState=\(n) at \(Int(p.x)),\(Int(p.y)) → \(elementAt(p))")
    case .keyDown:
        // Sem caractere e sem keycode. Isto é um terminal.
        keys += 1
        emit("key", "#\(keys)")
    case .scrollWheel:
        scrolls += 1
        if scrolls % 10 == 1 { emit("scroll", "#\(scrolls)") }
    default:
        break
    }
    return Unmanaged.passUnretained(event)
}

guard let tap = CGEvent.tapCreate(
    tap: .cgSessionEventTap,
    place: .headInsertEventTap,
    options: .listenOnly,
    eventsOfInterest: CGEventMask(mask),
    callback: callback,
    userInfo: nil
) else {
    emit("ERRO", "não consegui criar o event tap — falta permissão de Acessibilidade/Input Monitoring para este binário")
    exit(1)
}
let source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
CFRunLoopAddSource(CFRunLoopGetCurrent(), source, .commonModes)
CGEvent.tapEnable(tap: tap, enable: true)
emit("perms", "event tap ativo")

emit("start", "observando \"\(target)\" — ctrl-c para parar")
signal(SIGINT) { _ in
    print("\n--- total: \(clicks) cliques, \(keys) teclas, \(scrolls) eventos de scroll")
    exit(0)
}
RunLoop.main.run()
