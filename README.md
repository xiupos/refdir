# refdir

A file-based reference library. Each entry is a folder, named by its citation key, with its BibTeX in `entry.bib` and its PDFs and other files beside it. There is no database: any program that reads a folder can read it, any sync can carry it, and the `refdir` command, its GNOME app, and [refdir-ios](https://github.com/xiupos/refdir-ios) open the same one. The format is [FORMAT.md](FORMAT.md), with its rules as examples in [tests/cases.json](tests/cases.json).

## Usage

Papers come from [INSPIRE-HEP](https://inspirehep.net) by their arXiv number, DOI, texkey, or
link, or from doi.org for a DOI INSPIRE does not have; PDFs from arXiv. Anything else is added
as BibTeX: a book's from its publisher, a `.bib` from another program.

```
refdir add 1207.7214 Maldacena:1997re 10.1103/PhysRevD.49.6587 --pdf
refdir add books.bib              # its entries, as they are; - reads standard input
refdir bib main.aux > refs.bib    # adds from INSPIRE the cited keys the library lacks
refdir update                     # shows what INSPIRE has changed: +doi +journal ~title …
refdir update --write Old:2012    # writes it, keeping the key, tags, and note
refdir check                      # a paper twice, a title missing, Syncthing's conflicts
refdir ls
refdir find maldacena 1997
refdir tag Maldacena:1997re +to-read
refdir tags --rename to-read reading   # in every entry; --remove TAG takes it off
refdir open Maldacena:1997re
```

The library is `--library DIR` or `$REFDIR`. Exit status: 0, 1 when something was not found or
not done, 2 when the command could not run.

The app, `refdir-gtk`, does the same for GNOME: the library by tag, finding as you type, adding
by identifier or pasted BibTeX, an entry's tags, note, and files, reading or getting its PDF,
editing its BibTeX, and exporting a `.bib`. Installed with Meson, it also answers GNOME Shell's
search: an entry found there opens in the app.

## Building

Rust 1.88 or later. The app needs GTK 4.12 and libadwaita 1.8.

```
cargo build --release                                     # both
cargo build --release --no-default-features --features cli   # the command only
meson setup build && meson compile -C build               # both, with the app's desktop files
sudo meson install -C build --no-rebuild
```

## See also

- [refdir-ios](https://github.com/xiupos/refdir-ios): Refdir for iPhone and iPad

## License

[MIT](LICENSE)
