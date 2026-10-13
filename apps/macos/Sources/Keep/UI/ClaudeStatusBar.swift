import AppKit

/// The bar at the foot of a Claude Code or Codex conversation: the model and
/// the effort it runs with, both of which can be changed by clicking them,
/// and how many agents it has going (Claude Code lists them; Codex does not).
///
/// Changing one types into the conversation — the slash command in Claude
/// Code, Codex's own picker in Codex — so it only offers to when the turn is
/// over and the prompt is empty: typing into a draft, a dialog or a running
/// turn would spoil it.
@MainActor
final class ClaudeStatusBar: NSView {
    /// Tall enough for its text at the terminal's zoom.
    static var height: CGFloat { (ClaudeStatusBar.fontSize + 13).rounded() }
    /// Readable: the size of the terminal's own text, zoom included.
    static var fontSize: CGFloat { 13 * CGFloat(GhosttyApp.shared.zoomLevel) }

    /// A level was picked from the effort menu (`low` … `max`).
    var onEffort: ((String) -> Void)?
    /// A model was picked from the model menu (its id).
    var onModel: ((String) -> Void)?

    private(set) var status: ClaudeStatus?
    private let modelButton = ClaudeStatusBar.chip(id: "claude-model")
    private let effortButton = ClaudeStatusBar.chip(id: "claude-effort")
    private let agentsLabel = NSTextField(labelWithString: "")
    private let effortDot = ClaudeStatusBar.dot()
    private let agentsDot = ClaudeStatusBar.dot()
    private let rule = NSBox()

    override init(frame: NSRect) {
        super.init(frame: frame)
        rule.boxType = .custom
        rule.borderWidth = 0
        rule.fillColor = .separatorColor
        rule.translatesAutoresizingMaskIntoConstraints = false
        agentsLabel.font = Self.font
        agentsLabel.textColor = .secondaryLabelColor
        agentsLabel.setAccessibilityIdentifier("claude-agents")
        modelButton.target = self
        modelButton.action = #selector(showModels(_:))
        effortButton.target = self
        effortButton.action = #selector(showEfforts(_:))
        let stack = NSStackView(views: [modelButton, effortDot, effortButton, agentsDot, agentsLabel])
        stack.orientation = .horizontal
        stack.spacing = 6
        stack.translatesAutoresizingMaskIntoConstraints = false
        addSubview(rule)
        addSubview(stack)
        NSLayoutConstraint.activate([
            rule.leadingAnchor.constraint(equalTo: leadingAnchor),
            rule.trailingAnchor.constraint(equalTo: trailingAnchor),
            rule.topAnchor.constraint(equalTo: topAnchor),
            rule.heightAnchor.constraint(equalToConstant: 1),
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 12),
            stack.centerYAnchor.constraint(equalTo: centerYAnchor, constant: 0.5),
        ])
        setAccessibilityIdentifier("claude-status-bar")
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("not supported") }

    private static var font: NSFont { NSFont.monospacedSystemFont(ofSize: fontSize, weight: .regular) }

    private static func chip(id: String) -> NSButton {
        let button = NSButton(title: "", target: nil, action: nil)
        button.isBordered = false
        button.setButtonType(.momentaryChange)
        button.setAccessibilityIdentifier(id)
        return button
    }

    private static func dot() -> NSTextField {
        let label = NSTextField(labelWithString: "·")
        label.font = font
        label.textColor = .tertiaryLabelColor
        return label
    }

    private func title(_ text: String) -> NSAttributedString {
        NSAttributedString(
            string: text + " ▾",
            attributes: [.font: Self.font, .foregroundColor: NSColor.labelColor])
    }

    /// Show what the conversation has; nil hides the bar.
    func apply(_ status: ClaudeStatus?) {
        self.status = status
        isHidden = status == nil
        guard let status else { return }
        for label in [agentsLabel, effortDot, agentsDot] { label.font = Self.font }
        modelButton.attributedTitle = title(status.modelName)
        modelButton.toolTip = "Modelo: \(status.modelID). Clique para trocar."
        effortButton.isHidden = status.effort == nil
        effortDot.isHidden = status.effort == nil
        effortButton.attributedTitle = title("esforço \(status.effort ?? "")")
        effortButton.toolTip = "Esforço de raciocínio. Clique para trocar."
        agentsLabel.isHidden = status.agents == nil
        agentsDot.isHidden = status.agents == nil
        if let agents = status.agents {
            agentsLabel.stringValue = agents == 1 ? "1 agente" : "\(agents) agentes"
        }
    }

    @objc private func showEfforts(_ sender: NSButton) {
        guard let status else { return }
        let menu = menu(title: "Esforço")
        for level in efforts(for: status) {
            let item = NSMenuItem(title: level, action: #selector(pickedEffort(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = level
            item.state = level == status.effort ? .on : .off
            item.isEnabled = status.canType
            menu.addItem(item)
        }
        popUp(menu, from: sender)
    }

    @objc private func showModels(_ sender: NSButton) {
        guard let status else { return }
        let menu = menu(title: "Modelo")
        for model in models(for: status) {
            let item = NSMenuItem(title: model.name, action: #selector(pickedModel(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = model.id
            item.state = model.id == status.modelID ? .on : .off
            item.isEnabled = status.canType
            menu.addItem(item)
        }
        popUp(menu, from: sender)
    }

    /// The efforts to offer: Claude Code's five, or what Codex's cache says
    /// the model takes.
    private func efforts(for status: ClaudeStatus) -> [String] {
        guard status.program == "codex" else { return ClaudeStatusReader.efforts }
        let model = CodexStatusReader.model(named: status.modelID, in: CodexModelsCache.shared.models())
        return model.map(\.efforts).flatMap { $0.isEmpty ? nil : $0 } ?? CodexStatusReader.efforts
    }

    /// The models to offer: Claude Code's, or the ones Codex lists.
    private func models(for status: ClaudeStatus) -> [(name: String, id: String)] {
        guard status.program == "codex" else { return ClaudeStatusReader.models }
        return CodexModelsCache.shared.models().map { ($0.name, $0.slug) }
    }

    private func menu(title: String) -> NSMenu {
        let menu = NSMenu(title: title)
        menu.autoenablesItems = false
        if status?.canType == false {
            let note = NSMenuItem(
                title: "Disponível com o turno terminado e o prompt vazio", action: nil, keyEquivalent: "")
            note.isEnabled = false
            menu.addItem(note)
            menu.addItem(.separator())
        }
        return menu
    }

    private func popUp(_ menu: NSMenu, from button: NSButton) {
        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: button.bounds.height + 2), in: button)
    }

    @objc private func pickedEffort(_ sender: NSMenuItem) {
        if let level = sender.representedObject as? String { onEffort?(level) }
    }

    @objc private func pickedModel(_ sender: NSMenuItem) {
        if let id = sender.representedObject as? String { onModel?(id) }
    }
}

/// Changes Codex's model or effort the way a person does, through its own
/// picker: `/model`, the model's number, then the level's — by way of "More
/// reasoning…" for Max and Ultra. Each step waits for the screen it needs
/// (read from the daemon) before it presses anything, and a step that does
/// not come backs out with Esc rather than pressing on blind.
enum CodexPickerDriver {
    static func choose(
        model: CodexModel, effort: String, workspace: String, tab: UInt32,
        send: @escaping @MainActor (String) -> Void
    ) {
        DispatchQueue.global(qos: .userInitiated).async {
            func key(_ keys: String) { DispatchQueue.main.sync { send(keys) } }
            func screen(showing title: String) -> String? {
                for _ in 0..<50 {
                    if let text = try? Daemon.preview(workspace: workspace, tab: tab), text.contains(title) {
                        return text
                    }
                    usleep(100_000)
                }
                return nil
            }
            func giveUp(_ why: String) {
                Trace.log("codex", "picker: \(why); backing out")
                for _ in 0..<3 {
                    key("\u{1B}")
                    usleep(300_000)
                }
            }
            key("/model")
            usleep(250_000)
            key("\r")
            guard let list = screen(showing: CodexPicker.modelsTitle) else { return giveUp("no model list") }
            guard let n = CodexPicker.number(of: model.name, onScreen: list), n < 10 else {
                return giveUp("\(model.name) not in the list")
            }
            key(String(n))
            guard let levels = screen(showing: CodexPicker.levelsTitle) else { return giveUp("no levels") }
            let label = CodexPicker.label(ofEffort: effort)
            if let m = CodexPicker.number(of: label, onScreen: levels), m < 10 {
                key(String(m))
                Trace.log("codex", "picker: \(model.slug) \(effort)")
                return
            }
            guard let more = CodexPicker.number(of: CodexPicker.moreReasoning, onScreen: levels), more < 10 else {
                return giveUp("\(label) not offered")
            }
            key(String(more))
            guard let advanced = screen(showing: CodexPicker.advancedTitle),
                let a = CodexPicker.number(of: label, onScreen: advanced), a < 10
            else { return giveUp("\(label) not under More reasoning") }
            key(String(a))
            Trace.log("codex", "picker: \(model.slug) \(effort)")
        }
    }
}

/// Changes Claude Code's model or effort for this conversation only, through
/// its own pickers: `/model` (the arrows to the model, then "s") and
/// `/effort` (the arrows along its rule, then "s"). Each step reads the
/// screen first; the question Claude Code asks before switching models in a
/// conversation with a cache is answered yes — the pick was the person's. A
/// step that does not come backs out with Esc.
enum ClaudePickerDriver {
    static func choose(
        model label: String, workspace: String, tab: UInt32, send: @escaping @MainActor (String) -> Void
    ) {
        run(workspace: workspace, tab: tab, send: send) { key, screen, changed, giveUp in
            key("/model")
            usleep(250_000)
            key("\r")
            guard var shown = screen(ClaudePicker.modelSessionOnly), shown.contains(ClaudePicker.modelTitle)
            else { return giveUp("no model picker") }
            for _ in 0..<25 {
                guard let steps = ClaudePicker.steps(to: label, onScreen: shown) else {
                    return giveUp("\(label) not in the picker")
                }
                if steps == 0 { break }
                key(steps > 0 ? "\u{1B}[B" : "\u{1B}[A")
                // The next look is taken once the pointer has moved: a key
                // pressed the moment the picker opens can be lost, and is
                // then pressed again rather than counted.
                guard let next = changed(shown, ClaudePicker.modelSessionOnly)
                else { return giveUp("picker gone") }
                shown = next
            }
            guard ClaudePicker.steps(to: label, onScreen: shown) == 0 else { return giveUp("pointer never reached \(label)") }
            key("s")
            // The question, when the conversation has a cache.
            for _ in 0..<20 {
                usleep(250_000)
                guard let text = try? Daemon.preview(workspace: workspace, tab: tab) else { continue }
                if ClaudePicker.asksToSwitch(onScreen: text) {
                    key("\r")
                    break
                }
                if !text.contains(ClaudePicker.modelSessionOnly) { break }
            }
            Trace.log("claude", "picker: model \(label), this session only")
        }
    }

    static func choose(
        effort level: String, workspace: String, tab: UInt32, send: @escaping @MainActor (String) -> Void
    ) {
        run(workspace: workspace, tab: tab, send: send) { key, screen, changed, giveUp in
            key("/effort")
            usleep(250_000)
            key("\r")
            guard var shown = screen(ClaudePicker.effortSessionOnly) else { return giveUp("no effort picker") }
            for _ in 0..<10 {
                guard let steps = ClaudePicker.effortSteps(to: level, onScreen: shown) else {
                    return giveUp("\(level) not on the rule")
                }
                if steps == 0 { break }
                key(steps > 0 ? "\u{1B}[C" : "\u{1B}[D")
                guard let next = changed(shown, ClaudePicker.effortSessionOnly)
                else { return giveUp("picker gone") }
                shown = next
            }
            guard ClaudePicker.effortSteps(to: level, onScreen: shown) == 0 else {
                return giveUp("marker never reached \(level)")
            }
            key("s")
            Trace.log("claude", "picker: effort \(level), this session only")
        }
    }

    private static func run(
        workspace: String, tab: UInt32, send: @escaping @MainActor (String) -> Void,
        _ steps: @escaping (
            _ key: (String) -> Void, _ screen: (String) -> String?,
            _ changed: (String, String) -> String?, _ giveUp: (String) -> Void
        ) -> Void
    ) {
        DispatchQueue.global(qos: .userInitiated).async {
            func key(_ keys: String) { DispatchQueue.main.sync { send(keys) } }
            func screen(showing text: String) -> String? {
                for _ in 0..<50 {
                    if let shown = try? Daemon.preview(workspace: workspace, tab: tab), shown.contains(text) {
                        return shown
                    }
                    usleep(100_000)
                }
                return nil
            }
            /// The screen once it differs from `before` (the pointer or the
            /// marker moved), or as it is after a second; nil when the picker
            /// is gone.
            func changed(from before: String, showing text: String) -> String? {
                var last: String?
                for _ in 0..<10 {
                    usleep(100_000)
                    guard let shown = try? Daemon.preview(workspace: workspace, tab: tab), shown.contains(text)
                    else { continue }
                    if shown != before { return shown }
                    last = shown
                }
                return last
            }
            func giveUp(_ why: String) {
                Trace.log("claude", "picker: \(why); backing out")
                key("\u{1B}")
            }
            steps(key, screen(showing:), changed(from:showing:), giveUp)
        }
    }
}
