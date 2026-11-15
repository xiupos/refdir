# Implementation Notes

refdir: a folder format for reference libraries, and its implementation. These notes give what the code does not say: the reasons behind decisions, and the pitfalls.

**The standard is `schema/v1.json` alone** (CC0), short enough to read. It holds only what two apps would break each other's data with if they chose differently; anything else here is this implementation's choice. A change to the standard goes into `v1.json` with the lib change and its tests, in one commit. The code is MIT.

**Keep it simple.** Hand the work to well-used, maintained crates; write our own code only for refdir itself and for what no crate does (CSL → BibTeX). Prefer fewer dependencies, with a one-line reason next to each.

## Layout

| Path | Contents |
|---|---|
| `schema/v1.json` | The standard |
| `refdir/` | The `refdir` lib crate (`src/tags.rs`: tags; `src/csl.rs`: reading CSL-JSON fields, dates, first author; `src/bibtex.rs`: CSL → BibTeX; `src/aux.rs`: citation keys from a `.aux` file, which `bib` and `heplio cite` read). Tests in `refdir/tests/rules.rs`, one or more per rule; `csl` and `bibtex` keep their unit tests next to their private helpers |
| `src/main.rs` | The `refdir` command (package `refdir-cli`) |
| `src/schema.rs` | `check`'s JSON Schema validation |
| `tests/cli.rs` | The commands, on a library built from `v1.json`'s examples |
| `third_party/csl-schema` | Submodule: the CSL schema that `v1.json` `$ref`s |

Check with `cargo fmt`, `cargo clippy --all-targets` and `cargo test` (which runs both crates). Clone with `--recurse-submodules`: the CLI does not build without it.

## The lib crate

- **Two apps depend on it** through git, each pinning a commit in its `Cargo.lock` (`cargo update -p refdir` there to move it): heplio-gtk (`~/Projects/heplio-gtk`), a fetching app, which writes `sources.inspire` and copies `bibtex::of`; and refdir-gtk (`~/Projects/refdir-gtk`), the library app, which writes `csl` (`edit_csl`), `tags`, and `note` (`set_note`), adds files (`add_file`), and exports with `bibtex::export`, the `.bib` that `bib` makes too. A rule both apps need lives here, with its test, not in either app.
- **Only serde_json.** No CLI crates (clap, jsonschema, …). The API takes and returns `serde_json::Map` / `Value`; each app converts them to its own types.
- **`preserve_order` is required**: the resolved item layers `sources` in the order of their keys in the file. The lib turns it on itself.
- **`Error::NotV1` keeps `v`**, so that `check` can tell a later version (a warning: readers skip it) from a broken entry. `v` is compared as a number, so `1.0` is 1, as the schema's `const` has it.
- **`resolved_field` / `Entry::field`** read one field without cloning the rest. `id()` and `Library::find` run for every entry on every lookup, so use them, not `resolved()`. A test checks that they agree with `resolve`.
- **Writing reads the file again** just before writing (`write_field`, `edit_tags`, `set_note`, `edit_csl`): other apps may have changed it. The lib keeps no copy to write back.

## This implementation's choices

- **Name order (`cmp_names`)**: ASCII letters compared in lower case, then code point order. It orders entries and files; the main file is the first file directly in the folder.
- **Files** in subfolders belong to the entry. The walk does not follow a linked folder (`symlink_metadata`): a link to a parent would never end. Entry folders and main files are followed.
- **Folder names**: the key with `:` → `-`, then `-2`, `-3`, … until unique ignoring ASCII case (`free_name`). `name_matches_id` accepts the same, case included; an entry without a key always matches. A mismatch is a `check` warning.
- **Writing** goes through a `.` file in the same folder, then a rename (`write_atomic`).
- **Several libraries**: `--library`, repeated, or `$REFDIR` split as `PATH`. The CLI joins their `entries` in order; the lib knows one library. Keys and INSPIRE records are checked across them, and `check` then names folders by path. A key in two of them: `bib` uses the first, warns, and exits with 1.
- **`find`** searches the resolved `title`, `author` (every string part), `id`, and `note` (not `csl.note`).

## Editing

- **`edit_csl`** writes one key of the entry's own `csl`, which the resolved item shows over the sources. A value equal to the sources' resolved value, or empty, removes the key, so that the sources show through again. The standard's `csl` is a whole CSL item, which requires `id` and `type` (its example repeats them beside `note`): while `csl` has fields of its own, `edit_csl` keeps `id` and `type` in it, copied from the sources if it has none, and without any it removes `csl`. A key (`id`) must pass `bibtex::is_key` and be free across the library (`id_taken`). How a field is typed (one author per line, `Family, Given`) is the app's.
- **`set_note`**: the text as typed; a blank one removes `note`. `note` and `csl` of the wrong type are refused (`Error::Note`, `Error::Csl`), not written over.
- **`csl::clean`** fits another program's CSL-JSON to the CSL schema, read from the same file `check` uses (`csl::SCHEMA`, so the lib needs the submodule too): Crossref's types mapped (`journal-article` → `article-journal`, …; an unknown one is `document`), fields and name or date parts the schema lacks dropped, arrays where a string is wanted cut to their first item (doi.org's `ISSN`, `ISBN`), `id` dropped. doi.org gives Crossref's items for most DOIs and DataCite's for arXiv's and Zenodo's: both needed it (checked 2026-11-15, on an article, a book, a chapter, a proceedings paper, arXiv, and Zenodo).
- **`new_id`**: the first author's family name (or literal name's first word but `The`), folded to ASCII with a small table of marked letters (the lib has no Unicode tables), `:`, the year; then `a`, `b`, …. A name with no ASCII letters gives `ref`.
- **DOIs** are compared in lower case (`Entry::doi`); `check` reports entries that share one, as it does keys and INSPIRE records.
- **`create_entry`** writes a new entry in a free folder named by its key, which must be a BibTeX key free in the library. It takes the whole `entry.json`: what goes in it (`csl` or a source, `added`) is the caller's.
- **`add_file`** copies through a `.` file, then renames, to a free name: `-2`, `-3`, … before the extension, ignoring ASCII case. Leading `.`s are dropped from the name: such a file would be no file of the entry's.
- **Finding** (`Entry::matches`, `search_text`) is the lib's, so that the CLI's `find` and refdir-gtk's field find the same entries: every word of the search, folded (`fold`: lower case, marked letters plain with the same table as `new_id`, so no Unicode tables), in the resolved title, container, key, DOI, year, each name and short name (`J.M. Maldacena`), the tags, and the note. Letters the table lacks stay as they are, so a search must have them as written.

## Tags

The standard asks only for distinct non-empty strings. The rest is shared by heplio-gtk and refdir-gtk through the lib.

- **Same only as written; case matters**, as `uniqueItems` compares. Keys ignore case (`same_id`) because BibTeX does and folder names come from them; nothing outside compares tags. Completions from `Library::tags` keep spellings apart instead.
- **No normalizing.** Trimming input is the UI's job; only an empty tag is refused.
- **The user's order**: new tags at the end, repeats dropped, the first kept. `Library::tags` sorts for lists (`tags::cmp`).
- **No namespace is special, `status:` included.** The data is the same tag either way, so a status UI can come later without a rule that is hard to take back. Switching a status is a remove and an add in one `edit_tags`.
- **Reading is lenient, writing strict.** `Entry::tags` skips what is not a string; `edit_tags` refuses such `tags` (`Error::Tags`) rather than lose it, and writes nothing when nothing changes.

## The CLI

- **Exit status**: 0 when all went well; 1 when a key is missing or ambiguous, `check` finds an error, or `bib` leaves an entry out; 2 when the command cannot run (anyhow errors, and clap's usage errors).
- **Output.** Results go to stdout through `out!`, which exits quietly on a broken pipe. Warnings go to stderr through `warn!`. `check` prints its findings to stdout and its summary line to stderr, so that a clean library prints nothing to stdout.
- **`ls`**: tab-separated, with no column padding: CJK names would need a width crate. An entry without a key shows `[folder]`; a key can never contain brackets.
- **`tag`**: `+TAG` / `-TAG` are applied in order and written once. `-TAG` needs `allow_hyphen_values`, so a stray `-q` there is a tag to remove.
- **`create`** takes one item or an array (Zotero's CSL-JSON export), and writes nothing unless all of them can be written: each entry must pass `check` (the same `schema.rs`), its key and DOI must be free in all the libraries and among the items before it. An item's `id` that is not a BibTeX key (Zotero's are URLs) is replaced by `new_id`; a valid `id` that is taken is an error, since it is likely the same paper. Entries go in the first library. `added` is made by hand (`now`, days to a date), to keep the dependencies few.
- **`note`, `edit`, `attach`** call the lib's `set_note`, `edit_csl`, and `add_file`. `edit` checks the entry after writing and puts the old file back if `check` would reject it: the lib keeps `id` and `type`, so checking a copy beforehand would not see what it writes. A key is checked free across all the libraries. `bib --key` writes each key as asked for, as a `.aux` file's are.
- **`rm`** finds every key before it moves anything, and moves entries to the trash with the `trash` crate: the lib has no trash (its only dependency is serde_json), so each program trashes its own way (refdir-gtk with GIO).
- **`open`** uses `open::that_detached`, so that the command returns at once. To delete files, use the `trash` crate.

## BibTeX

- **Our own writer, not the `biblatex` crate** (0.12): see `refdir/src/bibtex.rs`. That crate's fields are also a BTreeMap, so they come out in alphabetical order. The writer is in the lib, so that heplio-gtk writes what `bib` writes.
- **Escaping**: `& % $ # _` get a backslash; `{ } \ ~ ^` become `\textbraceleft{}` and the like, which keep braces balanced. Titles are braced twice, to keep their case. DOI, URL and eprint are not escaped: styles and biblatex print them verbatim, and `\_` would end up in the link. Only their braces are percent-encoded.
- **Names**: `particle family, suffix, given`. A literal name, or a part containing a comma or an `and`, is braced.
- **Types**: CSL `article` (a preprint) maps to `@article`, as INSPIRE does. `container-title` goes to `booktitle` for chapters and conference papers, and to `journal` otherwise.
- **Fields written**: authors, editors, title, container, volume, pages, year, month (a macro: `nov`), DOI, URL, and eprint with `archivePrefix = {arXiv}` from `sources.inspire.arxiv`. Not yet `publisher`, `edition` and the like.
- **Keys**: a key outside `[A-Za-z0-9_:-]` would break the `.bib`, so that entry is skipped (`bibtex::is_key`).
- **Tests**: the exact output for the Maldacena, ATLAS:2012yve and inspire:2990001 examples is in `tests/cli.rs`. Update those strings only on purpose.

## `.aux` files

`\citation{a,b}` (BibTeX) and `\abx@aux@cite{…}{key}` (biblatex; the last group is the key, in both its one- and two-argument forms). `\@input` is followed relative to the main file's folder, as BibTeX does, up to 8 levels deep. `\citation{*}` means every entry, plus the cited keys. Keys are matched ignoring ASCII case, and are written as cited.

## Schema validation

- **No network at run time.** `jsonschema` is built without default features, which drops HTTP and file `$ref` resolution, and with `.offline()`. The CSL schema is registered under the URI that `v1.json` refers to, and both schemas are `include_str!`ed.
- **The CSL schema** is the submodule `third_party/csl-schema` at tag v1.0.2 (5060400). On 2026-11-14 its `schemas/input/csl-data.json` was byte-for-byte what `https://resource.citationstyles.org/schema/v1.0/input/json/csl-data.json` serves; the repository's HEAD differs from it. Before moving the submodule, compare the two again.
- **Formats are validated** (`should_validate_formats(true)`), so a bad `added` or `fetched` date-time is an error.

## Not verified

- `open` launching a real app (the tests cover only the error paths)
- Names that are not UTF-8
- Case-insensitive file systems (macOS, Windows)

## Milestones

- [x] lib; `ls`, `find`, `show`, `open`, `bib`, `check`, `tag`
- [ ] Next: `note`, `check --fix`
- Not planned: `add` (that is a fetching app's job)
