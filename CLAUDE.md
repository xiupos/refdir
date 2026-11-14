# Implementation Notes

refdir: a folder format for reference libraries, and its implementation. These notes cover what the code does not say by itself: the reasons behind decisions, and the pitfalls.

**The spec comes first.** `schema/v1.json` (CC0) is the whole spec: its `description`s and `examples` hold every rule. Rules that are already there are not repeated here. A change of behaviour goes into `v1.json` first, together with the lib change and its tests, in one commit. Spec and code were separate repositories until 2026-11-14 and were merged to make that possible. The code is MIT; the README says which license covers what.

**Keep it simple.** Hand the work to well-used, maintained crates. Write our own code only for the refdir rules themselves and for what no crate does (CSL → BibTeX). When in doubt, pick fewer dependencies and leave a one-line reason next to the dependency.

## Layout

| Path | Contents |
|---|---|
| `schema/v1.json` | The spec |
| `refdir/` | The `refdir` lib crate: the refdir rules only. Its tests are in `refdir/tests/rules.rs`, one or more per rule |
| `src/main.rs` | The `refdir` command (package `refdir-cli`): arguments, and each command |
| `src/csl.rs` | Reading CSL-JSON fields (dates, first author), for `ls` and BibTeX |
| `src/bibtex.rs` | CSL → BibTeX |
| `src/citations.rs` | Citation keys from a `.aux` file |
| `src/schema.rs` | `check`'s JSON Schema validation |
| `tests/cli.rs` | The commands, run on a library built from `v1.json`'s examples |
| `third_party/csl-schema` | Submodule: the CSL schema that `v1.json` `$ref`s |

Check with `cargo fmt`, `cargo clippy --all-targets` and `cargo test`. `default-members` includes the lib, so a plain `cargo test` runs both crates. Clone with `--recurse-submodules`; without the submodule, the CLI does not build.

## The lib crate

- **heplio-gtk (`~/Projects/heplio-gtk`) will depend on it** with `git = "https://github.com/xiupos/refdir"`. Keep the lib free of CLI crates (clap, jsonschema, …): its only dependency is serde_json. The API takes and returns `serde_json::Map` / `Value`, so that each app converts them to its own types.
- **`preserve_order` is required**, not cosmetic: the resolved item layers `sources` in the order their keys appear in the file. The lib turns the feature on itself, since a dependent might not.
- **heplio-gtk's `core.rs` (`// MARK: - Library`) is where these rules came from**: `read_entry`, `resolved`, `entry_files`, `free_name`, `new_id`, `write_atomic`. On 2026-11-14 it was still behind in two places: it sorts the main file by plain `sort()`, and `free_name` compares with Unicode `to_lowercase`. Switching it to this lib fixes both.
- **Errors.** `Error::NotV1` keeps `v` so that `check` can tell a later version's entry (a warning: readers skip it) from a broken one (an error). `v` is compared as a number, so `1.0` counts as 1, as the schema's `const` treats it.
- **`resolved_field` / `Entry::field`** read one field without cloning the rest. `id()` and `Library::find` run for every entry on every lookup, so use them, not `resolved()`, for a single field. A test checks that they agree with `resolve`.
- **Writing**: a library app writes only `csl`, `tags` and `note` (`Field`), and `write_field` reads the file again just before writing. Other apps may have changed it since; the lib does not keep a copy around to write back.

## Decisions the spec leaves to the implementation

As of 2026-11-14:

- **Symbolic links.** Entry folders and main files are followed (`is_dir` / `is_file`). The file walk does not descend into a linked folder (`symlink_metadata`), since a link to a parent would never end.
- **Folder name vs key (`name_matches_id`)**: the name matches when it equals the key with `:` → `-`, or that plus `-N` with N ≥ 2 and no leading zero. Case must match. An entry without a key always matches.
- **`find`** searches the resolved `title`, `author` (every string part), `id`, and the entry's `note` (not `csl.note`).
- **`show`'s sources**: for each source, its `csl.id`, `recid` and `arxiv` where present.
- **Several libraries** (since 2026-11-14, to use heplio-gtk's library with others): `--library`, repeated, or `$REFDIR` split by `std::env::split_paths`. The CLI reads each with `Library::read` and joins `entries` in that order; the lib and the spec still know one library. Keys and INSPIRE records are checked across them, as `bib` cannot write a key twice. With more than one library, `check` names folders by path.
- **Duplicate keys in `bib`**: the first entry by library, then by folder name, is used. The others get a warning, and the exit status is 1.

## The CLI

- **Exit status**: 0 when all went well; 1 when a key is missing or ambiguous, `check` finds an error, or `bib` leaves an entry out; 2 when the command cannot run (anyhow errors, and clap's usage errors).
- **Output.** Results go to stdout through `out!`, which exits quietly on a broken pipe (`refdir ls | head` used to panic). Warnings go to stderr through `warn!`. `check` prints its findings to stdout and its summary line to stderr, so that a clean library prints nothing to stdout.
- **`ls`**: tab-separated, with no column padding: CJK names would need a width crate. An entry without a key shows `[folder]`; a key can never contain brackets.
- **`open`** uses `open::that_detached`, so that the command returns at once. There is nothing to delete yet; when there is, use the `trash` crate.

## BibTeX

- **Our own writer, not the `biblatex` crate** (0.12, checked 2026-11-14). Its writer escapes `\` as `\\` (a line break), `~` / `^` as accent commands and `{` as `\{`, which BibTeX still counts as a brace. Its fields are a BTreeMap, so they come out in alphabetical order.
- **Escaping**: `& % $ # _` get a backslash; `{ } \ ~ ^` become `\textbraceleft{}` and the like, which keep braces balanced. Titles are braced twice, to keep their case. DOI, URL and eprint are not escaped: styles and biblatex print them verbatim, and `\_` would end up in the link. Only their braces are percent-encoded.
- **Names**: `particle family, suffix, given`. A literal name, or a part containing a comma or an `and`, is braced.
- **Types**: CSL `article` (a preprint) maps to `@article`, as INSPIRE does. `container-title` goes to `booktitle` for chapters and conference papers, and to `journal` otherwise.
- **Fields written**: authors, editors, title, container, volume, pages, year, month (a macro: `nov`), DOI, URL, and eprint with `archivePrefix = {arXiv}` from `sources.inspire.arxiv`. That is the scope set for the first version, so books lack `publisher`, `edition` and the like.
- **Keys**: a key outside `[A-Za-z0-9_:-]` would break the `.bib`, so that entry is skipped.
- **Tests**: the exact output for the Maldacena, ATLAS:2012yve and inspire:2990001 examples is in `tests/cli.rs`. Update those strings only on purpose.

## `.aux` files

`\citation{a,b}` (BibTeX) and `\abx@aux@cite{…}{key}` (biblatex; the last group is the key, which covers both the old one-argument form and the newer two-argument one). `\@input` is followed relative to the main file's folder, as BibTeX does, up to 8 levels deep. `\citation{*}` means every entry, plus the cited keys. Keys are matched ignoring ASCII case, and are written as cited.

## Schema validation

- **No network at run time.** `jsonschema` is built without default features, which drops HTTP and file `$ref` resolution, and with `.offline()`. The CSL schema is registered under the URI that `v1.json` refers to, and both schemas are `include_str!`ed.
- **The CSL schema** is the submodule `third_party/csl-schema` at tag v1.0.2 (5060400). On 2026-11-14 its `schemas/input/csl-data.json` was byte-for-byte what `https://resource.citationstyles.org/schema/v1.0/input/json/csl-data.json` serves; the repository's HEAD differs from it. Before moving the submodule, compare the two again.
- **Formats are validated** (`should_validate_formats(true)`), so a bad `added` or `fetched` date-time is an error.

## Not verified

- `open` launching a real app (the tests cover only the error paths)
- Names that are not UTF-8
- Case-insensitive file systems (macOS, Windows)
- heplio-gtk building against the lib as a git dependency

## Milestones

- [x] v0.1: lib; `ls`, `find`, `show`, `open`, `bib`, `check`
- [ ] Next: `tag`, `note`, `check --fix`
- Not planned: `add` (that is a fetching app's job)
