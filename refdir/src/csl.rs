//! Reading the fields of a CSL-JSON item, and making one valid

use serde_json::{Map, Value};
use std::sync::LazyLock;

/// The CSL-JSON schema (`third_party/csl-schema`), whose fields `clean` keeps. `check` validates
/// entries against it
pub const SCHEMA: &str = include_str!("../../third_party/csl-schema/schemas/input/csl-data.json");

/// Crossref's types, as doi.org gives them, and the CSL types they are
const CROSSREF_TYPES: [(&str, &str); 14] = [
    ("journal-article", "article-journal"),
    ("book-chapter", "chapter"),
    ("book-part", "chapter"),
    ("book-section", "chapter"),
    ("proceedings-article", "paper-conference"),
    ("posted-content", "article"),
    ("dissertation", "thesis"),
    ("monograph", "book"),
    ("edited-book", "book"),
    ("reference-book", "book"),
    ("book-set", "book"),
    ("reference-entry", "entry"),
    ("report-component", "report"),
    ("peer-review", "review"),
];

/// Runs of white space, line breaks too, as one space
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A string field, if it is not empty
pub fn str_field<'a>(item: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    item.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// A field that is a string or a number, as a string
pub fn text_field(item: &Map<String, Value>, key: &str) -> Option<String> {
    match item.get(key)? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// The first author's family name, or the name as it is
pub fn first_author(item: &Map<String, Value>) -> String {
    let Some(first) = item.get("author").and_then(|a| a.get(0)) else { return String::new() };
    let part = |key: &str| first.get(key).and_then(Value::as_str);
    one_line(part("literal").or(part("family")).or(part("given")).unwrap_or_default())
}

/// The year and month of `issued`
pub fn issued(item: &Map<String, Value>) -> Option<(i64, Option<u32>)> {
    item.get("issued").and_then(year_month)
}

/// The year and month of a CSL date: its first `date-parts`, or else its `raw` (`1997-11`)
pub fn year_month(date: &Value) -> Option<(i64, Option<u32>)> {
    let number = |v: &Value| v.as_i64().or_else(|| v.as_str()?.trim().parse().ok());
    let month = |m: i64| (1..=12).contains(&m).then_some(m as u32);
    if let Some(parts) = date.get("date-parts").and_then(|p| p.get(0)).and_then(Value::as_array) {
        let year = number(parts.first()?)?;
        return Some((year, parts.get(1).and_then(number).and_then(month)));
    }
    let raw = date.get("raw")?.as_str()?.trim();
    let mut parts = raw.split('-');
    let year = parts.next().filter(|y| y.len() == 4)?.parse().ok()?;
    Some((year, parts.next().and_then(|m| m.parse().ok()).and_then(month)))
}

/// The schema's item: its `properties`, and the properties of a name and of a date
struct Schema {
    item: Map<String, Value>,
    name: Map<String, Value>,
    date: Map<String, Value>,
}

static CSL: LazyLock<Schema> = LazyLock::new(|| {
    let schema: Value = serde_json::from_str(SCHEMA).expect("the CSL schema is JSON");
    let properties = |v: &Value| v["properties"].as_object().cloned().unwrap_or_default();
    let defs = &schema["definitions"];
    Schema {
        item: properties(&schema["items"]),
        name: properties(&defs["name-variable"]["anyOf"][0]),
        date: properties(&defs["date-variable"]["anyOf"][0]),
    }
});

/// What the schema allows for a field: `string`, or `string` and `number`, and the like
fn allows(property: &Value, kind: &str) -> bool {
    match &property["type"] {
        Value::String(t) => t == kind,
        Value::Array(types) => types.iter().any(|t| t == kind),
        _ => false,
    }
}

/// A value as the schema's `property` has it: an array's first item if it wants one value (doi.org
/// gives `ISSN` and `ISBN` as arrays), a number as a string if it wants a string. `None` if it
/// cannot be one
fn fit(value: Value, property: &Value) -> Option<Value> {
    match value {
        Value::Array(items) if !allows(property, "array") => items.into_iter().find_map(|v| fit(v, property)),
        Value::Number(n) if !allows(property, "number") && allows(property, "string") => {
            Some(n.to_string().into())
        }
        v @ Value::String(_) if allows(property, "string") => Some(v),
        v @ Value::Number(_) if allows(property, "number") => Some(v),
        v @ Value::Bool(_) if allows(property, "boolean") => Some(v),
        _ => None,
    }
}

/// An object with only the fields `properties` has, each fitted
fn keep(value: Value, properties: &Map<String, Value>) -> Option<Value> {
    let Value::Object(map) = value else { return None };
    let kept = map.into_iter().filter_map(|(k, v)| {
        let property = properties.get(&k)?;
        let v = if k == "date-parts" { v } else { fit(v, property)? };
        Some((k, v))
    });
    Some(Value::Object(kept.collect()))
}

/// A CSL-JSON item from another program, such as doi.org's (Crossref's or DataCite's), as one the
/// CSL schema accepts: Crossref's types as CSL's (any other unknown type is `document`), fields
/// the schema does not have left out, and the others fitted to their type. `id` is left out: the
/// key is the library's. Empty fields are left out too
pub fn clean(item: &Map<String, Value>) -> Map<String, Value> {
    let csl = &*CSL;
    let types = csl.item["type"]["enum"].as_array().cloned().unwrap_or_default();
    let mut out = Map::new();
    for (key, value) in item.clone() {
        let Some(property) = csl.item.get(&key).filter(|_| key != "id") else { continue };
        let value = match (key.as_str(), &property["items"]["$ref"], &property["$ref"]) {
            ("type", _, _) => {
                let t = value.as_str().unwrap_or_default();
                let t = CROSSREF_TYPES.iter().find(|(c, _)| *c == t).map_or(t, |(_, csl)| *csl);
                Some(Value::from(if types.iter().any(|known| known == t) { t } else { "document" }))
            }
            (_, Value::String(r), _) if r.ends_with("name-variable") => match value {
                Value::Array(names) => Some(names.into_iter().filter_map(|n| keep(n, &csl.name)).collect()),
                _ => None,
            },
            (_, _, Value::String(r)) if r.ends_with("date-variable") => keep(value, &csl.date),
            ("categories", _, _) => {
                Some(value).filter(|v| v.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
            }
            ("custom", _, _) => Some(value).filter(Value::is_object),
            _ => fit(value, property),
        };
        if let Some(value) = value.and_then(crate::prune) {
            out.insert(key, value);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn dates() {
        assert_eq!(year_month(&json!({ "raw": "1997-11" })), Some((1997, Some(11))));
        assert_eq!(year_month(&json!({ "raw": "2012-07-04" })), Some((2012, Some(7))));
        assert_eq!(year_month(&json!({ "raw": "1997" })), Some((1997, None)));
        assert_eq!(year_month(&json!({ "date-parts": [[1971]] })), Some((1971, None)));
        assert_eq!(year_month(&json!({ "date-parts": [["1801", "13"]] })), Some((1801, None)));
        assert_eq!(year_month(&json!({ "date-parts": [[2001, 2, 3], [2002]] })), Some((2001, Some(2))));
        assert_eq!(year_month(&json!({ "raw": "Nov 1997" })), None);
    }

    #[test]
    fn first_authors() {
        let first = |v: Value| first_author(v.as_object().unwrap());
        assert_eq!(first(json!({ "author": [{ "family": "高木", "given": "貞治" }] })), "高木");
        assert_eq!(
            first(json!({ "author": [{ "literal": "ATLAS\n Collaboration" }] })),
            "ATLAS Collaboration"
        );
        assert_eq!(first(json!({ "author": [{ "given": "Plato" }] })), "Plato");
        assert_eq!(first(json!({})), "");
    }
}
