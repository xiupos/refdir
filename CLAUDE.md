# Implementation Notes

refdir: a folder format for reference libraries, and its implementation. These notes give what the code does not say: the reasons behind decisions, and the pitfalls.

**The standard is `schema/v1.json` alone** (CC0), short enough to read. It holds only what two apps would break each other's data with if they chose differently; anything else here is this implementation's choice. A change to the standard goes into `v1.json` with the lib change and its tests, in one commit. The code is MIT.

**Keep it simple.** Hand the work to well-used, maintained crates; write our own code only for refdir itself and for what no crate does (CSL → BibTeX). Prefer fewer dependencies, with a one-line reason next to each.

## Layout

| Path | Contents |
|---|---|
| `schema/v1.json` | The standard |
| `refdir/` | The `refdir` lib crate (`src/tags.rs`: tags; `src/csl.rs`: reading CSL-JSON fields, dates, first author; `src/bibtex.rs`: CSL → BibTeX). Tests in `refdir/tests/rules.rs`, one or more per rule; `csl` and `bibtex` keep their unit tests next to their private helpers |
| `src/main.rs` | The `refdir` command (package `refdir-cli`) |
| `src/citations.rs` | Citation keys from a `.aux` file |
| `src/schema.rs` | `check`'s JSON Schema validation |
| `tests/cli.rs` | The commands, on a library built from `v1.json`'s examples |
| `third_party/csl-schema` | Submodule: the CSL schema that `v1.json` `$ref`s |

Check with `cargo fmt`, `cargo clippy --all-targets` and `cargo test` (which runs both crates). Clone with `--recurse-submodules`: the CLI does not build without it.

## The lib crate

- **heplio-gtk (`~/Projects/heplio-gtk`) depends on it** through git, and its `Cargo.lock` pins a commit: a change reaches it after `cargo update -p refdir` there. heplio-gtk is a fetching app and a library app: it writes `csl`, `tags`, and `note`, and copies BibTeX made by `bibtex`, which the CLI's `bib` makes too, so that both write the same `.bib`.
- **Only serde_json.** No CLI crates (clap, jsonschema, …). The API takes and returns `serde_json::Map` / `Value`; each app converts them to its own types.
- **`preserve_order` is required**: the resolved item layers `sources` in the order of their keys in the file. The lib turns it on itself.
- **`Error::NotV1` keeps `v`**, so that `check` can tell a later version (a warning: readers skip it) from a broken entry. `v` is compared as a number, so `1.0` is 1, as the schema's `const` has it.
- **`resolved_field` / `Entry::field`** read one field without cloning the rest. `id()` and `Library::find` run for every entry on every lookup, so use them, not `resolved()`. A test checks that they agree with `resolve`.
- **Writing reads the file again** just before writing (`write_field`, `edit_tags`): other apps may have changed it. The lib keeps no copy to write back.

## This implementation's choices

- **Name order (`cmp_names`)**: ASCII letters compared in lower case, then code point order. It orders entries and files; the main file is the first file directly in the folder.
- **Files** in subfolders belong to the entry. The walk does not follow a linked folder (`symlink_metadata`): a link to a parent would never end. Entry folders and main files are followed.
- **Folder names**: the key with `:` → `-`, then `-2`, `-3`, … until unique ignoring ASCII case (`free_name`). `name_matches_id` accepts the same, case included; an entry without a key always matches. A mismatch is a `check` warning.
- **Writing** goes through a `.` file in the same folder, then a rename (`write_atomic`).
- **Several libraries**: `--library`, repeated, or `$REFDIR` split as `PATH`. The CLI joins their `entries` in order; the lib knows one library. Keys and INSPIRE records are checked across them, and `check` then names folders by path. A key in two of them: `bib` uses the first, warns, and exits with 1.
- **`find`** searches the resolved `title`, `author` (every string part), `id`, and `note` (not `csl.note`).

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
