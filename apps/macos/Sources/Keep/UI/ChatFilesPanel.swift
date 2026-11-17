import AppKit
import SwiftUI

/// What the panel beside a conversation shows.
@MainActor
final class ChatFilesModel: ObservableObject {
    @Published var files = ChatFiles()
    /// Said instead of the lists: still reading, or no conversation to read.
    @Published var note: String?
    /// The window has the keyboard: the inks of a window at rest otherwise, as the sidebar's.
    @Published var inUse = true
}

/// The panel at the right of a Claude Code conversation, on the sidebar's ground
/// and in its inks: the files the conversation made above, the ones the person
/// sent below them, then the Mac's Downloads folder, each a list of name, type, modified and size, sorted by a click
/// on a column's title (its arrow turns with a second click). A double click
/// opens a file as a click on its name in the terminal does; it can be dragged out.
struct ChatFilesPanel: View {
    @ObservedObject var model: ChatFilesModel
    /// One order for both lists, newest first until a title is clicked.
    @State private var order = [KeyPathComparator(\ChatFileItem.modified, order: .reverse)]
    /// The rows picked, which a double click opens and the menu acts on.
    @State private var selected = Set<URL>()
    /// The sidebar's text size: the zoom, a quarter up (see `WorkspaceSidebar.textZoom`).
    private var textZoom: Double { GhosttyApp.shared.zoomLevel * GhosttyApp.textBoost }

    var body: some View {
        Group {
            if let note = model.note {
                Text(note)
                    .font(.system(size: 12 * textZoom))
                    .foregroundStyle(UsageInk.inkFaint)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                VStack(alignment: .leading, spacing: 0) {
                    section("Gerados", model.files.made, empty: "Nada gerado ainda")
                    section("Enviados", model.files.sent, empty: "Nada enviado nesta conversa")
                    section("Downloads", model.files.downloads, empty: model.files.downloadsDenied
                        ? "Sem acesso à pasta Downloads: libere em Ajustes do Sistema › Privacidade › Arquivos e Pastas"
                        : "A pasta Downloads está vazia")
                }
                .padding(.top, 10)
            }
        }
        .environment(\.sidebarInUse, model.inUse)
        .accessibilityIdentifier("chat-files-panel")
    }

    private func section(_ title: String, _ files: [ChatFileItem], empty: String) -> some View {
        let items = files.sorted(using: order)
        return VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                Text(title).font(.system(size: 12 * textZoom, weight: .semibold)).foregroundStyle(UsageInk.ink)
                Text("\(files.count)").font(.system(size: 12 * textZoom)).foregroundStyle(UsageInk.inkFaint)
            }
            .padding(.horizontal, 16)
            if items.isEmpty {
                Text(empty)
                    .font(.system(size: 12 * textZoom))
                    .foregroundStyle(UsageInk.inkFaint)
                    .padding(.horizontal, 16)
                    .padding(.bottom, 12)
            } else {
                table(items)
            }
        }
        .frame(maxHeight: items.isEmpty ? nil : .infinity, alignment: .top)
    }

    private func table(_ items: [ChatFileItem]) -> some View {
        Table(of: ChatFileItem.self, selection: $selected, sortOrder: $order) {
            TableColumn("Nome", value: \.name) { item in
                Text(item.name)
                    .foregroundStyle(UsageInk.ink)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .help((item.url.path as NSString).abbreviatingWithTildeInPath)
                    .accessibilityLabel(item.url.lastPathComponent)
                    .accessibilityIdentifier("chat-file-" + item.url.lastPathComponent)
            }
            TableColumn("Tipo", value: \.type) { Text($0.type).foregroundStyle(UsageInk.inkResting) }
                .width(min: 28, ideal: 40)
            TableColumn("Modificado", value: \.modified) { item in
                Text(item.modified, format: .dateTime.day(.twoDigits).month(.twoDigits).hour().minute())
                    .foregroundStyle(UsageInk.inkResting)
            }
            .width(min: 70, ideal: 92)
            TableColumn("Tamanho", value: \.size) { item in
                Text(ByteCountFormatter.string(fromByteCount: item.size, countStyle: .file))
                    .foregroundStyle(UsageInk.inkResting)
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }
            .width(min: 50, ideal: 62)
        } rows: {
            ForEach(items) { item in
                TableRow(item).itemProvider { NSItemProvider(contentsOf: item.url) }
            }
        }
        .tableStyle(.inset(alternatesRowBackgrounds: false))
        .scrollContentBackground(.hidden)
        .font(.system(size: 12 * textZoom))
        .contextMenu(forSelectionType: ChatFileItem.ID.self) { urls in
            if let url = urls.first {
                Button("Abrir") { LinkOpener.open(url) }
                Button("Mostrar no Finder") { LinkOpener.reveal(url) }
                Button("Copiar caminho") {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(url.path, forType: .string)
                }
            }
        } primaryAction: { urls in
            urls.forEach { LinkOpener.open($0) }
        }
    }
}

/// The panel's left edge, dragged as the sidebar's divider is: wider, narrower, or shut.
final class PanelEdge: NSView {
    /// How far the edge moved, in points; positive widens the panel.
    var onDrag: ((CGFloat) -> Void)?
    var onEnd: (() -> Void)?
    private var lastX: CGFloat = 0

    override func resetCursorRects() { addCursorRect(bounds, cursor: .resizeLeftRight) }
    override func mouseDown(with event: NSEvent) { lastX = event.locationInWindow.x }
    override func mouseDragged(with event: NSEvent) {
        let x = event.locationInWindow.x
        onDrag?(lastX - x)
        lastX = x
    }
    override func mouseUp(with event: NSEvent) { onEnd?() }
}
