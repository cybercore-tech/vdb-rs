//! End-to-end round trip across lower-level storage primitives: create a
//! collection, write a vector to its `.vectors` file, record where it
//! landed in the `vectors` LMDB table, then read it back through both.
//!
//! Lower-level primitives remain available; use VectorDb for recovery guarantees.
#![cfg(feature = "storage")]

use vdb::kv::{CollectionConfig, KvEngine, VectorMeta};
use vdb::vector::{VectorFile, VectorFileWriter};

#[test]
fn write_a_vector_and_read_it_back_through_kv_engine_and_vector_file() {
    let dir = tempfile::tempdir().unwrap();

    let engine = KvEngine::open(dir.path()).unwrap();
    engine
        .put_collection(
            "docs",
            &CollectionConfig {
                dim: 3,
                metric: "cosine".into(),
                next_id: 0,
                generation: 0,
                revision: 0,
            },
        )
        .unwrap();

    let vectors_path = dir.path().join("docs.vectors");
    let mut writer = VectorFileWriter::create(&vectors_path, 3).unwrap();
    let offset = writer.append(&[0.1, 0.2, 0.3]).unwrap();
    writer.flush().unwrap();

    engine
        .put_vector_meta(
            1,
            &VectorMeta {
                collection: "docs".into(),
                offset,
                dim: 3,
            },
        )
        .unwrap();
    engine
        .put_metadata(1, &serde_json::json!({ "title": "hello" }))
        .unwrap();

    let meta = engine.get_vector_meta(1).unwrap().unwrap();
    let vf = VectorFile::open(&vectors_path).unwrap();
    let vector = vf.read_at(meta.offset).unwrap();

    assert_eq!(vector, &[0.1, 0.2, 0.3]);
    assert_eq!(meta.collection, "docs");
    assert_eq!(engine.get_collection("docs").unwrap().unwrap().dim, 3);
    assert_eq!(
        engine.get_metadata(1).unwrap().unwrap()["title"],
        serde_json::json!("hello")
    );
}
