// The Refdir format (refdir's FORMAT.md): BibTeX entries, an entry's folder, the library, adding,
// files, names, plain text, and finding. Foundation only, no networking, so that it builds and is
// tested on Linux: the tests are at the end, and run cases.json, refdir's examples

import Foundation

/// The file of an entry's BibTeX
let entryFile = "entry.bib"

extension StringProtocol {
    var trimmed: String { trimmingCharacters(in: .whitespacesAndNewlines) }
}

enum CoreError: LocalizedError {
    case notBibTeX(String)
    case entries(Int)
    case same(String)
    case notKey(String)
    case noFreeKey

    var errorDescription: String? {
        switch self {
        case .notBibTeX(let why): "Not BibTeX: \(why)."
        case .entries(let n): "\(entryFile) has \(n) entries, not one."
        case .same(let key): "The library has it, as \(key)."
        case .notKey(let key): "“\(key)” cannot be a key."
        case .noFreeKey: "No free key."
        }
    }
}

// MARK: - BibTeX

/// A part of a field's value: text, written in braces, or a macro, such as `jun`
enum Chunk: Equatable {
    case text(String)
    case macro(String)
}

struct Field: Equatable {
    var name: String
    var value: [Chunk]
}

/// One BibTeX entry, its fields in their order and their values as written
struct Bib: Equatable {
    var kind: String
    var key: String
    var fields: [Field]

    /// The entries of a BibTeX text. `@string`, `@comment`, and `@preamble` are not entries
    static func parse(_ text: String) throws -> [Bib] {
        var reader = BibReader(s: Array(text))
        return try reader.entries()
    }

    /// A field's value as written, its macros by name; nil if it is missing or blank
    func get(_ name: String) -> String? {
        guard let field = fields.first(where: { $0.name.caseInsensitiveEquals(name) }) else { return nil }
        let text = field.value.map { chunk in
            switch chunk {
            case .text(let t), .macro(let t): t
            }
        }.joined().trimmed
        return text.isEmpty ? nil : text
    }

    /// Sets a field to a text, in its place or at the end; an empty text removes it
    mutating func set(_ name: String, _ value: String) {
        let at = fields.firstIndex { $0.name.caseInsensitiveEquals(name) }
        switch (at, value.trimmed.isEmpty) {
        case (let i?, true): fields.remove(at: i)
        case (let i?, false): fields[i].value = [.text(value)]
        case (nil, false): fields.append(Field(name: name, value: [.text(value)]))
        case (nil, true): break
        }
    }

    /// The entry as BibTeX, one field a line, ending with a line break
    var bibtex: String {
        let lines = fields.map { field in
            let parts = field.value.map { chunk in
                switch chunk {
                case .text(let t): "{\(t)}"
                case .macro(let m): m
                }
            }
            return "    \(field.name) = \(parts.joined(separator: " # ")),\n"
        }
        return "@\(kind){\(key),\n\(lines.joined())}\n"
    }
}

/// The library's `.bib`: the entries, separated by empty lines
func bibtex(_ bibs: [Bib]) -> String {
    bibs.map(\.bibtex).joined(separator: "\n")
}

/// Reads BibTeX as BibTeX does: what is outside an entry is a comment
private struct BibReader {
    let s: [Character]
    var i = 0

    var atEnd: Bool { i >= s.count }

    mutating func entries() throws -> [Bib] {
        var bibs: [Bib] = []
        while let at = s[i...].firstIndex(of: "@") {
            i = at + 1
            skipSpace()
            let kind = name().lowercased()
            skipSpace()
            guard !atEnd, s[i] == "{" || s[i] == "(" else { throw CoreError.notBibTeX("no “{” after @\(kind)") }
            let close: Character = s[i] == "{" ? "}" : ")"
            i += 1
            switch kind {
            case "comment", "preamble", "string":
                try skipGroup(close)
            default:
                bibs.append(try entry(kind, close))
            }
        }
        return bibs
    }

    private mutating func entry(_ kind: String, _ close: Character) throws -> Bib {
        skipSpace()
        var key = ""
        while !atEnd, s[i] != ",", s[i] != close, !s[i].isWhitespace {
            key.append(s[i])
            i += 1
        }
        skipSpace()
        var bib = Bib(kind: kind, key: key, fields: [])
        guard !atEnd else { throw CoreError.notBibTeX("@\(kind){\(key) is not closed") }
        if s[i] == close {
            i += 1
            return bib
        }
        guard s[i] == ",", !key.isEmpty else { throw CoreError.notBibTeX("a key cannot be “\(key) …”") }
        i += 1
        while true {
            skipSpace()
            guard !atEnd else { throw CoreError.notBibTeX("\(key) is not closed") }
            if s[i] == close {
                i += 1
                return bib
            }
            let field = name()
            skipSpace()
            guard !field.isEmpty, !atEnd, s[i] == "=" else { throw CoreError.notBibTeX("no “=” after \(field) in \(key)") }
            i += 1
            var value: [Chunk] = []
            repeat {
                skipSpace()
                value.append(try chunk())
                skipSpace()
            } while !atEnd && s[i] == "#" && { i += 1; return true }()
            bib.fields.append(Field(name: field, value: value))
            guard !atEnd else { throw CoreError.notBibTeX("\(key) is not closed") }
            if s[i] == "," { i += 1 } else if s[i] != close { throw CoreError.notBibTeX("no “,” after \(field) in \(key)") }
        }
    }

    /// `{…}`, `"…"`, a number, or a macro
    private mutating func chunk() throws -> Chunk {
        guard !atEnd else { throw CoreError.notBibTeX("a value is missing") }
        switch s[i] {
        case "{":
            i += 1
            let start = i
            try skipGroup("}")
            return .text(String(s[start..<i - 1]))
        case "\"":
            i += 1
            let start = i
            var depth = 0
            while !atEnd, depth > 0 || s[i] != "\"" {
                if s[i] == "{" { depth += 1 } else if s[i] == "}" { depth -= 1 }
                i += 1
            }
            guard !atEnd else { throw CoreError.notBibTeX("a quote is not closed") }
            i += 1
            return .text(String(s[start..<i - 1]))
        default:
            let word = name()
            guard !word.isEmpty else { throw CoreError.notBibTeX("a value is missing") }
            return word.allSatisfy(\.isNumber) ? .text(word) : .macro(word)
        }
    }

    /// Up to `close`, outside braces
    private mutating func skipGroup(_ close: Character) throws {
        var depth = 0
        while !atEnd {
            let c = s[i]
            i += 1
            if c == close && depth == 0 { return }
            if c == "{" { depth += 1 } else if c == "}" { depth -= 1 }
        }
        throw CoreError.notBibTeX("a brace is not closed")
    }

    private mutating func name() -> String {
        var out = ""
        while !atEnd, !s[i].isWhitespace, !"{}(),=#\"%".contains(s[i]) {
            out.append(s[i])
            i += 1
        }
        return out
    }

    private mutating func skipSpace() {
        while !atEnd && s[i].isWhitespace { i += 1 }
    }
}

// MARK: - Entries

/// Whether a citation key can be an entry's: no white space, none of `, { } " # % \ /`, and not
/// starting with `.`
func isKey(_ key: String) -> Bool {
    !key.isEmpty && !key.hasPrefix(".") && !key.contains { $0.isWhitespace || ",{}\"#%\\/".contains($0) }
}

/// An entry: its folder, whose name is its key, and its BibTeX
struct Entry: Equatable {
    var dir: URL
    var bib: Bib

    var key: String { bib.key }

    /// Reads the entry in `dir`. Its key is the folder's name, whatever `entry.bib` has
    static func read(_ dir: URL) throws -> Entry {
        let data = try Data(contentsOf: dir.appendingPathComponent(entryFile))
        guard let text = String(data: data, encoding: .utf8) else { throw CoreError.notBibTeX("not UTF-8") }
        var bibs = try Bib.parse(text)
        guard bibs.count == 1 else { throw CoreError.entries(bibs.count) }
        bibs[0].key = dir.lastPathComponent
        return Entry(dir: dir, bib: bibs[0])
    }

    /// Writes `entry.bib`, through a `.` file in the folder
    func write() throws {
        try writeAtomic(dir.appendingPathComponent(entryFile), Data(bib.bibtex.utf8))
    }

    /// `keywords`, split at commas
    var tags: [String] {
        (bib.get("keywords") ?? "").split(separator: ",").map { String($0).trimmed }.filter { !$0.isEmpty }
    }

    mutating func setTags(_ tags: [String]) {
        bib.set("keywords", tags.joined(separator: ", "))
    }

    /// `comment`
    var note: String { bib.get("comment") ?? "" }

    mutating func setNote(_ note: String) {
        bib.set("comment", note)
    }

    /// The other files of the folder, but those starting with `.`, by name
    func files() -> [String] {
        let fm = FileManager.default
        let names = (try? fm.contentsOfDirectory(atPath: dir.path)) ?? []
        return refdirFiles(names.filter { name in
            var isDir: ObjCBool = false
            return fm.fileExists(atPath: dir.appendingPathComponent(name).path, isDirectory: &isDir) && !isDir.boolValue
        })
    }

    /// The first PDF of its files, by name
    func pdf() -> URL? {
        files().first(where: isPDF).map { dir.appendingPathComponent($0) }
    }

    /// Writes a file into the folder, named `name`, or with `-2`, `-3`, … if it is taken
    @discardableResult
    func addFile(_ name: String, _ data: Data) throws -> URL {
        let names = try FileManager.default.contentsOfDirectory(atPath: dir.path)
        let url = dir.appendingPathComponent(freeName(name, taken: names))
        try writeAtomic(url, data)
        return url
    }

    /// The title as plain text
    var title: String { bib.get("title").map(plain) ?? "" }

    private var nameList: [String] { splitAnd(bib.get("author") ?? bib.get("editor") ?? "") }

    /// The authors, or else the editors, by their short names: `J.M. Maldacena`. `others` is not one
    var names: [String] { nameList.filter { $0 != "others" }.map(shortName) }

    /// Whether the list of names ends with `and others`
    var hasOthers: Bool { nameList.last == "others" }

    /// The first author's (or editor's) family name, as plain text; a name in braces whole
    var firstFamily: String? {
        guard let first = nameList.first else { return nil }
        let braced = first.hasPrefix("{") && first.hasSuffix("}")
        let family = plain(braced ? first : familyGiven(first).family)
        return family.isEmpty ? nil : family
    }

    /// The first author's short name, and ` et al.` if there are more
    var byline: String {
        let names = nameList
        let first = names.first.map(shortName) ?? ""
        return names.count > 1 ? "\(first) et al." : first
    }

    var year: String? { bib.get("year") ?? bib.get("date").map { String($0.prefix(4)) } }

    /// What finding looks in, folded: the title, the names, the key, the journal, the year, the
    /// tags, and the note
    var searchText: String {
        let fields = ["title", "author", "editor", "journal", "booktitle", "doi", "eprint", "keywords", "comment"]
        var text = fields.compactMap { bib.get($0) }
        text += [title, names.joined(separator: " "), key]
        if let year { text.append(year) }
        return fold(text.joined(separator: "\n"))
    }
}

/// Whether every word of `search`, folded, is in `text`, folded already (`Entry.searchText`)
func hasWords(_ text: String, _ search: String) -> Bool {
    fold(search).split(whereSeparator: \.isWhitespace).allSatisfy { text.contains($0) }
}

// MARK: - The library

/// A library's entries, by key, and the folders with an `entry.bib` that cannot be read
struct Library {
    var entries: [Entry] = []
    var unreadable: [(dir: URL, error: String)] = []

    static func read(_ folder: URL) throws -> Library {
        var library = Library()
        let fm = FileManager.default
        for name in try fm.contentsOfDirectory(atPath: folder.path) where !name.hasPrefix(".") {
            let dir = folder.appendingPathComponent(name)
            var isDir: ObjCBool = false
            let exists = fm.fileExists(atPath: dir.appendingPathComponent(entryFile).path, isDirectory: &isDir)
            guard exists && !isDir.boolValue else { continue }
            do {
                library.entries.append(try Entry.read(dir))
            } catch {
                library.unreadable.append((dir, error.localizedDescription))
            }
        }
        library.entries.sort { byName($0.key, $1.key) }
        return library
    }
}

/// The entry that is the same paper as `bib`: by its DOI, or its arXiv number, ignoring ASCII case
func samePaper(_ entries: [Entry], _ bib: Bib) -> Entry? {
    func same(_ field: String) -> Entry? {
        guard let value = bib.get(field) else { return nil }
        return entries.first { $0.bib.get(field)?.caseInsensitiveEquals(value) == true }
    }
    return same("doi") ?? same("eprint")
}

/// The key a new entry for `bib` takes, among `entries` in folders named `folders`: its key, or
/// with `a`, `b`, … after it if a folder has it, ignoring ASCII case. Refuses the same paper
func newKey(_ entries: [Entry], folders: [String], _ bib: Bib) throws -> String {
    if let entry = samePaper(entries, bib) { throw CoreError.same(entry.key) }
    guard isKey(bib.key) else { throw CoreError.notKey(bib.key) }
    let keys = [bib.key] + "abcdefghijklmnopqrstuvwxyz".map { bib.key + String($0) }
    guard let key = keys.first(where: { k in !folders.contains { $0.caseInsensitiveEquals(k) } }) else {
        throw CoreError.noFreeKey
    }
    return key
}

/// Adds an entry to the library for `bib`, under a free key (`newKey`)
func addEntry(_ folder: URL, _ bib: Bib) throws -> Entry {
    let library = try Library.read(folder)
    var bib = bib
    bib.key = try newKey(library.entries, folders: try FileManager.default.contentsOfDirectory(atPath: folder.path), bib)
    let dir = folder.appendingPathComponent(bib.key)
    try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: false)
    let entry = Entry(dir: dir, bib: bib)
    try entry.write()
    return entry
}

/// How entries are listed
enum Order: String, CaseIterable, Identifiable {
    /// Newest first
    case year
    case title
    /// By the first author's family name
    case author
    case key

    var id: String { rawValue }

    var label: String {
        switch self {
        case .year: "Newest First"
        case .title: "Title"
        case .author: "First Author"
        case .key: "Key"
        }
    }
}

/// What an entry is sorted by, in an order: whether it lacks it, its year, or its text, folded
struct SortKey {
    let missing: Bool
    let year: Int
    let text: String
    let key: String

    init(_ entry: Entry, _ order: Order) {
        key = entry.key
        var year: Int?
        var text: String?
        switch order {
        case .year: year = entry.year.flatMap { Int($0.prefix { $0.isASCII && $0.isNumber }) }
        case .title: text = fold(entry.title)
        case .author: text = entry.firstFamily.map(fold)
        case .key: break
        }
        switch order {
        case .year: missing = year == nil
        case .title, .author: missing = text?.isEmpty ?? true
        case .key: missing = false
        }
        self.year = year ?? 0
        self.text = text ?? ""
    }

    /// Without it last, then newest first, then by text in code points, then by key
    static func < (a: SortKey, b: SortKey) -> Bool {
        if a.missing != b.missing { return !a.missing }
        if a.year != b.year { return a.year > b.year }
        if !a.text.unicodeScalars.elementsEqual(b.text.unicodeScalars) {
            return a.text.unicodeScalars.lexicographicallyPrecedes(b.text.unicodeScalars) { $0.value < $1.value }
        }
        return byName(a.key, b.key)
    }
}

/// Sorts entries: by year, newest first; by title or by the first author's family name, folded; or
/// by key. Entries without what they are sorted by go last; ties go by key
func sortEntries(_ entries: [Entry], by order: Order) -> [Entry] {
    entries.map { (SortKey($0, order), $0) }.sorted { $0.0 < $1.0 }.map(\.1)
}

/// Adds an entry for each, as `addEntry`: those added, and why the others were not, by key
func addEntries(_ folder: URL, _ bibs: [Bib]) -> (added: [Entry], refused: [(key: String, why: String)]) {
    var added: [Entry] = []
    var refused: [(key: String, why: String)] = []
    for bib in bibs {
        do {
            added.append(try addEntry(folder, bib))
        } catch {
            refused.append((bib.key, error.localizedDescription))
        }
    }
    return (added, refused)
}

/// Whether a text can be a tag: not blank, and without a comma, which separates tags
func isTag(_ tag: String) -> Bool {
    !tag.trimmed.isEmpty && !tag.contains(",")
}

/// Tags with `old` renamed to `new`, or removed if `new` is blank; a tag the list then has twice
/// is kept in its first place. nil if it does not have `old`
func renamed(_ tags: [String], _ old: String, _ new: String) -> [String]? {
    guard tags.contains(old) else { return nil }
    let new = new.trimmed
    var out: [String] = []
    for tag in tags.map({ $0 == old ? new : $0 }) where !tag.isEmpty && !out.contains(tag) {
        out.append(tag)
    }
    return out
}

/// The entries that have the tag `old`, with it renamed to `new`, or removed if `new` is blank
func renameTag(_ entries: [Entry], _ old: String, _ new: String) -> [Entry] {
    entries.compactMap { entry in
        guard let tags = renamed(entry.tags, old, new) else { return nil }
        var entry = entry
        entry.setTags(tags)
        return entry
    }
}

/// A tag of the library, and how many entries have it
struct TagCount: Hashable, Identifiable {
    let tag: String
    var count: Int
    var id: String { tag }
}

/// Every tag of the entries, by name, with how many have it
func tagCounts(_ entries: [Entry]) -> [TagCount] {
    var counts: [TagCount] = []
    for tag in entries.flatMap(\.tags) {
        if let i = counts.firstIndex(where: { $0.tag == tag }) {
            counts[i].count += 1
        } else {
            counts.append(TagCount(tag: tag, count: 1))
        }
    }
    return counts.sorted { byName($0.tag, $1.tag) }
}

/// An entry's files among the names in its folder: not `entry.bib`, not starting with `.`, by
/// name
func refdirFiles(_ names: [String]) -> [String] {
    names.filter { $0 != entryFile && !$0.hasPrefix(".") }.sorted(by: byName)
}

func isPDF(_ name: String) -> Bool {
    guard let dot = name.lastIndex(of: ".") else { return false }
    return name[name.index(after: dot)...].caseInsensitiveEquals("pdf")
}

/// A file's name, without a leading `.`, or with `-2`, `-3`, … before its extension if one of
/// `taken` has it, ignoring ASCII case
func freeName(_ name: String, taken: [String]) -> String {
    let name = String(name.drop { $0 == "." })
    var (stem, ext) = (name, "")
    if let dot = name.lastIndex(of: "."), dot != name.startIndex {
        (stem, ext) = (String(name[..<dot]), String(name[dot...]))
    }
    var candidate = name
    var n = 1
    while taken.contains(where: { $0.caseInsensitiveEquals(candidate) }) {
        n += 1
        candidate = "\(stem)-\(n)\(ext)"
    }
    return candidate
}

/// Writes a file through a `.` file next to it, then a rename
func writeAtomic(_ url: URL, _ data: Data) throws {
    let tmp = url.deletingLastPathComponent()
        .appendingPathComponent(".\(url.lastPathComponent).\(ProcessInfo.processInfo.processIdentifier).tmp")
    do {
        try data.write(to: tmp)
        guard rename(tmp.path, url.path) == 0 else {
            throw CocoaError(.fileWriteUnknown, userInfo: [NSFilePathErrorKey: url.path])
        }
    } catch {
        try? FileManager.default.removeItem(at: tmp)
        throw error
    }
}

// MARK: - Names and order

extension StringProtocol {
    /// Equal, ignoring ASCII case
    func caseInsensitiveEquals<T: StringProtocol>(_ other: T) -> Bool {
        asciiLowercased == other.asciiLowercased
    }

    var asciiLowercased: [Unicode.Scalar] {
        unicodeScalars.map { ("A"..."Z").contains($0) ? Unicode.Scalar($0.value + 32)! : $0 }
    }
}

/// Names in order: ASCII letters in lower case, then by code point
func byName(_ a: String, _ b: String) -> Bool {
    let (la, lb) = (a.asciiLowercased, b.asciiLowercased)
    if la != lb { return la.lexicographicallyPrecedes(lb) { $0.value < $1.value } }
    return a.unicodeScalars.lexicographicallyPrecedes(b.unicodeScalars) { $0.value < $1.value }
}

/// The names of a BibTeX name list, split at ` and ` outside braces, ignoring ASCII case
func splitAnd(_ list: String) -> [String] {
    let bytes = Array(list.utf8)
    var (names, depth, start) = ([String](), 0, 0)
    let and = Array(" and ".utf8)
    var i = 0
    while i < bytes.count {
        switch bytes[i] {
        case UInt8(ascii: "{"): depth += 1
        case UInt8(ascii: "}"): depth -= 1
        default:
            if depth == 0, bytes.count - i > 5,
               zip(bytes[i..<i + 5], and).allSatisfy({ ($0 | 0x20) == ($1 | 0x20) }) {
                names.append(String(decoding: bytes[start..<i], as: UTF8.self).trimmed)
                start = i + 5
            }
        }
        i += 1
    }
    names.append(String(decoding: bytes[start...], as: UTF8.self).trimmed)
    return names.filter { !$0.isEmpty }
}

/// `Maldacena, Juan Martin` → `J.M. Maldacena`; `Juan Maldacena` → `J. Maldacena`; a name in
/// braces, such as `{ATLAS Collaboration}`, as it is
func shortName(_ name: String) -> String {
    if name.hasPrefix("{") && name.hasSuffix("}") { return plain(name) }
    let (family, given) = familyGiven(name)
    // `Jean-Pierre` → `J.-P.`; `S.W.` → `S.W.`
    func initial(_ word: Substring) -> String {
        word.split(separator: "-", omittingEmptySubsequences: false).map { part in
            part.split(separator: ".").compactMap { $0.trimmed.first.map { "\($0)." } }.joined()
        }.joined(separator: "-")
    }
    let initials = given.split(whereSeparator: \.isWhitespace).map(initial).joined()
    let short = plain(family)
    return initials.isEmpty ? short : "\(initials) \(short)"
}

/// A name's family and given names: `Family, Given` or `Given Family`
func familyGiven(_ name: String) -> (family: String, given: String) {
    if let comma = name.firstIndex(of: ",") {
        return (name[..<comma].trimmed, name[name.index(after: comma)...].trimmed)
    }
    if let space = name.lastIndex(of: " ") {
        return (String(name[name.index(after: space)...]), String(name[..<space]))
    }
    return (name, "")
}

/// For finding: decomposed, without combining marks, in lower case (`Gürsey` → `gursey`)
func fold(_ text: String) -> String {
    let marks: Set<Unicode.GeneralCategory> = [.nonspacingMark, .spacingMark, .enclosingMark]
    var scalars = String.UnicodeScalarView()
    scalars.append(contentsOf: text.decomposedStringWithCanonicalMapping.unicodeScalars.filter {
        !marks.contains($0.properties.generalCategory)
    })
    return String(scalars).lowercased()
}

// MARK: - Identifiers

/// INSPIRE's query for an identifier: `recid:N` for an INSPIRE link, `arxiv:<id>` for an arXiv
/// link, `doi:<doi>` for a doi.org link. Anything else as it is: INSPIRE's search finds a bare arXiv
/// number, DOI, or texkey
func inspireQuery(_ id: String) -> String {
    let id = id.trimmed
    func after(_ prefix: String) -> String? {
        guard let range = id.range(of: prefix) else { return nil }
        var rest = String(id[range.upperBound...])
        if let end = rest.firstIndex(where: { $0 == "?" || $0 == "#" }) { rest = String(rest[..<end]) }
        while rest.hasSuffix("/") { rest.removeLast() }
        while rest.hasSuffix(".pdf") { rest.removeLast(4) }
        return rest
    }
    if let recid = after("inspirehep.net/literature/") { return "recid:\(recid)" }
    if let arxiv = after("arxiv.org/abs/") ?? after("arxiv.org/pdf/") { return "arxiv:\(arxiv)" }
    if let doi = after("doi.org/") { return "doi:\(doi)" }
    return id
}

/// The DOI a query asks for, if it is one: `doi:…`, or a bare `10.…/…`
func doiOf(query q: String) -> String? {
    if q.hasPrefix("doi:") { return String(q.dropFirst(4)) }
    return q.hasPrefix("10.") && q.contains("/") ? q : nil
}

/// A month as BibTeX's macro: `June` → `jun`
func month(_ name: String) -> String {
    String(name.prefix(3)).lowercased()
}

/// doi.org's months as BibTeX's macros
func normalizeMonths(_ bib: Bib) -> Bib {
    var bib = bib
    for i in bib.fields.indices where bib.fields[i].name.caseInsensitiveEquals("month") {
        bib.fields[i].value = bib.fields[i].value.map {
            if case .macro(let m) = $0 { .macro(month(m)) } else { $0 }
        }
    }
    return bib
}

/// INSPIRE's query for the paper, by its arXiv number or DOI
func paperQuery(_ bib: Bib) -> String? {
    bib.get("eprint").map { "arxiv:\($0)" } ?? bib.get("doi").map { "doi:\($0)" }
}

/// The paper on INSPIRE
func inspireURL(_ bib: Bib) -> URL? {
    paperQuery(bib).flatMap { URL(string: "https://inspirehep.net/search?p=\(percentEncoded($0))") }
}

/// The paper got again, with the entry's key, tags, and note
func keepingOwn(_ new: Bib, of entry: Entry) -> Bib {
    var bib = new
    bib.key = entry.key
    for field in ["keywords", "comment"] {
        bib.set(field, entry.bib.get(field) ?? "")
    }
    return bib
}

/// Percent-encodes all but the unreserved characters, `+` included, which INSPIRE reads as a space
func percentEncoded(_ s: String) -> String {
    s.utf8.map { b -> String in
        switch b {
        case UInt8(ascii: "A")...UInt8(ascii: "Z"), UInt8(ascii: "a")...UInt8(ascii: "z"),
             UInt8(ascii: "0")...UInt8(ascii: "9"), UInt8(ascii: "-"), UInt8(ascii: "."), UInt8(ascii: "_"),
             UInt8(ascii: "~"):
            String(UnicodeScalar(b))
        default:
            String(format: "%%%02X", b)
        }
    }.joined()
}

// MARK: - Plain text

/// TeX as plain text: `{The Large $N$ limit}` → `The Large N limit`, `$H\to\gamma\gamma$` → `H→γγ`.
/// An odd `$` is a dollar, not math
func plain(_ tex: String) -> String {
    var parser = TeX(tex)
    let out = parser.convert(math: false, closing: nil)
    return out.precomposedStringWithCanonicalMapping.split(whereSeparator: \.isWhitespace).joined(separator: " ")
}

/// A small converter of TeX to text: commands become their symbols, scripts become Unicode
/// superscripts and subscripts where there are some, and accents become combining marks
private struct TeX {
    let s: [Character]
    var i = 0
    /// The indices of the `$` that open or close math. An odd one is a dollar
    let toggles: Set<Int>

    init(_ text: String) {
        let chars = Array(text)
        s = chars
        var dollars = chars.indices.filter { chars[$0] == "$" && !TeX.escaped(chars, $0) }
        if dollars.count % 2 == 1 { dollars.removeLast() }
        toggles = Set(dollars)
    }

    /// Whether the character at `i` follows an odd number of backslashes
    private static func escaped(_ s: [Character], _ i: Int) -> Bool {
        var n = 0
        var j = i - 1
        while j >= 0 && s[j] == "\\" { n += 1; j -= 1 }
        return n % 2 == 1
    }

    var atEnd: Bool { i >= s.count }

    /// Converts until `closing` (a `}`), or the end. A `$` toggles math
    mutating func convert(math startMath: Bool, closing: Character?) -> String {
        var math = startMath
        var out = ""
        while !atEnd {
            let c = s[i]
            if c == closing { i += 1; return out }
            if c == "$" && toggles.contains(i) {
                i += 1
                math.toggle()
                continue
            }
            switch c {
            case "\\": out += command(math: math)
            case "{":
                i += 1
                out += convert(math: math, closing: "}")
            case "}": i += 1
            case "^" where math, "_" where math:
                i += 1
                out += script(argument(math: true), up: c == "^")
            case "'" where math:
                i += 1
                out += "′"
            case "~" where math:
                i += 1
                out += " "
            case _ where math && c.isWhitespace:
                i += 1
            case "-" where !math && peek("---"):
                i += 3
                out += "—"
            case "-" where !math && peek("--"):
                i += 2
                out += "–"
            case "`" where !math && peek("``"):
                i += 2
                out += "“"
            case "'" where !math && peek("''"):
                i += 2
                out += "”"
            // Quotes, as TeX sets them
            case "'" where !math:
                i += 1
                out += "’"
            case "`" where !math:
                i += 1
                out += "‘"
            default:
                i += 1
                out.append(c)
            }
        }
        return out
    }

    private func peek(_ text: String) -> Bool {
        let t = Array(text)
        return i + t.count <= s.count && Array(s[i..<i + t.count]) == t
    }

    private mutating func skipSpaces() {
        while !atEnd && s[i].isWhitespace && s[i] != "\n" { i += 1 }
    }

    /// One argument: a group, a command, or a character
    private mutating func argument(math: Bool) -> String {
        skipSpaces()
        guard !atEnd else { return "" }
        switch s[i] {
        case "{":
            i += 1
            return convert(math: math, closing: "}")
        case "\\":
            return command(math: math)
        case "$" where toggles.contains(i), "}":
            return ""
        default:
            i += 1
            return String(s[i - 1])
        }
    }

    /// Skips an optional argument in brackets
    private mutating func skipOptional() {
        skipSpaces()
        guard !atEnd, s[i] == "[", let end = s[i...].firstIndex(of: "]") else { return }
        i = end + 1
    }

    /// The command at `i`, a backslash
    private mutating func command(math: Bool) -> String {
        i += 1
        guard !atEnd else { return "" }
        let first = s[i]
        guard first.isASCII && first.isLetter else {
            i += 1
            if let accent = TeX.accents[String(first)] { return accented(argument(math: math), accent) }
            switch first {
            case ",", ";", ":", " ", "\n", "\\": return " "
            case "!", "/", "-": return ""
            default: return String(first)
            }
        }
        var name = ""
        while !atEnd && s[i].isASCII && s[i].isLetter {
            name.append(s[i])
            i += 1
        }
        // TeX ends a command's name with the spaces after it
        skipSpaces()
        if let symbol = TeX.symbols[name] { return symbol }
        if let accent = TeX.accents[name] { return accented(argument(math: math), accent) }
        switch name {
        case "text", "textrm", "textit", "textbf", "textsf", "texttt", "textup", "textnormal", "textsc",
             "emph", "mbox", "hbox", "url":
            return argument(math: false)
        case "mathrm", "mathit", "mathbf", "mathsf", "mathtt", "mathcal", "mathscr", "mathfrak", "boldsymbol",
             "bm", "operatorname", "ensuremath":
            // Upright letters: the font is not kept
            return argument(math: math)
        case "mathbb":
            let letters: [Character: String] = ["R": "ℝ", "C": "ℂ", "N": "ℕ", "Z": "ℤ", "Q": "ℚ", "P": "ℙ"]
            return argument(math: math).map { letters[$0] ?? String($0) }.joined()
        case "frac", "dfrac", "tfrac":
            let (a, b) = (argument(math: true), argument(math: true))
            return "\(grouped(a))/\(grouped(b))"
        case "sqrt":
            skipOptional()
            return "√" + grouped(argument(math: true))
        case "not":
            return argument(math: math) + "\u{338}"
        case "left", "right", "bigl", "bigr", "Bigl", "Bigr", "big", "Big", "bigg", "Bigg", "middle":
            skipSpaces()
            if !atEnd && s[i] == "." { i += 1 }
            return ""
        case "cite", "ref", "label", "eqref", "footnote", "vspace", "hspace":
            _ = argument(math: math)
            return ""
        case "quad", "qquad", "space", "enspace", "thinspace", "medspace", "thickspace":
            return " "
        default:
            // Unknown: dropped, and its argument, if it has one, read as text
            return ""
        }
    }

    /// `a+b` → `(a+b)`, for a fraction or a root
    private func grouped(_ text: String) -> String {
        text.count > 1 && text.contains(where: { "+-−=/ ".contains($0) }) ? "(\(text))" : text
    }

    private func accented(_ text: String, _ mark: String) -> String {
        text.map { String($0) + mark }.joined()
    }

    /// `x` as a superscript or subscript: in Unicode if every character has one, else after `^` or
    /// `_`, in parentheses if it is longer than a character
    private func script(_ text: String, up: Bool) -> String {
        let text = text.trimmed
        if text.isEmpty { return "" }
        if up && ["′", "*", "∗", "†", "‡", "°", "∘"].contains(text) { return text == "∘" ? "°" : text }
        let table = up ? TeX.superscripts : TeX.subscripts
        let mapped = text.compactMap { table[$0] }
        if mapped.count == text.count { return String(mapped) }
        return (up ? "^" : "_") + (text.count > 1 ? "(\(text))" : text)
    }

    static let superscripts: [Character: Character] = {
        var t: [Character: Character] = [:]
        for (a, b) in zip("0123456789+-−=()niabcdefghijklmoprstuvwxyzABDEGHIJKLMNOPRTUVWαβγδεθιφχ",
                          "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁻⁼⁽⁾ⁿⁱᵃᵇᶜᵈᵉᶠᵍʰⁱʲᵏˡᵐᵒᵖʳˢᵗᵘᵛʷˣʸᶻᴬᴮᴰᴱᴳᴴᴵᴶᴷᴸᴹᴺᴼᴾᴿᵀᵁⱽᵂᵅᵝᵞᵟᵋᶿᶥᵠᵡ") {
            t[a] = b
        }
        return t
    }()

    static let subscripts: [Character: Character] = {
        var t: [Character: Character] = [:]
        for (a, b) in zip("0123456789+-−=()aehijklmnoprstuvxβγρφχ", "₀₁₂₃₄₅₆₇₈₉₊₋₋₌₍₎ₐₑₕᵢⱼₖₗₘₙₒₚᵣₛₜᵤᵥₓᵦᵧᵨᵩᵪ") {
            t[a] = b
        }
        return t
    }()

    /// Combining marks, by command
    static let accents: [String: String] = [
        "bar": "\u{304}", "overline": "\u{305}", "tilde": "\u{303}", "widetilde": "\u{303}", "hat": "\u{302}",
        "widehat": "\u{302}", "vec": "\u{20D7}", "dot": "\u{307}", "ddot": "\u{308}", "check": "\u{30C}",
        "breve": "\u{306}", "acute": "\u{301}", "grave": "\u{300}",
        "'": "\u{301}", "`": "\u{300}", "^": "\u{302}", "\"": "\u{308}", "~": "\u{303}", "=": "\u{304}",
        ".": "\u{307}", "u": "\u{306}", "v": "\u{30C}", "H": "\u{30B}", "c": "\u{327}", "r": "\u{30A}",
        "k": "\u{328}",
    ]

    static let symbols: [String: String] = [
        // Greek
        "alpha": "α", "beta": "β", "gamma": "γ", "delta": "δ", "epsilon": "ϵ", "varepsilon": "ε", "zeta": "ζ",
        "eta": "η", "theta": "θ", "vartheta": "ϑ", "iota": "ι", "kappa": "κ", "lambda": "λ", "mu": "μ",
        "nu": "ν", "xi": "ξ", "omicron": "ο", "pi": "π", "varpi": "ϖ", "rho": "ρ", "varrho": "ϱ",
        "sigma": "σ", "varsigma": "ς", "tau": "τ", "upsilon": "υ", "phi": "φ", "varphi": "φ", "chi": "χ",
        "psi": "ψ", "omega": "ω", "Gamma": "Γ", "Delta": "Δ", "Theta": "Θ", "Lambda": "Λ", "Xi": "Ξ",
        "Pi": "Π", "Sigma": "Σ", "Upsilon": "Υ", "Phi": "Φ", "Psi": "Ψ", "Omega": "Ω",
        // Arrows
        "to": "→", "rightarrow": "→", "leftarrow": "←", "gets": "←", "leftrightarrow": "↔",
        "Rightarrow": "⇒", "Leftarrow": "⇐", "Leftrightarrow": "⇔", "longrightarrow": "⟶",
        "longleftarrow": "⟵", "Longrightarrow": "⟹", "mapsto": "↦", "uparrow": "↑", "downarrow": "↓",
        "rightleftharpoons": "⇌", "iff": "⟺", "implies": "⟹",
        // Relations
        "le": "≤", "leq": "≤", "ge": "≥", "geq": "≥", "ll": "≪", "gg": "≫", "ne": "≠", "neq": "≠",
        "sim": "∼", "simeq": "≃", "approx": "≈", "cong": "≅", "equiv": "≡", "propto": "∝", "lesssim": "≲",
        "gtrsim": "≳", "lapprox": "≲", "gapprox": "≳", "in": "∈", "notin": "∉", "ni": "∋",
        "subset": "⊂", "supset": "⊃", "subseteq": "⊆", "supseteq": "⊇", "perp": "⊥", "parallel": "∥",
        "mid": "|", "vdash": "⊢", "models": "⊨", "prec": "≺", "succ": "≻",
        // Operators
        "times": "×", "pm": "±", "mp": "∓", "cdot": "·", "div": "÷", "ast": "∗", "star": "⋆",
        "circ": "∘", "bullet": "•", "otimes": "⊗", "oplus": "⊕", "ominus": "⊖", "odot": "⊙", "wedge": "∧",
        "land": "∧", "vee": "∨", "lor": "∨", "cup": "∪", "cap": "∩", "setminus": "∖", "dagger": "†",
        "ddagger": "‡", "sum": "∑", "prod": "∏", "coprod": "∐", "int": "∫", "iint": "∬", "iiint": "∭",
        "oint": "∮", "bigcup": "⋃", "bigcap": "⋂", "bigoplus": "⨁", "bigotimes": "⨂", "neg": "¬",
        "lnot": "¬", "forall": "∀", "exists": "∃", "nabla": "∇", "partial": "∂", "Box": "□", "square": "□",
        "triangle": "△", "wr": "≀", "rtimes": "⋊", "ltimes": "⋉",
        // Letters and other symbols
        "infty": "∞", "hbar": "ℏ", "ell": "ℓ", "Re": "ℜ", "Im": "ℑ", "aleph": "ℵ", "emptyset": "∅",
        "varnothing": "∅", "prime": "′", "degree": "°", "angle": "∠", "ldots": "…", "dots": "…",
        "cdots": "⋯", "vdots": "⋮", "ddots": "⋱", "langle": "⟨", "rangle": "⟩", "lfloor": "⌊",
        "rfloor": "⌋", "lceil": "⌈", "rceil": "⌉", "vert": "|", "Vert": "‖", "lbrace": "{", "rbrace": "}",
        "backslash": "∖", "S": "§", "P": "¶", "copyright": "©", "dag": "†", "ddag": "‡",
        "i": "ı", "j": "ȷ", "o": "ø", "O": "Ø", "ss": "ß", "ae": "æ", "AE": "Æ", "oe": "œ", "OE": "Œ",
        "aa": "å", "AA": "Å", "l": "ł", "L": "Ł", "textendash": "–", "textemdash": "—",
        "textasciitilde": "~", "textasciicircum": "^", "textbackslash": "\\", "texttimes": "×", "textpm": "±",
        "textless": "<", "textgreater": ">", "textunderscore": "_", "textbar": "|", "textellipsis": "…",
        "textquoteleft": "‘", "textquoteright": "’", "textquotedblleft": "“", "textquotedblright": "”",
        "textdegree": "°", "textmu": "µ", "textperthousand": "‰", "LaTeX": "LaTeX",
        "TeX": "TeX",
        // Functions, upright
        "sin": "sin", "cos": "cos", "tan": "tan", "cot": "cot", "sec": "sec", "csc": "csc", "arcsin": "arcsin",
        "arccos": "arccos", "arctan": "arctan", "sinh": "sinh", "cosh": "cosh", "tanh": "tanh", "exp": "exp",
        "log": "log", "ln": "ln", "lg": "lg", "det": "det", "dim": "dim", "ker": "ker", "deg": "deg",
        "arg": "arg", "max": "max", "min": "min", "sup": "sup", "inf": "inf", "lim": "lim", "limsup": "lim sup",
        "liminf": "lim inf", "Pr": "Pr", "gcd": "gcd", "hom": "hom", "Tr": "Tr", "tr": "tr",
        // Spacing and sizing, dropped
        "displaystyle": "", "textstyle": "", "scriptstyle": "", "scriptscriptstyle": "", "limits": "",
        "nolimits": "", "small": "", "large": "", "Large": "", "huge": "", "normalsize": "", "footnotesize": "",
        "scriptsize": "", "tiny": "", "noindent": "", "nonumber": "", "relax": "", "protect": "", "it": "",
        "rm": "", "bf": "", "sf": "", "tt": "", "em": "", "cal": "", "sc": "", "bigskip": " ", "medskip": " ",
        "smallskip": " ", "newline": " ", "par": " ", "cr": " ", "hfill": " ", "nobreak": "",
    ]
}

// MARK: - Tests

// Linux only: the app never builds this. Run with
//   swiftc -parse-as-library Sources/Core.swift -o /tmp/refdir-ios-test && /tmp/refdir-ios-test
// It runs cases.json of this project, or the file whose path it is given, such as refdir's
// tests/cases.json, to check that this copy is current
#if os(Linux)
@main
enum Tests {
    static var failures = 0
    static var checks = 0

    static func check(_ ok: Bool, _ message: @autoclosure () -> String) {
        checks += 1
        if !ok {
            failures += 1
            print("FAIL:", message())
        }
    }

    static func main() {
        let path = CommandLine.arguments.dropFirst().first
            ?? URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
                .appendingPathComponent("cases.json").path
        let data = try! Data(contentsOf: URL(fileURLWithPath: path))
        let cases = try! JSONSerialization.jsonObject(with: data) as! [String: Any]
        func section(_ name: String) -> [Any] {
            guard let s = cases[name] as? [Any] else { fatalError("no section \(name)") }
            return s
        }
        bibtexCases(section("bibtex"))
        for case let c as [Any] in section("keys") {
            check(isKey(c[0] as! String) == (c[1] as! Bool), "key \(c)")
        }
        for case let c as [String: Any] in section("names") {
            var e = entry("x", "@misc{x,}")
            for field in ["author", "editor"] {
                if let v = c[field] as? String { e.bib.set(field, v) }
            }
            check(e.names == c["names"] as! [String], "names \(c): \(e.names)")
            check(e.byline == c["byline"] as! String, "byline \(c): \(e.byline)")
        }
        for case let c as [String] in section("plain") {
            check(plain(c[0]) == c[1], "plain \(c[0]): \(plain(c[0]))")
        }
        for case let c as [String: Any] in section("tags") {
            var e = entry("x", "@misc{x,}")
            e.bib.set("keywords", c["keywords"] as! String)
            check(e.tags == c["tags"] as! [String], "tags \(c): \(e.tags)")
        }
        for case let c as [String: Any] in section("find") {
            let text = c["bibtex"] as! String
            let e = entry(try! Bib.parse(text)[0].key, text)
            for search in c["matches"] as! [String] { check(hasWords(e.searchText, search), "matches \(search)") }
            for search in c["not"] as! [String] { check(!hasWords(e.searchText, search), "does not match \(search)") }
        }
        addCases(section("add"))
        for case let c as [String: Any] in section("files") {
            let files = refdirFiles(c["names"] as! [String])
            check(files == c["files"] as! [String], "files \(c): \(files)")
            check(files.first(where: isPDF) == c["pdf"] as? String, "pdf \(c)")
        }
        for case let c as [String: Any] in section("free_names") {
            let free = freeName(c["name"] as! String, taken: c["taken"] as! [String])
            check(free == c["free"] as! String, "free name \(c): \(free)")
        }
        for case let c as [String] in section("query") {
            check(inspireQuery(c[0]) == c[1], "query \(c[0]): \(inspireQuery(c[0]))")
        }
        for case let c as [String] in section("months") {
            check(month(c[0]) == c[1], "month \(c)")
        }
        for case let c as [String: Any] in section("sort") {
            let entries = (c["entries"] as! [String: String]).map { entry($0.key, $0.value) }
            for order in Order.allCases {
                let keys = sortEntries(entries, by: order).map(\.key)
                check(keys == c[order.rawValue] as! [String], "sort by \(order): \(keys)")
            }
        }
        for case let c as [String: Any] in section("rename_tag") {
            let result = renamed(c["tags"] as! [String], c["old"] as! String, c["new"] as! String)
            check(result == c["renamed"] as? [String], "renamed \(c): \(String(describing: result))")
        }
        for case let c as [Any] in section("is_tag") {
            check(isTag(c[0] as! String) == (c[1] as! Bool), "tag \(c)")
        }
        folders()
        print("\(checks - failures)/\(checks) passed")
        exit(failures == 0 ? 0 : 1)
    }

    static func entry(_ key: String, _ text: String) -> Entry {
        var bib = try! Bib.parse(text)[0]
        bib.key = key
        return Entry(dir: URL(fileURLWithPath: "/nowhere").appendingPathComponent(key), bib: bib)
    }

    static func bibtexCases(_ cases: [Any]) {
        for case let c as [String: Any] in cases {
            let about = c["about"] as? String ?? ""
            guard let bibs = try? Bib.parse(c["bibtex"] as! String) else {
                check(c["error"] as? Bool == true, "parses: \(about)")
                continue
            }
            check(c["error"] == nil, "an error: \(about)")
            guard c["error"] == nil else { continue }
            let entries = c["entries"] as! [[String: Any]]
            check(bibs.count == entries.count, "entries: \(about)")
            for (bib, want) in zip(bibs, entries) {
                let fields = (want["fields"] as! [[Any]]).map { f in
                    Field(name: f[0] as! String, value: (f[1] as! [[String: String]]).map { chunk in
                        chunk["text"].map(Chunk.text) ?? .macro(chunk["macro"]!)
                    })
                }
                let expected = Bib(kind: want["kind"] as! String, key: want["key"] as! String, fields: fields)
                check(bib == expected, "\(about): \(bib)")
            }
            check(bibtex(bibs) == c["written"] as! String, "written: \(about): \(bibtex(bibs))")
        }
    }

    static func addCases(_ cases: [Any]) {
        for case let c as [String: Any] in cases {
            let about = c["about"] as! String
            let entries = (c["entries"] as! [String: String]).map { entry($0.key, $0.value) }
            let folders = entries.map(\.key) + (c["folders"] as? [String] ?? [])
            var bib = try! Bib.parse(c["bibtex"] as! String)[0]
            if let key = c["as"] as? String { bib.key = key }
            do {
                let key = try newKey(entries, folders: folders, bib)
                check(c["key"] as? String == key, "\(about): \(key)")
            } catch CoreError.same(let key) {
                check(c["same"] as? String == key, "\(about): same as \(key)")
            } catch {
                check(c["invalid"] as? Bool == true, "\(about): \(error)")
            }
        }
    }

    /// What needs folders: reading, adding, writing, files
    static func folders() {
        let fm = FileManager.default
        let lib = fm.temporaryDirectory.appendingPathComponent("refdir-ios-\(UUID().uuidString)")
        try! fm.createDirectory(at: lib, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: lib) }
        func write(_ key: String, _ text: String) {
            try! fm.createDirectory(at: lib.appendingPathComponent(key), withIntermediateDirectories: true)
            try! Data(text.utf8).write(to: lib.appendingPathComponent(key).appendingPathComponent(entryFile))
        }
        write("Mine", "@article{Other, title = {$H\\to\\gamma\\gamma$}, year = 2012}")
        write("a", "@misc{a, title = {A}}")
        write(".app", "@misc{x}")
        write("broken", "@misc{one} @misc{two}")
        try! fm.createDirectory(at: lib.appendingPathComponent("folder"), withIntermediateDirectories: true)
        let library = try! Library.read(lib)
        check(library.entries.map(\.key) == ["a", "Mine"], "keys are folder names: \(library.entries.map(\.key))")
        check(library.unreadable.count == 1, "unreadable")
        check(library.entries.last?.title == "H→γγ", "title")

        var added = try! addEntry(lib, try! Bib.parse("@misc{mine, doi = {10.1/x}, keywords = {b, a}}")[0])
        check(added.key == "minea", "a free key: \(added.key)")
        check((try? addEntry(lib, try! Bib.parse("@misc{z, doi = {10.1/X}}")[0])) == nil, "the same paper")
        added.setTags(added.tags + ["c"])
        added.setNote("Read §3.")
        try! added.write()
        let again = try! Entry.read(added.dir)
        check(again == added, "written and read again")
        check(again.tags == ["b", "a", "c"] && again.note == "Read §3.", "tags and note")
        check(tagCounts(try! Library.read(lib).entries).map(\.tag) == ["a", "b", "c"], "tag counts")

        check(try! added.addFile("Paper.pdf", Data("%PDF-".utf8)).lastPathComponent == "Paper.pdf", "add file")
        check(try! added.addFile("paper.PDF", Data()).lastPathComponent == "paper-2.PDF", "add file, taken")
        try! fm.createDirectory(at: added.dir.appendingPathComponent("0-folder"), withIntermediateDirectories: true)
        check(added.files() == ["paper-2.PDF", "Paper.pdf"], "files: \(added.files())")
        check(added.pdf()?.lastPathComponent == "paper-2.PDF", "pdf")
        let left = try! fm.contentsOfDirectory(atPath: added.dir.path).filter { $0.hasPrefix(".") }
        check(left.isEmpty, "no temporary files: \(left)")

        let pasted = try! Bib.parse("@book{W:1995, title = {QFT}} @misc{Twice, doi = {10.1/X}}")
        let (some, refused) = addEntries(lib, pasted)
        check(some.map(\.key) == ["W:1995"], "BibTeX added")
        check(refused.map(\.key) == ["Twice"] && refused[0].why == "The library has it, as minea.", "refused: \(refused)")

        let doi = normalizeMonths(try! Bib.parse("@article{T_2006, month = June, year = 2006}")[0])
        check(doi.get("month") == "jun", "doi.org's month")
        check(inspireURL(added.bib)?.absoluteString == "https://inspirehep.net/search?p=doi%3A10.1%2Fx", "INSPIRE link")
        check(doiOf(query: "10.1103/X") == "10.1103/X" && doiOf(query: "1207.7214") == nil, "DOI query")
        let kept = keepingOwn(try! Bib.parse("@article{New, title = {T}, keywords = {theirs}}")[0], of: added)
        check(kept.key == "minea" && kept.get("keywords") == "b, a, c" && kept.get("comment") == "Read §3.", "kept")
    }
}
#endif
