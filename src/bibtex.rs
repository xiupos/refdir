//! CSL-JSON → BibTeX.
//!
//! Written here and not with the `biblatex` crate: its writer turns `\` into `\\` (a line break),
//! `~` and `^` into accents and `{` into an unbalanced `\{`, which BibTeX counts as a brace.

use serde_json::{Map, Value};

const MONTHS: [&str; 12] =
    ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

/// The BibTeX entry of a resolved item. `arxiv` is `sources.inspire.arxiv`
pub fn entry(key: &str, item: &Map<String, Value>, arxiv: Option<&str>) -> String {
    let str_field = |name: &str| item.get(name).and_then(Value::as_str).filter(|s| !s.is_empty());
    let kind = entry_type(str_field("type").unwrap_or(""));
    let mut fields: Vec<(&str, String)> = Vec::new();
    for (csl, bib) in [("author", "author"), ("editor", "editor")] {
        if let Some(names) =
            item.get(csl).and_then(Value::as_array).map(|n| names(n)).filter(|n| !n.is_empty())
        {
            fields.push((bib, names));
        }
    }
    if let Some(title) = str_field("title") {
        // Braced again so that the style keeps its case
        fields.push(("title", format!("{{{}}}", escape(title))));
    }
    if let Some(container) = str_field("container-title") {
        let name = if matches!(kind, "incollection" | "inproceedings") { "booktitle" } else { "journal" };
        fields.push((name, escape(container)));
    }
    if let Some(volume) = text(item.get("volume")) {
        fields.push(("volume", escape(&volume)));
    }
    if let Some(page) = text(item.get("page")) {
        fields.push(("pages", escape(&page_range(&page))));
    }
    if let Some((year, month)) = item.get("issued").and_then(year_month) {
        fields.push(("year", year.to_string()));
        if let Some(m) = month {
            // A macro, not braced
            fields.push(("month", MONTHS[m as usize - 1].to_owned()));
        }
    }
    if let Some(doi) = str_field("DOI") {
        fields.push(("doi", verbatim(doi)));
    }
    if let Some(url) = str_field("URL") {
        fields.push(("url", verbatim(url)));
    }
    if let Some(arxiv) = arxiv {
        fields.push(("eprint", verbatim(arxiv)));
        fields.push(("archivePrefix", "arXiv".to_owned()));
    }

    let mut out = format!("@{kind}{{{key},\n");
    for (name, value) in fields {
        if name == "month" {
            out += &format!("    {name} = {value},\n");
        } else {
            out += &format!("    {name} = {{{value}}},\n");
        }
    }
    out + "}\n"
}

/// The BibTeX entry type of a CSL item type
fn entry_type(csl: &str) -> &'static str {
    match csl {
        // `article` is a preprint in CSL. INSPIRE, too, makes preprints `@article`
        "article" | "article-journal" | "article-magazine" | "article-newspaper" => "article",
        "book" => "book",
        "chapter" => "incollection",
        "paper-conference" => "inproceedings",
        "thesis" => "phdthesis",
        "report" => "techreport",
        "manuscript" => "unpublished",
        _ => "misc",
    }
}

/// Plain text → TeX
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' | '%' | '$' | '#' | '_' => {
                out.push('\\');
                out.push(c);
            }
            // `\{` would leave a brace that BibTeX counts
            '{' => out += "\\textbraceleft{}",
            '}' => out += "\\textbraceright{}",
            '\\' => out += "\\textbackslash{}",
            '~' => out += "\\textasciitilde{}",
            '^' => out += "\\textasciicircum{}",
            c => out.push(c),
        }
    }
    out
}

/// A DOI, URL or arXiv number, which styles and `biblatex` print as they are: only the braces,
/// which would break the entry, are percent-encoded
fn verbatim(text: &str) -> String {
    text.replace('{', "%7B").replace('}', "%7D")
}

/// A string or a number
fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// `231-252` → `231--252`
fn page_range(page: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in page.chars() {
        if c == '-' || c == '–' {
            if !dash {
                out += "--";
            }
            dash = true;
        } else {
            out.push(c);
            dash = false;
        }
    }
    out
}

/// `{ "family": …, "given": … }`s → `Family, Given and {Literal}`
fn names(names: &[Value]) -> String {
    let list: Vec<String> = names.iter().filter_map(name).collect();
    list.join(" and ")
}

fn name(name: &Value) -> Option<String> {
    let part = |key: &str| name.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(protect);
    if let Some(literal) = name.get("literal").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        return Some(format!("{{{}}}", escape(literal)));
    }
    let particles = [part("dropping-particle"), part("non-dropping-particle")];
    let von: Vec<String> = particles.into_iter().flatten().collect();
    let last = von.into_iter().chain(part("family")).collect::<Vec<_>>().join(" ");
    let (given, suffix) = (part("given"), part("suffix"));
    match (last.is_empty(), given, suffix) {
        (true, None, _) => None,
        (true, Some(given), _) => Some(format!("{{{given}}}")),
        (false, None, None) => Some(last),
        (false, given, Some(suffix)) => Some(format!("{last}, {suffix}, {}", given.unwrap_or_default())),
        (false, Some(given), None) => Some(format!("{last}, {given}")),
    }
}

/// A part of a name, escaped, and braced if BibTeX would split it: at a comma or an `and`
fn protect(part: &str) -> String {
    let escaped = escape(part);
    if part.contains(',') || part.split_whitespace().any(|w| w.eq_ignore_ascii_case("and")) {
        format!("{{{escaped}}}")
    } else {
        escaped
    }
}

/// The year and month of a CSL date: its first `date-parts`, or else its `raw` (`1997-11`)
fn year_month(date: &Value) -> Option<(i64, Option<u32>)> {
    let number = |v: &Value| v.as_i64().or_else(|| v.as_str()?.trim().parse().ok());
    if let Some(parts) = date.get("date-parts").and_then(|p| p.get(0)).and_then(Value::as_array) {
        let year = number(parts.first()?)?;
        let month = parts.get(1).and_then(number).and_then(month);
        return Some((year, month));
    }
    let raw = date.get("raw")?.as_str()?.trim();
    let mut parts = raw.split('-');
    let year = parts.next().filter(|y| y.len() == 4)?.parse().ok()?;
    let month = parts.next().and_then(|m| m.parse().ok()).and_then(month);
    Some((year, month))
}

fn month(m: i64) -> Option<u32> {
    (1..=12).contains(&m).then_some(m as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bib(value: Value, arxiv: Option<&str>) -> String {
        let item = value.as_object().unwrap();
        entry(item["id"].as_str().unwrap(), item, arxiv)
    }

    #[test]
    fn special_characters_are_escaped() {
        assert_eq!(escape("a & b % c $ d # e _ f"), r"a \& b \% c \$ d \# e \_ f");
        assert_eq!(
            escape(r"{x} \ ~ ^"),
            r"\textbraceleft{}x\textbraceright{} \textbackslash{} \textasciitilde{} \textasciicircum{}"
        );
        assert_eq!(escape("高木 – é"), "高木 – é");
    }

    #[test]
    fn names() {
        let names = |v: Value| super::names(v.as_array().unwrap());
        assert_eq!(names(json!([{ "literal": "ATLAS Collaboration" }])), "{ATLAS Collaboration}");
        assert_eq!(
            names(json!([{ "family": "Maldacena", "given": "Juan Martin" }, { "family": "Witten" }])),
            "Maldacena, Juan Martin and Witten"
        );
        assert_eq!(
            names(json!([{ "family": "Gogh", "given": "Vincent", "non-dropping-particle": "van" }])),
            "van Gogh, Vincent"
        );
        assert_eq!(
            names(json!([{ "family": "King", "given": "Martin Luther", "suffix": "Jr." }])),
            "King, Jr., Martin Luther"
        );
        assert_eq!(
            names(json!([{ "family": "Smith and Sons" }, { "given": "Plato" }])),
            "{Smith and Sons} and {Plato}"
        );
        assert_eq!(names(json!([{ "literal": "R&D Team" }, {}])), r"{R\&D Team}");
    }

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
    fn pages() {
        assert_eq!(page_range("231-252"), "231--252");
        assert_eq!(page_range("231--252"), "231--252");
        assert_eq!(page_range("231–252"), "231--252");
        assert_eq!(page_range("e123"), "e123");
    }

    #[test]
    fn verbatim_fields_are_not_escaped() {
        let out = bib(json!({ "id": "x", "DOI": "10.1000/a_b%c", "URL": "https://e.org/a_b#c{d}" }), None);
        assert!(out.contains("doi = {10.1000/a_b%c},"));
        assert!(out.contains("url = {https://e.org/a_b#c%7Bd%7D},"));
    }

    #[test]
    fn titles_and_containers() {
        let out = bib(
            json!({ "id": "x", "type": "chapter", "title": "Fun & {Games}", "container-title": "A_B", "volume": 3 }),
            None,
        );
        assert_eq!(
            out,
            "@incollection{x,\n    title = {{Fun \\& \\textbraceleft{}Games\\textbraceright{}}},\n    \
             booktitle = {A\\_B},\n    volume = {3},\n}\n"
        );
    }
}
