mod bibtex;
mod citations;
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
            eprintln!("refdir: warning: skipped {}: {e}", dir.display());
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

fn has_tags(entry: &Entry, tags: &[String]) -> bool {
    let own = entry.tags();
    tags.iter().all(|t| own.contains(&t.as_str()))
}

fn list(library: &Library, args: &ListArgs, keep: impl Fn(&Entry) -> bool) -> Result<bool> {
    let entries = library.entries.iter().filter(|e| has_tags(e, &args.tag) && keep(e));
    for entry in entries {
        let item = entry.resolved();
        let id = entry.id().unwrap_or_default();
        if args.q {
            if !id.is_empty() {
                out!("{id}");
            }
        } else if args.json {
            out!("{}", summary(entry, item));
        } else {
            let title = item.get("title").and_then(Value::as_str).unwrap_or_default();
            let year = item.get("issued").map(year).unwrap_or_default();
            // An entry without a key by its folder, in brackets, which a key cannot have
            let id = if id.is_empty() { format!("[{}]", entry.name().to_string_lossy()) } else { id };
            out!("{id}\t{}\t{year}\t{}", first_author(&item), one_line(title));
        }
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

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The first author's family name, or the name as it is
fn first_author(item: &Map<String, Value>) -> String {
    let Some(first) = item.get("author").and_then(|a| a.get(0)) else { return String::new() };
    let part = |key: &str| first.get(key).and_then(Value::as_str);
    one_line(part("literal").or(part("family")).or(part("given")).unwrap_or_default())
}

/// The year of a CSL date: of its first `date-parts`, or the digits `raw` starts with
fn year(date: &Value) -> String {
    if let Some(year) = date.get("date-parts").and_then(|p| p.get(0)).and_then(|p| p.get(0)) {
        return year.as_str().map_or_else(|| year.to_string(), str::to_owned);
    }
    let raw = date.get("raw").and_then(Value::as_str).unwrap_or_default().trim();
    raw.chars().take_while(char::is_ascii_digit).collect()
}

/// Whether the resolved title, authors or key, or the note, contain `word`, in lower case
fn matches(entry: &Entry, word: &str) -> bool {
    let item = entry.resolved();
    let has = |text: &str| text.to_lowercase().contains(word);
    let authors = item.get("author").and_then(Value::as_array).into_iter().flatten();
    let names = authors.filter_map(Value::as_object).flat_map(|a| a.values().filter_map(Value::as_str));
    let mut texts = ["title", "id"].into_iter().filter_map(|k| item.get(k).and_then(Value::as_str));
    texts.any(has) || names.into_iter().any(has) || entry.note().is_some_and(has)
}

/// The entries with the key. None is said on standard error
fn find<'a>(library: &'a Library, key: &str) -> Vec<&'a Entry> {
    let found = library.find(key);
    match found.len() {
        0 => eprintln!("refdir: no entry has the key {key}"),
        1 => {}
        n => eprintln!("refdir: warning: {n} entries have the key {key}"),
    }
    found
}

fn show(library: &Library, key: &str) -> Result<bool> {
    let found = find(library, key);
    if found.is_empty() {
        return Ok(false);
    }
    for (i, entry) in found.iter().enumerate() {
        if i > 0 {
            out!();
        }
        out!("{}", entry.id().unwrap_or_default());
        out!("folder:  {}", entry.dir.display());
        let files = entry.files().with_context(|| format!("cannot read {}", entry.dir.display()))?;
        let files: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
        if !files.is_empty() {
            out!("files:   {}", files.join(", "));
        }
        if let Some(main) = entry.main_file()? {
            out!("main:    {}", main.file_name().unwrap_or_default().to_string_lossy());
        }
        if !entry.tags().is_empty() {
            out!("tags:    {}", entry.tags().join(", "));
        }
        for (name, source) in entry.data.get("sources").and_then(Value::as_object).into_iter().flatten() {
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
    Ok(true)
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
    let found = find(library, key);
    if found.is_empty() {
        return Ok(false);
    }
    if found.len() > 1 {
        let dirs: Vec<String> = found.iter().map(|e| e.dir.display().to_string()).collect();
        bail!("more than one entry has the key {key}: {}", dirs.join(", "));
    }
    let Some(main) = found[0].main_file()? else {
        eprintln!("refdir: {} has no file", found[0].dir.display());
        return Ok(false);
    };
    open::that_detached(&main).with_context(|| format!("cannot open {}", main.display()))?;
    Ok(true)
}

fn bib(library: &Library, aux: Option<&Path>, tags: &[String], format: Format) -> Result<bool> {
    let mut ok = true;
    // The entries, each with the key it is cited by
    let mut entries: Vec<(String, &Entry)> = Vec::new();
    // Says whether the key is new
    fn add<'a>(entries: &mut Vec<(String, &'a Entry)>, key: String, entry: &'a Entry) -> bool {
        if entries.iter().any(|(k, _)| refdir::same_id(k, &key)) {
            return entries.iter().any(|(_, e)| e.dir == entry.dir);
        }
        entries.push((key, entry));
        true
    }
    let citations = aux.map(citations::read).transpose()?;
    if citations.as_ref().is_none_or(|c| c.all) {
        let tagged = library.entries.iter().filter(|e| has_tags(e, tags));
        for entry in tagged {
            match entry.id() {
                Some(id) if !add(&mut entries, id.clone(), entry) => {
                    eprintln!(
                        "refdir: warning: skipped {}: another entry has the key {id}",
                        entry.dir.display()
                    );
                    ok = false;
                }
                Some(_) => {}
                None => eprintln!("refdir: warning: skipped {}: it has no key", entry.dir.display()),
            }
        }
    }
    for key in citations.iter().flat_map(|c| &c.keys) {
        match library.find(key).as_slice() {
            [] => {
                eprintln!("refdir: warning: no entry has the key {key}");
                ok = false;
            }
            [entry] => drop(add(&mut entries, key.clone(), entry)),
            found => {
                eprintln!("refdir: warning: {} entries have the key {key}; the first is used", found.len());
                ok = false;
                add(&mut entries, key.clone(), found[0]);
            }
        }
    }

    match format {
        Format::Bibtex => {
            let mut out = Vec::new();
            for (key, entry) in &entries {
                if !key.chars().all(|c| c.is_ascii_alphanumeric() || "_:-".contains(c)) {
                    eprintln!(
                        "refdir: warning: skipped {}: the key {key:?} cannot be BibTeX's",
                        entry.dir.display()
                    );
                    ok = false;
                    continue;
                }
                let arxiv = entry.data.get("sources").and_then(|s| s["inspire"]["arxiv"].as_str());
                out.push(bibtex::entry(key, &entry.resolved(), arxiv));
            }
            out_raw!("{}", out.join("\n"));
        }
        Format::CslJson => {
            let items: Vec<Value> = entries
                .iter()
                .map(|(key, entry)| {
                    let mut item = entry.resolved();
                    item.insert("id".to_owned(), json!(key));
                    Value::Object(item)
                })
                .collect();
            out!("{}", serde_json::to_string_pretty(&items)?);
        }
    }
    Ok(ok)
}

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
            // A later version's entry, which this version does not check
            refdir::Error::NotV1(Some(v)) if v.is_u64() => {
                out!("warning: {}: version {v}, not checked", folder(dir));
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
    let dirs = |entries: &[&Entry]| entries.iter().map(|e| folder(&e.dir)).collect::<Vec<_>>();
    for (_, entries) in library.duplicate_ids() {
        let id = entries[0].id().unwrap_or_default();
        error(format!("{} entries have the key {id}: {}", entries.len(), dirs(&entries).join(", ")));
    }
    for (recid, entries) in library.duplicate_recids() {
        error(format!(
            "{} entries have INSPIRE record {recid}: {}",
            entries.len(),
            dirs(&entries).join(", ")
        ));
    }
    for entry in library.entries.iter().filter(|e| !e.name_matches_id()) {
        let id = entry.id().unwrap_or_default();
        out!(
            "warning: {}: the folder name is not {} for the key {id}",
            folder(&entry.dir),
            refdir::folder_name(&id)
        );
        warnings += 1;
    }
    eprintln!("{} entries: {errors} errors, {warnings} warnings", library.entries.len());
    Ok(errors == 0)
}
