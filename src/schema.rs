//! Checking entries against `schema/v1.json` and the CSL schema it refers to, both embedded at
//! build time (the CSL schema from the git submodule in `third_party/`): nothing is fetched

use anyhow::{Context, Result};
use serde_json::Value;

const V1: &str = include_str!("../schema/v1.json");
const CSL_DATA: &str = include_str!("../third_party/csl-schema/schemas/input/csl-data.json");
const CSL_DATA_URI: &str = "https://resource.citationstyles.org/schema/v1.0/input/json/csl-data.json";

pub struct Schema(jsonschema::Validator);

impl Schema {
    pub fn new() -> Result<Schema> {
        let csl: Value = serde_json::from_str(CSL_DATA).context("csl-data.json")?;
        let v1: Value = serde_json::from_str(V1).context("v1.json")?;
        let registry = jsonschema::Registry::new().add(CSL_DATA_URI, csl)?.prepare()?;
        let validator = jsonschema::options()
            .offline()
            .with_registry(&registry)
            // `added` and `fetched` are `date-time`
            .should_validate_formats(true)
            .build(&v1)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        Ok(Schema(validator))
    }

    /// What is wrong with an `entry.json`: `/path: message`s
    pub fn errors(&self, entry: &Value) -> Vec<String> {
        let errors = self.0.iter_errors(entry);
        errors.map(|e| format!("{}: {e}", display_path(&e.instance_path().to_string()))).collect()
    }
}

fn display_path(path: &str) -> &str {
    if path.is_empty() { "/" } else { path }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_examples_are_valid() {
        let schema = Schema::new().unwrap();
        let v1: Value = serde_json::from_str(V1).unwrap();
        for example in v1["examples"].as_array().unwrap() {
            assert_eq!(schema.errors(example), Vec::<String>::new(), "{example}");
        }
    }

    #[test]
    fn invalid_entries() {
        let schema = Schema::new().unwrap();
        let errors = |v: Value| schema.errors(&v);
        assert_eq!(errors(json!({})).len(), 1);
        assert_eq!(errors(json!({ "v": 2 })).len(), 1);
        assert!(errors(json!({ "v": 1, "added": "yesterday" }))[0].starts_with("/added: "));
        assert!(
            errors(json!({ "v": 1, "csl": { "id": "a b", "type": "article" } }))[0].starts_with("/csl/id: ")
        );
        // The CSL schema
        assert!(!errors(json!({ "v": 1, "csl": { "id": "a", "type": "no-such-type" } })).is_empty());
        assert!(
            !errors(
                json!({ "v": 1, "sources": { "x": { "csl": { "id": "a", "type": "book", "author": "A" } } } })
            )
            .is_empty()
        );
        assert!(!errors(json!({ "v": 1, "sources": { "inspire": { "csl": { "id": "a", "type": "book" }, "recid": 0 } } }))
            .is_empty());
        assert!(!errors(json!({ "v": 1, "tags": ["a", "a"] })).is_empty());
        assert_eq!(errors(json!({ "v": 1, "tags": [""] })).len(), 1);
        // Any other string, as it is
        for tag in [
            " a ",
            "a
b",
            ":",
            "a b",
            "a::b",
            "topic:AdS/CFT",
            "教科書",
            "status:done",
            "status:read",
        ] {
            assert_eq!(errors(json!({ "v": 1, "tags": [tag] })), Vec::<String>::new(), "{tag:?}");
        }
    }
}
