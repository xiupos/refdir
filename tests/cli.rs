//! The commands, on a library of the examples in `schema/v1.json`

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use tempfile::TempDir;

/// A library of the examples, without the first, which has the second's key
fn library() -> TempDir {
    let schema = fs::read_to_string("schema/v1.json").unwrap();
    let schema: Value = serde_json::from_str(&schema).unwrap();
    let lib = TempDir::new().unwrap();
    let names =
        ["Maldacena-1997re", "ATLAS-2012yve", "inspire-2990001", "Takagi-1971", "Gauss-1966", "pyhf", "x"];
    for (name, example) in names.iter().zip(&schema["examples"].as_array().unwrap()[1..]) {
        write(lib.path(), name, example);
    }
    lib
}

fn write(lib: &Path, name: &str, value: &Value) {
    fs::create_dir_all(lib.join(name)).unwrap();
    fs::write(lib.join(name).join("entry.json"), value.to_string()).unwrap();
}

fn refdir(lib: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_refdir")).env("REFDIR", lib).args(args).output().unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

const MALDACENA: &str = "@article{Maldacena:1997re,
    author = {Maldacena, Juan Martin},
    title = {{The Large N limit of superconformal field theories and supergravity}},
    journal = {Adv. Theor. Math. Phys.},
    volume = {2},
    pages = {231--252},
    year = {1997},
    month = nov,
    doi = {10.4310/ATMP.1998.v2.n2.a1},
    eprint = {hep-th/9711200},
    archivePrefix = {arXiv},
}
";

const ATLAS: &str = "@article{ATLAS:2012yve,
    author = {{ATLAS Collaboration}},
    title = {{Observation of a new particle in the search for the Standard Model Higgs boson with the ATLAS detector at the LHC}},
    journal = {Phys. Lett. B},
    volume = {716},
    pages = {1--29},
    year = {2012},
    month = jul,
    doi = {10.1016/j.physletb.2012.08.020},
    eprint = {1207.7214},
    archivePrefix = {arXiv},
}
";

const INSPIRE: &str = "@article{inspire:2990001,
    author = {Example, Alex},
    title = {{An example paper without a texkey}},
    year = {2026},
    month = nov,
    eprint = {2611.01234},
    archivePrefix = {arXiv},
}
";

#[test]
fn bibtex_of_the_cited_entries() {
    let lib = library();
    let aux = lib.path().join(".main.aux");
    fs::write(&aux, "\\citation{maldacena:1997re,ATLAS:2012yve}\n\\abx@aux@cite{0}{inspire:2990001}\n")
        .unwrap();
    let out = refdir(lib.path(), &["bib", aux.to_str().unwrap()]);
    assert!(out.status.success());
    // By the key it is cited by
    assert_eq!(
        stdout(&out),
        [MALDACENA.replacen("Maldacena:", "maldacena:", 1), ATLAS.into(), INSPIRE.into()].join("\n")
    );
}

#[test]
fn keys_not_in_the_library_are_said() {
    let lib = library();
    let aux = lib.path().join(".main.aux");
    fs::write(&aux, "\\citation{ATLAS:2012yve,Nobody:2000}\n").unwrap();
    let out = refdir(lib.path(), &["bib", aux.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(stdout(&out), ATLAS);
    assert!(String::from_utf8_lossy(&out.stderr).contains("Nobody:2000"));
}

#[test]
fn bib_by_tag_and_all() {
    let lib = library();
    let out = refdir(lib.path(), &["bib", "--tag", "topic:number-theory"]);
    assert!(stdout(&out).starts_with("@book{Gauss:1966,"));
    assert!(stdout(&out).contains("\n@book{Takagi:1971,\n    author = {高木, 貞治},\n"));

    let out = refdir(lib.path(), &["bib", "--all", "--format", "csl-json"]);
    assert!(out.status.success());
    let items: Value = serde_json::from_slice(&out.stdout).unwrap();
    let ids: Vec<&str> = items.as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert_eq!(
        ids,
        ["ATLAS:2012yve", "Gauss:1966", "inspire:2990001", "Maldacena:1997re", "pyhf", "Takagi:1971"]
    );
    assert_eq!(items[3]["note"], "Reprinted in Int. J. Theor. Phys. 38 (1999) 1113–1133.");

    assert_eq!(refdir(lib.path(), &["bib"]).status.code(), Some(2));
}

#[test]
fn ls_and_find() {
    let lib = library();
    let out = stdout(&refdir(lib.path(), &["ls"]));
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 7);
    assert!(lines.contains(&"Takagi:1971\t高木\t1971\t初等整数論講義"));
    assert!(lines.contains(&"pyhf\tHeinrich\t\tpyhf"));
    assert!(lines.contains(&"[x]\t\t\t"));

    assert_eq!(stdout(&refdir(lib.path(), &["ls", "-q", "--tag", "status:read"])), "Maldacena:1997re\n");
    // Title, author, key, note; ignoring case
    assert_eq!(stdout(&refdir(lib.path(), &["find", "-q", "GAUSS"])), "Gauss:1966\n");
    assert_eq!(stdout(&refdir(lib.path(), &["find", "-q", "相関関数"])), "Maldacena:1997re\n");
    assert_eq!(stdout(&refdir(lib.path(), &["find", "-q", "heinrich"])), "pyhf\n");
    assert_eq!(stdout(&refdir(lib.path(), &["find", "-q", "inspire:"])), "inspire:2990001\n");
    // Every word; marks ignored
    assert_eq!(stdout(&refdir(lib.path(), &["find", "-q", "maldacena", "1997"])), "Maldacena:1997re\n");
    assert_eq!(stdout(&refdir(lib.path(), &["find", "-q", "maldacena", "2012"])), "");

    let out = stdout(&refdir(lib.path(), &["ls", "--json", "--tag", "status:read"]));
    let line: Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(line["csl"]["id"], "Maldacena:1997re");
    assert_eq!(line["tags"][1], "topic:AdS/CFT");
}

#[test]
fn show() {
    let lib = library();
    fs::write(lib.path().join("ATLAS-2012yve/paper.pdf"), "").unwrap();
    let out = refdir(lib.path(), &["show", "atlas:2012YVE"]);
    let text = stdout(&out);
    assert!(text.starts_with("ATLAS:2012yve\n"));
    assert!(text.contains("main:    paper.pdf\n"));
    assert!(text.contains("source:  inspire (csl.id ATLAS:2012yve, recid 1124337, arXiv 1207.7214)\n"));
    assert_eq!(refdir(lib.path(), &["show", "Nobody:2000"]).status.code(), Some(1));
}

#[test]
fn check() {
    let lib = library();
    let out = refdir(lib.path(), &["check"]);
    assert!(out.status.success(), "{}", stdout(&out));
    assert_eq!(stdout(&out), "");

    // The first example has the second's key and INSPIRE record
    let schema: Value = serde_json::from_str(&fs::read_to_string("schema/v1.json").unwrap()).unwrap();
    write(lib.path(), "old-name", &schema["examples"][0]);
    write(lib.path(), "bad", &serde_json::json!({ "v": 1, "added": "yesterday" }));
    write(lib.path(), "later", &serde_json::json!({ "v": 2 }));
    let out = refdir(lib.path(), &["check"]);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("error: bad: /added: "));
    assert!(text.contains("error: 2 entries have the key Maldacena:1997re: Maldacena-1997re, old-name\n"));
    assert!(text.contains("error: 2 entries have INSPIRE record 451647: Maldacena-1997re, old-name\n"));
    assert!(text.contains(
        "warning: old-name: the folder name is not Maldacena-1997re for the key Maldacena:1997re\n"
    ));
    assert!(text.contains("warning: later: version 2, skipped\n"));
}

#[test]
fn no_library() {
    let out = Command::new(env!("CARGO_BIN_EXE_refdir")).env_remove("REFDIR").arg("ls").output().unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn several_libraries() {
    let lib = library();
    let other = TempDir::new().unwrap();
    let schema: Value = serde_json::from_str(&fs::read_to_string("schema/v1.json").unwrap()).unwrap();
    // The first example has the key and INSPIRE record of Maldacena-1997re in `lib`
    write(other.path(), "Maldacena-1997re", &schema["examples"][0]);
    write(other.path(), "Witten-1998", &serde_json::json!({ "v": 1, "csl": { "id": "Witten:1998" } }));

    // $REFDIR is separated as PATH is; empty parts are left out
    let both = std::env::join_paths([lib.path(), Path::new(""), other.path()]).unwrap();
    let out = refdir(Path::new(&both), &["ls", "-q"]);
    let keys = stdout(&out);
    assert!(keys.starts_with("ATLAS:2012yve\n"));
    assert!(keys.ends_with("Maldacena:1997re\nWitten:1998\n"));

    // --library, repeated, replaces $REFDIR
    let (a, b) = (lib.path().to_str().unwrap(), other.path().to_str().unwrap());
    let out = refdir(Path::new("/nonexistent"), &["show", "witten:1998", "--library", b, "--library", a]);
    assert!(out.status.success());

    // A key in both: the earlier library's entry, a warning and status 1
    let out = refdir(lib.path(), &["--library", b, "--library", a, "bib", "--all", "--format", "csl-json"]);
    assert_eq!(out.status.code(), Some(1));
    let items: Value = serde_json::from_slice(&out.stdout).unwrap();
    let maldacena = items.as_array().unwrap().iter().find(|i| i["id"] == "Maldacena:1997re").unwrap();
    assert_eq!(maldacena["note"], "Int.J.Theor.Phys. 38 (1999) 1113");
    assert!(String::from_utf8_lossy(&out.stderr).contains("another entry has the key Maldacena:1997re"));

    // check names folders by their path
    let out = refdir(Path::new(&both), &["check"]);
    assert_eq!(out.status.code(), Some(1));
    let (m1, m2) = (lib.path().join("Maldacena-1997re"), other.path().join("Maldacena-1997re"));
    let expected =
        format!("error: 2 entries have the key Maldacena:1997re: {}, {}\n", m1.display(), m2.display());
    assert!(stdout(&out).contains(&expected), "{}", stdout(&out));

    let out = refdir(lib.path(), &["--library", "/nonexistent", "ls"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn tag() {
    let lib = library();
    // The library's tags, by namespace, then name
    let out = stdout(&refdir(lib.path(), &["tag"]));
    assert_eq!(
        out,
        "1\tHistFactory\n1\tstatus:read\n1\tstatus:reading\n1\tstatus:to-read\n\
         1\ttopic:AdS/CFT\n2\ttopic:number-theory\n1\ttopic:statistics\n"
    );
    assert_eq!(stdout(&refdir(lib.path(), &["tag", "gauss:1966"])), "status:to-read\ntopic:number-theory\n");

    // Changes in order; `status:` is a namespace like any other
    let changes = ["-status:to-read", "+status:reading", "-topic:number-theory", "+Gauss"];
    let out = refdir(lib.path(), &[&["tag", "Gauss:1966"][..], &changes].concat());
    assert!(out.status.success());
    assert_eq!(stdout(&out), "status:reading\nGauss\n");
    assert_eq!(
        stdout(&refdir(lib.path(), &["ls", "-q", "--tag", "status:reading"])),
        "Gauss:1966\nTakagi:1971\n"
    );
    // Case matters
    assert_eq!(
        stdout(&refdir(lib.path(), &["ls", "-q", "--tag", "Gauss"])),
        "Gauss:1966
"
    );
    assert_eq!(stdout(&refdir(lib.path(), &["ls", "-q", "--tag", "gauss"])), "");

    // Nothing is written when a change is wrong
    let before = fs::read_to_string(lib.path().join("Gauss-1966/entry.json")).unwrap();
    for change in ["+", "x"] {
        assert_eq!(
            refdir(lib.path(), &["tag", "Gauss:1966", "+y", change]).status.code(),
            Some(2),
            "{change:?}"
        );
    }
    assert_eq!(fs::read_to_string(lib.path().join("Gauss-1966/entry.json")).unwrap(), before);
    assert_eq!(refdir(lib.path(), &["tag", "Nobody:2000", "+x"]).status.code(), Some(1));

    // Removing the last tag leaves no `tags`
    refdir(lib.path(), &["tag", "pyhf", "-topic:statistics", "-HistFactory"]);
    let pyhf: Value =
        serde_json::from_str(&fs::read_to_string(lib.path().join("pyhf/entry.json")).unwrap()).unwrap();
    assert!(pyhf.get("tags").is_none());
}

#[test]
fn create() {
    let lib = library();
    let item = lib.path().join(".item.json");
    let csl = r#"[{ "id": "Witten:1998qj", "type": "article-journal", "title": "Anti-de Sitter space and holography", "author": [{ "family": "Witten", "given": "Edward" }] }]"#;
    fs::write(&item, csl).unwrap();
    // Outside the entries: a folder starting with `.` is not one
    fs::create_dir(lib.path().join(".in")).unwrap();
    let pdf = lib.path().join(".in/paper.pdf");
    fs::write(&pdf, "%PDF-").unwrap();
    let out = refdir(lib.path(), &["create", item.to_str().unwrap(), "--file", pdf.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let dir = lib.path().join("Witten-1998qj");
    assert_eq!(stdout(&out).trim(), dir.to_str().unwrap());
    let entry: Value = serde_json::from_str(&fs::read_to_string(dir.join("entry.json")).unwrap()).unwrap();
    assert_eq!(entry["csl"]["title"], "Anti-de Sitter space and holography");
    assert!(entry["added"].as_str().unwrap().ends_with('Z'));
    assert_eq!(fs::read_to_string(dir.join("paper.pdf")).unwrap(), "%PDF-");
    assert!(refdir(lib.path(), &["check"]).status.success());

    // A key taken, ignoring case; a key BibTeX cannot have; another key with --key, as a source
    let out = refdir(lib.path(), &["create", item.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    let out = refdir(lib.path(), &["create", "--key", "a b", item.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    let out = refdir(lib.path(), &["create", "--key", "W:1998", "--source", "mine", item.to_str().unwrap()]);
    assert!(out.status.success());
    let entry: Value =
        serde_json::from_str(&fs::read_to_string(lib.path().join("W-1998/entry.json")).unwrap()).unwrap();
    assert_eq!(entry["sources"]["mine"]["csl"]["id"], "W:1998");
}

#[test]
fn create_refuses_what_check_would() {
    let lib = library();
    let item = lib.path().join(".item.json");
    // Crossref's CSL-JSON, as doi.org gives it
    fs::write(
        &item,
        r#"{ "id": "X:2000a", "type": "journal-article", "title": "T", "ISSN": ["0556-2821"] }"#,
    )
    .unwrap();
    let before = fs::read_dir(lib.path()).unwrap().count();
    let out = refdir(lib.path(), &["create", item.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("/csl/type"));
    assert_eq!(fs::read_dir(lib.path()).unwrap().count(), before);
}

#[test]
fn create_many_with_keys_made_and_dois_free() {
    let lib = library();
    let items = lib.path().join(".items.json");
    // Two items without a key, one with a key BibTeX cannot have, and one whose DOI is taken
    let csl = r#"[
        { "type": "book", "title": "The TeXbook", "author": [{ "family": "Knuth", "given": "Donald E." }], "issued": { "date-parts": [[1984]] } },
        { "id": "http://zotero.org/items/X", "type": "book", "title": "TAOCP", "author": [{ "family": "Knuth" }], "issued": { "date-parts": [[1984]] } }
    ]"#;
    fs::write(&items, csl).unwrap();
    let out = refdir(lib.path(), &["create", items.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout(&out).ends_with("Knuth-1984a\n"));
    for name in ["Knuth-1984", "Knuth-1984a"] {
        assert!(lib.path().join(name).join("entry.json").is_file(), "{name}");
    }

    // The Maldacena example's DOI, in another case: nothing is written
    let taken = r#"[{ "type": "book", "title": "New" }, { "type": "article", "title": "Again", "DOI": "10.4310/atmp.1998.V2.n2.a1" }]"#;
    fs::write(&items, taken).unwrap();
    let before = fs::read_dir(lib.path()).unwrap().count();
    let out = refdir(lib.path(), &["create", items.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("item 2: "));
    assert_eq!(fs::read_dir(lib.path()).unwrap().count(), before);
}

#[test]
fn create_cleans_what_doi_org_gives() {
    let lib = library();
    let item = lib.path().join(".item.json");
    let crossref = r#"{ "id": "https://doi.org/10.1/x", "type": "journal-article", "title": "T", "ISSN": ["0556-2821"],
        "author": [{ "family": "Doe", "given": "J.", "sequence": "first" }], "issued": { "date-parts": [[2001]] }, "DOI": "10.1/x", "score": 1 }"#;
    fs::write(&item, crossref).unwrap();
    let out = refdir(lib.path(), &["create", "--clean", item.to_str().unwrap()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let entry: Value =
        serde_json::from_str(&fs::read_to_string(lib.path().join("Doe-2001/entry.json")).unwrap()).unwrap();
    assert_eq!(entry["csl"]["type"], "article-journal");
    assert_eq!(entry["csl"]["id"], "Doe:2001");
    assert!(refdir(lib.path(), &["check"]).status.success());
}

#[test]
fn note_edit_attach_and_bib_by_key() {
    let lib = library();
    let key = "Maldacena:1997re";
    assert!(refdir(lib.path(), &["note", key, "Read §3"]).status.success());
    assert_eq!(stdout(&refdir(lib.path(), &["note", key])), "Read §3\n");
    assert!(refdir(lib.path(), &["note", key, ""]).status.success());
    assert_eq!(stdout(&refdir(lib.path(), &["note", key])), "");

    assert!(refdir(lib.path(), &["edit", key, "title", "Large N"]).status.success());
    let out = refdir(lib.path(), &["edit", key, "author", "--json", r#""not a list""#]);
    assert_eq!(out.status.code(), Some(2));
    // Taken by another entry, ignoring case
    assert_eq!(refdir(lib.path(), &["edit", key, "id", "atlas:2012YVE"]).status.code(), Some(2));
    assert!(refdir(lib.path(), &["check"]).status.success());
    // Found ignoring case, and written as asked for, as a .aux file's keys are
    let bib = stdout(&refdir(lib.path(), &["bib", "--key", "maldacena:1997RE"]));
    assert!(bib.starts_with("@article{maldacena:1997RE,") && bib.contains("{{Large N}}"), "{bib}");

    let file = lib.path().join(".in.txt");
    fs::write(&file, "x").unwrap();
    let out = stdout(&refdir(lib.path(), &["attach", key, file.to_str().unwrap(), file.to_str().unwrap()]));
    let dir = lib.path().join("Maldacena-1997re");
    assert_eq!(out, format!("{}\n{}\n", dir.join("in.txt").display(), dir.join("in-2.txt").display()));
}
