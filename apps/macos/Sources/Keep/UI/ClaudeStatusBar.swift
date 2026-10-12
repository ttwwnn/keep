import AppKit

/// The bar at the foot of a Claude Code conversation: the model and the
/// effort it runs with, both of which can be changed by clicking them, and
/// how many agents it has going.
///
/// Changing one types the slash command into the conversation, so it only
/// offers to when the turn is over and the prompt is empty — typing into a
/// draft, a dialog or a running turn would spoil it.
@MainActor
final class ClaudeStatusBar: NSView {
    static let height: CGFloat = 22

    /// A level was picked from the effort menu (`low` … `max`).
    var onEffort: ((String) -> Void)?
    /// A model was picked from the model menu (its id).
    var onModel: ((String) -> Void)?

    private(set) var status: ClaudeStatus?
    private let modelButton = ClaudeStatusBar.chip(id: "claude-model")
    private let effortButton = ClaudeStatusBar.chip(id: "claude-effort")
    private let agentsLabel = NSTextField(labelWithString: "")
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
        let stack = NSStackView(views: [modelButton, Self.dot(), effortButton, Self.dot(), agentsLabel])
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

    private static let font = NSFont.monospacedSystemFont(ofSize: 11, weight: .regular)

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
        modelButton.attributedTitle = title(status.modelName)
        modelButton.toolTip = "Modelo: \(status.modelID). Clique para trocar."
        effortButton.isHidden = status.effort == nil
        effortButton.attributedTitle = title("esforço \(status.effort ?? "")")
        effortButton.toolTip = "Esforço de raciocínio. Clique para trocar."
        agentsLabel.stringValue = status.agents == 1 ? "1 agente" : "\(status.agents) agentes"
    }

    @objc private func showEfforts(_ sender: NSButton) {
        guard let status else { return }
        let menu = menu(title: "Esforço")
        for level in ClaudeStatusReader.efforts {
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
        for model in ClaudeStatusReader.models {
            let item = NSMenuItem(title: model.name, action: #selector(pickedModel(_:)), keyEquivalent: "")
            item.target = self
            item.representedObject = model.id
            item.state = model.id == status.modelID ? .on : .off
            item.isEnabled = status.canType
            menu.addItem(item)
        }
        popUp(menu, from: sender)
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
