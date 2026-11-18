# Implementation Notes

Refdir for iOS and iPadOS: a library of references as a folder of BibTeX entries. What the code
does not say by itself: the reasons behind decisions, pitfalls, and what only the iPad can check.

It implements the Refdir format of [refdir](https://github.com/xiupos/refdir) (checked out at
`~/Projects/refdir`): `FORMAT.md`, and its code, `src/lib.rs` and `src/fetch.rs`, which is the rule
where the format says nothing. `cases.json` here is a copy of its `tests/cases.json`: copy it again
when it changes there. A change of the format goes into refdir first. The app does what refdir's
GNOME app does, in the platform's way; its `CLAUDE.md` has the reasons that do not depend on the
platform.

**Keep it small enough to read.** Four source files, no subfolders. No browsing of INSPIRE (Safari
does it), no cache of responses, no TeX typeset in a web view.

## Environment

- Swift Playgrounds on an iPad; there is no Mac and no Xcode. The project is written on Linux and
  synced to the iPad with Syncthing.
- `Package.swift` is as Playgrounds writes it: change it only as its app settings would, except
  for `additionalInfoPlistContentFilePath: "Info.plist"`. One target, `AppModule`, with
  `path: "."`, so every `.swift` file in the project is compiled. No packages, no extensions.
- `Info.plist` holds `ITSAppUsesNonExemptEncryption` = `false`: the app uses only the
  system's HTTPS, which is exempt; and `NSUserActivityTypes` with Spotlight's
  `com.apple.corespotlightitem`, without which SwiftUI may not give the app what was chosen there. Playgrounds may drop the line of `Package.swift` that merges
  it: check it before uploading a build.
- Fixed: the name Refdir, bundle ID `net.xiupos.refdir`, the team ID, the AppIcon (refdir's icon
  on white, as iOS icons are opaque; for a dark home screen, on none, with its blues a step
  lighter; for a tinted one, in grey on none, as iOS tints it), made from refdir's SVG with
  `rsvg-convert` and ImageMagick, the blue accent colour, iOS 26, Swift 5 language mode. UI
  text is in English.
- Syncthing may leave `*.sync-conflict-*` copies of files. In `Sources/` they cause "invalid
  redeclaration" errors: delete them first.

## Layout

| File | Contents |
|---|---|
| `Sources/Core.swift` | Foundation only, and everything in the format: BibTeX (`Bib`, read and written), `Entry`, `Library`, adding, files, names, plain text, finding, identifiers. Has the tests |
| `Sources/Fetch.swift` | The network: INSPIRE, doi.org, arXiv |
| `Sources/App.swift` | `@main`, `Store` (the folder, its bookmark, reading it, every change to it), the places, the list, adding, exporting, Spotlight |
| `Sources/Entry.swift` | `EntryPage`: one entry. The BibTeX editor, Quick Look |
| `cases.json` | refdir's examples |

## Testing

`Core.swift` builds on Linux (Swift 6.4 here). Its tests, at the end of the file under
`#if os(Linux)`, run every section of `cases.json` as refdir's `tests/cases.rs` does, and what
needs folders (reading, adding, writing, files) on a temporary folder.

```bash
swiftc -parse-as-library Sources/Core.swift -o /tmp/refdir-ios-test && /tmp/refdir-ios-test
/tmp/refdir-ios-test ~/Projects/refdir/tests/cases.json   # is the copy current?
swiftc -parse-as-library -typecheck Sources/Core.swift Sources/Fetch.swift
swiftc -frontend -parse Sources/App.swift Sources/Entry.swift   # syntax only
```

Everything that imports SwiftUI needs the iPad.

## Core

- **BibTeX is read by our own reader** (`BibReader`), as refdir reads it with biblatex's raw
  parser: values as written (text in braces or quotes, numbers, macros by name, `#`), field names
  as written, the type in lower case; `@string`, `@comment`, `@preamble` skipped; what is outside
  an entry is a comment. The `bibtex` cases hold it to refdir's.
- **Names.** A method hides a global function of the same base name inside its type, so
  `Entry.files()` calls `refdirFiles`, refdir's `files`; and `addEntry` is apart from
  `Store.add`.
- **Plain text** is a small converter of its own (`TeX`), as refdir's is a Rust library
  (techxt): commands to symbols, scripts to Unicode where every character has one (else `^x`,
  `_(xy)`), accents to combining marks, upright letters. The `plain` cases hold it to refdir's.
- **Pure rules** (`newKey`, `refdirFiles`, `freeName`, `month`) take values, as refdir's do, so
  that the cases need no folders.

## UI

- **Three columns**: a `NavigationSplitView` of the places (all, the tags), the list, and the
  entry. An iPhone shows one at a time, with no code of ours.
- **The places** are all entries, then the tags (`tagItems`, in `byNamespace`'s order): each
  namespace's under its name, its first letter upper case, each by its name there, then the
  others under "Tags", as refdir's GNOME app shows them. One `ForEach`, with the headings as rows
  that cannot be selected: rows in `Section`s made by a `ForEach` did not all show on the iPad, empty ones in their place,
  neither when the list was made again on a change (`.id`) nor with All Entries in a section.
- **Rows are made when the library is read** (`Row`), off the main thread: the title is TeX made
  plain and the PDF is found in the folder, so neither is done while scrolling or typing.
- **The order** (newest first, title, first author, key) is `Store.order`, kept in
  `UserDefaults`; each `Row` has its sort keys for every order, so that changing it only sorts.
  The rule (`SortKey`) is refdir's `sort`, with its cases.
- **The library** is read when the folder is opened, after every change, on pull to refresh,
  and when the app becomes active (`scenePhase`), so that a synced change shows. A read overtaken
  by a newer one is dropped.
- **Changes read the entry first** (`Store.edit`): a tag, a note, an edit, or getting it again
  changes `entry.bib` as it is on disk, not as it was last read, so that two quick changes do not
  undo each other.
- **The note** is saved when its editor loses focus and when the page goes. Each entry has its own
  page (`.id(key)`), so that changing entries saves it.
- **The folder** is chosen with `fileImporter` and kept as a security-scoped bookmark. Nothing
  stores its path, which changes on iOS. iCloud Drive leaves a file not on the device as a hidden
  `.<name>.icloud`: `downloadEvicted` asks for such `entry.bib`s.
- **PDFs** open with Quick Look (`PDFReader`, a `QLPreviewController` presented from UIKit, since
  `quickLookPreview` cannot allow editing), which saves markup into the file itself
  (`.updateContents`); the library is then read again. Other files open with it too. Share PDF is
  how a PDF opens in another app: iOS has no default app for PDFs. Opened from the list, Quick
  Look has the list's PDFs (`items`), the one opened first, so that a swipe goes on to the PDFs
  before and after it; entries without one are passed over.
- **Read on an entry's page** opens its PDF in another app (`OpenIn`, the "Open in" menu of a
  `UIDocumentInteractionController`), which may keep the page it was at, as Quick Look does not;
  Quick Look if no app can. The list's Read, and the files, stay Quick Look's. An app that opens
  documents in place writes its markup into the library's file; others take a copy.
- **Dragging a row** (`dragged`) gives its PDF, as a file, to other apps, and its key, seen only
  within the app (`.ownProcess`), to a tag of the places, which then the entry has. Other apps
  take a copy. Only an iPad shows the places and the list together, and drags between apps.
- **Adding** takes an identifier, or BibTeX (text starting with `@`), added as it is under the
  rules of adding (`addEntries`), for what INSPIRE and doi.org do not have. It is the alert's
  one-line field: BibTeX reads the same whether pasted line breaks are kept or made spaces.
- **Menus on rows** (long press, or a right click on an iPad): an entry's Read, Share PDF, Copy
  Key, Copy BibTeX, Remove from Tag (in a tag's place, so that taking it out is not deleting it), and Delete, so that a key is copied while writing without opening its entry,
  with its PDF's first page over them (`FirstPage`, PDFKit's thumbnail, made only when the menu
  shows it), as Files shows a file. A tap on it only closes the menu: SwiftUI's preview has no
  action. A swipe to the left deletes, after asking; nothing is to the right, where a swipe goes
  back to the places;
  a tag's Rename and Remove (`Store.retag`, refdir's `rename_tag`: a name some have already
  merges the two; the place follows its tag). Copy Key and Copy BibTeX are on the entry's page
  too, whose buttons are on one line where they fit, else on two (`ViewThatFits`).
- **Spotlight** has the entries, put in again after every read (`Store.index`, one domain,
  `entries`, emptied first, so that removed entries go): the title, the byline and year, the key,
  and the names and tags as keywords. One chosen there opens the app on it
  (`onContinueUserActivity`, `CSSearchableItemActivityIdentifier` being the key), in All Entries,
  on its page where only one column fits (`preferredCompactColumn`).
  It is refdir's GNOME Shell search.
- **Deleting** removes the entry's folder after asking: iOS has no trash for the user's folders.
- **Exporting** shares a `.bib` written when it is shared (`BibFile`, a `FileRepresentation`).

## Not verified

What only the iPad can check:

- That it builds: `appCategory: .reference`, and the SwiftUI and Quick Look code
- Everything in the UI: choosing the folder, adding, Get PDF, Read and markup, Read in another
  app (opened in place or copied, the page kept), dragging rows to other apps and to tags, tags, the note,
  editing BibTeX, exporting, deleting, the layouts on iPhone and iPad
- Synced changes showing when the app becomes active
- That Playgrounds builds the dark and tinted icons (`appearances` in the AppIcon's
  `Contents.json`): if not, the light one shows in every mode
- Pasting BibTeX of several lines into the add alert's field: if the field cuts it at the first
  line, adding needs a sheet with a `TextEditor`
