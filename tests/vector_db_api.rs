//! End-to-end exercise of the real public API (`VectorDb`/`Collection`/
//! `QueryBuilder`), matching `PROJECT_SPEC.md`'s "Primary user
//! experience" example — this is the first real implementation of it, so
//! this test doubles as proof the example actually works, and is the
//! source for the README's usage example (kept honest the same way
//! `tests/basic_round_trip.rs` keeps the Phase 2/3 primitives example
//! honest).
#![cfg(all(feature = "storage", feature = "metrics", feature = "serde-query"))]

use vdb::{Filter, Metric, VectorDb};

#[test]
fn create_collection_upsert_and_query_with_a_filter() {
    let dir = tempfile::tempdir().unwrap();
    let db = VectorDb::open(dir.path()).unwrap();

    let docs = db.create_collection("docs", 3, Metric::Cosine).unwrap();

    let rust_id = docs
        .upsert_vector(&[1.0, 0.0, 0.0], serde_json::json!({ "tags": ["rust"] }))
        .unwrap();
    docs.upsert_vector(&[0.0, 1.0, 0.0], serde_json::json!({ "tags": ["python"] }))
        .unwrap();
    docs.upsert_vector(
        &[0.9, 0.1, 0.0],
        serde_json::json!({ "tags": ["rust", "db"] }),
    )
    .unwrap();

    let results = docs
        .query(&[1.0, 0.0, 0.0], 10)
        .filter(Filter {
            field: "tags".into(),
            contains: "rust".into(),
        })
        .execute()
        .unwrap();

    // Both "rust"-tagged vectors match the filter; the exact match ranks first.
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].id, rust_id);

    // Collection state persists across a fresh handle to the same db.
    let reopened = db.collection("docs").unwrap();
    assert_eq!(reopened.config().unwrap().dim, 3);
    assert_eq!(db.list_collections().unwrap(), vec!["docs"]);
}
