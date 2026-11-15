// One entry: its paper, Read or Get PDF, its tags, note, and files, and editing its BibTeX

import QuickLook
import SwiftUI
import UniformTypeIdentifiers

struct EntryPage: View {
    @Environment(Store.self) private var store
    let entry: Entry
    @State private var note = ""
    @FocusState private var writingNote: Bool
    @State private var newTag = ""
    @State private var reading: URL?
    @State private var downloading = false
    /// What was copied last: shown on its button
    @State private var copied: String?
    @State private var editing = false
    @State private var addingFiles = false
    @State private var deleting = false

    var body: some View {
        let files = entry.files()
        List {
            Section {
                heading
                    .listRowBackground(Color.clear)
                    .listRowInsets(EdgeInsets())
            }
            Section("Tags") {
                ForEach(entry.tags, id: \.self) { tag in
                    Text(tag)
                }
                .onDelete { at in
                    store.edit(entry) { e in
                        var tags = e.tags
                        tags.remove(atOffsets: at)
                        e.setTags(tags)
                    }
                }
                TextField("Add Tag", text: $newTag)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .onSubmit(addTag)
            }
            Section("Note") {
                TextEditor(text: $note)
                    .frame(minHeight: 100)
                    .focused($writingNote)
            }
            Section("Files") {
                ForEach(files, id: \.self) { name in
                    Button {
                        reading = entry.dir.appendingPathComponent(name)
                    } label: {
                        Label(name, systemImage: isPDF(name) ? "doc.richtext" : "doc")
                    }
                }
                Button("Add Files…", systemImage: "plus") { addingFiles = true }
            }
        }
        .navigationTitle(entry.key)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            Menu {
                Button("Edit BibTeX…", systemImage: "pencil") { editing = true }
                Button("Get Again from INSPIRE", systemImage: "arrow.clockwise") {
                    Task { await store.getAgain(entry) }
                }
                .disabled(paperQuery(entry.bib) == nil)
                if let pdf = entry.pdf() {
                    // How the PDF opens in another app: iOS has no default app for PDFs
                    ShareLink(item: pdf) { Label("Share PDF", systemImage: "square.and.arrow.up") }
                }
                Button("Delete…", systemImage: "trash", role: .destructive) { deleting = true }
            } label: {
                Label("More", systemImage: "ellipsis.circle")
            }
        }
        .background { PDFReader(url: $reading) { store.refresh() } }
        .sheet(isPresented: $editing) { BibEditor(entry: entry) }
        .fileImporter(isPresented: $addingFiles, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            guard case .success(let urls) = result else { return }
            store.change("the files") {
                for url in urls {
                    let access = url.startAccessingSecurityScopedResource()
                    defer { if access { url.stopAccessingSecurityScopedResource() } }
                    try entry.addFile(url.lastPathComponent, Data(contentsOf: url))
                }
            }
        }
        .confirmationDialog("Delete \(entry.key)?", isPresented: $deleting, titleVisibility: .visible) {
            Button("Delete Entry and Its Files", role: .destructive) { store.delete(entry) }
        } message: {
            Text("Its folder is deleted, with \(files.count == 1 ? "its file" : "its \(files.count) files").")
        }
        .onAppear { note = entry.note }
        // Changed elsewhere, while not being written here
        .onChange(of: entry.note) { _, new in
            if !writingNote { note = new }
        }
        .onChange(of: writingNote) { _, writing in
            if !writing { saveNote() }
        }
        .onDisappear(perform: saveNote)
    }

    /// The title, the names, where it was published, and what can be done with it
    private var heading: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(entry.title.isEmpty ? entry.key : entry.title)
                .font(.title2.bold())
                .textSelection(.enabled)
            if !names.isEmpty {
                Text(names)
            }
            Text(details)
                .foregroundStyle(.secondary)
            // On one line where they fit, else on two
            ViewThatFits {
                HStack {
                    mainButton
                    copyButtons
                    inspireLink
                }
                VStack(alignment: .leading) {
                    HStack {
                        mainButton
                        inspireLink
                    }
                    HStack { copyButtons }
                }
            }
            .padding(.top, 8)
        }
        .padding(.vertical)
    }

    @ViewBuilder
    private var copyButtons: some View {
        Button(copied == "key" ? "Copied" : "Copy Key") {
            UIPasteboard.general.string = entry.key
            copied = "key"
        }
        .buttonStyle(.bordered)
        Button(copied == "bibtex" ? "Copied" : "Copy BibTeX") {
            UIPasteboard.general.string = entry.bib.bibtex
            copied = "bibtex"
        }
        .buttonStyle(.bordered)
    }

    @ViewBuilder
    private var inspireLink: some View {
        if let url = inspireURL(entry.bib) {
            Link("INSPIRE", destination: url)
                .buttonStyle(.bordered)
        }
    }

    @ViewBuilder
    private var mainButton: some View {
        if let pdf = entry.pdf() {
            Button("Read") { reading = pdf }
                .buttonStyle(.borderedProminent)
        } else if entry.bib.get("eprint") != nil {
            Button(downloading ? "Downloading…" : "Get PDF") {
                downloading = true
                Task {
                    await store.getPDF(entry)
                    downloading = false
                }
            }
            .buttonStyle(.borderedProminent)
            .disabled(downloading)
        }
    }

    /// Ten names, then how many more
    private var names: String {
        let names = entry.names
        let more = names.count > 10 ? " and \(names.count - 10) more" : entry.hasOthers ? " et al." : ""
        return names.prefix(10).joined(separator: ", ") + more
    }

    private var details: String {
        let journal = (entry.bib.get("journal") ?? entry.bib.get("booktitle")).map(plain)
        return [entry.year, journal, entry.key].compactMap { $0 }.joined(separator: " · ")
    }

    private func addTag() {
        let tag = newTag.trimmed
        newTag = ""
        guard isTag(tag) else { return }
        store.edit(entry) { e in
            if !e.tags.contains(tag) { e.setTags(e.tags + [tag]) }
        }
    }

    private func saveNote() {
        guard note.trimmed != entry.note else { return }
        let note = note
        store.edit(entry) { $0.setNote(note) }
    }
}

/// The entry's `entry.bib`, as text. Saved only as one entry, under the folder's key
private struct BibEditor: View {
    @Environment(Store.self) private var store
    @Environment(\.dismiss) private var dismiss
    let entry: Entry
    @State private var text = ""
    @State private var problem: String?

    var body: some View {
        NavigationStack {
            TextEditor(text: $text)
                .font(.body.monospaced())
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .padding(.horizontal)
                .navigationTitle("Edit BibTeX")
                .navigationBarTitleDisplayMode(.inline)
                .safeAreaInset(edge: .bottom) {
                    if let problem {
                        Text(problem)
                            .foregroundStyle(.red)
                            .padding()
                    }
                }
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Cancel") { dismiss() }
                    }
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Save", action: save)
                    }
                }
        }
        .onAppear { text = entry.bib.bibtex }
    }

    private func save() {
        do {
            let bibs = try Bib.parse(text)
            guard bibs.count == 1 else { throw CoreError.entries(bibs.count) }
            var bib = bibs[0]
            bib.key = entry.key
            store.edit(entry) { $0.bib = bib }
            dismiss()
        } catch {
            problem = error.localizedDescription
        }
    }
}

/// Quick Look, which saves markup into the file itself. Presented from UIKit: `quickLookPreview`
/// cannot allow editing
struct PDFReader: UIViewControllerRepresentable {
    @Binding var url: URL?
    /// Called when the markup has been saved
    let saved: () -> Void

    func makeUIViewController(context: Context) -> UIViewController {
        UIViewController()
    }

    func updateUIViewController(_ host: UIViewController, context: Context) {
        context.coordinator.parent = self
        guard let url, host.presentedViewController == nil else { return }
        context.coordinator.url = url
        let preview = QLPreviewController()
        preview.dataSource = context.coordinator
        preview.delegate = context.coordinator
        host.present(preview, animated: true)
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    // Quick Look calls these on the main thread, though its protocols do not say so
    @MainActor
    final class Coordinator: NSObject, QLPreviewControllerDataSource,
                             @preconcurrency QLPreviewControllerDelegate {
        var parent: PDFReader
        var url: URL?

        init(_ parent: PDFReader) {
            self.parent = parent
        }

        func numberOfPreviewItems(in controller: QLPreviewController) -> Int {
            url == nil ? 0 : 1
        }

        // Asked only while there is a URL: see `numberOfPreviewItems`
        func previewController(_ controller: QLPreviewController, previewItemAt index: Int) -> QLPreviewItem {
            url! as NSURL
        }

        func previewController(_ controller: QLPreviewController,
                               editingModeFor previewItem: QLPreviewItem) -> QLPreviewItemEditingMode {
            .updateContents
        }

        func previewController(_ controller: QLPreviewController, didUpdateContentsOf previewItem: QLPreviewItem) {
            parent.saved()
        }

        func previewControllerDidDismiss(_ controller: QLPreviewController) {
            parent.url = nil
        }
    }
}
