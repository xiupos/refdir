mod bibtex;
mod citations;
mod csl;
mod schema;

use anyhow::{Context, Result, bail};
use clap::{ArgGroup, Parser, Subcommand, ValueEnum};
use refdir::{Entry, Library};
use serde_json::{Map, Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// `println!`, which ends the program quietly when the reader has gone, as with `| head`
macro_rules! out {
    ($($arg:tt)*) => { written(writeln!(std::io::stdout(), $($arg)*)) };
}

/// A warning on standard error
macro_rules! warn {
    ($($arg:tt)*) => { eprintln!("refdir: warning: {}", format_args!($($arg)*)) };
}

/// `print!`, the same
macro_rules! out_raw {
    ($($arg:tt)*) => { written(write!(std::io::stdout(), $($arg)*)) };
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
    /// The library folder
    #[arg(long, env = "REFDIR", global = true)]
    library: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the entries: key, first author, year, title
    Ls(ListArgs),
    /// List the entries whose title, authors, key or note contain a word, ignoring case
    Find {
        word: String,
        #[command(flatten)]
        list: ListArgs,
    },
    /// Show an entry: its resolved item, tags, note, folder, files and sources
    Show { key: String },
    /// Open an entry's main file with the default app
    Open { key: String },
    /// Write the entries cited in a .aux file, or those with a tag, or all, to standard output
    #[command(group(ArgGroup::new("input").required(true).args(["aux", "tag", "all"])))]
    Bib {
        /// The .aux file: \citation{…} and \abx@aux@cite{…}
        aux: Option<PathBuf>,
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
    /// Only the entries with this tag; more than one means all of them
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
    let Some(path) = cli.library else { bail!("no library: set REFDIR or pass --library") };
    let library =
        Library::read(&path).with_context(|| format!("cannot read the library {}", path.display()))?;
    if !matches!(cli.command, Command::Check) {
        for (dir, e) in &library.unreadable {
            warn!("skipped {}: {e}", dir.display());
        }
    }
    match cli.command {
        Command::Ls(args) => list(&library, &args, |_| true),
        Command::Find { word, list: args } => {
            let word = word.to_lowercase();
            list(&library, &args, |e| matches(e, &word))
        }
        Command::Show { key } => show(&library, &key),
        Command::Open { key } => open(&library, &key),
        Command::Bib { aux, tag, all: _, format } => bib(&library, aux.as_deref(), &tag, format),
        Command::Check => check(&library),
    }
}

// MARK: - ls, find

fn has_tags(entry: &Entry, tags: &[String]) -> bool {
    let own = entry.tags();
    tags.iter().all(|t| own.contains(&t.as_str()))
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

/// Whether the resolved title, authors or key, or the note, contain `word`, in lower case
fn matches(entry: &Entry, word: &str) -> bool {
    let has = |text: &str| text.to_lowercase().contains(word);
    let field = |key| entry.field(key).and_then(Value::as_str);
    let authors = entry.field("author").and_then(Value::as_array).into_iter().flatten();
    let mut names = authors.filter_map(Value::as_object).flat_map(|a| a.values().filter_map(Value::as_str));
    [field("title"), field("id"), entry.note()].into_iter().flatten().any(has) || names.any(has)
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

fn open(library: &Library, key: &str) -> Result<bool> {
    let entry = match find(library, key).as_slice() {
        [] => return Ok(false),
        [entry] => *entry,
        found => {
            let dirs: Vec<String> = found.iter().map(|e| e.dir.display().to_string()).collect();
            bail!("more than one entry has the key {key}: {}", dirs.join(", "));
        }
    };
    let Some(main) = entry.main_file()? else {
        eprintln!("refdir: {} has no file", entry.dir.display());
        return Ok(false);
    };
    open::that_detached(&main).with_context(|| format!("cannot open {}", main.display()))?;
    Ok(true)
}

// MARK: - bib

/// An entry to write, by the key it is cited by
type Cited<'a> = (String, &'a Entry);

fn bib(library: &Library, aux: Option<&Path>, tags: &[String], format: Format) -> Result<bool> {
    let (cited, ok) = select(library, aux, tags)?;
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
fn select<'a>(library: &'a Library, aux: Option<&Path>, tags: &[String]) -> Result<(Vec<Cited<'a>>, bool)> {
    let mut ok = true;
    let mut cited: Vec<Cited> = Vec::new();
    let taken =
        |cited: &[Cited<'a>], key: &str| cited.iter().find(|(k, _)| refdir::same_id(k, key)).map(|c| c.1);

    let citations = aux.map(citations::read).transpose()?;
    if citations.as_ref().is_none_or(|c| c.all) {
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
    for key in citations.iter().flat_map(|c| &c.keys) {
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
        if !key.chars().all(|c| c.is_ascii_alphanumeric() || "_:-".contains(c)) {
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

/// A folder's name, as `check` says it
fn folder(dir: &Path) -> String {
    dir.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

fn check(library: &Library) -> Result<bool> {
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
    for entry in library.entries.iter().filter(|e| !e.name_matches_id()) {
        let id = entry.id().unwrap_or_default();
        let expected = refdir::folder_name(id);
        out!("warning: {}: the folder name is not {expected} for the key {id}", folder(&entry.dir));
        warnings += 1;
    }
    eprintln!("{} entries: {errors} errors, {warnings} warnings", library.entries.len());
    Ok(errors == 0)
}
