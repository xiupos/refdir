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
