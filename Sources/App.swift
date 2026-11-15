// The app: the library's state (`Store`), the folder and its bookmark, the places, the list,
// adding, and exporting

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

/// A tag of the library, and how many entries have it
struct TagCount: Hashable, Identifiable {
    let tag: String
    let count: Int
    var id: String { tag }
}

/// An entry's row in the list, made when the library is read: the title is TeX made plain, and
/// whether it has a PDF is the folder's
struct Row: Identifiable {
    let entry: Entry
    let title: String
    let subtitle: String
    let hasPDF: Bool
    /// What finding looks in, folded
    let text: String
    /// What it is sorted by, in each order
    let sortKeys: [Order: SortKey]
    var id: String { entry.key }

    init(_ entry: Entry) {
        self.entry = entry
        title = entry.title.isEmpty ? entry.key : entry.title
        subtitle = [entry.byline, entry.year ?? ""].filter { !$0.isEmpty }.joined(separator: " · ")
        hasPDF = entry.pdf() != nil
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
    /// By key
    private(set) var entries: [Entry] = []
    /// In `order`
    private(set) var rows: [Row] = []
    /// How the list is sorted, kept for the next time
    var order: Order {
        didSet {
            defaults.set(order.rawValue, forKey: "order")
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

    private let defaults = UserDefaults.standard
    /// Counts the reads begun: one overtaken by a newer one is dropped
    private var started = 0

    init() {
        order = Order(rawValue: UserDefaults.standard.string(forKey: "order") ?? "") ?? .year
        guard let bookmark = defaults.data(forKey: "folder") else { return }
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
        entries = []
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
            defaults.set(bookmark, forKey: "folder")
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
            entries = library.entries
            rows = sorted(read)
            tags = tagCounts(library.entries).map { TagCount(tag: $0.tag, count: $0.count) }
            unreadable = library.unreadable.count
        case .failure:
            error = "The library folder cannot be read."
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
        entries.first { $0.key == key }
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

    /// Changes an entry as it is on disk now, not as it was last read, and writes it
    func edit(_ entry: Entry, _ edit: (inout Entry) -> Void) {
        change("the entry") {
            var current = try Entry.read(entry.dir)
            let before = current
            edit(&current)
            if current != before { try current.write() }
        }
    }

    /// Adds the paper an identifier finds. Its key, or nil if it was not added
    func add(_ id: String) async -> String? {
        guard let folder else {
            choosingFolder = true
            return nil
        }
        do {
            let bib = try await Fetch.find(id)
            let entry = try await Task.detached { try addEntry(folder, bib) }.value
            await reload()
            return entry.key
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
func message(_ error: Error, _ what: String) -> String {
    if let error = error as? LocalizedError, error is CoreError || error is Failure {
        return error.errorDescription ?? "Could not write \(what)."
    }
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

    var body: some View {
        @Bindable var store = store
        NavigationSplitView {
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

    var body: some View {
        List(selection: $place) {
            Label("All Entries", systemImage: "books.vertical")
                .badge(store.entries.count)
                .tag(Place.all)
            if !store.tags.isEmpty {
                Section("Tags") {
                    ForEach(store.tags) { t in
                        Label(t.tag, systemImage: "tag")
                            .badge(t.count)
                            .tag(Place.tag(t.tag))
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

    private var inPlace: [Row] { store.rows.filter { place.has($0.entry) } }

    var body: some View {
        @Bindable var store = store
        let shown = inPlace.filter { hasWords($0.text, search) }
        List(shown, selection: $selection) { row in
            EntryRow(row: row).tag(row.id)
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
            TextField("arXiv number, DOI, or texkey", text: $identifier)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
            Button("Cancel", role: .cancel) {}
            Button("Add") { add() }
        } message: {
            Text("From INSPIRE, or from doi.org for a DOI INSPIRE does not have.")
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
            if row.hasPDF {
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
