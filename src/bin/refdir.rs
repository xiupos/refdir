//! `refdir`: a library of references from the command line (`FORMAT.md`)

use clap::{Parser, Subcommand};
use refdir::{Bib, Entry, Library, fetch};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

/// INSPIRE allows about 15 requests per 5 seconds
const PAUSE: Duration = Duration::from_millis(350);

/// A library of references: a folder of BibTeX entries
#[derive(Parser)]
#[command(name = "refdir", version)]
struct Cli {
    /// The library folder
    #[arg(long, env = "REFDIR")]
    library: PathBuf,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Add the paper each identifier finds: an arXiv number or link, a DOI or link, a texkey, or an
    /// INSPIRE link. A .bib file, or - for standard input, adds its entries as they are
    Add {
        #[arg(required = true)]
        ids: Vec<String>,
        /// Download each paper's PDF from arXiv too
        #[arg(long)]
        pdf: bool,
    },
    /// Write BibTeX: of the entries a .aux file cites, adding from INSPIRE those the library lacks,
    /// by their keys; without one, of every entry
    Bib { aux: Option<PathBuf> },
    /// Get entries again from INSPIRE, keeping their keys, tags, and notes, and show the fields
    /// that change (+added ~changed -removed): those with the keys given, or all. Writes nothing
    /// without --write
    Update {
        keys: Vec<String>,
        /// Write the entries that change
        #[arg(long)]
        write: bool,
    },
    /// List the entries: key, first author, year, title
    Ls,
    /// List the entries that have every word, ignoring case and marks
    Find {
        #[arg(required = true)]
        words: Vec<String>,
    },
    /// Show an entry's tags, or change them: +TAG adds one, -TAG removes one
    Tag {
        key: String,
        #[arg(allow_hyphen_values = true)]
        changes: Vec<String>,
    },
    /// List the tags, with how many entries have each; or rename one in every entry, or remove it
    Tags {
        #[arg(long, num_args = 2, value_names = ["OLD", "NEW"])]
        rename: Option<Vec<String>>,
        #[arg(long, value_name = "TAG", conflicts_with = "rename")]
        remove: Option<String>,
    },
    /// Open an entry's PDF, or else its folder
    Open { key: String },
    /// List what is wrong in the library: a paper twice, a key twice, a title or names missing,
    /// an entry.bib that cannot be read, and the files Syncthing kept in a conflict
    Check,
}

/// 0: done; 1: something was not found or not done; 2: the command could not run
fn main() -> ExitCode {
    let cli = Cli::parse();
    let library = match Library::read(&cli.library) {
        Ok(library) => library,
        Err(e) => return fail(format!("cannot read the library {}: {e}", cli.library.display())),
    };
    for (dir, e) in &library.unreadable {
        eprintln!("refdir: skipped {}: {e}", dir.display());
    }
    let result = match cli.command {
        Command::Add { ids, pdf } => Ok(all(ids.iter().map(|id| {
            if is_bib(id) { add_file(&cli.library, id, pdf) } else { add(&cli.library, id, None, pdf) }
        }))),
        Command::Bib { aux } => bib(&cli.library, library, aux.as_deref()),
        Command::Update { keys, write } => update(&library, &keys, write),
        Command::Ls => Ok(list(library.entries.iter())),
        Command::Find { words } => Ok(list(library.entries.iter().filter(|e| e.matches(&words.join(" "))))),
        Command::Tag { key, changes } => tag(&library, &key, &changes),
        Command::Tags { rename, remove } => match (rename.as_deref(), remove) {
            (Some([old, new]), _) => retag(&library, old, new),
            (_, Some(tag)) => retag(&library, &tag, ""),
            _ => {
                let mut tags = refdir::tags(&library.entries);
                refdir::by_namespace(&mut tags);
                tags.iter().for_each(|(t, n)| out(&format!("{t}\t{n}\n")));
                Ok(true)
            }
        },
        Command::Open { key } => open(&library, &key),
        Command::Check => check(&cli.library, &library),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => fail(e),
    }
}

fn fail(message: String) -> ExitCode {
    eprintln!("refdir: {message}");
    ExitCode::from(2)
}

/// Writes to standard output, ending quietly when the reader has gone, as with `| head`
fn out(text: &str) {
    if let Err(e) = std::io::stdout().write_all(text.as_bytes()) {
        if e.kind() != std::io::ErrorKind::BrokenPipe {
            eprintln!("refdir: cannot write: {e}");
        }
        std::process::exit(0);
    }
}

fn found<'a>(library: &'a Library, key: &str) -> Result<&'a Entry, String> {
    library.find(key).ok_or(format!("no entry has the key {key}"))
}

/// Whether every one is true; every one is tried, whether those before it were or not
fn all(results: impl Iterator<Item = bool>) -> bool {
    results.fold(true, |all, ok| all & ok)
}

fn list<'a>(entries: impl Iterator<Item = &'a Entry>) -> bool {
    for e in entries {
        out(&format!("{}\t{}\t{}\t{}\n", e.key(), e.byline(), e.year().unwrap_or_default(), e.title()));
    }
    true
}

/// Adds the paper an identifier finds, under `key` if given, and prints its folder. Says whether it
/// did; why not goes to standard error
fn add(folder: &Path, id: &str, key: Option<&str>, pdf: bool) -> bool {
    std::thread::sleep(PAUSE);
    match fetch::find(id) {
        Ok(mut bib) => {
            if let Some(key) = key {
                bib.key = key.to_owned();
            }
            add_bib(folder, id, bib, pdf)
        }
        Err(e) => {
            eprintln!("refdir: {id}: {e}");
            false
        }
    }
}

/// Whether an argument of `add` is BibTeX to read: `-`, or a `.bib` file
fn is_bib(id: &str) -> bool {
    id == "-" || id.to_ascii_lowercase().ends_with(".bib") && Path::new(id).is_file()
}

/// Adds every entry of a `.bib` file, or of standard input, as it is
fn add_file(folder: &Path, path: &str, pdf: bool) -> bool {
    let text = match path {
        "-" => std::io::read_to_string(std::io::stdin()),
        _ => std::fs::read_to_string(path),
    };
    match text.and_then(|t| Bib::parse(&t)) {
        Ok(bibs) if bibs.is_empty() => {
            eprintln!("refdir: {path}: it has no BibTeX entry");
            false
        }
        // Every one is tried
        Ok(bibs) => all(bibs.into_iter().map(|bib| add_bib(folder, &bib.key.clone(), bib, pdf))),
        Err(e) => {
            eprintln!("refdir: {path}: {e}");
            false
        }
    }
}

/// Adds an entry for `bib`, and prints its folder; its PDF too, if asked. `what` names it in errors
fn add_bib(folder: &Path, what: &str, bib: Bib, pdf: bool) -> bool {
    let entry = match refdir::add(folder, bib) {
        Ok(entry) => entry,
        Err(e) => {
            eprintln!("refdir: {what}: {e}");
            return false;
        }
    };
    out(&format!("{}\n", entry.dir.display()));
    if pdf
        && entry.bib.get("eprint").is_some()
        && let Err(e) = fetch::pdf(&entry)
    {
        eprintln!("refdir: {what}: no PDF: {e}");
        return false;
    }
    true
}

fn bib(folder: &Path, library: Library, aux: Option<&Path>) -> Result<bool, String> {
    let Some(aux) = aux else {
        out(&refdir::bibtex(&library.entries));
        return Ok(true);
    };
    let cited = refdir::aux::read(aux).map_err(|e| e.to_string())?;
    let mut ok = true;
    // The keys the library lacks, as INSPIRE's texkeys, kept under the key cited
    for key in cited.keys.iter().filter(|k| library.find(k).is_none()) {
        ok &= add(folder, &format!("texkey:{key}"), Some(key), false);
    }
    let library = Library::read(folder).map_err(|e| e.to_string())?;
    let mut entries: Vec<Entry> = if cited.all { library.entries.clone() } else { Vec::new() };
    for key in &cited.keys {
        match library.find(key) {
            // Written as cited: BibTeX warns of a key in another case
            Some(e) if !entries.iter().any(|x| x.key().eq_ignore_ascii_case(key)) => {
                let mut e = e.clone();
                e.bib.key = key.clone();
                entries.push(e);
            }
            Some(_) => {}
            None => ok = false,
        }
    }
    out(&refdir::bibtex(&entries));
    Ok(ok)
}

fn update(library: &Library, keys: &[String], write: bool) -> Result<bool, String> {
    let entries: Vec<&Entry> = if keys.is_empty() {
        library.entries.iter().collect()
    } else {
        keys.iter().map(|k| found(library, k)).collect::<Result<_, _>>()?
    };
    let (mut ok, mut changed) = (true, 0);
    for entry in entries {
        std::thread::sleep(PAUSE);
        match fetch::again(entry) {
            Ok(bib) if bib == entry.bib => {}
            Ok(bib) => {
                out(&format!("{}\t{}\n", entry.key(), refdir::changes(&entry.bib, &bib).join(" ")));
                if write {
                    Entry { dir: entry.dir.clone(), bib }
                        .write()
                        .map_err(|e| format!("{}: {e}", entry.key()))?;
                }
                changed += 1;
            }
            Err(e) => {
                eprintln!("refdir: {}: {e}", entry.key());
                ok = false;
            }
        }
    }
    match write {
        true => eprintln!("{changed} updated"),
        false => eprintln!("{changed} would change; --write writes them"),
    }
    Ok(ok)
}

fn tag(library: &Library, key: &str, changes: &[String]) -> Result<bool, String> {
    let mut entry = found(library, key)?.clone();
    let mut tags = entry.tags();
    for change in changes {
        let tag = change.get(1..).unwrap_or_default().trim();
        match change.chars().next() {
            Some('+') if !tag.is_empty() && !tags.iter().any(|t| t == tag) => tags.push(tag.to_owned()),
            Some('+') => {}
            Some('-') => tags.retain(|t| t != tag),
            _ => return Err(format!("{change:?} is neither +TAG nor -TAG")),
        }
    }
    if !changes.is_empty() {
        entry.set_tags(&tags);
        entry.write().map_err(|e| e.to_string())?;
    }
    tags.iter().for_each(|t| out(&format!("{t}\n")));
    Ok(true)
}

/// Renames a tag in every entry that has it, or removes it if `new` is empty
fn retag(library: &Library, old: &str, new: &str) -> Result<bool, String> {
    if !new.is_empty() && !refdir::is_tag(new) {
        return Err(format!("{new:?} cannot be a tag"));
    }
    let (mut ok, mut changed) = (true, 0);
    for entry in refdir::rename_tag(&library.entries, old, new) {
        match entry.write() {
            Ok(()) => changed += 1,
            Err(e) => {
                eprintln!("refdir: {}: {e}", entry.key());
                ok = false;
            }
        }
    }
    eprintln!("{changed} changed");
    Ok(ok)
}

fn open(library: &Library, key: &str) -> Result<bool, String> {
    let entry = found(library, key)?;
    let path = entry.pdf().unwrap_or(entry.dir.clone());
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    std::process::Command::new(opener).arg(&path).spawn().map_err(|e| format!("cannot run {opener}: {e}"))?;
    Ok(true)
}

/// Prints each problem as the key or name, a tab, and what is wrong. Says whether there is none
fn check(folder: &Path, library: &Library) -> Result<bool, String> {
    let mut problems = refdir::problems(&library.entries);
    for (dir, e) in &library.unreadable {
        problems.push((name(dir), format!("{}: {e}", refdir::ENTRY)));
    }
    let names = std::fs::read_dir(folder).map_err(|e| e.to_string())?.flatten().map(|e| e.file_name());
    for n in names.map(|n| n.to_string_lossy().into_owned()).filter(|n| refdir::is_conflict(n)) {
        problems.push((n, "kept by Syncthing in a conflict".into()));
    }
    for entry in &library.entries {
        for file in entry.files().into_iter().filter(|f| refdir::is_conflict(f)) {
            problems.push((entry.key().to_owned(), format!("{file}: kept by Syncthing in a conflict")));
        }
    }
    problems.iter().for_each(|(key, problem)| out(&format!("{key}\t{problem}\n")));
    Ok(problems.is_empty())
}

fn name(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().into_owned()
}
