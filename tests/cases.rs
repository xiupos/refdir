//! The examples of `tests/cases.json`, which every refdir program runs

use refdir::{Bib, Chunk, Entry, fetch};
use serde_json::{Value, json};

fn cases(section: &str) -> Vec<Value> {
    static CASES: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!("cases.json")).expect("cases.json is JSON")
    });
    CASES[section].as_array().unwrap_or_else(|| panic!("no section {section}")).clone()
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().map(|x| s(x).to_owned()).collect()).unwrap_or_default()
}

fn bib(text: &str) -> Bib {
    Bib::parse(text).unwrap().remove(0)
}

fn entry(key: &str, text: &str) -> Entry {
    let mut bib = bib(text);
    bib.key = key.to_owned();
    Entry { dir: key.into(), bib }
}

#[test]
fn bibtex() {
    for c in cases("bibtex") {
        let parsed = Bib::parse(s(&c["bibtex"]));
        if c["error"] == true {
            assert!(parsed.is_err(), "{c}");
            continue;
        }
        let parsed = parsed.unwrap();
        let entries: Vec<Value> = parsed
            .iter()
            .map(|b| {
                let chunk = |c: &Chunk| match c {
                    Chunk::Text(t) => json!({"text": t}),
                    Chunk::Macro(m) => json!({"macro": m}),
                };
                let fields: Vec<Value> = b
                    .fields
                    .iter()
                    .map(|(n, v)| json!([n, v.iter().map(chunk).collect::<Vec<_>>()]))
                    .collect();
                json!({"kind": b.kind, "key": b.key, "fields": fields})
            })
            .collect();
        assert_eq!(Value::from(entries), c["entries"], "{}", c["about"]);
        assert_eq!(
            refdir::bibtex(&parsed.into_iter().map(|bib| Entry { dir: "".into(), bib }).collect::<Vec<_>>()),
            s(&c["written"])
        );
    }
}

#[test]
fn keys() {
    for c in cases("keys") {
        assert_eq!(refdir::is_key(s(&c[0])), c[1] == true, "{c}");
    }
}

#[test]
fn names() {
    for c in cases("names") {
        let mut e = entry("x", "@misc{x,}");
        for field in ["author", "editor"] {
            if let Some(v) = c[field].as_str() {
                e.bib.set(field, v);
            }
        }
        assert_eq!(e.names(), strings(&c["names"]), "{c}");
        assert_eq!(e.byline(), s(&c["byline"]), "{c}");
    }
}

#[test]
fn plain() {
    for c in cases("plain") {
        assert_eq!(refdir::plain(s(&c[0])), s(&c[1]), "{c}");
    }
}

#[test]
fn tags() {
    for c in cases("tags") {
        let mut e = entry("x", "@misc{x,}");
        e.bib.set("keywords", s(&c["keywords"]));
        assert_eq!(e.tags(), strings(&c["tags"]), "{c}");
    }
}

#[test]
fn find() {
    for c in cases("find") {
        let b = bib(s(&c["bibtex"]));
        let e = entry(&b.key.clone(), s(&c["bibtex"]));
        for search in strings(&c["matches"]) {
            assert!(e.matches(&search), "{search:?}");
        }
        for search in strings(&c["not"]) {
            assert!(!e.matches(&search), "{search:?}");
        }
    }
}

#[test]
fn add() {
    for c in cases("add") {
        let entries: Vec<Entry> =
            c["entries"].as_object().unwrap().iter().map(|(k, v)| entry(k, s(v))).collect();
        let mut folders: Vec<String> = entries.iter().map(|e| e.key().to_owned()).collect();
        folders.extend(strings(&c["folders"]));
        let mut new = bib(s(&c["bibtex"]));
        if let Some(key) = c["as"].as_str() {
            new.key = key.to_owned();
        }
        let key = refdir::new_key(&entries, &folders, &new);
        match (c["key"].as_str(), c["same"].as_str()) {
            (Some(k), _) => assert_eq!(key.unwrap(), k, "{}", c["about"]),
            (_, Some(k)) => assert_eq!(key.unwrap_err().to_string(), format!("the library has it, as {k}")),
            _ => assert!(key.is_err() && c["invalid"] == true, "{}", c["about"]),
        }
    }
}

#[test]
fn files() {
    for c in cases("files") {
        let files = refdir::files(strings(&c["names"]));
        assert_eq!(files, strings(&c["files"]), "{c}");
        assert_eq!(files.first().is_some_and(|f| refdir::is_pdf(f)), c["pdf"] == true, "{c}");
    }
    for c in cases("free_names") {
        assert_eq!(refdir::free_name(s(&c["name"]), &strings(&c["taken"])), s(&c["free"]), "{c}");
    }
}

#[test]
fn identifiers() {
    for c in cases("query") {
        assert_eq!(fetch::query(s(&c[0])), s(&c[1]), "{c}");
    }
    for c in cases("months") {
        assert_eq!(fetch::month(s(&c[0])), s(&c[1]), "{c}");
    }
}

#[test]
fn sort() {
    for c in cases("sort") {
        let entries: Vec<Entry> =
            c["entries"].as_object().unwrap().iter().map(|(k, v)| entry(k, s(v))).collect();
        for (order, name) in refdir::Order::ALL {
            let mut sorted = entries.clone();
            refdir::sort(&mut sorted, order);
            let keys: Vec<&str> = sorted.iter().map(Entry::key).collect();
            assert_eq!(keys, strings(&c[name]), "{name}");
        }
    }
}
