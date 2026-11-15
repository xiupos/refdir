//! Papers as BibTeX from INSPIRE-HEP, or from doi.org for a DOI it does not have, and their PDFs
//! from arXiv. Blocking

use crate::{Bib, Chunk, Entry};
use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::Duration;

const INSPIRE: &str = "https://inspirehep.net/api/literature";
/// The program and where to find it, as INSPIRE asks of its clients
const USER_AGENT: &str = concat!("refdir/", env!("CARGO_PKG_VERSION"), " (https://github.com/xiupos/refdir)");

/// INSPIRE's query for an identifier: `recid:N` for an INSPIRE link, `arxiv:<id>` for an arXiv
/// link, `doi:<doi>` for a doi.org link. Anything else as it is: INSPIRE's search finds a bare arXiv
/// number, DOI, or texkey
pub fn query(id: &str) -> String {
    let id = id.trim();
    let after = |prefix: &str| {
        let (_, rest) = id.split_once(prefix)?;
        let rest = rest.split(['?', '#']).next().unwrap_or(rest);
        Some(rest.trim_end_matches('/').trim_end_matches(".pdf").to_owned())
    };
    if let Some(recid) = after("inspirehep.net/literature/") {
        return format!("recid:{recid}");
    }
    if let Some(arxiv) = after("arxiv.org/abs/").or_else(|| after("arxiv.org/pdf/")) {
        return format!("arxiv:{arxiv}");
    }
    if let Some(doi) = after("doi.org/") {
        return format!("doi:{doi}");
    }
    id.to_owned()
}

/// The one paper an identifier finds: INSPIRE's BibTeX, or else, for a DOI, doi.org's
pub fn find(id: &str) -> Result<Bib, String> {
    let q = query(id);
    let url = format!("{INSPIRE}?q={}&size=2&format=bibtex", encode(&q));
    let mut found = parse(&get(&url, None, 30)?)?;
    match found.len() {
        1 => Ok(found.remove(0)),
        0 => match q
            .strip_prefix("doi:")
            .or(Some(q.as_str()).filter(|q| q.starts_with("10.") && q.contains('/')))
        {
            Some(doi) => from_doi(doi),
            None => Err(format!("INSPIRE finds nothing for {id}")),
        },
        _ => Err(format!("INSPIRE finds more than one paper for {id}")),
    }
}

/// doi.org's BibTeX for a DOI, with its month as BibTeX's macro: `June` → `jun`
fn from_doi(doi: &str) -> Result<Bib, String> {
    let text = get(&format!("https://doi.org/{doi}"), Some("application/x-bibtex"), 30)?;
    let mut bib = parse(&text)?.into_iter().next().ok_or(format!("doi.org has no BibTeX for {doi}"))?;
    for (name, value) in &mut bib.fields {
        if name.eq_ignore_ascii_case("month") {
            for chunk in value.iter_mut() {
                if let Chunk::Macro(m) = chunk {
                    *m = m.chars().take(3).collect::<String>().to_lowercase();
                }
            }
        }
    }
    Ok(bib)
}

/// The entry's paper again from INSPIRE, by its arXiv number or DOI, with its key, tags, and note
pub fn again(entry: &Entry) -> Result<Bib, String> {
    let arxiv = entry.bib.get("eprint").map(|e| format!("arxiv:{e}"));
    let id = arxiv.or_else(|| entry.bib.get("doi").map(|d| format!("doi:{d}")));
    let mut bib = find(&id.ok_or("It has no arXiv number or DOI")?)?;
    bib.key = entry.key().to_owned();
    for field in ["keywords", "comment"] {
        bib.set(field, &entry.bib.get(field).unwrap_or_default());
    }
    Ok(bib)
}

/// Downloads the entry's PDF from arXiv into its folder, as `<arXiv number>.pdf`
pub fn pdf(entry: &Entry) -> Result<PathBuf, String> {
    let arxiv = entry.bib.get("eprint").ok_or("It has no arXiv number")?;
    let data = get(&format!("https://arxiv.org/pdf/{arxiv}"), None, 300)?;
    if !data.starts_with(b"%PDF-") {
        return Err("arXiv gave no PDF".into());
    }
    entry.add_file(&format!("{}.pdf", arxiv.replace('/', "-")), &data).map_err(|e| e.to_string())
}

/// The paper on INSPIRE, by its arXiv number or DOI
pub fn inspire_url(bib: &Bib) -> Option<String> {
    let q = bib
        .get("eprint")
        .map(|e| format!("arxiv:{e}"))
        .or_else(|| Some(format!("doi:{}", bib.get("doi")?)))?;
    Some(format!("https://inspirehep.net/search?p={}", encode(&q)))
}

fn parse(bytes: &[u8]) -> Result<Vec<Bib>, String> {
    Bib::parse(&String::from_utf8_lossy(bytes)).map_err(|e| e.to_string())
}

/// A response, in at most `seconds`. Errors are short sentences naming the host; the details go to
/// stderr
fn get(url: &str, accept: Option<&str>, seconds: u64) -> Result<Vec<u8>, String> {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    let agent = AGENT.get_or_init(|| ureq::Agent::config_builder().user_agent(USER_AGENT).build().into());
    let host = url.split('/').nth(2).unwrap_or(url).to_owned();
    let unreachable = |e: &dyn std::fmt::Display| {
        eprintln!("{url}: {e}");
        format!("{host} cannot be reached")
    };
    let mut request = agent.get(url);
    if let Some(accept) = accept {
        request = request.header("Accept", accept);
    }
    let timeout = Some(Duration::from_secs(seconds));
    match request.config().timeout_global(timeout).build().call() {
        Ok(mut r) => r.body_mut().with_config().limit(200 << 20).read_to_vec().map_err(|e| unreachable(&e)),
        Err(ureq::Error::StatusCode(404)) => Err(format!("{host} does not have it")),
        Err(ureq::Error::StatusCode(code)) => Err(format!("{host} answered with error {code}")),
        Err(e) => Err(unreachable(&e)),
    }
}

/// Percent-encodes all but the unreserved characters, `+` included, which INSPIRE reads as a space
fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
            _ => write!(out, "%{b:02X}").unwrap(),
        }
    }
    out
}
