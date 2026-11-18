# The Refdir format

A library is a folder. Each entry is a folder directly in it, whose name is the entry's
citation key, with one BibTeX entry in `entry.bib`:

    library/
      Maldacena:1997re/
        entry.bib
        hep-th-9711200.pdf
        notes.tex

- **The key** is the folder's name. The key written in `entry.bib` is read as the folder's
  name, whatever it is. Keys are unique in the library ignoring ASCII case. A key does not
  start with `.`, and has no white space and none of `, { } " # % \ /`, which BibTeX or a
  folder name cannot hold.
- **`entry.bib`** holds exactly one BibTeX entry, in UTF-8. Any type and any field is allowed;
  values are kept as written. A folder whose `entry.bib` does not hold one entry is unreadable,
  and is left alone.
- **Tags** are `keywords`, separated by commas, each trimmed. **The note** is `comment`.
- **A namespace.** A tag `namespace:name`, split at its first `:`, both trimmed and not empty,
  is in the namespace, which is the same ignoring ASCII case. Programs show the tags of each
  namespace together, under the namespace's name, the namespaces by name, before the tags in
  none; the tag itself is still the whole text.
- **Files** are the other files in the entry's folder, but those whose names start with `.`.
  They are ordered by name, ignoring ASCII case. **The PDF** is the first of them whose name
  ends with `.pdf`, ignoring ASCII case: what "read" opens, and what says the entry has its
  paper. Other files, such as an EPUB of the same paper, are kept beside it.
- **The same paper** is one with the same `doi` or the same `eprint`, ignoring ASCII case. A
  library holds it once.
- **The library's `.bib`** is every `entry.bib`, in key order, separated by empty lines.
- Folders whose name starts with `.`, and folders without `entry.bib`, are not entries: they
  are free for other programs.
- `tests/cases.json` has these rules as examples, which every Refdir program runs as tests.
- A file is written through a temporary file starting with `.` in the same folder, then a
  rename, so that a reader never sees half of it.
