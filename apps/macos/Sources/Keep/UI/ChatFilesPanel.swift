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

/// The panel at the right of a Claude Code conversation, dressed as the sidebar
/// at the left (its ground, its inks, its rows): the files the conversation made
/// above, the ones the person sent below. A click opens one as a click on its
/// name in the terminal does; it can be dragged out.
struct ChatFilesPanel: View {
    @ObservedObject var model: ChatFilesModel
    @State private var hovered: URL?
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
                ScrollView {
                    VStack(alignment: .leading, spacing: 2) {
                        section("Gerados", model.files.made, empty: "Nada gerado ainda")
                        section("Enviados", model.files.sent, empty: "Nada enviado nesta conversa")
                            .padding(.top, 10)
                    }
                    .padding(.vertical, 10)
                }
            }
        }
        .environment(\.sidebarInUse, model.inUse)
        .accessibilityIdentifier("chat-files-panel")
    }

    private func section(_ title: String, _ files: [URL], empty: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(spacing: 6) {
                Text(title).font(.system(size: 12 * textZoom, weight: .semibold)).foregroundStyle(UsageInk.ink)
                Text("\(files.count)").font(.system(size: 12 * textZoom)).foregroundStyle(UsageInk.inkFaint)
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 4)
            if files.isEmpty {
                Text(empty)
                    .font(.system(size: 12 * textZoom))
                    .foregroundStyle(UsageInk.inkFaint)
                    .padding(.horizontal, 24)
            }
            ForEach(files, id: \.self) { row($0) }
        }
    }

    private func row(_ url: URL) -> some View {
        Button { LinkOpener.open(url) } label: {
            HStack(spacing: 6) {
                Image(nsImage: NSWorkspace.shared.icon(forFile: url.path))
                    .resizable()
                    .frame(width: 16 * textZoom, height: 16 * textZoom)
                Text(url.lastPathComponent)
                    .font(.system(size: 12 * textZoom))
                    .foregroundStyle(hovered == url ? UsageInk.ink : UsageInk.inkResting)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            // The sidebar's tab row: its padding, and the capsule wash under the pointer.
            .padding(.horizontal, 12)
            .padding(.vertical, 5)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background {
                if hovered == url { Capsule(style: .continuous).fill(UsageInk.wash(0.055)) }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.leading, 12)
        .padding(.trailing, 8)
        .onHover { inside in hovered = inside ? url : (hovered == url ? nil : hovered) }
        .help((url.path as NSString).abbreviatingWithTildeInPath)
        .accessibilityLabel(url.lastPathComponent)
        .accessibilityIdentifier("chat-file")
        .onDrag { NSItemProvider(contentsOf: url) ?? NSItemProvider() }
        .contextMenu {
            Button("Abrir") { LinkOpener.open(url) }
            Button("Mostrar no Finder") { LinkOpener.reveal(url) }
            Button("Copiar caminho") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(url.path, forType: .string)
            }
        }
    }
}
