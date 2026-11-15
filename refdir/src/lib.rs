//! Reading and writing [refdir](https://github.com/xiupos/refdir) v1 libraries. The specification
//! is `schema/v1.json` there; where it leaves a choice open (name order, folder names, tags, …),
//! this crate makes one.
//!
//! The functions take and return `serde_json` maps and values, which apps convert to their own
//! types.

use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::{fmt, fs, io};

pub mod aux;
pub mod bibtex;
pub mod csl;
pub mod tags;

/// The file that makes a folder an entry
pub const ENTRY: &str = "entry.json";

/// Why an entry cannot be read
#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// `entry.json` is not JSON
    Json(serde_json::Error),
    /// `entry.json` is not an object
    NotObject,
    /// `v` is not 1. It holds `v`, if it is there
    NotV1(Option<Value>),
    /// `tags` is not an array of strings, so it is not written
    Tags,
    /// A tag to write is empty
    EmptyTag,
    /// `note` is not a string, so it is not written over
    Note,
    /// `csl` is not an object, so it is not written over
    Csl,
    /// A citation key to write has a character outside `A–Z a–z 0–9 _ : -`
    BadKey,
    /// Another entry has the citation key to write, ignoring ASCII case
    KeyTaken,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Json(e) => write!(f, "{ENTRY} is not JSON: {e}"),
            Error::NotObject => write!(f, "{ENTRY} is not an object"),
            Error::NotV1(None) => write!(f, "{ENTRY} has no 'v'"),
            Error::NotV1(Some(v)) => write!(f, "{ENTRY} has 'v' {v}, not 1"),
            Error::Tags => write!(f, "{ENTRY} has 'tags' that is not an array of strings"),
            Error::EmptyTag => write!(f, "a tag cannot be empty"),
            Error::Note => write!(f, "{ENTRY} has a 'note' that is not a string"),
            Error::Csl => write!(f, "{ENTRY} has a 'csl' that is not an object"),
            Error::BadKey => write!(f, "a citation key can have only A–Z, a–z, 0–9, _, :, and -"),
            Error::KeyTaken => write!(f, "another entry has this citation key"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

/// Whether a name is for apps: one starting with `.`
pub fn is_hidden(name: &OsStr) -> bool {
    name.as_encoded_bytes().starts_with(b".")
}

/// The order of names: ASCII case ignored, then code point order
pub fn cmp_names(a: &OsStr, b: &OsStr) -> Ordering {
    let (a, b) = (a.as_encoded_bytes(), b.as_encoded_bytes());
    let lower = |s: &[u8]| s.iter().map(u8::to_ascii_lowercase).collect::<Vec<_>>();
    // In UTF-8, byte order is code point order
    lower(a).cmp(&lower(b)).then_with(|| a.cmp(b))
}

/// Whether two citation keys are the same, ignoring case
pub fn same_id(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// The entries' folders: those directly in the library, not starting with `.`, with `entry.json`.
/// In name order
pub fn entry_dirs(library: &Path) -> io::Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    for e in fs::read_dir(library)? {
        let e = e?;
        if !is_hidden(&e.file_name()) && e.path().is_dir() && e.path().join(ENTRY).is_file() {
            dirs.push(e.path());
        }
    }
    dirs.sort_by(|a, b| cmp_names(a.as_os_str(), b.as_os_str()));
    Ok(dirs)
}

/// Reads `entry.json` in `dir`: an object with `v` 1. As for the schema's `const`, `1.0` is 1
pub fn read_entry(dir: &Path) -> Result<Map<String, Value>, Error> {
    let text = fs::read_to_string(dir.join(ENTRY))?;
    let Value::Object(data) = serde_json::from_str(&text).map_err(Error::Json)? else {
        return Err(Error::NotObject);
    };
    match data.get("v") {
        Some(v) if v.as_f64() == Some(1.0) => Ok(data),
        v => Err(Error::NotV1(v.cloned())),
    }
}

/// `sources`, by name, in the order of the keys in the file
pub fn sources(data: &Map<String, Value>) -> impl DoubleEndedIterator<Item = (&String, &Value)> {
    data.get("sources").and_then(Value::as_object).into_iter().flatten()
}

/// The layers of the resolved item, earliest first: each `sources.*.csl` in key order, then `csl`.
/// Those that are not objects are left out
fn layers(data: &Map<String, Value>) -> impl DoubleEndedIterator<Item = &Map<String, Value>> {
    let sources = sources(data).filter_map(|(_, s)| s.get("csl"));
    sources.chain(data.get("csl")).filter_map(Value::as_object)
}

/// The resolved item: a later layer's field replaces an earlier one whole
pub fn resolve(data: &Map<String, Value>) -> Map<String, Value> {
    let mut item = Map::new();
    for layer in layers(data) {
        item.extend(layer.clone());
    }
    item
}

/// A field of the resolved item, without resolving the rest
pub fn resolved_field<'a>(data: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    layers(data).rev().find_map(|layer| layer.get(key))
}

/// A readable entry
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub dir: PathBuf,
    /// `entry.json`, as it is
    pub data: Map<String, Value>,
}

impl Entry {
    pub fn read(dir: &Path) -> Result<Entry, Error> {
        Ok(Entry { dir: dir.to_owned(), data: read_entry(dir)? })
    }

    pub fn resolved(&self) -> Map<String, Value> {
        resolve(&self.data)
    }

    /// A field of the resolved item
    pub fn field(&self, key: &str) -> Option<&Value> {
        resolved_field(&self.data, key)
    }

    /// The citation key: the resolved `csl.id`, if it is a string
    pub fn id(&self) -> Option<&str> {
        self.field("id")?.as_str()
    }

    /// `tags`, skipping what is not a string. Writing is stricter: see [`edit_tags`]
    pub fn tags(&self) -> Vec<&str> {
        self.tag_iter().collect()
    }

    /// Whether the entry has the tag, as written
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tag_iter().any(|t| t == tag)
    }

    fn tag_iter(&self) -> impl Iterator<Item = &str> {
        let tags = self.data.get("tags").and_then(Value::as_array).into_iter().flatten();
        tags.filter_map(Value::as_str)
    }

    pub fn note(&self) -> Option<&str> {
        self.data.get("note").and_then(Value::as_str)
    }

    pub fn sources(&self) -> impl DoubleEndedIterator<Item = (&String, &Value)> {
        sources(&self.data)
    }

    fn inspire(&self) -> Option<&Value> {
        self.data.get("sources")?.get("inspire")
    }

    /// What finding looks in, folded ([`fold`]): the resolved `title`, `container-title`, `id`,
    /// `DOI`, and year, each author's and editor's names (and short name: `J.M. Maldacena`), the
    /// tags, and the note
    pub fn search_text(&self) -> String {
        let item = self.resolved();
        let mut parts: Vec<String> = ["title", "container-title", "id", "DOI"]
            .iter()
            .filter_map(|k| csl::text_field(&item, k))
            .collect();
        parts.extend(csl::issued(&item).map(|(year, _)| year.to_string()));
        let names = ["author", "editor"].iter().filter_map(|k| item.get(*k)?.as_array()).flatten();
        for name in names {
            let part = |k: &str| name.get(k).and_then(Value::as_str).unwrap_or_default();
            let initials: Vec<String> = part("given")
                .split_whitespace()
                .filter_map(|w| w.chars().next())
                .map(|c| format!("{c}."))
                .collect();
            parts.extend([part("literal"), part("given"), part("family")].map(str::to_owned));
            parts.push(format!("{} {}", initials.concat(), part("family")));
        }
        parts.extend(self.tags().into_iter().map(str::to_owned));
        parts.extend(self.note().map(str::to_owned));
        fold(&parts.join("\n"))
    }

    /// Whether every word of `search`, folded, is in [`Entry::search_text`]. An empty search finds all
    pub fn matches(&self, search: &str) -> bool {
        let text = self.search_text();
        fold(search).split_whitespace().all(|word| text.contains(word))
    }

    /// The resolved `DOI`, trimmed, in lower case: DOIs ignore ASCII case
    pub fn doi(&self) -> Option<String> {
        let doi = self.field("DOI")?.as_str()?.trim();
        (!doi.is_empty()).then(|| doi.to_ascii_lowercase())
    }

    /// `sources.inspire.recid`
    pub fn inspire_recid(&self) -> Option<u64> {
        self.inspire()?.get("recid")?.as_u64()
    }

    /// `sources.inspire.arxiv`
    pub fn inspire_arxiv(&self) -> Option<&str> {
        self.inspire()?.get("arxiv")?.as_str()
    }

    /// The folder's name
    pub fn name(&self) -> &OsStr {
        self.dir.file_name().unwrap_or_default()
    }

    /// The entry's files, relative to its folder: all but `entry.json` and names starting with `.`,
    /// in subfolders too. Each folder's in name order, its files before its subfolders' files
    pub fn files(&self) -> io::Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        walk(&self.dir, Path::new(""), &mut files)?;
        Ok(files)
    }

    /// The main file: the first file directly in the folder by name
    pub fn main_file(&self) -> io::Result<Option<PathBuf>> {
        let mut first: Option<PathBuf> = None;
        for e in fs::read_dir(&self.dir)? {
            let e = e?;
            let name = e.file_name();
            if is_hidden(&name) || name == ENTRY || !e.path().is_file() {
                continue;
            }
            if first.as_ref().is_none_or(|f| cmp_names(&name, f.as_os_str()) == Ordering::Less) {
                first = Some(name.into());
            }
        }
        Ok(first.map(|name| self.dir.join(name)))
    }

    /// Whether the folder's name is what the citation key makes: the key with `:` → `-`, perhaps
    /// with `-2`, `-3`, …. Without a key, it is
    pub fn name_matches_id(&self) -> bool {
        let Some(id) = self.id() else { return true };
        let base = folder_name(id);
        let name = self.name().to_string_lossy();
        let Some(rest) = name.strip_prefix(base.as_str()) else { return false };
        let number = rest.strip_prefix('-').filter(|n| !n.starts_with('0'));
        rest.is_empty() || number.and_then(|n| n.parse::<u64>().ok()).is_some_and(|n| n >= 2)
    }
}

fn walk(root: &Path, rel: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let mut names: Vec<_> =
        fs::read_dir(root.join(rel))?.map(|e| e.map(|e| e.file_name())).collect::<io::Result<_>>()?;
    names.retain(|n| !is_hidden(n) && !(rel.as_os_str().is_empty() && n == ENTRY));
    names.sort_by(|a, b| cmp_names(a, b));
    let mut dirs = Vec::new();
    for name in names {
        let path = rel.join(&name);
        // Not `is_dir`, which follows links: a link to a folder above would never end
        if fs::symlink_metadata(root.join(&path))?.is_dir() {
            dirs.push(path);
        } else {
            files.push(path);
        }
    }
    for dir in dirs {
        walk(root, &dir, files)?;
    }
    Ok(())
}

/// The library's entries, in name order, and the folders with an `entry.json` that cannot be read
#[derive(Debug, Default)]
pub struct Library {
    pub entries: Vec<Entry>,
    pub unreadable: Vec<(PathBuf, Error)>,
}

impl Library {
    pub fn read(library: &Path) -> io::Result<Library> {
        let mut lib = Library::default();
        for dir in entry_dirs(library)? {
            match Entry::read(&dir) {
                Ok(entry) => lib.entries.push(entry),
                Err(e) => lib.unreadable.push((dir, e)),
            }
        }
        Ok(lib)
    }

    /// The entries with a citation key, ignoring case. More than one is a conflict
    pub fn find(&self, id: &str) -> Vec<&Entry> {
        self.entries.iter().filter(|e| e.id().is_some_and(|i| same_id(i, id))).collect()
    }

    /// The entries with a DOI, ignoring case
    pub fn find_doi(&self, doi: &str) -> Vec<&Entry> {
        let doi = doi.trim().to_ascii_lowercase();
        self.entries.iter().filter(|e| e.doi().as_deref() == Some(doi.as_str())).collect()
    }

    /// The entries that share a DOI, by DOI in lower case
    pub fn duplicate_dois(&self) -> Vec<(String, Vec<&Entry>)> {
        duplicates(self.entries.iter().filter_map(|e| Some((e.doi()?, e))))
    }

    /// The entries that share a citation key, ignoring case, by key in lower case
    pub fn duplicate_ids(&self) -> Vec<(String, Vec<&Entry>)> {
        duplicates(self.entries.iter().filter_map(|e| Some((e.id()?.to_ascii_lowercase(), e))))
    }

    /// Every tag in the library and the number of entries with it, in [`tags::cmp`] order
    pub fn tags(&self) -> Vec<(&str, usize)> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for entry in &self.entries {
            let mut own = entry.tags();
            own.sort_unstable();
            own.dedup();
            own.into_iter().for_each(|t| *counts.entry(t).or_default() += 1);
        }
        let mut counts: Vec<_> = counts.into_iter().collect();
        counts.sort_by(|(a, _), (b, _)| tags::cmp(a, b));
        counts
    }

    /// The entries that share `sources.inspire.recid`
    pub fn duplicate_recids(&self) -> Vec<(u64, Vec<&Entry>)> {
        duplicates(self.entries.iter().filter_map(|e| Some((e.inspire_recid()?, e))))
    }
}

fn duplicates<'a, K: Ord>(items: impl Iterator<Item = (K, &'a Entry)>) -> Vec<(K, Vec<&'a Entry>)> {
    let mut groups: BTreeMap<K, Vec<&Entry>> = BTreeMap::new();
    for (k, e) in items {
        groups.entry(k).or_default().push(e);
    }
    groups.into_iter().filter(|(_, g)| g.len() > 1).collect()
}

/// Letters with marks, and the ASCII letter each is written as in a key
const FOLDS: [(&str, char); 15] = [
    ("àáâãäåāăą", 'a'),
    ("çćčĉ", 'c'),
    ("ďđ", 'd'),
    ("èéêëēĕėęě", 'e'),
    ("ĝğġģ", 'g'),
    ("ìíîïīĭį", 'i'),
    ("ĺļľł", 'l'),
    ("ñńņň", 'n'),
    ("òóôõöøōŏő", 'o'),
    ("ŕŗř", 'r'),
    ("śŝşš", 's'),
    ("ţťŧ", 't'),
    ("ùúûüũūŭůűų", 'u'),
    ("ýÿŷ", 'y'),
    ("źżž", 'z'),
];

/// The plain ASCII letter, in lower case, that a marked letter is written as: `Ü` → `u`
fn plain(c: char) -> Option<char> {
    let lower = c.to_lowercase().next().unwrap_or(c);
    FOLDS.iter().find(|(marked, _)| marked.contains(lower)).map(|(_, plain)| *plain)
}

/// Text as finding compares it: in lower case, with marked letters plain (`Gürsey` → `gursey`, `ß`
/// → `ss`). Letters the table does not have, and other scripts, stay as they are
pub fn fold(text: &str) -> String {
    let fold = |c: char| match plain(c) {
        Some(p) => p.to_string(),
        None if c == 'ß' => "ss".into(),
        None => c.to_lowercase().collect(),
    };
    text.chars().map(fold).collect()
}

/// A citation key for an item that has none: its first author's family name (or literal name, its
/// first word but `The`) in ASCII letters and digits, `:`, and its year, as INSPIRE's keys are: `Knuth:1984`.
/// If `taken` says it is, `a`, `b`, … follow the year. Without a name, `ref`
pub fn new_id(item: &Map<String, Value>, taken: impl Fn(&str) -> bool) -> String {
    let first = item.get("author").or_else(|| item.get("editor")).and_then(|a| a.get(0));
    let part = |key| first.and_then(|f| f.get(key)).and_then(Value::as_str);
    let literal = || part("literal")?.split_whitespace().find(|w| !w.eq_ignore_ascii_case("the"));
    let name = part("family").filter(|f| !f.is_empty()).or_else(literal).or(part("given"));
    let ascii = |c: char| match plain(c) {
        Some(p) if c.is_uppercase() => Some(p.to_ascii_uppercase().to_string()),
        Some(p) => Some(p.to_string()),
        None if c == 'ß' => Some("ss".into()),
        None => c.is_ascii_alphanumeric().then(|| c.to_string()),
    };
    let base: String = name.unwrap_or_default().chars().filter_map(ascii).collect();
    let base = if base.is_empty() { "ref".to_owned() } else { base };
    let year = item.get("issued").and_then(csl::year_month).map(|(y, _)| y.to_string());
    let base = year.map_or(base.clone(), |y| format!("{base}:{y}"));
    let letters = ('a'..='z').map(|c| format!("{base}{c}"));
    let mut keys = std::iter::once(base.clone()).chain(letters).chain((2..).map(|n| format!("{base}-{n}")));
    keys.find(|k| !taken(k)).unwrap()
}

/// The folder name a citation key makes: `Maldacena:1997re` → `Maldacena-1997re`
pub fn folder_name(id: &str) -> String {
    id.replace(':', "-")
}

/// A folder name for a new entry: [`folder_name`], then with `-2`, `-3`, … until no name in the
/// library is the same, ignoring ASCII case. Names without a citation key are any
pub fn free_name(library: &Path, id: &str) -> io::Result<String> {
    let taken: Vec<String> = fs::read_dir(library)?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().to_ascii_lowercase()))
        .collect::<io::Result<_>>()?;
    let base = folder_name(id);
    let free = |n: &String| !taken.contains(&n.to_ascii_lowercase());
    Ok(std::iter::once(base.clone()).chain((2..).map(|i| format!("{base}-{i}"))).find(free).unwrap())
}

/// Whether another entry's citation key is `id`, ignoring case. `own` is the entry's folder, left
/// out
pub fn id_taken(library: &Path, id: &str, own: Option<&Path>) -> io::Result<bool> {
    let lib = Library::read(library)?;
    Ok(lib.find(id).iter().any(|e| Some(e.dir.as_path()) != own))
}

/// The keys a library app writes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Csl,
    Tags,
    Note,
}

impl Field {
    pub fn key(self) -> &'static str {
        match self {
            Field::Csl => "csl",
            Field::Tags => "tags",
            Field::Note => "note",
        }
    }
}

/// Reads the entry in `dir` again, sets `field` to `value` without its empty fields (or removes
/// it, if it is empty), and writes it. The other keys stay as they are. For `tags`, use
/// [`edit_tags`]
pub fn write_field(dir: &Path, field: Field, value: Value) -> Result<(), Error> {
    put(dir, read_entry(dir)?, field, value)
}

/// Reads the entry in `dir` again, edits its tags and writes them without repeats, the first of
/// each kept. Returns the tags as written. Writes nothing if they are unchanged, if `edit` fails,
/// or if `tags` is not an array of strings ([`Error::Tags`]), which [`Entry::tags`] reads anyway
pub fn edit_tags(
    dir: &Path,
    edit: impl FnOnce(&mut Vec<String>) -> Result<(), Error>,
) -> Result<Vec<String>, Error> {
    let data = read_entry(dir)?;
    let old = tags::read(&data)?;
    let mut new = old.clone();
    edit(&mut new)?;
    tags::dedup(&mut new);
    if new != old {
        put(dir, data, Field::Tags, new.clone().into())?;
    }
    Ok(new)
}

fn put(dir: &Path, mut data: Map<String, Value>, field: Field, value: Value) -> Result<(), Error> {
    match prune(value) {
        Some(value) => data.insert(field.key().to_owned(), value),
        None => data.shift_remove(field.key()),
    };
    Ok(write_atomic(&dir.join(ENTRY), &Value::Object(data))?)
}

/// [`edit_tags`] with [`tags::add`]
pub fn add_tag(dir: &Path, tag: &str) -> Result<Vec<String>, Error> {
    edit_tags(dir, |t| tags::add(t, tag))
}

/// [`edit_tags`] with [`tags::remove`]
pub fn remove_tag(dir: &Path, tag: &str) -> Result<Vec<String>, Error> {
    edit_tags(dir, |t| {
        tags::remove(t, tag);
        Ok(())
    })
}

/// Reads the entry in `dir` again and writes `note`; a blank `text` removes it. A `note` that is not
/// a string is not written over ([`Error::Note`]). Writes nothing if it is unchanged
pub fn set_note(dir: &Path, text: &str) -> Result<(), Error> {
    let data = read_entry(dir)?;
    let new = (!text.trim().is_empty()).then(|| Value::from(text));
    match data.get("note") {
        Some(old) if !old.is_string() => Err(Error::Note),
        old if old == new.as_ref() => Ok(()),
        _ => put(dir, data, Field::Note, new.unwrap_or_default()),
    }
}

/// Reads the entry in `dir` again and sets `key` of its own `csl` to `value`, which the resolved
/// item then shows over the sources'. `None`, or the sources' value, removes it from `csl`. While
/// `csl` has fields of its own, it has `id` and `type` too, as the standard's items do: the sources'
/// if it has none; without, it is removed. An
/// `id` must be a BibTeX key ([`bibtex::is_key`]) that no other entry of `library` has. A `csl`
/// that is not an object is not written over ([`Error::Csl`]). Writes nothing if it is unchanged
pub fn edit_csl(library: &Path, dir: &Path, key: &str, value: Option<Value>) -> Result<(), Error> {
    let data = read_entry(dir)?;
    let mut csl = match data.get("csl") {
        None => Map::new(),
        Some(Value::Object(csl)) => csl.clone(),
        Some(_) => return Err(Error::Csl),
    };
    let value = value.and_then(prune);
    if let (true, Some(id)) = (key == "id", value.as_ref().and_then(Value::as_str)) {
        if !bibtex::is_key(id) {
            return Err(Error::BadKey);
        }
        if id_taken(library, id, Some(dir))? {
            return Err(Error::KeyTaken);
        }
    }
    let mut sources = data.clone();
    sources.shift_remove("csl");
    let theirs = |k: &str| resolved_field(&sources, k).cloned();
    let before = csl.clone();
    match value.filter(|v| Some(v) != theirs(key).as_ref()) {
        Some(v) => csl.insert(key.to_owned(), v),
        None => csl.shift_remove(key),
    };
    // The standard's `csl` is a whole CSL item, with `id` and `type`: while it has fields of its own,
    // they are copied from the sources if it has none; without, they go too
    let own = |k: &str| csl.get(k).is_some_and(|v| Some(v) != theirs(k).as_ref());
    let needed = own("id") || own("type") || csl.keys().any(|k| k != "id" && k != "type");
    for k in ["id", "type"] {
        match (needed, theirs(k)) {
            (false, _) => drop(csl.shift_remove(k)),
            (true, Some(v)) if !csl.contains_key(k) => drop(csl.insert(k.to_owned(), v)),
            _ => {}
        }
    }
    if csl == before {
        return Ok(());
    }
    put(dir, data, Field::Csl, Value::Object(csl))
}

/// Creates an entry in `library` with `data` as its `entry.json`, in a folder named by its key `id`
/// ([`free_name`]), and returns the folder. The key must be a BibTeX key ([`bibtex::is_key`]) that
/// no entry of the library has; `data` is written as it is, so its resolved `id` should be `id`
pub fn create_entry(library: &Path, id: &str, data: &Map<String, Value>) -> Result<PathBuf, Error> {
    if !bibtex::is_key(id) {
        return Err(Error::BadKey);
    }
    if id_taken(library, id, None)? {
        return Err(Error::KeyTaken);
    }
    let dir = library.join(free_name(library, id)?);
    fs::create_dir(&dir)?;
    write_atomic(&dir.join(ENTRY), &Value::Object(data.clone()))?;
    Ok(dir)
}

/// Adds a file to the readable entry in `dir`, named `name` without leading `.`s, or with `-2`, `-3`, … before its
/// extension if a file in the folder has that name, ignoring ASCII case. `copy` writes it to the
/// path it is given, a `.` file that is then renamed. Returns the file's path
pub fn add_file(
    dir: &Path,
    name: &str,
    copy: impl FnOnce(&Path) -> io::Result<()>,
) -> Result<PathBuf, Error> {
    read_entry(dir)?;
    // A name starting with `.` would be no file of the entry's
    let name = name.trim_start_matches('.');
    let name = if name.is_empty() { "file" } else { name };
    let taken: Vec<String> = fs::read_dir(dir)?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().to_ascii_lowercase()))
        .collect::<io::Result<_>>()?;
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    let names = std::iter::once(name.to_owned()).chain((2..).map(|i| format!("{stem}-{i}{ext}")));
    let path = dir.join(names.into_iter().find(|n| !taken.contains(&n.to_ascii_lowercase())).unwrap());
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    copy(&tmp).and_then(|()| fs::rename(&tmp, &path)).inspect_err(|_| drop(fs::remove_file(&tmp)))?;
    Ok(path)
}

/// `value` without its empty fields, recursively: `null`, `""`, `[]` and `{}`. `None` if it is
/// empty itself
pub fn prune(value: Value) -> Option<Value> {
    let value = match value {
        Value::Object(map) => {
            Value::Object(map.into_iter().filter_map(|(k, v)| Some((k, prune(v)?))).collect())
        }
        Value::Array(items) => Value::Array(items.into_iter().filter_map(prune).collect()),
        v => v,
    };
    let empty = match &value {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        _ => false,
    };
    (!empty).then_some(value)
}

/// Writes `value` as indented JSON to a temporary file next to `path`, named with a `.`, then
/// renames it
pub fn write_atomic(path: &Path, value: &Value) -> io::Result<()> {
    let text = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    fs::write(&tmp, text + "\n")?;
    fs::rename(&tmp, path).inspect_err(|_| drop(fs::remove_file(&tmp)))
}
