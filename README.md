# refdir

A folder format for reference libraries, and an implementation of it.

- **The specification** is [`schema/v1.json`](schema/v1.json): a JSON Schema whose `description`s
  and `examples` hold all the rules. It is under CC0 ([`LICENSE-CC0`](LICENSE-CC0)).
- **The implementation**, under MIT ([`LICENSE-MIT`](LICENSE-MIT)):
  - [`refdir/`](refdir): the `refdir` library crate, with the rules of refdir only. It reads and
    writes `serde_json` maps and values, which apps convert to their own types.
  - The root: the `refdir-cli` package, with the `refdir` command.

## Install the command

```sh
git clone --recurse-submodules https://github.com/xiupos/refdir
cd refdir
cargo install --path .
```

## Use

The library is `$REFDIR`, or `--library <folder>`. Several libraries are read as one:
`REFDIR=~/heplio:~/papers` (separated as `PATH` is), or `--library` repeated. A key in more than
one of them is a duplicate, as within one library; the earlier library's entry is used.

```sh
refdir ls [--tag T] [-q] [--json]     # key, first author, year, title; -q: keys; --json: JSON Lines
refdir find <word>                    # in title, authors, key and note, ignoring case
refdir show <key>                     # resolved item, tags, note, folder, files, sources
refdir open <key>                     # the main file, with the default app
refdir bib <file.aux> | --tag T | --all [--format bibtex|csl-json]
refdir check                          # schema/v1.json, duplicate keys and INSPIRE records
```

Keys are found ignoring case. `refdir ls -q | fzf | xargs refdir open` picks a paper.

`refdir bib main.aux > refs.bib` writes the entries cited by `\citation{…}` (BibTeX) and
`\abx@aux@cite{…}` (biblatex), in the `\@input` files too.

Exit status: 0 when all is well; 1 when a key is not in the library, `check` finds an error, or
`bib` leaves an entry out; 2 when the command cannot run.

## The CSL schema

`schema/v1.json` refers to the CSL-JSON schema. `refdir check` embeds it at build time and uses no
network. It is the git submodule `third_party/csl-schema`:
[citation-style-language/schema](https://github.com/citation-style-language/schema) v1.0.2, MIT.
Its `schemas/input/csl-data.json` is what
`https://resource.citationstyles.org/schema/v1.0/input/json/csl-data.json` serves.
