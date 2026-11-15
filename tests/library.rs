//! The rules of `FORMAT.md` that need folders, on libraries in temporary folders; the others are
//! `cases.json`

use refdir::{Bib, ENTRY, Entry, Library};
use std::fs;
use std::path::Path;
use tempfile::TempDir;

/// INSPIRE's BibTeX, as `format=bibtex` gives it
const MALDACENA: &str = r#"@article{Maldacena:1997re,
    author = "Maldacena, Juan Martin",
    title = "{The Large $N$ limit of superconformal field theories and supergravity}",
    eprint = "hep-th/9711200",
    archivePrefix = "arXiv",
    doi = "10.4310/ATMP.1998.v2.n2.a1",
    journal = "Adv. Theor. Math. Phys.",
    volume = "2",
    pages = "231--252",
    year = "1998"
}
"#;

fn bib(text: &str) -> Bib {
    Bib::parse(text).unwrap().remove(0)
}

fn write(library: &Path, key: &str, text: &str) {
    fs::create_dir(library.join(key)).unwrap();
    fs::write(library.join(key).join(ENTRY), text).unwrap();
}

#[test]
fn an_entry_is_shown_by_its_title_names_and_year() {
    let lib = TempDir::new().unwrap();
    write(lib.path(), "Maldacena:1997re", MALDACENA);
    let e = Entry::read(&lib.path().join("Maldacena:1997re")).unwrap();
    assert_eq!(e.title(), "The Large N limit of superconformal field theories and supergravity");
    assert_eq!(e.byline(), "J.M. Maldacena");
    assert_eq!(e.year().as_deref(), Some("1998"));
    let atlas = bib("@article{A, author = {Aad, Georges and others}, title = {$H\\to\\gamma\\gamma$}}");
    let atlas = Entry { dir: lib.path().join("A"), bib: atlas };
    assert_eq!(atlas.byline(), "G. Aad et al.");
    assert_eq!(atlas.names(), ["G. Aad"]);
    assert_eq!(atlas.title(), "H→γγ");
}

#[test]
fn the_folder_name_is_the_key() {
    let lib = TempDir::new().unwrap();
    write(lib.path(), "Mine", MALDACENA);
    write(lib.path(), "a", "@misc{a, title = {A}}");
    write(lib.path(), ".app", "@misc{x}");
    write(lib.path(), "broken", "@misc{one} @misc{two}");
    let library = Library::read(lib.path()).unwrap();
    let keys: Vec<&str> = library.entries.iter().map(Entry::key).collect();
    assert_eq!(keys, ["a", "Mine"]);
    assert_eq!(library.unreadable.len(), 1);
    assert!(library.find("MINE").is_some());
}

#[test]
fn adding_takes_a_free_key_and_refuses_the_same_paper() {
    let lib = TempDir::new().unwrap();
    write(lib.path(), "maldacena:1997RE", "@misc{x, title = {Another}}");
    let entry = refdir::add(lib.path(), bib(MALDACENA)).unwrap();
    assert_eq!(entry.key(), "Maldacena:1997rea");
    assert_eq!(fs::read_to_string(entry.dir.join(ENTRY)).unwrap(), entry.bib.to_bibtex());
    // The same DOI, in another case
    let again = bib(&MALDACENA.replace("Maldacena:1997re", "X").replace("ATMP", "atmp"));
    assert_eq!(refdir::add(lib.path(), again).unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);
    let mut spaced = bib("@misc{x, title = {x}}");
    spaced.key = "a b".into();
    assert!(refdir::add(lib.path(), spaced).is_err());
}

#[test]
fn tags_are_keywords_and_the_note_is_comment() {
    let lib = TempDir::new().unwrap();
    let mut e = refdir::add(lib.path(), bib(MALDACENA)).unwrap();
    e.set_tags(&["to-read".into(), "topic:AdS/CFT".into()]);
    e.set_note("Read §3.");
    e.write().unwrap();
    let e = Entry::read(&e.dir).unwrap();
    assert_eq!(e.tags(), ["to-read", "topic:AdS/CFT"]);
    assert_eq!(e.note(), "Read §3.");
    assert_eq!(
        refdir::tags(&Library::read(lib.path()).unwrap().entries),
        [("to-read".into(), 1), ("topic:AdS/CFT".into(), 1)]
    );
    let mut e = e;
    e.set_note(" ");
    assert!(e.bib.get("comment").is_none());
}

#[test]
fn files_are_added_under_free_names_and_the_first_pdf_is_read() {
    let lib = TempDir::new().unwrap();
    let e = refdir::add(lib.path(), bib(MALDACENA)).unwrap();
    assert_eq!(e.add_file("Paper.pdf", b"%PDF-").unwrap(), e.dir.join("Paper.pdf"));
    assert_eq!(e.add_file("paper.PDF", b"x").unwrap(), e.dir.join("paper-2.PDF"));
    assert_eq!(e.add_file(".hidden", b"x").unwrap(), e.dir.join("hidden"));
    assert_eq!(e.files().len(), 3);
    // The first by name is not a PDF
    assert_eq!(e.pdf(), None);
    fs::remove_file(e.dir.join("hidden")).unwrap();
    assert_eq!(e.pdf(), Some(e.dir.join("paper-2.PDF")));
    let names: Vec<_> = fs::read_dir(&e.dir).unwrap().map(|f| f.unwrap().file_name()).collect();
    assert!(names.iter().all(|n| !n.to_string_lossy().starts_with('.')));
}
