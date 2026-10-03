//! Runnable insertion, filtering, ANN search and durable reopening example.
use serde_json::json;
use vdb::{Filter, Metric, VectorDb};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    {
        let db = VectorDb::open(dir.path())?;
        let docs = db.create_collection("docs", 3, Metric::Cosine)?;
        docs.upsert_vector(&[1., 0., 0.], json!({"title":"Rust", "tags":["rust"]}))?;
        docs.upsert_vector(&[0., 1., 0.], json!({"title":"Python", "tags":["python"]}))?;
        println!("ANN: {:?}", docs.query(&[1., 0., 0.], 2).execute()?);
        println!(
            "Filtered: {:?}",
            docs.query(&[1., 0., 0.], 2)
                .filter(Filter {
                    field: "tags".into(),
                    contains: "rust".into()
                })
                .execute()?
        );
    }
    let db = VectorDb::open(dir.path())?;
    println!(
        "After reopening: {:?}",
        db.collection("docs")?.query(&[1., 0., 0.], 2).execute()?
    );
    Ok(())
}
