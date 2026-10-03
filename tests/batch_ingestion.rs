//! Batch atomicity, durable IDs and concurrent visibility through the public API.
#![cfg(all(feature = "storage", feature = "metrics", feature = "serde-query"))]
use serde_json::json;
use vdb::{DbOptions, Error, Metric, VectorDb};

#[test]
fn batches_validate_before_mutation_and_preserve_input_order_on_reopen() {
    let dir = tempfile::tempdir().unwrap();
    {
        let db = VectorDb::open(dir.path()).unwrap();
        let c = db.create_collection("docs", 2, Metric::Euclidean).unwrap();
        let before = c.config().unwrap();
        let bytes = std::fs::read(dir.path().join("docs.vectors")).unwrap();
        assert!(c.upsert_batch(&[]).unwrap().is_empty());
        for invalid in [vec![1.], vec![1., f32::NAN], vec![f32::INFINITY, 0.]] {
            assert!(
                c.upsert_batch(&[(vec![3., 4.], json!("valid")), (invalid, json!("invalid"))])
                    .is_err()
            );
        }
        assert_eq!(c.config().unwrap().revision, before.revision);
        assert_eq!(c.config().unwrap().next_id, before.next_id);
        assert_eq!(
            std::fs::read(dir.path().join("docs.vectors")).unwrap(),
            bytes
        );
        assert!(c.query(&[0., 0.], 10).exact().execute().unwrap().is_empty());
        assert_eq!(
            c.upsert_batch(&[
                (vec![3., 4.], json!({"row":0})),
                (vec![0., 0.], json!({"row":1}))
            ])
            .unwrap(),
            vec![0, 1]
        );
        assert_eq!(c.config().unwrap().revision, before.revision + 1);
        let other = db.create_collection("other", 2, Metric::Euclidean).unwrap();
        assert_eq!(
            other.upsert_batch(&[(vec![1., 0.], json!(null))]).unwrap(),
            vec![2]
        );
        assert!(!other.delete_vector(0).unwrap());
    }
    let db = VectorDb::open(dir.path()).unwrap();
    let c = db.collection("docs").unwrap();
    let results = c.query(&[0., 0.], 10).execute().unwrap();
    assert_eq!(results.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 0]);
    assert_eq!(results[0].metadata.as_ref().unwrap()["row"], 1);
    assert_eq!(results[1].score, 5.);
    assert_eq!(c.upsert_vector(&[1., 1.], json!(null)).unwrap(), 3);
    db.delete_collection("docs").unwrap();
    db.create_collection("docs", 2, Metric::Euclidean).unwrap();
    assert!(matches!(
        c.upsert_batch(&[]),
        Err(Error::StaleCollection(_))
    ));
}

#[test]
fn map_full_rolls_back_the_entire_batch_and_id_allocation() {
    let dir = tempfile::tempdir().unwrap();
    let db = VectorDb::open_with_options(
        dir.path(),
        DbOptions {
            map_size: 1024 * 1024,
        },
    )
    .unwrap();
    let c = db.create_collection("docs", 1, Metric::Euclidean).unwrap();
    let before = std::fs::read(dir.path().join("docs.vectors")).unwrap();
    assert!(
        c.upsert_batch(&[
            (vec![1.], json!("small")),
            (vec![2.], json!("x".repeat(2 * 1024 * 1024)))
        ])
        .is_err()
    );
    assert_eq!(c.config().unwrap().revision, 0);
    assert_eq!(
        std::fs::read(dir.path().join("docs.vectors")).unwrap(),
        before
    );
    assert!(c.query(&[0.], 10).exact().execute().unwrap().is_empty());
    assert_eq!(
        c.upsert_batch(&[(vec![3.], json!("retry"))]).unwrap(),
        vec![0]
    );
}

#[test]
fn concurrent_batches_are_contiguous_and_queries_see_whole_batches() {
    let dir = tempfile::tempdir().unwrap();
    let db = VectorDb::open(dir.path()).unwrap();
    let c = db.create_collection("docs", 2, Metric::Euclidean).unwrap();
    let threads: Vec<_> = (0..4)
        .map(|worker| {
            let c = c.clone();
            std::thread::spawn(move || {
                let mut ids = Vec::new();
                for batch in 0..5 {
                    let rows: Vec<_> = (0..8)
                        .map(|row| {
                            (
                                vec![worker as f32, (batch * 8 + row) as f32],
                                json!({"worker":worker}),
                            )
                        })
                        .collect();
                    let batch_ids = c.upsert_batch(&rows).unwrap();
                    assert!(batch_ids.windows(2).all(|pair| pair[1] == pair[0] + 1));
                    ids.extend(batch_ids);
                    let results = c.query(&[0., 0.], 200).exact().execute().unwrap();
                    assert_eq!(results.len() % 8, 0);
                }
                ids
            })
        })
        .collect();
    let mut ids: Vec<_> = threads
        .into_iter()
        .flat_map(|t| t.join().unwrap())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, (0..160).collect::<Vec<_>>());
}
