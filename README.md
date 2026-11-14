# refdir-cli

A command-line tool for [refdir](https://github.com/xiupos/refdir) v1 libraries.

This is an implementation of the refdir specification (CC0), `schema/v1.json` in
[xiupos/refdir](https://github.com/xiupos/refdir).

## Install

```sh
git clone --recurse-submodules https://github.com/xiupos/refdir-cli
cargo install --path refdir-cli
```

## Use

The library is `$REFDIR`, or `--library <folder>`.

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

## Crates

- `refdir/`: the `refdir` library crate, with the rules of refdir only. It reads and writes
  `serde_json` maps and values, which apps convert to their own types.
- The root: the `refdir-cli` package, with the `refdir` command.

## Schemas

`refdir check` checks against these, embedded at build time, so it uses no network. They are git
submodules at fixed commits:

- `schema/refdir`: [xiupos/refdir](https://github.com/xiupos/refdir), CC0.
- `schema/csl`: [citation-style-language/schema](https://github.com/citation-style-language/schema)
  v1.0.2, MIT. Its `schemas/input/csl-data.json` is what
  `https://resource.citationstyles.org/schema/v1.0/input/json/csl-data.json`, which `v1.json` refers
  to, serves.

## License

MIT
