//! Corrupt and unaligned file inputs must return errors rather than panic.
#![cfg(feature = "storage")]
use vdb::vector::{VectorFile, VectorFileWriter};
#[test]
fn vector_reader_rejects_truncation_offsets_and_corrupt_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("valid.vectors");
    let mut writer = VectorFileWriter::create(&path, 8).unwrap();
    let offset = writer.append(&[1.; 8]).unwrap();
    writer.flush().unwrap();
    drop(writer);
    let bytes = std::fs::read(&path).unwrap();
    {
        let vectors = VectorFile::open(&path).unwrap();
        for offset in [0, 1, 31, 33, 64, u64::MAX] {
            assert!(vectors.read_at(offset).is_err());
        }
    }
    for len in 0..bytes.len() {
        std::fs::write(&path, &bytes[..len]).unwrap();
        assert!(
            VectorFile::open(&path).is_err(),
            "accepted truncation at {len}"
        );
        assert!(VectorFileWriter::open_append(&path).is_err());
    }
    let mut damaged = bytes.clone();
    damaged[offset as usize] ^= 1;
    std::fs::write(&path, &damaged).unwrap();
    assert!(VectorFile::open(&path).unwrap().read_at(offset).is_err());
    damaged = bytes;
    damaged[4] ^= 1;
    std::fs::write(&path, &damaged).unwrap();
    assert!(VectorFile::open(&path).is_err());
}

#[cfg(all(feature = "metrics", feature = "serde-query", feature = "index-hnsw"))]
#[test]
fn mmap_graph_rejects_truncation_and_checksum_damage() {
    use vdb::{Metric, VectorDb};
    let dir = tempfile::tempdir().unwrap();
    {
        let db = VectorDb::open(dir.path()).unwrap();
        let docs = db.create_collection("docs", 2, Metric::Euclidean).unwrap();
        for i in 0..4 {
            docs.upsert_vector(&[i as f32, 0.], serde_json::json!(null))
                .unwrap();
        }
        docs.query(&[0., 0.], 2).execute().unwrap();
    }
    let path = dir.path().join("docs.index");
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(vdb::index::IndexFile::open(&path).unwrap().len(), 4);
    for len in 0..bytes.len() {
        std::fs::write(&path, &bytes[..len]).unwrap();
        assert!(
            vdb::index::IndexFile::open(&path).is_err(),
            "accepted truncation at {len}"
        );
    }
    for position in [8, 20, 48, 64, bytes.len() - 1] {
        let mut damaged = bytes.clone();
        damaged[position] ^= 1;
        std::fs::write(&path, &damaged).unwrap();
        assert!(vdb::index::IndexFile::open(&path).is_err());
    }
}
