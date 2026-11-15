//! A refdir library (`FORMAT.md`): a folder of entries, each a folder named by its citation key
//! with one BibTeX entry in `entry.bib`. Reading and writing entries, finding them, and the
//! library's `.bib`

pub mod aux;
pub mod fetch;

use biblatex::{RawBibliography, RawChunk};
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::{fs, io};

/// The file of an entry's BibTeX
pub const ENTRY: &str = "entry.bib";

/// A part of a field's value: text, written in braces, or a macro, such as `jun`
#[derive(Debug, Clone, PartialEq)]
pub enum Chunk {
    Text(String),
    Macro(String),
}

/// One BibTeX entry, its fields in their order and their values as written
#[derive(Debug, Clone, PartialEq)]
pub struct Bib {
    pub kind: String,
    pub key: String,
    pub fields: Vec<(String, Vec<Chunk>)>,
}

impl Bib {
    /// The entries of a BibTeX text
    pub fn parse(text: &str) -> io::Result<Vec<Bib>> {
        let raw = RawBibliography::parse(text).map_err(|e| invalid(format!("not BibTeX: {e}")))?;
        let chunk = |c: &RawChunk| match c {
            RawChunk::Normal(s) => Chunk::Text((*s).to_owned()),
            RawChunk::Abbreviation(s) => Chunk::Macro((*s).to_owned()),
        };
        let bib = |e: &biblatex::RawEntry| Bib {
            kind: e.kind.v.to_lowercase(),
            key: e.key.v.to_owned(),
            fields: e
                .fields
                .iter()
                .map(|p| (p.key.v.to_owned(), p.value.v.iter().map(|c| chunk(&c.v)).collect()))
                .collect(),
        };
        Ok(raw.entries.iter().map(|e| bib(&e.v)).collect())
    }

    /// A field's value as written, its macros by name; `None` if it is missing or blank
    pub fn get(&self, name: &str) -> Option<String> {
        let (_, chunks) = self.fields.iter().find(|(n, _)| n.eq_ignore_ascii_case(name))?;
        let text: String = chunks
            .iter()
            .map(|c| match c {
                Chunk::Text(t) | Chunk::Macro(t) => t.as_str(),
            })
            .collect();
        Some(text.trim().to_owned()).filter(|t| !t.is_empty())
    }

    /// Sets a field to a text, in its place or at the end; an empty text removes it
    pub fn set(&mut self, name: &str, value: &str) {
        let at = self.fields.iter().position(|(n, _)| n.eq_ignore_ascii_case(name));
        match (at, value.trim().is_empty()) {
            (Some(i), true) => drop(self.fields.remove(i)),
            (Some(i), false) => self.fields[i].1 = vec![Chunk::Text(value.to_owned())],
            (None, false) => self.fields.push((name.to_owned(), vec![Chunk::Text(value.to_owned())])),
            (None, true) => {}
        }
    }

    /// The entry as BibTeX, one field a line, ending with a line break
    pub fn to_bibtex(&self) -> String {
        let value = |chunks: &[Chunk]| {
            let parts: Vec<String> = chunks
                .iter()
                .map(|c| match c {
                    Chunk::Text(t) => format!("{{{t}}}"),
                    Chunk::Macro(m) => m.clone(),
                })
                .collect();
            parts.join(" # ")
        };
        let fields: String = self.fields.iter().map(|(n, v)| format!("    {n} = {},\n", value(v))).collect();
        format!("@{}{{{},\n{fields}}}\n", self.kind, self.key)
    }
}

/// Whether a citation key can be an entry's: no white space, none of `, { } " # % \ /`, and not
/// starting with `.`
pub fn is_key(key: &str) -> bool {
    !key.is_empty()
        && !key.starts_with('.')
        && !key.chars().any(|c| c.is_whitespace() || ",{}\"#%\\/".contains(c))
}

/// An entry: its folder, whose name is its key, and its BibTeX
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub dir: PathBuf,
    pub bib: Bib,
}

impl Entry {
    /// Reads the entry in `dir`. Its key is the folder's name, whatever `entry.bib` has
    pub fn read(dir: &Path) -> io::Result<Entry> {
        let text = fs::read_to_string(dir.join(ENTRY))?;
        let mut bibs = Bib::parse(&text)?;
        if bibs.len() != 1 {
            return Err(invalid(format!("{ENTRY} has {} entries, not one", bibs.len())));
        }
        let mut bib = bibs.remove(0);
        bib.key = name(dir);
        Ok(Entry { dir: dir.to_owned(), bib })
    }

    pub fn key(&self) -> &str {
        &self.bib.key
    }

    /// Writes `entry.bib`, through a `.` file in the folder
    pub fn write(&self) -> io::Result<()> {
        write_atomic(&self.dir.join(ENTRY), self.bib.to_bibtex().as_bytes())
    }

    /// `keywords`, split at commas
    pub fn tags(&self) -> Vec<String> {
        let keywords = self.bib.get("keywords").unwrap_or_default();
        keywords.split(',').map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned).collect()
    }

    pub fn set_tags(&mut self, tags: &[String]) {
        self.bib.set("keywords", &tags.join(", "));
    }

    /// `comment`
    pub fn note(&self) -> String {
        self.bib.get("comment").unwrap_or_default()
    }

    pub fn set_note(&mut self, note: &str) {
        self.bib.set("comment", note);
    }

    /// The other files of the folder, but those starting with `.`, by name
    pub fn files(&self) -> Vec<String> {
        let names = folder_names(&self.dir).unwrap_or_default();
        files(names.into_iter().filter(|n| self.dir.join(n).is_file()).collect())
    }

    /// The first PDF of its files, by name
    pub fn pdf(&self) -> Option<PathBuf> {
        self.files().into_iter().find(|f| is_pdf(f)).map(|f| self.dir.join(f))
    }

    /// Copies a file into the folder, named `name`, or with `-2`, `-3`, … before its extension if a
    /// file there has that name, ignoring ASCII case. Returns its path
    pub fn add_file(&self, name: &str, data: &[u8]) -> io::Result<PathBuf> {
        let path = self.dir.join(free_name(name, &folder_names(&self.dir)?));
        write_atomic(&path, data)?;
        Ok(path)
    }

    /// The title as plain text
    pub fn title(&self) -> String {
        self.bib.get("title").map(|t| plain(&t)).unwrap_or_default()
    }

    /// The authors, or else the editors, by their short names: `J.M. Maldacena`. `others` is not
    /// one
    pub fn names(&self) -> Vec<String> {
        split_and(&self.name_list()).into_iter().filter(|n| *n != "others").map(short_name).collect()
    }

    /// Whether the names end with `and others`
    pub fn has_others(&self) -> bool {
        split_and(&self.name_list()).last() == Some(&"others")
    }

    /// The authors, or else the editors, as written
    fn name_list(&self) -> String {
        self.bib.get("author").or_else(|| self.bib.get("editor")).unwrap_or_default()
    }

    /// The first author's (or editor's) family name, as plain text; a name in braces whole
    fn first_family(&self) -> Option<String> {
        let list = self.name_list();
        let first = *split_and(&list).first()?;
        let braced = first.starts_with('{') && first.ends_with('}');
        Some(plain(if braced { first } else { family_given(first).0 })).filter(|f| !f.is_empty())
    }

    /// The first author's short name, and ` et al.` if there are more
    pub fn byline(&self) -> String {
        let list = self.name_list();
        let names = split_and(&list);
        let first = names.first().map(|n| short_name(n)).unwrap_or_default();
        if names.len() > 1 { format!("{first} et al.") } else { first }
    }

    pub fn year(&self) -> Option<String> {
        let date = self.bib.get("date").map(|d| d.chars().take(4).collect());
        self.bib.get("year").or(date)
    }

    /// What finding looks in, folded: the title, the names, the key, the year, the journal, the
    /// DOI and arXiv number, the tags, and the note
    pub fn search_text(&self) -> String {
        let fields =
            ["title", "author", "editor", "journal", "booktitle", "doi", "eprint", "keywords", "comment"];
        let mut text: Vec<String> = fields.iter().filter_map(|f| self.bib.get(f)).collect();
        text.extend([self.title(), self.names().join(" "), self.key().to_owned()]);
        text.extend(self.year());
        fold(&text.join("\n"))
    }

    /// Whether every word of `search`, folded, is in its text
    pub fn matches(&self, search: &str) -> bool {
        matches(&self.search_text(), search)
    }
}

/// A library's entries, by key, and the folders with an `entry.bib` that cannot be read
#[derive(Debug, Default)]
pub struct Library {
    pub entries: Vec<Entry>,
    pub unreadable: Vec<(PathBuf, io::Error)>,
}

impl Library {
    pub fn read(folder: &Path) -> io::Result<Library> {
        let mut library = Library::default();
        for e in fs::read_dir(folder)? {
            let dir = e?.path();
            if name(&dir).starts_with('.') || !dir.join(ENTRY).is_file() {
                continue;
            }
            match Entry::read(&dir) {
                Ok(entry) => library.entries.push(entry),
                Err(e) => library.unreadable.push((dir, e)),
            }
        }
        library.entries.sort_by(|a, b| by_name(a.key(), b.key()));
        Ok(library)
    }

    /// The entry with a key, ignoring ASCII case
    pub fn find(&self, key: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.key().eq_ignore_ascii_case(key))
    }
}

/// Every tag of the entries, by name, with how many have it
pub fn tags<'a>(entries: impl IntoIterator<Item = &'a Entry>) -> Vec<(String, usize)> {
    let mut tags: Vec<(String, usize)> = Vec::new();
    for tag in entries.into_iter().flat_map(Entry::tags) {
        match tags.iter_mut().find(|(t, _)| *t == tag) {
            Some((_, n)) => *n += 1,
            None => tags.push((tag, 1)),
        }
    }
    tags.sort_by(|(a, _), (b, _)| by_name(a, b));
    tags
}

/// Whether a text can be a tag: not blank, and without a comma, which separates tags
pub fn is_tag(tag: &str) -> bool {
    !tag.trim().is_empty() && !tag.contains(',')
}

/// Tags with `old` renamed to `new`, or removed if `new` is blank; a tag the list then has twice
/// is kept in its first place. `None` if it does not have `old`
pub fn renamed(tags: &[String], old: &str, new: &str) -> Option<Vec<String>> {
    if !tags.iter().any(|t| t == old) {
        return None;
    }
    let new = new.trim();
    let mut out: Vec<String> = Vec::new();
    for tag in tags.iter().map(|t| if t == old { new } else { t }) {
        if !tag.is_empty() && !out.iter().any(|t| t == tag) {
            out.push(tag.to_owned());
        }
    }
    Some(out)
}

/// The entries that have the tag `old`, with it renamed to `new`, or removed if `new` is blank
pub fn rename_tag(entries: &[Entry], old: &str, new: &str) -> Vec<Entry> {
    let rename = |e: &Entry| {
        let tags = renamed(&e.tags(), old, new)?;
        let mut e = e.clone();
        e.set_tags(&tags);
        Some(e)
    };
    entries.iter().filter_map(rename).collect()
}

/// Adds an entry to the library for `bib`, by its key, or with `a`, `b`, … after it if an entry has
/// it. Refuses a paper the library has (`same`)
pub fn add(folder: &Path, mut bib: Bib) -> io::Result<Entry> {
    let library = Library::read(folder)?;
    bib.key = new_key(&library.entries, &folder_names(folder)?, &bib)?;
    let dir = folder.join(&bib.key);
    fs::create_dir(&dir)?;
    let entry = Entry { dir, bib };
    entry.write()?;
    Ok(entry)
}

/// The `.bib` of entries, in their order, separated by empty lines
pub fn bibtex<'a>(entries: impl IntoIterator<Item = &'a Entry>) -> String {
    entries.into_iter().map(|e| e.bib.to_bibtex()).collect::<Vec<_>>().join("\n")
}

/// The fields `new` adds (`+doi`), changes (`~title`), and removes (`-note`), in their order
pub fn changes(old: &Bib, new: &Bib) -> Vec<String> {
    let value = |bib: &Bib, name: &str| {
        bib.fields.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|f| f.1.clone())
    };
    let mut changes: Vec<String> = new
        .fields
        .iter()
        .filter_map(|(name, v)| match value(old, name) {
            None => Some(format!("+{name}")),
            Some(o) if o != *v => Some(format!("~{name}")),
            Some(_) => None,
        })
        .collect();
    changes.extend(old.fields.iter().filter(|(n, _)| value(new, n).is_none()).map(|(n, _)| format!("-{n}")));
    if old.kind != new.kind {
        changes.insert(0, format!("@{}", new.kind));
    }
    changes
}

/// What is wrong with the entries, each with its key: a paper twice, a key twice ignoring ASCII
/// case, a key BibTeX cannot hold, and a title or names missing
pub fn problems(entries: &[Entry]) -> Vec<(String, String)> {
    let mut problems = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        let mut problem = |p: String| problems.push((e.key().to_owned(), p));
        if let Some(other) = same(&entries[..i], &e.bib) {
            problem(format!("the same paper as {}", other.key()));
        }
        if let Some(other) = entries[..i].iter().find(|o| o.key().eq_ignore_ascii_case(e.key())) {
            problem(format!("the same key as {}, ignoring case", other.key()));
        }
        if !is_key(e.key()) {
            problem("not a key BibTeX can hold".into());
        }
        if e.bib.get("title").is_none() {
            problem("no title".into());
        }
        if e.name_list().is_empty() {
            problem("no author or editor".into());
        }
    }
    problems
}

/// Whether a file or folder is one Syncthing kept beside another, as both were changed
pub fn is_conflict(name: &str) -> bool {
    name.contains(".sync-conflict-")
}

/// Writes a file through a `.` file next to it, then a rename
fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let tmp = path.with_file_name(format!(".{}.{}.tmp", name(path), std::process::id()));
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path).inspect_err(|_| drop(fs::remove_file(&tmp)))
}

fn name(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// The entry that is the same paper as `bib`: by its DOI, or its arXiv number, ignoring ASCII case
fn same<'a>(entries: &'a [Entry], bib: &Bib) -> Option<&'a Entry> {
    let same = |f: &str| {
        let value = bib.get(f)?;
        entries.iter().find(|e| e.bib.get(f).is_some_and(|v| v.eq_ignore_ascii_case(&value)))
    };
    same("doi").or_else(|| same("eprint"))
}

/// The key a new entry for `bib` takes, among `entries` in folders named `folders`: its key, or
/// with `a`, `b`, … after it if a folder has it, ignoring ASCII case. Refuses the same paper
pub fn new_key(entries: &[Entry], folders: &[String], bib: &Bib) -> io::Result<String> {
    if let Some(entry) = same(entries, bib) {
        let message = format!("the library has it, as {}", entry.key());
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, message));
    }
    if !is_key(&bib.key) {
        return Err(invalid(format!("{:?} cannot be a key", bib.key)));
    }
    let keys = std::iter::once(bib.key.clone()).chain(('a'..='z').map(|c| format!("{}{c}", bib.key)));
    let taken = |k: &String| folders.iter().any(|f| f.eq_ignore_ascii_case(k));
    keys.into_iter().find(|k| !taken(k)).ok_or_else(|| invalid("no free key".into()))
}

/// An entry's files among the names in its folder: not `entry.bib`, not starting with `.`, by
/// name
pub fn files(mut names: Vec<String>) -> Vec<String> {
    names.retain(|n| n != ENTRY && !n.starts_with('.'));
    names.sort_by(|a, b| by_name(a, b));
    names
}

pub fn is_pdf(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, x)| x.eq_ignore_ascii_case("pdf"))
}

/// A file's name, without a leading `.`, or with `-2`, `-3`, … before its extension if one of
/// `taken` has it, ignoring ASCII case
pub fn free_name(name: &str, taken: &[String]) -> String {
    let name = name.trim_start_matches('.');
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    let mut names = std::iter::once(name.to_owned()).chain((2..).map(|i| format!("{stem}-{i}{ext}")));
    names.find(|n| !taken.iter().any(|t| t.eq_ignore_ascii_case(n))).unwrap()
}

fn folder_names(dir: &Path) -> io::Result<Vec<String>> {
    Ok(fs::read_dir(dir)?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
}

/// How entries are listed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// Newest first
    Year,
    Title,
    /// By the first author's family name
    Author,
    Key,
}

impl Order {
    pub const ALL: [(Order, &str); 4] =
        [(Order::Year, "year"), (Order::Title, "title"), (Order::Author, "author"), (Order::Key, "key")];

    /// An order by its name; the key's for any other
    pub fn named(name: &str) -> Order {
        Order::ALL.iter().find(|(_, n)| *n == name).map_or(Order::Key, |(o, _)| *o)
    }
}

/// Whether every word of `search`, folded, is in `text`, folded already (`Entry::search_text`)
pub fn matches(text: &str, search: &str) -> bool {
    fold(search).split_whitespace().all(|w| text.contains(w))
}

/// Sorts entries: by year, newest first; by title or by the first author's family name, folded; or
/// by key. Entries without what they are sorted by go last; ties go by key
pub fn sort(entries: &mut [Entry], order: Order) {
    entries.sort_by_cached_key(|e| {
        let year = || e.year()?.split(|c: char| !c.is_ascii_digit()).next()?.parse::<i64>().ok();
        let (year, text) = match order {
            Order::Year => (year(), None),
            Order::Title => (None, Some(fold(&e.title()))),
            Order::Author => (None, e.first_family().map(|f| fold(&f))),
            Order::Key => (None, None),
        };
        let missing = match order {
            Order::Year => year.is_none(),
            Order::Title | Order::Author => text.as_ref().is_none_or(|t| t.is_empty()),
            Order::Key => false,
        };
        (
            missing,
            -year.unwrap_or(0),
            text.unwrap_or_default(),
            e.key().to_ascii_lowercase(),
            e.key().to_owned(),
        )
    });
}

/// Names in order: ASCII letters in lower case, then by code point
fn by_name(a: &str, b: &str) -> Ordering {
    a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase()).then_with(|| a.cmp(b))
}

/// The names of a BibTeX name list, split at ` and ` outside braces
fn split_and(list: &str) -> Vec<&str> {
    let (mut names, mut depth, mut start) = (Vec::new(), 0, 0);
    let bytes = list.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ if depth == 0 && bytes[i..].len() > 5 && bytes[i..i + 5].eq_ignore_ascii_case(b" and ") => {
                names.push(list[start..i].trim());
                start = i + 5;
            }
            _ => {}
        }
    }
    names.push(list[start..].trim());
    names.into_iter().filter(|n| !n.is_empty()).collect()
}

/// `Maldacena, Juan Martin` → `J.M. Maldacena`; `Juan Maldacena` → `J. Maldacena`; a name in
/// braces, such as `{ATLAS Collaboration}`, as it is
fn short_name(name: &str) -> String {
    if name.starts_with('{') && name.ends_with('}') {
        return plain(name);
    }
    let (family, given) = family_given(name);
    // `Jean-Pierre` → `J.-P.`; `S.W.` → `S.W.`
    let initial = |part: &str| -> String {
        part.split('.').filter_map(|w| w.trim().chars().next()).map(|c| format!("{c}.")).collect()
    };
    let initial = |w: &str| w.split('-').map(initial).collect::<Vec<_>>().join("-");
    let initials: String = given.split_whitespace().map(initial).collect();
    let family = plain(family);
    if initials.is_empty() { family } else { format!("{initials} {family}") }
}

/// A name's family and given names: `Family, Given` or `Given Family`
fn family_given(name: &str) -> (&str, &str) {
    match name.split_once(',') {
        Some((family, given)) => (family.trim(), given.trim()),
        None => name.rsplit_once(' ').map_or((name, ""), |(given, family)| (family, given)),
    }
}

/// TeX as plain text: `{The Large $N$ limit}` → `The Large N limit`, `$H\to\gamma\gamma$` → `H→γγ`.
/// An odd `$` is a dollar, not math
pub fn plain(tex: &str) -> String {
    static TEX: LazyLock<techxt::Converter> = LazyLock::new(|| {
        use techxt::convert::{FontStyle, MathMode};
        let builder = techxt::Converter::builder().math_mode(MathMode::Plain).wrap_width(None);
        // Upright letters: `x`, not `𝑥`
        builder.text_font(FontStyle::Disabled).math_font(FontStyle::Disabled).build().unwrap()
    });
    let bare = |i: usize| !tex[..i].ends_with('\\');
    let dollars: Vec<usize> = tex.match_indices('$').map(|(i, _)| i).filter(|&i| bare(i)).collect();
    let odd = dollars.last().filter(|_| dollars.len() % 2 == 1);
    let mut escaped = String::new();
    for (i, c) in tex.char_indices() {
        if Some(&i) == odd {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    let text = TEX.latex_to_text(&escaped).map_or(escaped, |c| c.text);
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// For finding: decomposed, without combining marks, in lower case (`Gürsey` → `gursey`)
pub fn fold(text: &str) -> String {
    use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};
    text.nfd().filter(|c| !is_combining_mark(*c)).collect::<String>().to_lowercase()
}
