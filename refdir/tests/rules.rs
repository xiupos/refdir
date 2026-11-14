//! A test for each rule of refdir v1

use refdir::*;
use serde_json::{Map, Value, json};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// `examples` of `schema/v1.json`
fn examples() -> Vec<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../schema/v1.json");
    let schema: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
    schema["examples"].as_array().unwrap().clone()
}

fn write_entry(library: &Path, name: &str, value: &Value) -> PathBuf {
    let dir = library.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(ENTRY), serde_json::to_string(value).unwrap()).unwrap();
    dir
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

fn names(dirs: &[PathBuf]) -> Vec<String> {
    dirs.iter().map(|d| d.file_name().unwrap().to_string_lossy().into_owned()).collect()
}

// MARK: - Listing entries

#[test]
fn entries_are_folders_directly_in_the_library_with_entry_json() {
    let lib = TempDir::new().unwrap();
    let p = lib.path();
    write_entry(p, "b", &json!({ "v": 1 }));
    write_entry(p, "A", &json!({ "v": 1 }));
    write_entry(p, ".hidden", &json!({ "v": 1 }));
    write_entry(p, "outer/inner", &json!({ "v": 1 }));
    fs::create_dir(p.join("empty")).unwrap();
    fs::create_dir_all(p.join("dir-entry/entry.json")).unwrap();
    fs::write(p.join("file"), "").unwrap();
    fs::write(p.join(ENTRY), "{\"v\":1}").unwrap();
    assert_eq!(names(&entry_dirs(p).unwrap()), ["A", "b"]);
}

#[test]
fn only_objects_with_v_1_are_read() {
    let lib = TempDir::new().unwrap();
    let p = lib.path();
    let ok = write_entry(p, "ok", &json!({ "v": 1, "x": [] }));
    let float = write_entry(p, "float", &json!({ "v": 1.0 }));
    let v2 = write_entry(p, "v2", &json!({ "v": 2 }));
    let no_v = write_entry(p, "no-v", &json!({}));
    let string_v = write_entry(p, "string-v", &json!({ "v": "1" }));
    let array = write_entry(p, "array", &json!([{ "v": 1 }]));
    let broken = p.join("broken");
    fs::create_dir(&broken).unwrap();
    fs::write(broken.join(ENTRY), "{").unwrap();

    assert!(read_entry(&ok).is_ok());
    assert!(read_entry(&float).is_ok());
    assert!(matches!(read_entry(&v2), Err(Error::NotV1(Some(_)))));
    assert!(matches!(read_entry(&no_v), Err(Error::NotV1(None))));
    assert!(matches!(read_entry(&string_v), Err(Error::NotV1(Some(_)))));
    assert!(matches!(read_entry(&array), Err(Error::NotObject)));
    assert!(matches!(read_entry(&broken), Err(Error::Json(_))));

    let library = Library::read(p).unwrap();
    assert_eq!(names(&library.entries.iter().map(|e| e.dir.clone()).collect::<Vec<_>>()), ["float", "ok"]);
    assert_eq!(library.unreadable.len(), 5);
}

#[test]
fn every_example_is_read() {
    let lib = TempDir::new().unwrap();
    for (i, example) in examples().iter().enumerate() {
        write_entry(lib.path(), &format!("e{i}"), example);
    }
    let library = Library::read(lib.path()).unwrap();
    assert!(library.unreadable.is_empty());
    assert_eq!(library.entries.len(), examples().len());
}

// MARK: - Resolved item

#[test]
fn the_user_csl_replaces_the_sources_fields_whole() {
    // The second example: `csl` replaces `type` and `note`, and the rest is INSPIRE's
    let item = resolve(&object(examples()[1].clone()));
    assert_eq!(item["id"], "Maldacena:1997re");
    assert_eq!(item["note"], "Reprinted in Int. J. Theor. Phys. 38 (1999) 1113–1133.");
    assert_eq!(item["title"], "The Large N limit of superconformal field theories and supergravity");
    assert_eq!(item["author"], json!([{ "family": "Maldacena", "given": "Juan Martin" }]));
}

#[test]
fn sources_are_layered_in_key_order_in_the_file() {
    let data: Map<String, Value> = serde_json::from_str(
        r#"{ "v": 1, "sources": {
            "zeta": { "csl": { "id": "z", "title": "Z", "author": [{ "family": "A" }, { "family": "B" }] } },
            "alpha": { "csl": { "id": "a", "author": [{ "family": "C" }] } }
        } }"#,
    )
    .unwrap();
    let item = resolve(&data);
    assert_eq!(item["id"], "a");
    assert_eq!(item["title"], "Z");
    // Whole: not merged with the earlier list
    assert_eq!(item["author"], json!([{ "family": "C" }]));
}

#[test]
fn a_resolved_field_is_the_same_as_in_the_resolved_item() {
    let layered = json!({ "v": 1, "sources": { "b": { "csl": { "id": "b", "title": "B" } }, "a": { "csl": { "id": "a" } } } });
    for example in examples().into_iter().chain([layered]) {
        let data = object(example);
        for (key, value) in &resolve(&data) {
            assert_eq!(resolved_field(&data, key), Some(value));
        }
        assert_eq!(resolved_field(&data, "no-such-field"), None);
    }
}

#[test]
fn layers_that_are_not_objects_are_left_out() {
    let data = object(json!({ "v": 1, "sources": { "x": { "csl": "no" }, "y": 3 }, "csl": [] }));
    assert!(resolve(&data).is_empty());
}

#[test]
fn the_resolved_item_keeps_the_key_order() {
    let item = resolve(&object(examples()[0].clone()));
    let keys: Vec<&str> = item.keys().map(String::as_str).collect();
    assert_eq!(keys[..3], ["id", "type", "title"]);
}

// MARK: - Citation keys

#[test]
fn citation_keys_are_found_ignoring_case() {
    let lib = TempDir::new().unwrap();
    let p = lib.path();
    let examples = examples();
    let first = write_entry(p, "Maldacena-1997re", &examples[0]);
    write_entry(p, "ATLAS-2012yve", &examples[2]);
    write_entry(p, "copy", &json!({ "v": 1, "csl": { "id": "maldacena:1997RE" } }));
    write_entry(p, "none", &json!({ "v": 1 }));

    let library = Library::read(p).unwrap();
    assert_eq!(library.find("atlas:2012YVE").len(), 1);
    assert_eq!(library.find("Maldacena:1997re").len(), 2);
    let dups = library.duplicate_ids();
    assert_eq!(dups.len(), 1);
    assert_eq!(dups[0].0, "maldacena:1997re");

    assert!(id_taken(p, "ATLAS:2012YVE", None).unwrap());
    assert!(!id_taken(p, "other", None).unwrap());
    // The entry itself does not take its key
    let atlas = p.join("ATLAS-2012yve");
    assert!(!id_taken(p, "ATLAS:2012yve", Some(&atlas)).unwrap());
    assert!(id_taken(p, "Maldacena:1997re", Some(&first)).unwrap());
}

#[test]
fn entries_of_the_same_inspire_record_are_found() {
    let lib = TempDir::new().unwrap();
    let examples = examples();
    write_entry(lib.path(), "a", &examples[0]);
    write_entry(lib.path(), "b", &examples[1]);
    write_entry(lib.path(), "c", &examples[2]);
    let library = Library::read(lib.path()).unwrap();
    let dups = library.duplicate_recids();
    assert_eq!(dups.len(), 1);
    assert_eq!(dups[0].0, 451647);
    assert_eq!(dups[0].1.len(), 2);
}

// MARK: - Folder names

#[test]
fn folder_names_are_the_key_with_dashes_unique_ignoring_case() {
    let lib = TempDir::new().unwrap();
    let p = lib.path();
    assert_eq!(folder_name("Maldacena:1997re"), "Maldacena-1997re");
    assert_eq!(free_name(p, "inspire:2990001").unwrap(), "inspire-2990001");
    fs::create_dir(p.join("maldacena-1997RE")).unwrap();
    assert_eq!(free_name(p, "Maldacena:1997re").unwrap(), "Maldacena-1997re-2");
    fs::write(p.join("MALDACENA-1997RE-2"), "").unwrap();
    assert_eq!(free_name(p, "Maldacena:1997re").unwrap(), "Maldacena-1997re-3");
}

#[test]
fn folder_names_are_only_a_hint() {
    let lib = TempDir::new().unwrap();
    let p = lib.path();
    let id = |id: &str| json!({ "v": 1, "csl": { "id": id } });
    let matches =
        |name: &str, value: Value| Entry::read(&write_entry(p, name, &value)).unwrap().name_matches_id();
    assert!(matches("Maldacena-1997re", id("Maldacena:1997re")));
    assert!(matches("Maldacena-1997re-2", id("Maldacena:1997re")));
    assert!(matches("anything", json!({ "v": 1 })));
    assert!(!matches("Maldacena-1997re-1", id("Maldacena:1997re")));
    assert!(!matches("Maldacena-1997re-02", id("Maldacena:1997re")));
    assert!(!matches("Maldacena-1997rex", id("Maldacena:1997re")));
    assert!(!matches("old-name", id("Maldacena:1997re")));
    // The entry is found by its content, whatever its folder
    let library = Library::read(p).unwrap();
    assert_eq!(library.find("Maldacena:1997re").len(), 6);
}

// MARK: - Files

#[test]
fn files_are_all_but_entry_json_and_dot_names() {
    let lib = TempDir::new().unwrap();
    let dir = write_entry(lib.path(), "e", &json!({ "v": 1 }));
    for f in ["b.pdf", ".app", "sub/x.txt", "sub/.DS_Store", "sub/entry.json", ".hidden/y", "a.txt"] {
        let path = dir.join(f);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
    }
    let entry = Entry::read(&dir).unwrap();
    let files: Vec<PathBuf> = entry.files().unwrap();
    let expected: Vec<PathBuf> = ["a.txt", "b.pdf", "sub/entry.json", "sub/x.txt"].map(PathBuf::from).into();
    assert_eq!(files, expected);
}

#[test]
fn the_main_file_is_the_first_file_directly_in_the_folder_by_name() {
    let lib = TempDir::new().unwrap();
    let dir = write_entry(lib.path(), "e", &json!({ "v": 1 }));
    let entry = Entry::read(&dir).unwrap();
    assert_eq!(entry.main_file().unwrap(), None);

    fs::create_dir(dir.join("0-folder")).unwrap();
    fs::write(dir.join(".0-hidden"), "").unwrap();
    fs::write(dir.join("b.pdf"), "").unwrap();
    assert_eq!(entry.main_file().unwrap(), Some(dir.join("b.pdf")));
    // ASCII case ignored
    fs::write(dir.join("C.pdf"), "").unwrap();
    fs::write(dir.join("a.pdf"), "").unwrap();
    assert_eq!(entry.main_file().unwrap(), Some(dir.join("a.pdf")));
    // Then code point order
    fs::write(dir.join("A.pdf"), "").unwrap();
    assert_eq!(entry.main_file().unwrap(), Some(dir.join("A.pdf")));
}

#[test]
fn names_are_ordered_ignoring_ascii_case_then_by_code_point() {
    let mut names = ["b", "B", "a", "É", "é", "e", "Z", "A"];
    names.sort_by(|a, b| cmp_names(OsStr::new(a), OsStr::new(b)));
    assert_eq!(names, ["A", "a", "B", "b", "e", "Z", "É", "é"]);
}

// MARK: - Writing

#[test]
fn writing_keeps_unknown_keys_and_leaves_out_empty_fields() {
    let lib = TempDir::new().unwrap();
    let dir = write_entry(lib.path(), "e", &examples()[1]);
    let before = read_entry(&dir).unwrap();

    write_field(&dir, Field::Tags, json!(["status:read", ""])).unwrap();
    write_field(&dir, Field::Note, json!("")).unwrap();
    write_field(&dir, Field::Csl, json!({ "id": "Maldacena:1997re", "title": "", "author": [] })).unwrap();

    let after = read_entry(&dir).unwrap();
    assert_eq!(after["tags"], json!(["status:read"]));
    assert!(!after.contains_key("note"));
    assert_eq!(after["csl"], json!({ "id": "Maldacena:1997re" }));
    assert_eq!(after["sources"], before["sources"]);
    assert_eq!(after["added"], before["added"]);
    // The keys keep their order
    let keys: Vec<&String> = after.keys().collect();
    assert_eq!(keys, ["v", "csl", "sources", "tags", "added"]);
}

#[test]
fn writing_goes_through_a_dot_file_and_leaves_none() {
    let lib = TempDir::new().unwrap();
    let dir = write_entry(lib.path(), "e", &json!({ "v": 1, "other-app": { "x": 1 } }));
    write_field(&dir, Field::Note, json!("a note")).unwrap();
    let names: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, [ENTRY]);
    assert_eq!(read_entry(&dir).unwrap()["other-app"], json!({ "x": 1 }));
    assert!(fs::read_to_string(dir.join(ENTRY)).unwrap().ends_with("}\n"));
}

#[test]
fn entries_that_cannot_be_read_are_not_written() {
    let lib = TempDir::new().unwrap();
    let dir = write_entry(lib.path(), "e", &json!({ "v": 2, "note": "keep" }));
    assert!(write_field(&dir, Field::Note, json!("x")).is_err());
    assert_eq!(fs::read_to_string(dir.join(ENTRY)).unwrap(), r#"{"v":2,"note":"keep"}"#);
}

#[test]
fn pruning_removes_empty_values() {
    assert_eq!(
        prune(json!({ "a": null, "b": [{}], "c": 0, "d": false })),
        Some(json!({ "c": 0, "d": false }))
    );
    assert_eq!(prune(json!({ "a": { "b": "" } })), None);
}

// MARK: - Tags

#[test]
fn a_tag_is_any_string_but_the_empty_one_as_it_is() {
    let mut t = Vec::new();
    for tag in [
        " a ",
        "a",
        "a
b",
        ":",
        "status:done",
    ] {
        tags::add(&mut t, tag).unwrap();
    }
    assert_eq!(
        t,
        [
            " a ",
            "a",
            "a
b",
            ":",
            "status:done"
        ]
    );
    assert!(matches!(tags::add(&mut t, ""), Err(Error::EmptyTag)));
}

#[test]
fn the_namespace_is_before_the_first_colon() {
    assert_eq!(tags::split("topic:AdS/CFT"), (Some("topic"), "AdS/CFT"));
    assert_eq!(tags::split("a:b:c"), (Some("a"), "b:c"));
    assert_eq!(tags::split("HistFactory"), (None, "HistFactory"));
}

#[test]
fn tags_are_the_same_only_as_written() {
    let entry = Entry { dir: PathBuf::new(), data: object(examples()[1].clone()) };
    assert!(entry.has_tag("topic:AdS/CFT"));
    assert!(!entry.has_tag("topic:ads/cft"));
    let mut t = vec!["topic:AdS/CFT".to_owned()];
    tags::add(&mut t, "topic:ads/cft").unwrap();
    assert_eq!(t, ["topic:AdS/CFT", "topic:ads/cft"]);
}

#[test]
fn new_tags_go_at_the_end_and_the_order_stays() {
    let mut t = vec!["b".to_owned(), "a".to_owned()];
    tags::add(&mut t, "c").unwrap();
    tags::add(&mut t, "b").unwrap();
    assert_eq!(t, ["b", "a", "c"]);
    // Removed as written
    let mut t = vec![" odd ".to_owned(), "a".to_owned()];
    tags::remove(&mut t, "odd");
    assert_eq!(t.len(), 2);
    tags::remove(&mut t, " odd ");
    assert_eq!(t, ["a"]);
}

#[test]
fn status_is_a_namespace_like_any_other() {
    let mut t = vec!["status:to-read".to_owned()];
    tags::add(&mut t, "status:read").unwrap();
    tags::add(&mut t, "status:done").unwrap();
    assert_eq!(t, ["status:to-read", "status:read", "status:done"]);
    assert_eq!(tags::split("status:read"), (Some("status"), "read"));
}

#[test]
fn writing_tags_reads_the_entry_again_and_keeps_the_rest() {
    let lib = TempDir::new().unwrap();
    let dir = write_entry(lib.path(), "e", &examples()[1]);
    let before = read_entry(&dir).unwrap();
    // Another app changes the file after it was read
    let entry = Entry::read(&dir).unwrap();
    let mut changed = before.clone();
    changed.insert("other-app".into(), json!(1));
    write_atomic(&dir.join(ENTRY), &Value::Object(changed)).unwrap();

    assert_eq!(
        add_tag(&entry.dir, "topic:holography").unwrap(),
        ["status:read", "topic:AdS/CFT", "topic:holography"]
    );
    assert_eq!(remove_tag(&dir, "topic:AdS/CFT").unwrap(), ["status:read", "topic:holography"]);
    let after = read_entry(&dir).unwrap();
    assert_eq!(after["other-app"], 1);
    assert_eq!(after["sources"], before["sources"]);
    let keys: Vec<&String> = after.keys().collect();
    assert_eq!(keys, ["v", "csl", "sources", "tags", "note", "added", "other-app"]);

    // No tags left: no `tags`
    remove_tag(&dir, "topic:holography").unwrap();
    assert_eq!(remove_tag(&dir, "status:read").unwrap(), Vec::<String>::new());
    assert!(!read_entry(&dir).unwrap().contains_key("tags"));
}

#[test]
fn writing_tags_removes_those_that_came_before() {
    let lib = TempDir::new().unwrap();
    let dir = write_entry(lib.path(), "e", &json!({ "v": 1, "tags": ["a", "b", "a"] }));
    assert_eq!(add_tag(&dir, "c").unwrap(), ["a", "b", "c"]);
}

#[test]
fn tags_that_do_not_change_are_not_written() {
    let lib = TempDir::new().unwrap();
    let text = r#"{"v":1,"tags":["status:read","a"]}"#;
    let dir = lib.path().join("e");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join(ENTRY), text).unwrap();
    add_tag(&dir, "a").unwrap();
    add_tag(&dir, "status:read").unwrap();
    remove_tag(&dir, "b").unwrap();
    edit_tags(&dir, |_| Ok(())).unwrap();
    assert_eq!(fs::read_to_string(dir.join(ENTRY)).unwrap(), text);
    // Nor are entries without tags, given none
    fs::write(dir.join(ENTRY), r#"{"v":1}"#).unwrap();
    remove_tag(&dir, "a").unwrap();
    assert_eq!(fs::read_to_string(dir.join(ENTRY)).unwrap(), r#"{"v":1}"#);
}

#[test]
fn tags_that_cannot_be_read_are_not_written_over() {
    let lib = TempDir::new().unwrap();
    for (name, tags) in [("object", json!({ "a": 1 })), ("number", json!(["a", 1])), ("null", json!(null))] {
        let dir = write_entry(lib.path(), name, &json!({ "v": 1, "tags": tags }));
        let text = fs::read_to_string(dir.join(ENTRY)).unwrap();
        assert!(matches!(add_tag(&dir, "b"), Err(Error::Tags)), "{name}");
        assert!(matches!(remove_tag(&dir, "a"), Err(Error::Tags)), "{name}");
        assert_eq!(fs::read_to_string(dir.join(ENTRY)).unwrap(), text);
    }
    // Reading skips what is not a string
    let entry = Entry::read(&lib.path().join("number")).unwrap();
    assert_eq!(entry.tags(), ["a"]);
    // And an empty tag is not written
    let dir = write_entry(lib.path(), "ok", &json!({ "v": 1 }));
    assert!(matches!(add_tag(&dir, ""), Err(Error::EmptyTag)));
    assert_eq!(read_entry(&dir).unwrap(), object(json!({ "v": 1 })));
}

#[test]
fn the_library_tags_are_counted_by_namespace_then_name() {
    let lib = TempDir::new().unwrap();
    for (i, example) in examples().iter().enumerate() {
        write_entry(lib.path(), &format!("e{i}"), example);
    }
    write_entry(
        lib.path(),
        "x",
        &json!({ "v": 1, "tags": ["b", "topic:Number-theory", "a:z", "topic:number-theory", "b"] }),
    );
    let library = Library::read(lib.path()).unwrap();
    assert_eq!(
        library.tags(),
        [
            ("b", 1),
            ("HistFactory", 1),
            ("a:z", 1),
            ("status:read", 1),
            ("status:reading", 1),
            ("status:to-read", 1),
            ("topic:AdS/CFT", 1),
            ("topic:Number-theory", 1),
            ("topic:number-theory", 3),
            ("topic:statistics", 1),
        ]
    );
}

#[test]
fn tags_are_ordered_without_a_namespace_first() {
    let mut t = ["topic:b", "z", "a-b:c", "a:c", "A", "topic:A"];
    t.sort_by(|a, b| tags::cmp(a, b));
    assert_eq!(t, ["A", "z", "a:c", "a-b:c", "topic:A", "topic:b"]);
}
