mod schema;

use anyhow::{Context, Result, bail};
use clap::{ArgGroup, Parser, Subcommand, ValueEnum};
use refdir::{Entry, Library, bibtex, csl};
use serde_json::{Map, Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// `println!`, which ends the program quietly when the reader has gone, as with `| head`
macro_rules! out {
    ($($arg:tt)*) => { written(writeln!(std::io::stdout(), $($arg)*)) };
}

/// `print!`, the same
macro_rules! out_raw {
    ($($arg:tt)*) => { written(write!(std::io::stdout(), $($arg)*)) };
}

/// A warning on standard error
macro_rules! warn {
    ($($arg:tt)*) => { eprintln!("refdir: warning: {}", format_args!($($arg)*)) };
}

fn written(result: std::io::Result<()>) {
    if let Err(e) = result {
        if e.kind() != std::io::ErrorKind::BrokenPipe {
            eprintln!("refdir: cannot write: {e}");
            std::process::exit(2);
        }
        std::process::exit(0);
    }
}

/// A command-line tool for refdir v1 libraries
#[derive(Parser)]
#[command(name = "refdir", version)]
struct Cli {
    /// A library folder; repeat for more. Without it, $REFDIR: folders separated as in PATH (':').
    /// Entries come in the order of the libraries, then of their folders
    #[arg(long, global = true)]
    library: Vec<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the entries: key, first author, year, title
    Ls(ListArgs),
    /// List the entries that have every word, ignoring case and marks (`gursey` finds `Gürsey`): in
    /// the title, journal or book, key, DOI, year, names, tags, or note
    Find {
        #[arg(required = true)]
        words: Vec<String>,
        #[command(flatten)]
        list: ListArgs,
    },
    /// Show an entry: its resolved item, tags, note, folder, files and sources
    Show { key: String },
    /// Open an entry's main file with the default app
    Open { key: String },
    /// Without a key, list the library's tags and how many entries have each. With a key, show the
    /// entry's tags, or change them: +TAG adds one at the end, -TAG removes one, in that order
    Tag {
        key: Option<String>,
        /// +TAG or -TAG
        #[arg(allow_hyphen_values = true, requires = "key")]
        changes: Vec<String>,
    },
    /// Show an entry's note, or replace it with TEXT (`-`: standard input). An empty TEXT removes it
    Note { key: String, text: Option<String> },
    /// Set a field of an entry's own `csl`, over its sources' values. Without VALUE, remove it, so
    /// that the sources' value shows again. The entry must still pass `check`
    Edit {
        key: String,
        /// A CSL-JSON field: title, author, issued, …
        field: String,
        /// A string, or JSON with --json: `[{"family": "Knuth", "given": "Donald E."}]`
        value: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Copy files into an entry, under their names, or with -2, -3, … before the extension if taken
    Attach {
        key: String,
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// Move entries to the trash, with their files, and print their folders
    Rm {
        #[arg(required = true)]
        keys: Vec<String>,
    },
    /// Create entries from CSL-JSON items, and print their folders
    ///
    /// An item without an `id` that BibTeX can have gets one: `Knuth:1984`, then `Knuth:1984a`, ….
    /// Nothing is written unless every entry would pass `check`, and no DOI is in the library twice
    Create {
        /// A CSL-JSON item, or an array of them, as Zotero exports; without it, standard input
        item: Option<PathBuf>,
        /// The citation key, in place of the item's `id`; for one item
        #[arg(long)]
        key: Option<String>,
        /// Keep only what the CSL schema has, fitted to it, as for doi.org's items (Crossref's or
        /// DataCite's): `curl -LH "Accept: application/vnd.citationstyles.csl+json" https://doi.org/<DOI>`
        #[arg(long)]
        clean: bool,
        /// Keep the item as `sources.NAME`, as a fetching app does, not as the entry's own `csl`
        #[arg(long, value_name = "NAME")]
        source: Option<String>,
        /// A file to copy into the entry; repeat for more; for one item
        #[arg(long)]
        file: Vec<PathBuf>,
    },
    /// Write the entries cited in a .aux file, or those with a tag, or all, to standard output
    #[command(group(ArgGroup::new("input").required(true).args(["aux", "key", "tag", "all"])))]
    Bib {
        /// The .aux file: \citation{…} and \abx@aux@cite{…}
        aux: Option<PathBuf>,
        /// The entry with this key; repeat for more
        #[arg(long)]
        key: Vec<String>,
        /// The entries with this tag; more than one means all of them
        #[arg(long)]
        tag: Vec<String>,
        /// All entries
        #[arg(long)]
        all: bool,
        #[arg(long, value_enum, default_value_t = Format::Bibtex)]
        format: Format,
    },
    /// Check the library against schema/v1.json and for duplicates. Changes nothing
    Check,
}

#[derive(clap::Args)]
struct ListArgs {
    /// Only the entries with this tag; more than one means all of them. Case matters
    #[arg(long)]
    tag: Vec<String>,
    /// Only the keys
    #[arg(short, conflicts_with = "json")]
    q: bool,
    /// JSON Lines: the folder, resolved item, tags and note of each entry
    #[arg(long)]
    json: bool,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Bibtex,
    CslJson,
}

/// 0: done, 1: something not found or wrong, 2: could not run
fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("refdir: {e:#}");
            ExitCode::from(2)
        }
    }
}

/// Says whether all went well
fn run(cli: Cli) -> Result<bool> {
    let paths = if cli.library.is_empty() { from_env() } else { cli.library };
    if paths.is_empty() {
        bail!("no library: set REFDIR or pass --library");
    }
    let mut library = Library::default();
    for path in &paths {
        let one =
            Library::read(path).with_context(|| format!("cannot read the library {}", path.display()))?;
        library.entries.extend(one.entries);
        library.unreadable.extend(one.unreadable);
    }
    if !matches!(cli.command, Command::Check) {
        for (dir, e) in &library.unreadable {
            warn!("skipped {}: {e}", dir.display());
        }
    }
    match cli.command {
        Command::Ls(args) => list(&library, &args, |_| true),
        Command::Find { words, list: args } => list(&library, &args, |e| e.matches(&words.join(" "))),
        Command::Show { key } => show(&library, &key),
        Command::Open { key } => open(&library, &key),
        Command::Tag { key: None, .. } => {
            for (tag, n) in library.tags() {
                out!("{n}\t{tag}");
            }
            Ok(true)
        }
        Command::Tag { key: Some(key), changes } => tag_entry(&library, &key, &changes),
        Command::Bib { aux, key, tag, all: _, format } => bib(&library, aux.as_deref(), &key, &tag, format),
        Command::Note { key, text } => note(&library, &key, text),
        Command::Edit { key, field, value, json } => edit(&library, &key, &field, value, json),
        Command::Attach { key, files } => attach(&library, &key, &files),
        Command::Rm { keys } => rm(&library, &keys),
        Command::Check => check(&library, paths.len() > 1),
        Command::Create { item, key, clean, source, file } => {
            create(&library, &paths[0], item.as_deref(), key, clean, source.as_deref(), &file)
        }
    }
}

/// The libraries in `$REFDIR`, separated as `PATH` is. Empty ones are left out
fn from_env() -> Vec<PathBuf> {
    let Some(value) = std::env::var_os("REFDIR") else { return Vec::new() };
    std::env::split_paths(&value).filter(|p| !p.as_os_str().is_empty()).collect()
}

// MARK: - ls, find

fn has_tags(entry: &Entry, tags: &[String]) -> bool {
    tags.iter().all(|t| entry.has_tag(t))
}

fn list(library: &Library, args: &ListArgs, keep: impl Fn(&Entry) -> bool) -> Result<bool> {
    for entry in library.entries.iter().filter(|e| has_tags(e, &args.tag) && keep(e)) {
        if args.q {
            if let Some(id) = entry.id() {
                out!("{id}");
            }
            continue;
        }
        let item = entry.resolved();
        if args.json {
            out!("{}", summary(entry, item));
            continue;
        }
        // An entry without a key by its folder, in brackets, which a key cannot have
        let id = entry.id().map_or_else(|| format!("[{}]", entry.name().to_string_lossy()), str::to_owned);
        let year = csl::issued(&item).map(|(y, _)| y.to_string()).unwrap_or_default();
        let title = csl::one_line(csl::str_field(&item, "title").unwrap_or_default());
        out!("{id}\t{}\t{year}\t{title}", csl::first_author(&item));
    }
    Ok(true)
}

/// The folder, resolved item, tags and note of an entry
fn summary(entry: &Entry, item: Map<String, Value>) -> Value {
    let mut out = json!({ "dir": entry.dir.to_string_lossy(), "csl": item });
    if !entry.tags().is_empty() {
        out["tags"] = json!(entry.tags());
    }
    if let Some(note) = entry.note() {
        out["note"] = json!(note);
    }
    out
}

// MARK: - show, open

/// The entries with the key. None is said on standard error
fn find<'a>(library: &'a Library, key: &str) -> Vec<&'a Entry> {
    let found = library.find(key);
    match found.len() {
        0 => eprintln!("refdir: no entry has the key {key}"),
        1 => {}
        n => warn!("{n} entries have the key {key}"),
    }
    found
}

fn show(library: &Library, key: &str) -> Result<bool> {
    let found = find(library, key);
    for (i, entry) in found.iter().enumerate() {
        if i > 0 {
            out!();
        }
        out!("{}", entry.id().unwrap_or_default());
        out!("folder:  {}", entry.dir.display());
        let files = entry.files().with_context(|| format!("cannot read {}", entry.dir.display()))?;
        if !files.is_empty() {
            let files: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
            out!("files:   {}", files.join(", "));
        }
        if let Some(main) = entry.main_file()? {
            out!("main:    {}", main.file_name().unwrap_or_default().to_string_lossy());
        }
        if !entry.tags().is_empty() {
            out!("tags:    {}", entry.tags().join(", "));
        }
        for (name, source) in entry.sources() {
            out!("source:  {name}{}", source_ids(source));
        }
        if let Some(note) = entry.note() {
            out!("note:");
            note.lines().for_each(|l| out!("  {l}"));
        }
        out!("csl:");
        let item = serde_json::to_string_pretty(&entry.resolved())?;
        item.lines().for_each(|l| out!("  {l}"));
    }
    Ok(!found.is_empty())
}

/// ` (csl.id X, recid N, arXiv A)` for a source
fn source_ids(source: &Value) -> String {
    let ids = [("csl.id", &source["csl"]["id"]), ("recid", &source["recid"]), ("arXiv", &source["arxiv"])];
    let ids: Vec<String> = ids
        .iter()
        .filter_map(|(name, v)| match v {
            Value::String(s) => Some(format!("{name} {s}")),
            Value::Number(n) => Some(format!("{name} {n}")),
            _ => None,
        })
        .collect();
    if ids.is_empty() { String::new() } else { format!(" ({})", ids.join(", ")) }
}

/// The one entry with the key. None is said on standard error; more than one is an error
fn find_one<'a>(library: &'a Library, key: &str) -> Result<Option<&'a Entry>> {
    match find(library, key).as_slice() {
        [] => Ok(None),
        [entry] => Ok(Some(*entry)),
        found => {
            let dirs: Vec<String> = found.iter().map(|e| e.dir.display().to_string()).collect();
            bail!("more than one entry has the key {key}: {}", dirs.join(", "));
        }
    }
}

fn open(library: &Library, key: &str) -> Result<bool> {
    let Some(entry) = find_one(library, key)? else { return Ok(false) };
    let Some(main) = entry.main_file()? else {
        eprintln!("refdir: {} has no file", entry.dir.display());
        return Ok(false);
    };
    open::that_detached(&main).with_context(|| format!("cannot open {}", main.display()))?;
    Ok(true)
}

// MARK: - note, edit, attach

fn note(library: &Library, key: &str, text: Option<String>) -> Result<bool> {
    let Some(entry) = find_one(library, key)? else { return Ok(false) };
    let text = match text.as_deref() {
        None => {
            if let Some(note) = entry.note() {
                out_raw!("{note}");
                if !note.ends_with('\n') {
                    out!();
                }
            }
            return Ok(true);
        }
        Some("-") => std::io::read_to_string(std::io::stdin()).context("cannot read standard input")?,
        Some(text) => text.to_owned(),
    };
    refdir::set_note(&entry.dir, &text)
        .map_err(|e| anyhow::anyhow!("cannot write the note of {}: {e}", entry.dir.display()))?;
    Ok(true)
}

fn edit(library: &Library, key: &str, field: &str, value: Option<String>, json: bool) -> Result<bool> {
    let Some(entry) = find_one(library, key)? else { return Ok(false) };
    let value = match value {
        Some(v) if json => Some(serde_json::from_str(&v).context("the value is not JSON")?),
        Some(v) => Some(Value::from(v)),
        None => None,
    };
    // A key must be free in every library, not only the entry's
    if let Some(id) = value.as_ref().and_then(Value::as_str).filter(|_| field == "id")
        && library.find(id).iter().any(|e| e.dir != entry.dir)
    {
        bail!("another entry has the key {id}");
    }
    let own = entry.dir.parent().unwrap_or(Path::new("."));
    refdir::edit_csl(own, &entry.dir, field, value)
        .map_err(|e| anyhow::anyhow!("cannot edit {}: {e}", entry.dir.display()))?;
    // The entry must still pass check; else it is put back as it was
    let after = refdir::read_entry(&entry.dir)?;
    let errors = schema::Schema::new()?.errors(&Value::Object(after));
    if !errors.is_empty() {
        refdir::write_atomic(&entry.dir.join(refdir::ENTRY), &Value::Object(entry.data.clone()))?;
        errors.iter().for_each(|e| eprintln!("refdir: error: {e}"));
        bail!("the entry would not pass check; it is as it was");
    }
    Ok(true)
}

fn attach(library: &Library, key: &str, files: &[PathBuf]) -> Result<bool> {
    let Some(entry) = find_one(library, key)? else { return Ok(false) };
    if let Some(missing) = files.iter().find(|f| !f.is_file()) {
        bail!("{} is not a file", missing.display());
    }
    for path in files {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let added = refdir::add_file(&entry.dir, &name, |tmp| std::fs::copy(path, tmp).map(drop))
            .map_err(|e| anyhow::anyhow!("cannot add {}: {e}", path.display()))?;
        out!("{}", added.display());
    }
    Ok(true)
}

/// Each entry is found before any is moved, so that a key missing leaves the library as it was
fn rm(library: &Library, keys: &[String]) -> Result<bool> {
    let mut entries = Vec::new();
    for key in keys {
        match find_one(library, key)? {
            Some(entry) => entries.push(entry),
            None => return Ok(false),
        }
    }
    for entry in entries {
        trash::delete(&entry.dir)
            .with_context(|| format!("cannot move {} to the trash", entry.dir.display()))?;
        out!("{}", entry.dir.display());
    }
    Ok(true)
}

// MARK: - tag

/// Shows the entry's tags after the changes, which are written at once
fn tag_entry(library: &Library, key: &str, changes: &[String]) -> Result<bool> {
    let Some(entry) = find_one(library, key)? else { return Ok(false) };
    if changes.is_empty() {
        entry.tags().iter().for_each(|t| out!("{t}"));
        return Ok(true);
    }
    let changes = changes
        .iter()
        .map(|c| match c.split_at_checked(1) {
            Some((sign @ ("+" | "-"), tag)) => Ok((sign == "+", tag)),
            _ => bail!("{c:?} is neither +TAG nor -TAG"),
        })
        .collect::<Result<Vec<_>>>()?;
    let edit = |tags: &mut Vec<String>| {
        for &(add, tag) in &changes {
            if add {
                refdir::tags::add(tags, tag)?;
            } else {
                refdir::tags::remove(tags, tag);
            }
        }
        Ok(())
    };
    let tags = refdir::edit_tags(&entry.dir, edit)
        .map_err(|e| anyhow::anyhow!("cannot change the tags of {}: {e}", entry.dir.display()))?;
    tags.iter().for_each(|t| out!("{t}"));
    Ok(true)
}

// MARK: - create

/// Creates the entries in the first library, with keys and DOIs free in all of them
fn create(
    library: &Library,
    first: &Path,
    input: Option<&Path>,
    key: Option<String>,
    clean: bool,
    source: Option<&str>,
    files: &[PathBuf],
) -> Result<bool> {
    let text = match input {
        Some(path) => {
            std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?
        }
        None => std::io::read_to_string(std::io::stdin()).context("cannot read standard input")?,
    };
    let items = match serde_json::from_str(&text).context("the input is not JSON")? {
        Value::Array(items) => items,
        item => vec![item],
    };
    if items.len() != 1 && (key.is_some() || !files.is_empty()) {
        bail!("--key and --file are for one item; there are {}", items.len());
    }
    if let Some(missing) = files.iter().find(|f| !f.is_file()) {
        bail!("{} is not a file", missing.display());
    }
    let schema = schema::Schema::new()?;
    // Each entry to write, by its key, and what is wrong, by item
    let mut entries: Vec<(String, Value)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    // The DOIs of the entries to write, in lower case
    let mut dois: Vec<String> = Vec::new();
    let many = items.len() > 1;
    for (n, item) in items.into_iter().enumerate() {
        let at = if many { format!("item {}: ", n + 1) } else { String::new() };
        let Value::Object(mut item) = item else {
            errors.push(format!("{at}not a CSL-JSON object"));
            continue;
        };
        if clean {
            item = csl::clean(&item);
        }
        let taken =
            |k: &str| !library.find(k).is_empty() || entries.iter().any(|(e, _)| refdir::same_id(e, k));
        let given = key.clone().or_else(|| {
            item.get("id").and_then(Value::as_str).filter(|k| bibtex::is_key(k)).map(str::to_owned)
        });
        let id = match given {
            Some(id) if !bibtex::is_key(&id) => {
                errors.push(format!("{at}{id:?} cannot be a BibTeX key (A-Z a-z 0-9 _ : -)"));
                continue;
            }
            Some(id) if taken(&id) => {
                errors.push(format!("{at}another entry has the key {id}"));
                continue;
            }
            Some(id) => id,
            None => refdir::new_id(&item, taken),
        };
        let doi = item.get("DOI").and_then(Value::as_str).map(|d| d.trim().to_ascii_lowercase());
        if let Some(doi) = doi.filter(|d| !d.is_empty()) {
            if let Some(other) = library.find_doi(&doi).first() {
                errors.push(format!("{at}{} has the DOI {doi}", other.dir.display()));
                continue;
            }
            if dois.contains(&doi) {
                errors.push(format!("{at}an item before it has the DOI {doi}"));
                continue;
            }
            dois.push(doi);
        }
        item.insert("id".into(), json!(id));
        let item = refdir::prune(Value::Object(item)).unwrap_or_default();
        let mut data = json!({ "v": 1, "added": refdir::now() });
        match source {
            Some(name) => data["sources"] = json!({ name: { "csl": item } }),
            None => data["csl"] = item,
        }
        let wrong = schema.errors(&data);
        if !wrong.is_empty() {
            errors.extend(wrong.into_iter().map(|e| format!("{at}{e}")));
            continue;
        }
        entries.push((id, data));
    }
    if !errors.is_empty() {
        errors.iter().for_each(|e| eprintln!("refdir: error: {e}"));
        bail!("nothing was written");
    }
    for (id, data) in &entries {
        let dir = refdir::create_entry(first, id, data.as_object().unwrap())
            .map_err(|e| anyhow::anyhow!("cannot create the entry {id}: {e}"))?;
        for path in files {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            refdir::add_file(&dir, &name, |tmp| std::fs::copy(path, tmp).map(drop)).map_err(|e| {
                anyhow::anyhow!("created {}, but cannot add {}: {e}", dir.display(), path.display())
            })?;
        }
        out!("{}", dir.display());
    }
    Ok(true)
}

// MARK: - bib

/// An entry to write, by the key it is cited by
type Cited<'a> = (String, &'a Entry);

fn bib(
    library: &Library,
    aux: Option<&Path>,
    keys: &[String],
    tags: &[String],
    format: Format,
) -> Result<bool> {
    let (cited, ok) = select(library, aux, keys, tags)?;
    match format {
        Format::Bibtex => Ok(write_bibtex(&cited) && ok),
        Format::CslJson => {
            let items: Vec<Value> = cited
                .iter()
                .map(|(key, entry)| {
                    let mut item = entry.resolved();
                    item.insert("id".to_owned(), json!(key));
                    Value::Object(item)
                })
                .collect();
            out!("{}", serde_json::to_string_pretty(&items)?);
            Ok(ok)
        }
    }
}

/// The entries cited in the `.aux` file, or with the tags. Says whether each key named one entry
/// The entries cited in the `.aux` file, or with the keys, or with the tags. Says whether each key
/// named one entry
fn select<'a>(
    library: &'a Library,
    aux: Option<&Path>,
    keys: &[String],
    tags: &[String],
) -> Result<(Vec<Cited<'a>>, bool)> {
    let mut ok = true;
    let mut cited: Vec<Cited> = Vec::new();
    let taken =
        |cited: &[Cited<'a>], key: &str| cited.iter().find(|(k, _)| refdir::same_id(k, key)).map(|c| c.1);

    let citations = aux.map(refdir::aux::read).transpose()?;
    // Every entry with the tags: for `--tag`, `--all`, and `\citation{*}`
    let every = citations.as_ref().map_or(keys.is_empty(), |c| c.all);
    if every {
        for entry in library.entries.iter().filter(|e| has_tags(e, tags)) {
            let Some(id) = entry.id() else {
                warn!("skipped {}: it has no key", entry.dir.display());
                continue;
            };
            match taken(&cited, id) {
                None => cited.push((id.to_owned(), entry)),
                Some(other) if other.dir != entry.dir => {
                    warn!("skipped {}: another entry has the key {id}", entry.dir.display());
                    ok = false;
                }
                Some(_) => {}
            }
        }
    }
    for key in citations.iter().flat_map(|c| &c.keys).chain(keys) {
        let found = library.find(key);
        let Some(&entry) = found.first() else {
            warn!("no entry has the key {key}");
            ok = false;
            continue;
        };
        if found.len() > 1 {
            warn!("{} entries have the key {key}; the first is used", found.len());
            ok = false;
        }
        if taken(&cited, key).is_none() {
            cited.push((key.clone(), entry));
        }
    }
    Ok((cited, ok))
}

/// Says whether all the keys could be BibTeX's
fn write_bibtex(cited: &[Cited]) -> bool {
    let mut ok = true;
    let mut out = Vec::new();
    for (key, entry) in cited {
        if !bibtex::is_key(key) {
            warn!("skipped {}: the key {key:?} cannot be BibTeX's", entry.dir.display());
            ok = false;
            continue;
        }
        out.push(bibtex::entry(key, &entry.resolved(), entry.inspire_arxiv()));
    }
    out_raw!("{}", out.join("\n"));
    ok
}

// MARK: - check

/// A folder as `check` says it: its name, or its path when there is more than one library
fn check(library: &Library, several: bool) -> Result<bool> {
    let folder = |dir: &Path| {
        if several {
            dir.display().to_string()
        } else {
            dir.file_name().unwrap_or_default().to_string_lossy().into_owned()
        }
    };
    let schema = schema::Schema::new()?;
    let (mut errors, mut warnings) = (0, 0);
    let mut error = |text: String| {
        out!("error: {text}");
        errors += 1;
    };
    for (dir, e) in &library.unreadable {
        match e {
            // Readers skip entries of another version
            refdir::Error::NotV1(Some(v)) if v.is_u64() => {
                out!("warning: {}: version {v}, skipped", folder(dir));
                warnings += 1;
            }
            e => error(format!("{}: {e}", folder(dir))),
        }
    }
    for entry in &library.entries {
        for e in schema.errors(&Value::Object(entry.data.clone())) {
            error(format!("{}: {e}", folder(&entry.dir)));
        }
    }
    let dirs = |entries: &[&Entry]| entries.iter().map(|e| folder(&e.dir)).collect::<Vec<_>>().join(", ");
    for (_, entries) in library.duplicate_ids() {
        let id = entries[0].id().unwrap_or_default();
        error(format!("{} entries have the key {id}: {}", entries.len(), dirs(&entries)));
    }
    for (recid, entries) in library.duplicate_recids() {
        error(format!("{} entries have INSPIRE record {recid}: {}", entries.len(), dirs(&entries)));
    }
    for (doi, entries) in library.duplicate_dois() {
        error(format!("{} entries have the DOI {doi}: {}", entries.len(), dirs(&entries)));
    }
    for entry in library.entries.iter().filter(|e| !e.name_matches_id()) {
        let id = entry.id().unwrap_or_default();
        let expected = refdir::folder_name(id);
        out!("warning: {}: the folder name is not {expected} for the key {id}", folder(&entry.dir));
        warnings += 1;
    }
    eprintln!("{} entries: {errors} errors, {warnings} warnings", library.entries.len());
    Ok(errors == 0)
}
