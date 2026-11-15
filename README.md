# refdir-ios

A file-based reference library for iPhone and iPad. Each entry is a folder, named by its citation key, with its BibTeX in `entry.bib` and its PDFs and other files beside it. There is no database: any program that reads a folder can read it, iCloud Drive or any other sync can carry it, and the [refdir](https://github.com/xiupos/refdir) command and its GNOME app open the same one. The format is [refdir's FORMAT.md](https://github.com/xiupos/refdir/blob/main/FORMAT.md).

## Usage

- Add papers from INSPIRE-HEP by their arXiv number, DOI, texkey, or link (doi.org for a DOI
  INSPIRE does not have), and anything else by pasting its BibTeX
- Get their PDFs from arXiv, read them, and mark them up
- Tag them, write notes, keep other files with them
- Find entries as you type
- Copy an entry's BibTeX, edit it, or share a tag's entries as a `.bib`

## Building

Open `RefdirApp.swiftpm` in Swift Playgrounds on an iPad (iPadOS 26 or later) and run it. It has no dependencies.

## See also

- [refdir](https://github.com/xiupos/refdir): the format, the `refdir` command, and the GNOME app

## License

[MIT](LICENSE)
