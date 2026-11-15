//! `refdir`: a library of references from the command line (`FORMAT.md`)

use clap::{Parser, Subcommand};
use refdir::{Entry, Library, fetch};
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
    /// INSPIRE link
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
    /// Get entries again from INSPIRE, keeping their keys, tags, and notes: those with the keys
    /// given, or all
    Update { keys: Vec<String> },
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
    /// Open an entry's PDF, or else its folder
    Open { key: String },
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
        Command::Add { ids, pdf } => {
            // Every one is tried, whether those before it were added or not
            let added: Vec<bool> = ids.iter().map(|id| add(&cli.library, id, None, pdf)).collect();
            Ok(added.iter().all(|&a| a))
        }
        Command::Bib { aux } => bib(&cli.library, library, aux.as_deref()),
        Command::Update { keys } => update(&library, &keys),
        Command::Ls => Ok(list(library.entries.iter())),
        Command::Find { words } => Ok(list(library.entries.iter().filter(|e| e.matches(&words.join(" "))))),
        Command::Tag { key, changes } => tag(&library, &key, &changes),
        Command::Open { key } => open(&library, &key),
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
    let added = fetch::find(id).and_then(|mut bib| {
        if let Some(key) = key {
            bib.key = key.to_owned();
        }
        refdir::add(folder, bib).map_err(|e| e.to_string())
    });
    let entry = match added {
        Ok(entry) => entry,
        Err(e) => {
            eprintln!("refdir: {id}: {e}");
            return false;
        }
    };
    out(&format!("{}\n", entry.dir.display()));
    if pdf
        && entry.bib.get("eprint").is_some()
        && let Err(e) = fetch::pdf(&entry)
    {
        eprintln!("refdir: {id}: no PDF: {e}");
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

fn update(library: &Library, keys: &[String]) -> Result<bool, String> {
    let entries: Vec<&Entry> = if keys.is_empty() {
        library.entries.iter().collect()
    } else {
        keys.iter().map(|k| found(library, k)).collect::<Result<_, _>>()?
    };
    let (mut ok, mut updated) = (true, 0);
    for entry in entries {
        std::thread::sleep(PAUSE);
        match fetch::again(entry) {
            Ok(bib) if bib == entry.bib => {}
            Ok(bib) => {
                Entry { dir: entry.dir.clone(), bib }.write().map_err(|e| format!("{}: {e}", entry.key()))?;
                updated += 1;
            }
            Err(e) => {
                eprintln!("refdir: {}: {e}", entry.key());
                ok = false;
            }
        }
    }
    eprintln!("{updated} updated");
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

fn open(library: &Library, key: &str) -> Result<bool, String> {
    let entry = found(library, key)?;
    let path = entry.pdf().unwrap_or(entry.dir.clone());
    let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    std::process::Command::new(opener).arg(&path).spawn().map_err(|e| format!("cannot run {opener}: {e}"))?;
    Ok(true)
}
