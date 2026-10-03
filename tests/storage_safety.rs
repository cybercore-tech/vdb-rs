//! Public API regressions for isolation, concurrency and durable reopening.
#![cfg(all(feature = "storage", feature = "metrics", feature = "serde-query"))]
use serde_json::json;
use vdb::{DbOptions, Error, Metric, VectorDb};

#[test]
fn collections_never_overwrite_or_delete_each_other() {
    let dir = tempfile::tempdir().unwrap();
    let db = VectorDb::open(dir.path()).unwrap();
    let a = db.create_collection("a", 2, Metric::Euclidean).unwrap();
    let b = db.create_collection("b", 2, Metric::Euclidean).unwrap();
    let aid = a.upsert_vector(&[1., 0.], json!({"owner":"a"})).unwrap();
    let bid = b.upsert_vector(&[0., 1.], json!({"owner":"b"})).unwrap();
    assert_ne!(aid, bid);
    assert!(!a.delete_vector(bid).unwrap());
    assert!(!b.delete_vector(aid).unwrap());
    assert_eq!(
        a.query(&[1., 0.], 10).execute().unwrap()[0]
            .metadata
            .as_ref()
            .unwrap()["owner"],
        "a"
    );
    assert_eq!(b.query(&[0., 1.], 10).execute().unwrap()[0].id, bid);
    db.delete_collection("a").unwrap();
    assert_eq!(b.query(&[0., 1.], 10).execute().unwrap()[0].id, bid);
}

#[test]
fn delete_recreate_rejects_old_handles_and_keeps_ids_unique() {
    let dir = tempfile::tempdir().unwrap();
    let db = VectorDb::open(dir.path()).unwrap();
    let old = db.create_collection("docs", 2, Metric::Cosine).unwrap();
    let id = old.upsert_vector(&[1., 0.], json!(null)).unwrap();
    old.query(&[1., 0.], 1).execute().unwrap(); // publish index
    assert!(db.delete_collection("docs").unwrap());
    let new = db.create_collection("docs", 3, Metric::Euclidean).unwrap();
    assert!(new.query(&[0., 0., 0.], 10).execute().unwrap().is_empty());
    assert!(matches!(
        old.upsert_vector(&[1., 0.], json!(null)),
        Err(Error::StaleCollection(_))
    ));
    assert!(matches!(
        old.delete_vector(id),
        Err(Error::StaleCollection(_))
    ));
    assert!(matches!(
        old.query(&[1., 0.], 1).execute(),
        Err(Error::StaleCollection(_))
    ));
    assert!(!new.delete_vector(id).unwrap());
    assert!(new.upsert_vector(&[1., 0., 0.], json!(null)).unwrap() > id);
}

#[test]
fn real_close_reopen_preserves_data_and_allocation() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let db = VectorDb::open(dir.path()).unwrap();
        let c = db.create_collection("docs", 2, Metric::Euclidean).unwrap();
        let id = c
            .upsert_vector(&[3., 4.], json!({"title":"persisted"}))
            .unwrap();
        c.query(&[0., 0.], 1).execute().unwrap();
        id
    };
    let db = VectorDb::open(dir.path()).unwrap();
    let c = db.collection("docs").unwrap();
    let results = c.query(&[0., 0.], 1).execute().unwrap();
    assert_eq!(results[0].id, id);
    assert_eq!(results[0].score, 5.);
    assert_eq!(results[0].metadata.as_ref().unwrap()["title"], "persisted");
    assert!(c.upsert_vector(&[0., 0.], json!(null)).unwrap() > id);
}

#[test]
fn cloned_handles_serialize_concurrent_writes_and_queries() {
    let dir = tempfile::tempdir().unwrap();
    let db = VectorDb::open(dir.path()).unwrap();
    let c = db.create_collection("docs", 2, Metric::Euclidean).unwrap();
    let threads: Vec<_> = (0..4)
        .map(|worker| {
            let c = c.clone();
            std::thread::spawn(move || {
                let mut ids = Vec::new();
                for i in 0..25 {
                    ids.push(
                        c.upsert_vector(&[worker as f32, i as f32], json!({"worker":worker}))
                            .unwrap(),
                    );
                    c.query(&[0., 0.], 1).exact().execute().unwrap();
                }
                ids
            })
        })
        .collect();
    let mut ids: Vec<_> = threads
        .into_iter()
        .flat_map(|t| t.join().unwrap())
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 100);
    assert_eq!(
        c.query(&[0., 0.], 200).exact().execute().unwrap().len(),
        100
    );
    drop(c);
    drop(db);
    let db = VectorDb::open(dir.path()).unwrap();
    assert_eq!(
        db.collection("docs")
            .unwrap()
            .query(&[0., 0.], 200)
            .execute()
            .unwrap()
            .len(),
        100
    );
}

#[test]
fn exclusive_lock_lives_until_last_collection_handle_closes() {
    let dir = tempfile::tempdir().unwrap();
    let db = VectorDb::open(dir.path()).unwrap();
    let c = db.create_collection("docs", 1, Metric::Euclidean).unwrap();
    assert!(matches!(
        VectorDb::open(dir.path()),
        Err(Error::DatabaseLocked(_))
    ));
    drop(db);
    assert!(matches!(
        VectorDb::open(dir.path()),
        Err(Error::DatabaseLocked(_))
    ));
    drop(c);
    assert!(VectorDb::open(dir.path()).is_ok());
}

#[test]
fn invalid_names_dimensions_and_nonfinite_vectors_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let db = VectorDb::open(dir.path()).unwrap();
    for name in ["", "../escape", "/tmp/escape", ".", "a/b", "a\\b", "héllo"] {
        assert!(matches!(
            db.create_collection(name, 1, Metric::Cosine),
            Err(Error::InvalidInput(_))
        ));
    }
    for dim in [0, 65537] {
        assert!(matches!(
            db.create_collection("bad", dim, Metric::Cosine),
            Err(Error::InvalidInput(_))
        ));
    }
    let c = db.create_collection("docs", 1, Metric::Cosine).unwrap();
    for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(matches!(
            c.upsert_vector(&[value], json!(null)),
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            c.query(&[value], 1).execute(),
            Err(Error::InvalidInput(_))
        ));
    }
    assert!(c.query(&[0.], 0).execute().unwrap().is_empty());
    assert!(c.query(&[0.], 1).ef_search(0).execute().is_err());
}

#[test]
fn compaction_preserves_ids_metadata_and_rebuilds_index() {
    let dir = tempfile::tempdir().unwrap();
    let db = VectorDb::open(dir.path()).unwrap();
    let c = db.create_collection("docs", 2, Metric::Euclidean).unwrap();
    let a = c.upsert_vector(&[0., 0.], json!({"keep":false})).unwrap();
    let b = c.upsert_vector(&[3., 4.], json!({"keep":true})).unwrap();
    c.query(&[0., 0.], 2).execute().unwrap();
    c.delete_vector(a).unwrap();
    c.compact().unwrap();
    assert_eq!(
        std::fs::metadata(dir.path().join("docs.vectors"))
            .unwrap()
            .len(),
        64
    );
    let results = c.query(&[0., 0.], 10).execute().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, b);
    assert_eq!(results[0].metadata.as_ref().unwrap()["keep"], true);
}

#[test]
fn lmdb_capacity_failure_never_creates_a_partial_insert() {
    let dir = tempfile::tempdir().unwrap();
    let db = VectorDb::open_with_options(
        dir.path(),
        DbOptions {
            map_size: 1024 * 1024,
        },
    )
    .unwrap();
    let c = db.create_collection("docs", 1, Metric::Euclidean).unwrap();
    c.upsert_vector(&[1.], json!({"small":true})).unwrap();
    let before = std::fs::metadata(dir.path().join("docs.vectors"))
        .unwrap()
        .len();
    assert!(
        c.upsert_vector(&[2.], json!({"huge":"x".repeat(2*1024*1024)}))
            .is_err()
    );
    assert_eq!(
        std::fs::metadata(dir.path().join("docs.vectors"))
            .unwrap()
            .len(),
        before
    );
    let results = c.query(&[0.], 10).exact().execute().unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].metadata.as_ref().unwrap()["small"], true);
}

#[test]
fn compaction_preserves_f32_bits_in_recovery_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let db = VectorDb::open(dir.path()).unwrap();
    let c = db.create_collection("bits", 6, Metric::Euclidean).unwrap();
    let values = [
        f32::from_bits(1),
        -0.0,
        f32::MAX,
        f32::MIN_POSITIVE,
        0.1,
        -12345.678,
    ];
    c.upsert_vector(&values, json!(null)).unwrap();
    c.compact().unwrap();
    let file = vdb::vector::VectorFile::open(&dir.path().join("bits.vectors")).unwrap();
    let actual = file.read_at(32).unwrap();
    assert_eq!(
        actual.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        values.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
    );
}
