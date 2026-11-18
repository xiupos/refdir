// The app: the library's state (`Store`), the folder and its bookmark, the places, the list,
// adding, exporting, and the entries in Spotlight

import CoreSpotlight
import PDFKit
import SwiftUI
import UniformTypeIdentifiers

@main
struct RefdirApp: App {
    @State private var store = Store()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(store)
        }
    }
}

// MARK: - The store

/// An entry's row in the list, made when the library is read: the title is TeX made plain, and
/// its PDF is found in its folder
struct Row: Identifiable {
    let entry: Entry
    let title: String
    let subtitle: String
    let pdf: URL?
    /// What finding looks in, folded
    let text: String
    /// What it is sorted by, in each order
    let sortKeys: [Order: SortKey]
    var id: String { entry.key }

    init(_ entry: Entry) {
        self.entry = entry
        title = entry.title.isEmpty ? entry.key : entry.title
        subtitle = [entry.byline, entry.year ?? ""].filter { !$0.isEmpty }.joined(separator: " · ")
        pdf = entry.pdf()
        text = entry.searchText
        sortKeys = Dictionary(uniqueKeysWithValues: Order.allCases.map { ($0, SortKey(entry, $0)) })
    }
}

/// The library folder, as last read. One for the app. Every change to the folder reads it again
/// after, off the main thread
@MainActor
@Observable
final class Store {
    /// The chosen folder, while the app has access to it
    private(set) var folder: URL?
    /// In `order`
    private(set) var rows: [Row] = []
    /// How the list is sorted, kept for the next time
    var order: Order {
        didSet {
            UserDefaults.standard.set(order.rawValue, forKey: "order")
            rows = sorted(rows)
        }
    }
    private(set) var tags: [TagCount] = []
    /// The folders with an `entry.bib` that cannot be read
    private(set) var unreadable = 0
    /// Shown in an alert
    var error: String?
    /// Whether the folder picker is shown
    var choosingFolder = false

    /// Counts the reads begun: one overtaken by a newer one is dropped
    private var started = 0

    init() {
        order = Order(rawValue: UserDefaults.standard.string(forKey: "order") ?? "") ?? .year
        guard let bookmark = UserDefaults.standard.data(forKey: "folder") else { return }
        var stale = false
        if let url = try? URL(resolvingBookmarkData: bookmark, bookmarkDataIsStale: &stale) {
            open(url, remember: stale)
            refresh()
        } else {
            error = "The library folder could not be found. Choose it again."
        }
    }

    /// Opens a folder the user chose, in place of the current one
    func choose(_ url: URL) {
        folder?.stopAccessingSecurityScopedResource()
        rows = []
        tags = []
        open(url, remember: true)
        refresh()
    }

    private func open(_ url: URL, remember: Bool) {
        // false for folders the app may read anyway, such as its own
        _ = url.startAccessingSecurityScopedResource()
        folder = url
        if remember, let bookmark = try? url.bookmarkData() {
            UserDefaults.standard.set(bookmark, forKey: "folder")
        }
    }

    /// Reads the folder again
    func reload() async {
        guard let folder else { return }
        started += 1
        let mine = started
        let read = await Task.detached { () -> Result<(Library, [Row]), Error> in
            downloadEvicted(folder)
            return Result {
                let library = try Library.read(folder)
                return (library, library.entries.map(Row.init))
            }
        }.value
        guard mine == started else { return }
        switch read {
        case .success(let (library, read)):
            rows = sorted(read)
            index(read)
            tags = tagCounts(library.entries)
            unreadable = library.unreadable.count
        case .failure:
            error = "The library folder cannot be read."
        }
    }

    /// Puts the entries in Spotlight, in place of those before, so that the system's search finds
    /// them by title, names, key, and tags
    private func index(_ rows: [Row]) {
        Task.detached {
            let items = rows.map { row in
                let attributes = CSSearchableItemAttributeSet(contentType: .text)
                attributes.title = row.title
                attributes.contentDescription = [row.subtitle, row.id].filter { !$0.isEmpty }.joined(separator: " · ")
                attributes.keywords = [row.id] + row.entry.names + row.entry.tags
                return CSSearchableItem(uniqueIdentifier: row.id, domainIdentifier: "entries", attributeSet: attributes)
            }
            let index = CSSearchableIndex.default()
            try? await index.deleteSearchableItems(withDomainIdentifiers: ["entries"])
            try? await index.indexSearchableItems(items)
        }
    }

    private func sorted(_ rows: [Row]) -> [Row] {
        rows.sorted { $0.sortKeys[order]! < $1.sortKeys[order]! }
    }

    /// Reads the folder again, without waiting for it
    func refresh() {
        Task { await reload() }
    }

    /// The entry with a key, as last read
    func entry(_ key: String) -> Entry? {
        rows.first { $0.id == key }?.entry
    }

    /// Does something to the folder, says why if it fails, and reads it again
    func change(_ what: String, _ change: () throws -> Void) {
        do {
            try change()
        } catch {
            self.error = message(error, what)
        }
        refresh()
    }

    /// Reads the entry from disk, changes it, and writes it if it changed, so that quick changes do
    /// not undo each other
    func edit(_ entry: Entry, _ edit: (inout Entry) -> Void) {
        change("the entry") {
            var current = try Entry.read(entry.dir)
            let before = current
            edit(&current)
            if current != before { try current.write() }
        }
    }

    /// Adds the paper an identifier finds, or the entries of BibTeX, as they are: what INSPIRE
    /// and doi.org do not have. The key of the last added, or nil if none was
    func add(_ id: String) async -> String? {
        guard let folder else {
            choosingFolder = true
            return nil
        }
        do {
            let bibs = id.hasPrefix("@") ? try Bib.parse(id) : [try await Fetch.find(id)]
            guard !bibs.isEmpty else { throw Failure("It has no BibTeX entry.") }
            let (added, refused) = await Task.detached { addEntries(folder, bibs) }.value
            await reload()
            if let first = refused.first {
                error = added.isEmpty && refused.count == 1
                    ? first.why
                    : "\(added.count) added, \(refused.count) not. \(first.key): \(first.why)"
            }
            return added.last?.key
        } catch {
            self.error = message(error, "the entry")
            return nil
        }
    }

    func getPDF(_ entry: Entry) async {
        do {
            _ = try await Fetch.pdf(entry)
        } catch {
            self.error = message(error, "the PDF")
        }
        await reload()
    }

    func getAgain(_ entry: Entry) async {
        do {
            let bib = try await Fetch.again(entry)
            edit(entry) { $0.bib = bib }
        } catch {
            self.error = message(error, "the entry")
        }
    }

    /// Deletes the entry's folder, with its files. iOS has no trash for the user's folders
    func delete(_ entry: Entry) {
        change("the entry") { try FileManager.default.removeItem(at: entry.dir) }
    }

    /// Renames a tag in every entry that has it, or removes it if `new` is blank. A name some have
    /// already merges the two
    func retag(_ old: String, _ new: String) {
        guard new.trimmed.isEmpty || isTag(new) else {
            error = "A tag cannot have a comma."
            return
        }
        // Each entry read from disk, as `edit` does
        change("the tags") {
            for entry in renameTag(rows.compactMap { try? Entry.read($0.entry.dir) }, old, new) { try entry.write() }
        }
    }
}

/// Asks iCloud Drive for the `entry.bib`s not on the device, which it leaves as `.entry.bib.icloud`
private func downloadEvicted(_ folder: URL) {
    let fm = FileManager.default
    for name in (try? fm.contentsOfDirectory(atPath: folder.path)) ?? [] where !name.hasPrefix(".") {
        let dir = folder.appendingPathComponent(name)
        if fm.fileExists(atPath: dir.appendingPathComponent(".\(entryFile).icloud").path) {
            try? fm.startDownloadingUbiquitousItem(at: dir.appendingPathComponent(entryFile))
        }
    }
}

/// A short message for a failed change of `what`
private func message(_ error: Error, _ what: String) -> String {
    if error is CoreError || error is Failure { return error.localizedDescription }
    if let error = error as? CocoaError, [.fileWriteNoPermission, .fileReadNoPermission].contains(error.code) {
        return "No permission to write \(what)."
    }
    return "Could not write \(what)."
}

// MARK: - The window

/// What the list shows: every entry, or a tag's
enum Place: Hashable {
    case all
    case tag(String)

    var title: String {
        switch self {
        case .all: "All Entries"
        case .tag(let tag): tag
        }
    }

    func has(_ entry: Entry) -> Bool {
        switch self {
        case .all: true
        case .tag(let tag): entry.tags.contains(tag)
        }
    }
}

struct RootView: View {
    @Environment(Store.self) private var store
    @Environment(\.scenePhase) private var scenePhase
    @State private var place: Place? = .all
    @State private var selection: String?
    /// The column shown where only one fits, as on an iPhone
    @State private var column = NavigationSplitViewColumn.sidebar

    var body: some View {
        @Bindable var store = store
        NavigationSplitView(preferredCompactColumn: $column) {
            Places(place: $place)
        } content: {
            EntryList(place: place ?? .all, selection: $selection)
        } detail: {
            if let key = selection, let entry = store.entry(key) {
                // A page for each entry, so that its note is saved when another is shown
                EntryPage(entry: entry).id(key)
            } else {
                ContentUnavailableView("No Entry Selected", systemImage: "doc.text")
            }
        }
        .fileImporter(isPresented: $store.choosingFolder, allowedContentTypes: [.folder]) { result in
            if case .success(let url) = result { store.choose(url) }
        }
        .alert("Refdir", isPresented: Binding { store.error != nil } set: { if !$0 { store.error = nil } }) {
            Button("OK") {}
        } message: {
            Text(store.error ?? "")
        }
        .onContinueUserActivity(CSSearchableItemActionType) { activity in
            // An entry chosen in Spotlight, by its key
            guard let key = activity.userInfo?[CSSearchableItemActivityIdentifier] as? String else { return }
            place = .all
            selection = key
            column = .detail
        }
        .onChange(of: scenePhase) { _, phase in
            // A change synced while the app was away
            if phase == .active { store.refresh() }
        }
    }
}

/// Every entry, then the tags
private struct Places: View {
    @Environment(Store.self) private var store
    @Binding var place: Place?
    @State private var showingAbout = false
    /// The tag being renamed, and its new name
    @State private var renaming: String?
    @State private var newName = ""
    @State private var removing: String?

    var body: some View {
        List(selection: $place) {
            Label("All Entries", systemImage: "books.vertical")
                .badge(store.rows.count)
                .tag(Place.all)
            if !store.tags.isEmpty {
                Section("Tags") {
                    ForEach(store.tags) { t in
                        Label(t.tag, systemImage: "tag")
                            .badge(t.count)
                            .tag(Place.tag(t.tag))
                            // An entry of the list dropped on a tag, as its key, is given the tag
                            .dropDestination(for: String.self) { keys, _ in
                                let entries = keys.compactMap { k in store.rows.first { $0.id == k }?.entry }
                                for entry in entries {
                                    store.edit(entry) { e in
                                        if !e.tags.contains(t.tag) { e.setTags(e.tags + [t.tag]) }
                                    }
                                }
                                return !entries.isEmpty
                            }
                            .contextMenu {
                                Button("Rename Tag…", systemImage: "pencil") {
                                    newName = t.tag
                                    renaming = t.tag
                                }
                                Button("Remove Tag…", systemImage: "tag.slash", role: .destructive) { removing = t.tag }
                            }
                    }
                }
            }
        }
        .navigationTitle("Refdir")
        .toolbar {
            Menu {
                Button("Choose Library Folder…", systemImage: "folder") { store.choosingFolder = true }
                Button("About Refdir", systemImage: "info.circle") { showingAbout = true }
            } label: {
                Label("Menu", systemImage: "ellipsis.circle")
            }
        }
        .alert("Refdir", isPresented: $showingAbout) {
            Button("OK") {}
        } message: {
            Text("A library of references as a folder of BibTeX entries.\n\(store.folder?.lastPathComponent ?? "No folder")\(store.unreadable > 0 ? "\n\(store.unreadable) folders cannot be read." : "")")
        }
        .alert("Rename Tag", isPresented: Binding { renaming != nil } set: { if !$0 { renaming = nil } }) {
            TextField("Tag", text: $newName)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
            Button("Cancel", role: .cancel) {}
            Button("Rename") {
                guard let old = renaming else { return }
                store.retag(old, newName)
                // The place follows its tag
                if place == .tag(old), isTag(newName) { place = .tag(newName.trimmed) }
            }
        } message: {
            Text("Every entry with it is changed. A name some have already merges the two.")
        }
        .confirmationDialog("Remove “\(removing ?? "")”?",
                            isPresented: Binding { removing != nil } set: { if !$0 { removing = nil } },
                            titleVisibility: .visible) {
            Button("Remove Tag", role: .destructive) {
                if let tag = removing { store.retag(tag, "") }
            }
        } message: {
            let n = store.tags.first { $0.tag == removing }?.count ?? 0
            Text("It is removed from \(n == 1 ? "1 entry" : "\(n) entries"), which stay.")
        }
        // A tag that is gone leaves its place
        .onChange(of: store.tags) { _, tags in
            if case .tag(let tag) = place, !tags.contains(where: { $0.tag == tag }) { place = .all }
        }
    }
}

/// A place's entries, found as the user types
private struct EntryList: View {
    @Environment(Store.self) private var store
    let place: Place
    @Binding var selection: String?
    @State private var search = ""
    @State private var adding = false
    @State private var identifier = ""
    @State private var fetching = false
    @State private var reading: URL?
    @State private var deleting: Entry?

    /// Dragged, a row is its PDF to other apps, and its key, seen only within the app, to a tag of
    /// the places
    private func dragged(_ row: Row) -> NSItemProvider {
        let item = row.pdf.flatMap { NSItemProvider(contentsOf: $0) } ?? NSItemProvider()
        item.registerObject(row.entry.key as NSString, visibility: .ownProcess)
        return item
    }

    private var inPlace: [Row] { store.rows.filter { place.has($0.entry) } }

    var body: some View {
        @Bindable var store = store
        let shown = inPlace.filter { hasWords($0.text, search) }
        List(shown, selection: $selection) { row in
            Group {
                // With a PDF, its first page over the menu, as Files shows a file's
                if let pdf = row.pdf {
                    EntryRow(row: row).contextMenu { menu(row) } preview: { FirstPage(url: pdf) }
                } else {
                    EntryRow(row: row).contextMenu { menu(row) }
                }
            }
            .tag(row.id)
            .onDrag { dragged(row) }
            // Only to the left: a swipe to the right goes back to the places
            .swipeActions(edge: .trailing) {
                Button("Delete…", systemImage: "trash") { deleting = row.entry }
                    .tint(.red)
            }
        }
        .overlay {
            if store.folder == nil {
                ContentUnavailableView {
                    Label("No Library Folder", systemImage: "folder")
                } description: {
                    Text("Choose a folder for your references.")
                } actions: {
                    Button("Choose Folder") { store.choosingFolder = true }
                        .buttonStyle(.borderedProminent)
                }
            } else if inPlace.isEmpty {
                ContentUnavailableView("No Entries", systemImage: "books.vertical",
                                       description: Text("Add a paper with +, by its arXiv number, DOI, or texkey."))
            } else if shown.isEmpty {
                ContentUnavailableView.search(text: search)
            }
        }
        // A swipe in Quick Look goes on to the PDFs before and after in the list
        .background { PDFReader(url: $reading, items: shown.compactMap(\.pdf)) { store.refresh() } }
        .confirmationDialog("Delete \(deleting?.key ?? "")?",
                            isPresented: Binding { deleting != nil } set: { if !$0 { deleting = nil } },
                            titleVisibility: .visible) {
            Button("Delete Entry and Its Files", role: .destructive) {
                if let entry = deleting { store.delete(entry) }
            }
        } message: {
            Text("Its folder is deleted, with its files.")
        }
        .searchable(text: $search)
        .refreshable { await store.reload() }
        .navigationTitle(place.title)
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                if fetching {
                    ProgressView()
                } else {
                    Button("Add", systemImage: "plus") {
                        identifier = ""
                        adding = true
                    }
                    .keyboardShortcut("n")
                }
                Menu {
                    Picker("Sort", selection: $store.order) {
                        ForEach(Order.allCases) { Text($0.label).tag($0) }
                    }
                } label: {
                    Label("Sort", systemImage: "arrow.up.arrow.down")
                }
                ShareLink(item: BibFile(name: place.title, text: bibtex(inPlace.map(\.entry.bib))),
                          preview: SharePreview("\(place.title).bib")) {
                    Label("Export", systemImage: "square.and.arrow.up")
                }
                .disabled(inPlace.isEmpty)
            }
        }
        .alert("Add a Paper", isPresented: $adding) {
            TextField("arXiv number, DOI, texkey, or BibTeX", text: $identifier)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
            Button("Cancel", role: .cancel) {}
            Button("Add") { add() }
        } message: {
            Text("From INSPIRE, or from doi.org for a DOI INSPIRE does not have. Anything else: paste its BibTeX.")
        }
    }

    private func add() {
        let id = identifier.trimmed
        guard !id.isEmpty else { return }
        fetching = true
        Task {
            if let key = await store.add(id) {
                search = ""
                selection = key
            }
            fetching = false
        }
    }
}

/// The menu of a row: what the entry's page does most, without opening it, as the key while writing
extension EntryList {
    @ViewBuilder
    func menu(_ row: Row) -> some View {
        if let pdf = row.pdf {
            Button("Read", systemImage: "book") { reading = pdf }
            ShareLink(item: pdf) { Label("Share PDF", systemImage: "square.and.arrow.up") }
        }
        Button("Copy Key", systemImage: "doc.on.doc") { UIPasteboard.general.string = row.entry.key }
        Button("Copy BibTeX", systemImage: "doc.on.clipboard") {
            UIPasteboard.general.string = row.entry.bib.bibtex
        }
        // In a tag's place, so that taking an entry out of it is not deleting it
        if case .tag(let tag) = place {
            Button("Remove from Tag", systemImage: "tag.slash") {
                store.edit(row.entry) { e in e.setTags(e.tags.filter { $0 != tag }) }
            }
        }
        Button("Delete…", systemImage: "trash", role: .destructive) { deleting = row.entry }
    }
}

/// A PDF's first page, made when the menu shows it: its body is not made before
private struct FirstPage: View {
    let url: URL

    var body: some View {
        if let page = PDFDocument(url: url)?.page(at: 0)?.thumbnail(of: CGSize(width: 600, height: 800), for: .mediaBox) {
            Image(uiImage: page)
                .resizable()
                .scaledToFit()
                .frame(width: 300)
        } else {
            Image(systemName: "doc.richtext")
                .font(.largeTitle)
                .padding(40)
        }
    }
}

private struct EntryRow: View {
    let row: Row

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(row.title)
                    .lineLimit(3)
                Text(row.subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if row.pdf != nil {
                Image(systemName: "doc.richtext")
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("PDF")
            }
        }
    }
}

/// A `.bib`, written when it is shared
struct BibFile: Transferable {
    let name: String
    let text: String

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(exportedContentType: UTType(filenameExtension: "bib") ?? .plainText) { file in
            let url = FileManager.default.temporaryDirectory.appendingPathComponent("\(file.name.replacingOccurrences(of: "/", with: "-")).bib")
            try Data(file.text.utf8).write(to: url)
            return SentTransferredFile(url)
        }
    }
}
