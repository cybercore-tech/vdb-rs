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
            let rows: Vec<_> = (0..512)
                .map(|_| ((0..16).map(|_| random(&mut rng)).collect(), json!(null)))
                .collect();
            c.upsert_batch(&rows).unwrap();
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
        let added: Vec<_> = (0..64)
            .map(|_| ((0..16).map(|_| random(&mut rng)).collect(), json!("batch")))
            .collect();
        c.upsert_batch(&added).unwrap();
        for id in 0..8 {
            c.delete_vector(id).unwrap();
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
        assert!(
            hits >= 114,
            "incremental metric={metric:?} recall={hits}/120"
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

#[test]
fn incremental_refresh_retains_deleted_routes_across_reopen_and_compaction() {
    for metric in [Metric::Euclidean, Metric::Cosine, Metric::DotProduct] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("docs.index");
        let ids;
        {
            let db = VectorDb::open(dir.path()).unwrap();
            let c = db.create_collection("docs", 16, metric).unwrap();
            let mut rng = 123456789;
            let rows: Vec<_> = (0..128)
                .map(|i| {
                    (
                        (0..16).map(|_| random(&mut rng)).collect(),
                        json!({"row":i}),
                    )
                })
                .collect();
            ids = c.upsert_batch(&rows).unwrap();
            assert_eq!(
                c.query(&rows[0].0, 128)
                    .ef_search(256)
                    .execute()
                    .unwrap()
                    .len(),
                128
            );
            let snapshot = std::fs::read(&path).unwrap();
            for id in &ids[..120] {
                assert!(c.delete_vector(*id).unwrap());
            }
            // A stale snapshot survives clean mutations, to be refreshed after reopen.
            assert_eq!(std::fs::read(&path).unwrap(), snapshot);
            c.upsert_batch(&[
                (vec![0.5; 16], json!({"new":true})),
                (vec![-0.5; 16], json!({"new":true})),
            ])
            .unwrap();
        }
        {
            let db = VectorDb::open(dir.path()).unwrap();
            let c = db.collection("docs").unwrap();
            let query = vec![0.5; 16];
            let ann = c.query(&query, 20).ef_search(1).execute().unwrap();
            let exact = c.query(&query, 20).exact().execute().unwrap();
            assert_eq!(ann, exact, "metric={metric:?}");
            // Full live-only reconstruction would contain 10 nodes. Retaining
            // all 130 proves the old graph and its routing nodes were reused.
            let index = vdb::index::IndexFile::open(&path).unwrap();
            assert_eq!(index.len(), 130);
            assert_eq!(index.revision(), c.config().unwrap().revision);
            drop(index);
            for id in &ids[120..] {
                c.delete_vector(*id).unwrap();
            }
            c.delete_vector(128).unwrap();
            c.delete_vector(129).unwrap();
            assert!(c.query(&query, 10).execute().unwrap().is_empty());
            assert_eq!(vdb::index::IndexFile::open(&path).unwrap().len(), 130);
            c.upsert_batch(&[(query.clone(), json!("after-empty"))])
                .unwrap();
            assert_eq!(c.query(&query, 10).execute().unwrap()[0].id, 130);
            assert_eq!(vdb::index::IndexFile::open(&path).unwrap().len(), 131);
            c.compact().unwrap();
            assert_eq!(c.query(&query, 10).execute().unwrap().len(), 1);
            assert_eq!(vdb::index::IndexFile::open(&path).unwrap().len(), 1);
        }
        let db = VectorDb::open(dir.path()).unwrap();
        assert_eq!(
            db.collection("docs")
                .unwrap()
                .query(&[0.5; 16], 10)
                .execute()
                .unwrap()[0]
                .id,
            130
        );
    }
}
