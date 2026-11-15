# Refdir

A library of references as a folder: one folder per entry, named by its citation key, with
its BibTeX in `entry.bib` and its PDFs and other files beside it. The format is
[FORMAT.md](FORMAT.md), with its rules as examples in
[tests/cases.json](tests/cases.json). Any program that reads a folder can read it, and any sync can carry it.

Papers come from [INSPIRE-HEP](https://inspirehep.net) by their arXiv number, DOI, texkey, or
link, or from doi.org for a DOI INSPIRE does not have; PDFs from arXiv.

## The command

```
refdir add 1207.7214 Maldacena:1997re 10.1103/PhysRevD.49.6587 --pdf
refdir bib main.aux > refs.bib    # adds from INSPIRE the cited keys the library lacks
refdir update                     # gets every entry again from INSPIRE
refdir ls
refdir find maldacena 1997
refdir tag Maldacena:1997re +to-read
refdir open Maldacena:1997re
```

The library is `--library DIR` or `$REFDIR`. Exit status: 0, 1 when something was not found or
not done, 2 when the command could not run.

## The apps

`refdir-gtk`, for GNOME, and [RefdirApp](https://github.com/xiupos/RefdirApp), for iPhone and
iPad: the library by tag, finding as you type, adding by identifier, an
entry's tags, note, and files, reading or getting its PDF, editing its BibTeX, and exporting a
`.bib`.

## Building

Rust 1.88 or later. The app needs GTK 4.12 and libadwaita 1.8.

```
cargo build --release                                     # both
cargo build --release --no-default-features --features cli   # the command only
meson setup build && meson install -C build               # both, with the app's desktop files
```

MIT licence.
