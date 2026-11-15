# Implementation Notes

Refdir: a library of references as a folder of BibTeX entries (`FORMAT.md`), with a command
(`refdir`) and a GNOME app (`refdir-gtk`). What the code does not say by itself: the reasons
behind decisions, pitfalls, and how to test the app.

**Keep it small** (`wc -l src/*.rs src/bin/*.rs src/bin/*/*.rs tests/*.rs`), never squeezing
code to get there. A change of the format goes into `FORMAT.md` first, with
examples in `tests/cases.json`.

The name is Refdir; `refdir` is the command, the crate, and the folder names.

**Other programs.** [refdir-ios](https://github.com/xiupos/refdir-ios) (`~/Syncthing/swift-playgrounds-project/RefdirApp.swiftpm`), the
iOS app, implements the format again in Swift from this code, with a copy of `tests/cases.json`:
a rule changed here is changed there, and the copy replaced.

## Layout

One package: a library, two binaries behind features (`cli`: clap; `gui`: gtk4, libadwaita,
relm4), both on by default. `cargo build --no-default-features --features cli` builds the
command without GTK.

| Path | Contents |
|---|---|
| `src/lib.rs` | `Bib` (one BibTeX entry, its values as written), `Entry` (a folder), `Library`, adding, tags, notes, files, plain text, folding |
| `src/aux.rs` | The keys a LaTeX `.aux` cites |
| `src/fetch.rs` | Identifiers to BibTeX (INSPIRE, doi.org), PDFs (arXiv). Blocking |
| `src/bin/refdir.rs` | The command |
| `src/bin/refdir-gtk/main.rs` | The window: places (all, tags), the list, adding, the folder setting, watching the library |
| `src/bin/refdir-gtk/entry.rs` | `EntryPane`: one entry |
| `tests/cases.json`, `tests/cases.rs` | The format's rules as examples, shared with other programs, a section each |
| `tests/library.rs` | The rules that need folders, on temporary folders |
| `data/`, `build.rs`, `meson.build` | Icons, desktop entry, metainfo, the settings schema |

Crates are renamed: `gtk` is gtk4 and `adw` is libadwaita. Check with `cargo test`,
`cargo clippy --all-targets`, and `cargo fmt`.

## Core

- **BibTeX is the record.** `Bib` keeps the fields in their order and their values as written
  (text in braces, macros such as `jun` by name), so that an entry read and written again is the
  same. biblatex's `RawBibliography` reads it; our
  own `to_bibtex` writes it, one field a line, values in braces. biblatex's typed model is not
  used: it would rewrite values.
- **The folder name is the key.** `Entry::read` sets the key to the folder's name, so that the
  two never disagree; renaming an entry is renaming its folder.
- **Pure rules.** What the cases test takes values, not folders (`new_key`, `files`, `free_name`,
  `fetch::month`), so that the cases need no disk; `add` and `Entry` read the folder and call them.
- **Adding** (`refdir::add`) refuses the same paper (same `doi` or `eprint`), and makes the
  key free with `a`, `b`, … as INSPIRE does. INSPIRE's texkey is kept as it is; doi.org's key
  (`Takahashi_2006`) too.
- **BibTeX is the other way in.** What INSPIRE and doi.org do not have (books, theses, notes) is
  added from its BibTeX, as it is, under the rules of adding: `refdir add` takes a `.bib` file
  or `-`, and the app's add dialog takes text starting with `@`. A `.bib` argument is one that names a file; a texkey never ends in `.bib`.
- **Fetching.** INSPIRE is asked for BibTeX (`format=bibtex`). An
  identifier becomes INSPIRE's query (`fetch::query`): links to `recid:`, `arxiv:`, `doi:`;
  anything else as typed, since INSPIRE's search finds a bare arXiv number, DOI, or texkey.
  `size=2` tells one hit from many. A DOI INSPIRE does not have goes to doi.org, whose months
  (`June`) become BibTeX's macros (`jun`). `ureq`, one agent, 30 s; 5 minutes for a PDF.
  Errors are short sentences naming the host; the details go to stderr.
- **Getting again** (`fetch::again`) asks INSPIRE by `eprint`, else `doi`, and keeps the key,
  `keywords`, and `comment`: those are the user's, the rest is INSPIRE's. Never doi.org, and
  refused if INSPIRE's lacks the title or every name the entry has (`fetch::kept`): doi.org's BibTeX dropped them,
  for books and others. `refdir update` shows the fields that change (`refdir::changes`), and
  writes only with `--write`: getting a whole library again at once was not safe.
- **Checking** (`refdir check`, `refdir::problems`) lists what the format forbids but a sync or a
  hand can make: a paper twice, keys the same ignoring case, an unreadable `entry.bib`, files
  Syncthing kept in a conflict (`.sync-conflict-` in the name), and a title or names missing.
- **The command** pauses 350 ms before each INSPIRE request, as INSPIRE allows about 15 per 5
  seconds. `bib` adds the cited keys the library lacks as `texkey:KEY` under the key cited,
  and writes each entry under the key as cited, as BibTeX warns of another case. Exit status:
  0, 1 when something was not found or not done, 2 when it could not run.
- **Plain text** (titles in the list) is techxt's (`refdir::plain`), pinned, as it is in
  alpha: check the titles after updating it. An odd `$` is escaped first, or math would run to
  the end.
- **Finding** folds case and marks (NFD, without combining marks) on what `search_text` gathers,
  and needs every word (`refdir::matches`, which the app runs on each row's text, made once).

## App

- **Components** are relm4's, as its book writes them: `#[relm4::component]` with `view!` and
  `#[watch]`, factories for rows made from data (places, tags, files), `TypedListView` with a
  filter for the list, relm4's actions and `menu!`.
- **Three panes**: two `AdwNavigationSplitView`s, one inside the other's content. Under 860sp
  the inner one collapses, under 560sp both.
- **The places** are one `navigation-sidebar` list: all entries, then the tags with their
  counts, with a heading over the first tag (`set_header_func`, by the row's widget name;
  `invalidate_headers` after a rebuild).
- **The library is watched** with `GFileMonitor`s on its folder and every folder in it, as a
  program makes an entry's folder before its `entry.bib`. Changes read the library again 200 ms
  after the first (`Msg::Changed`), once for all that come meanwhile: renaming a tag, or a sync,
  writes many files at once. The list is rebuilt only if what was read changed.
- **Menus on rows.** A right click, or a long press with a finger, opens a row's menu
  (`context_menu`, a `PopoverMenu` the row keeps): an entry's Read, Show in Files, Copy Key,
  Copy BibTeX, Move to Trash; a tag's Rename and Remove. Their actions are the window's, with
  the key or the tag as their target, so that one menu model serves every row. The entry pane's
  buttons for the same send them to the window too (`RowAction`), which does each in one place.
- **Renaming a tag** (`refdir::rename_tag`) writes every entry that has it; a name some have
  already merges the two. The place follows the tag. A dialog opened from a menu focuses its
  field when shown (`connect_map`), as the menu gives focus back while closing.
- **The entry pane**: the main button is Read (the first PDF), else Get PDF (from
  arXiv). The note is saved when it loses focus, and when the pane goes: another entry opened,
  or the app closed. Edit BibTeX takes
  exactly one entry, and keeps the folder's key.
- **Settings** are GSettings (`net.xiupos.Refdir`: `folder`, else `$REFDIR`; `sort`). Meson
  installs the schema; `build.rs` compiles it into the build. `settings()` takes the built one
  first, while it is there: an installed schema may be older, without a key the program reads,
  which GLib makes a crash.
- **The order** (`refdir::sort`) is the setting's own action (`settings.create_action("sort")`,
  as `win.sort`), shown as the choices of the sort menu; a change rebuilds the list.
- **Pango markup.** `adw::ActionRow` titles are markup by default; rows with text from entries
  set `use_markup: false`, since titles contain `<` and `&`.
- **Moving to the trash** asks nothing: the toast's Undo gives the entry back (`untrash`), and
  the trash after it. GIO cannot restore by itself; `untrash` finds in gvfs's `trash:///` the newest item whose
  `trash::orig-path` is the entry's folder and moves it back, as Files does, unless a folder is
  there again.
- **Toasts** have high priority, so that a new one replaces the one shown.

## Testing the app

Run it on a virtual X server, so that it never touches the real screen. Needs
`xorg-server-xvfb`, `xdotool`, ImageMagick, and the network.

```bash
Xvfb :99 -screen 0 1400x900x24 -nolisten tcp &
mkdir -p /tmp/rv/config/glib-2.0/settings /tmp/rv/lib
printf "[net/xiupos/Refdir]\nfolder='/tmp/rv/lib'\n" > /tmp/rv/config/glib-2.0/settings/keyfile
dbus-run-session -- env GSETTINGS_BACKEND=keyfile GTK_A11Y=none DISPLAY=:99 WAYLAND_DISPLAY= GDK_BACKEND=x11 GSK_RENDERER=cairo GDK_DEBUG=no-portals XDG_CONFIG_HOME=/tmp/rv/config ./target/debug/refdir-gtk
```

Drive it with `xdotool`, and look with `DISPLAY=:99 import -window root /tmp/rv/shot.png`.

- The app takes a few seconds to show its window: wait for it before the first click.
- `xdotool search --onlyvisible --name .` finds the window for `xdotool windowsize`, which
  shows the narrow layouts.
- `xclip -o -selection clipboard -display :99` shows what Copy BibTeX copied.
- `refdir --library /tmp/rv/lib add …` in another terminal changes the library while the app
  runs, and shows the monitors at work.
- The trash: `/tmp` is tmpfs, which has none. Put the test library under `~/.cache` and give
  the session its own trash: `env XDG_DATA_HOME=~/.cache/rt/data dbus-run-session -- env …`
  (the variable before `dbus-run-session`, so that gvfs's trash has it too). A toast lasts 5
  seconds: click Undo in the same command that moves to the trash.
- The file dialogs are the portal's, and hard to drive here: set the folder in the keyfile.
- Another `refdir-gtk` may be the user's own: stop only those whose environment has
  `DISPLAY=:99`.

## Not verified

- Exporting and choosing a folder with the file dialogs
- Opening files and links in other apps
