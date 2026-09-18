//! Query builder and metadata filter DSL.
//!
//! Phase 1 scaffolding: only a single `field CONTAINS "value"` predicate is
//! represented. The full DSL (`AND`/`OR`, comparison operators) lands in
//! Phase 5 — see `PROJECT_SPEC.md`'s phase list.

use serde::{Deserialize, Serialize};

/// A parsed metadata filter expression.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Filter {
    /// The metadata field being filtered on.
    pub field: String,
    /// The value the field's array must contain.
    pub contains: String,
}

impl Filter {
    /// Does `metadata` satisfy this filter?
    pub fn matches(&self, metadata: &serde_json::Value) -> bool {
        metadata
            .get(&self.field)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .any(|item| item.as_str() == Some(self.contains.as_str()))
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn filter_matches_when_the_tag_array_contains_the_value() {
        let filter = Filter {
            field: "tags".into(),
            contains: "rust".into(),
        };
        let metadata = json!({ "tags": ["rust", "db"] });
        assert!(filter.matches(&metadata));
    }

    #[test]
    fn filter_does_not_match_when_the_tag_array_lacks_the_value() {
        let filter = Filter {
            field: "tags".into(),
            contains: "python".into(),
        };
        let metadata = json!({ "tags": ["rust", "db"] });
        assert!(!filter.matches(&metadata));
    }
}
