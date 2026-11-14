//! Tags. The specification asks only for distinct non-empty strings; the rest is this crate's
//! choice: tags are compared as written, keep the user's order, and no namespace is special.

use crate::Error;
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::ffi::OsStr;

/// The namespace and the value, split at the first `:`: `topic:AdS/CFT` → `(Some("topic"),
/// "AdS/CFT")`. A tag without `:` has no namespace
pub fn split(tag: &str) -> (Option<&str>, &str) {
    match tag.split_once(':') {
        Some((namespace, value)) => (Some(namespace), value),
        None => (None, tag),
    }
}

/// The order for lists: tags without a namespace first, then by namespace, then by value, each as
/// [`cmp_names`](crate::cmp_names) orders names
pub fn cmp(a: &str, b: &str) -> Ordering {
    let ((na, va), (nb, vb)) = (split(a), split(b));
    let names = |a: &str, b: &str| crate::cmp_names(OsStr::new(a), OsStr::new(b));
    na.is_some()
        .cmp(&nb.is_some())
        .then_with(|| names(na.unwrap_or(""), nb.unwrap_or("")))
        .then_with(|| names(va, vb))
}

/// Adds a tag at the end, as given, unless it is there. [`Error::EmptyTag`] if it is empty
pub fn add(tags: &mut Vec<String>, tag: &str) -> Result<(), Error> {
    if tag.is_empty() {
        return Err(Error::EmptyTag);
    }
    if !tags.iter().any(|t| t == tag) {
        tags.push(tag.to_owned());
    }
    Ok(())
}

/// Removes a tag
pub fn remove(tags: &mut Vec<String>, tag: &str) {
    tags.retain(|t| t != tag);
}

/// `tags`, strictly: [`Error::Tags`] if it is not an array of strings, so that writing does not
/// lose what someone put there
pub(crate) fn read(data: &Map<String, Value>) -> Result<Vec<String>, Error> {
    let Some(tags) = data.get("tags") else { return Ok(Vec::new()) };
    let tags = tags.as_array().ok_or(Error::Tags)?;
    tags.iter().map(|t| t.as_str().map(str::to_owned).ok_or(Error::Tags)).collect()
}

/// Removes repeated tags, keeping the first
pub(crate) fn dedup(tags: &mut Vec<String>) {
    let mut seen = HashSet::new();
    tags.retain(|t| seen.insert(t.clone()));
}
