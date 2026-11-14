//! Reading the fields of a CSL-JSON item

use serde_json::{Map, Value};

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
