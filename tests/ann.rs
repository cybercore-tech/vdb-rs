//! Recall and persisted mmap traversal against the exact reference path.
#![cfg(all(
    feature = "storage",
    feature = "metrics",
    feature = "serde-query",
    feature = "index-hnsw"
))]
use serde_json::json;
use vdb::{Metric, VectorDb};
fn random(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 40) as f32 / (1u32 << 24) as f32 * 2. - 1.
}
#[test]
fn hnsw_recall_survives_reopen_mutations_and_corrupt_index() {
    for metric in [Metric::Euclidean, Metric::Cosine, Metric::DotProduct] {
        let dir = tempfile::tempdir().unwrap();
        let mut rng = 123456789;
        let queries: Vec<Vec<f32>> = (0..12)
            .map(|_| (0..16).map(|_| random(&mut rng)).collect())
            .collect();
        {
            let db = VectorDb::open(dir.path()).unwrap();
            let c = db.create_collection("docs", 16, metric).unwrap();
            for _ in 0..512 {
                let v: Vec<_> = (0..16).map(|_| random(&mut rng)).collect();
                c.upsert_vector(&v, json!(null)).unwrap();
            }
            let mut hits = 0;
            for q in &queries {
                let exact = c.query(q, 10).exact().execute().unwrap();
                let ann = c.query(q, 10).ef_search(128).execute().unwrap();
                hits += ann
                    .iter()
                    .filter(|a| exact.iter().any(|e| a.id == e.id))
                    .count();
            }
            assert!(hits >= 114, "metric={metric:?} recall={hits}/120");
        }
        let db = VectorDb::open(dir.path()).unwrap();
        let c = db.collection("docs").unwrap();
        let best = c.query(&queries[0], 1).ef_search(128).execute().unwrap()[0].id;
        c.delete_vector(best).unwrap();
        assert!(
            c.query(&queries[0], 10)
                .execute()
                .unwrap()
                .iter()
                .all(|r| r.id != best)
        );
        let inserted = c.upsert_vector(&queries[0], json!({"new":true})).unwrap();
        assert!(
            c.query(&queries[0], 512)
                .ef_search(512)
                .execute()
                .unwrap()
                .iter()
                .any(|r| r.id == inserted)
        );
        drop(c);
        drop(db);
        std::fs::write(dir.path().join("docs.index"), b"corrupt").unwrap();
        let db = VectorDb::open(dir.path()).unwrap();
        assert_eq!(
            db.collection("docs")
                .unwrap()
                .query(&queries[0], 10)
                .execute()
                .unwrap()
                .len(),
            10
        );
    }
}
