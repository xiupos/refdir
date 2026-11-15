//! The citation keys in a LaTeX `.aux` file: `\citation{a,b}` (BibTeX) and `\abx@aux@cite{0}{a}`
//! (biblatex), in the files it `\@input`s too. `\nocite{*}` is `\citation{*}`, or biblatex's
//! `\abx@aux@nociteall`

use std::path::Path;
use std::{fs, io};

#[derive(Debug, Default, PartialEq)]
pub struct Citations {
    /// In order, each once
    pub keys: Vec<String>,
    /// `\nocite{*}`: every entry
    pub all: bool,
}

/// The keys cited in the `.aux` file at `path`. An error names the file it could not read
pub fn read(path: &Path) -> io::Result<Citations> {
    let mut citations = Citations::default();
    read_into(path, &mut citations, 0)?;
    Ok(citations)
}

fn read_into(path: &Path, citations: &mut Citations, depth: usize) -> io::Result<()> {
    let bytes = fs::read(path)
        .map_err(|e| io::Error::new(e.kind(), format!("cannot read {}: {e}", path.display())))?;
    let text = String::from_utf8_lossy(&bytes);
    for (command, groups) in commands(&text) {
        match command {
            "citation" => {
                groups.first().into_iter().flat_map(|g| g.split(',')).for_each(|k| citations.add(k))
            }
            "abx@aux@cite" => groups.last().into_iter().for_each(|k| citations.add(k)),
            "abx@aux@nociteall" => citations.all = true,
            // As BibTeX does, relative to the main `.aux` file's folder; `\include`s are one deep
            "@input" if depth < 8 => {
                let Some(name) = groups.first() else { continue };
                let input = path.parent().unwrap_or(Path::new("")).join(name.trim());
                if input.is_file() {
                    read_into(&input, citations, depth + 1)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

impl Citations {
    fn add(&mut self, key: &str) {
        let key = key.trim();
        if key == "*" {
            self.all = true;
        } else if !key.is_empty() && !self.keys.iter().any(|k| k == key) {
            self.keys.push(key.to_owned());
        }
    }
}

/// `\name{a}{b}` at the start of lines → `("name", ["a", "b"])`; `\name` alone → `("name", [])`
fn commands(text: &str) -> impl Iterator<Item = (&str, Vec<&str>)> {
    text.lines().filter_map(|line| {
        let rest = line.trim_end().trim_start().strip_prefix('\\')?;
        let end = rest.find('{').unwrap_or(rest.len());
        let (name, mut rest) = rest.split_at(end);
        let mut groups = Vec::new();
        while let Some(inner) = rest.strip_prefix('{') {
            let close = inner.find('}')?;
            groups.push(&inner[..close]);
            rest = &inner[close + 1..];
        }
        Some((name, groups))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bibtex_and_biblatex_citations() {
        let dir = tempfile::TempDir::new().unwrap();
        let main = dir.path().join("main.aux");
        fs::write(
            &main,
            "\\relax\n\\citation{Maldacena:1997re,ATLAS:2012yve}\n\\@input{chapter.aux}\n\
             \\abx@aux@cite{0}{pyhf}\n\\abx@aux@cite{Takagi:1971}\n\\citation{Maldacena:1997re}\n\
             \\bibstyle{JHEP}\n\\bibdata{refs}\n",
        )
        .unwrap();
        fs::write(dir.path().join("chapter.aux"), "\\citation{ inspire:2990001 , *}\n").unwrap();
        let citations = read(&main).unwrap();
        assert_eq!(
            citations.keys,
            ["Maldacena:1997re", "ATLAS:2012yve", "inspire:2990001", "pyhf", "Takagi:1971"]
        );
        assert!(citations.all);

        // biblatex's \nocite{*}
        fs::write(&main, "\\relax\n\\abx@aux@refcontext{nyt/global//global/global}\n\\abx@aux@nociteall\n")
            .unwrap();
        assert_eq!(read(&main).unwrap(), Citations { keys: Vec::new(), all: true });
    }
}
