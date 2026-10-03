//! Reproducible ingestion, first index build, warmed ANN/exact latency and recall.
use serde_json::json;
use std::time::Instant;
use vdb::{Metric, VectorDb};
fn random(state: &mut u64) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    (*state >> 40) as f32 / (1u32 << 24) as f32 * 2. - 1.
}
fn percentile(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * p).round() as usize]
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let n: usize = args.first().map(|s| s.parse()).transpose()?.unwrap_or(1000);
    let dim: u32 = args.get(1).map(|s| s.parse()).transpose()?.unwrap_or(32);
    let queries: usize = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(100);
    let batch_size: usize = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(128);
    if n < 10 || queries == 0 || batch_size == 0 {
        return Err("use at least 10 vectors, one query and a positive batch size".into());
    }
    let dir = tempfile::tempdir()?;
    let db = VectorDb::open(dir.path())?;
    let docs = db.create_collection("bench", dim, Metric::Cosine)?;
    let mut rng = 0x123456789abcdef;
    let rows: Vec<_> = (0..n)
        .map(|_| ((0..dim).map(|_| random(&mut rng)).collect(), json!(null)))
        .collect();
    let ingestion = Instant::now();
    for batch in rows.chunks(batch_size) {
        docs.upsert_batch(batch)?;
    }
    let ingest_seconds = ingestion.elapsed().as_secs_f64();
    let query_vectors: Vec<Vec<_>> = (0..queries)
        .map(|_| (0..dim).map(|_| random(&mut rng)).collect())
        .collect();
    let build = Instant::now();
    docs.query(&query_vectors[0], 10).ef_search(128).execute()?;
    let build_seconds = build.elapsed().as_secs_f64();
    let mut ann_times = Vec::new();
    let mut exact_times = Vec::new();
    let mut hits = 0;
    for query in &query_vectors {
        let start = Instant::now();
        let ann = docs.query(query, 10).ef_search(128).execute()?;
        ann_times.push(start.elapsed().as_secs_f64() * 1000.);
        let start = Instant::now();
        let exact = docs.query(query, 10).exact().execute()?;
        exact_times.push(start.elapsed().as_secs_f64() * 1000.);
        hits += ann
            .iter()
            .filter(|a| exact.iter().any(|e| e.id == a.id))
            .count();
    }
    // Five write/delete/query rounds on the already published graph.
    let mut mixed_times = Vec::new();
    let mut mixed_hits = 0;
    let mixed_batch_size = batch_size.min(16);
    for round in 0..5 {
        let added: Vec<_> = (0..mixed_batch_size)
            .map(|_| ((0..dim).map(|_| random(&mut rng)).collect(), json!(null)))
            .collect();
        let start = Instant::now();
        docs.upsert_batch(&added)?;
        docs.delete_vector(round)?;
        let ann = docs.query(&query_vectors[0], 10).ef_search(128).execute()?;
        mixed_times.push(start.elapsed().as_secs_f64() * 1000.);
        let exact = docs.query(&query_vectors[0], 10).exact().execute()?;
        mixed_hits += ann
            .iter()
            .filter(|a| exact.iter().any(|e| e.id == a.id))
            .count();
    }
    drop(docs);
    drop(db);
    let db = VectorDb::open(dir.path())?;
    let docs = db.collection("bench")?;
    let reopen = Instant::now();
    docs.query(&query_vectors[0], 10).ef_search(128).execute()?;
    let reopen_query_ms = reopen.elapsed().as_secs_f64() * 1000.;
    drop(docs);
    drop(db);
    // Same final live vectors, deliberately discard the derived graph to
    // measure full reconstruction rather than incremental maintenance.
    std::fs::remove_file(dir.path().join("bench.index"))?;
    let db = VectorDb::open(dir.path())?;
    let docs = db.collection("bench")?;
    let rebuild = Instant::now();
    docs.query(&query_vectors[0], 10).ef_search(128).execute()?;
    let final_full_rebuild_seconds = rebuild.elapsed().as_secs_f64();
    let disk_bytes: u64 = std::fs::read_dir(dir.path())?
        .filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum();
    let memory_kib = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|n| n.parse::<u64>().ok())
        });
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "vectors":n,"batch_size":batch_size,"dimensions":dim,"queries":queries,"metric":"cosine","k":10,"ef_search":128,
            "ingest_seconds":ingest_seconds,"vectors_per_second":n as f64/ingest_seconds,
            "first_build_and_query_seconds":build_seconds,"recall_at_10":hits as f64/(queries*10) as f64,
            "ann_p50_ms":percentile(&mut ann_times,0.5),"ann_p95_ms":percentile(&mut ann_times,0.95),
            "exact_p50_ms":percentile(&mut exact_times,0.5),"exact_p95_ms":percentile(&mut exact_times,0.95),
            "mixed_rounds":5,"mixed_vectors_per_round":mixed_batch_size,"mixed_recall_at_10":mixed_hits as f64/50.,
            "mixed_write_delete_query_p50_ms":percentile(&mut mixed_times,0.5),
            "mixed_write_delete_query_p95_ms":percentile(&mut mixed_times,0.95),
            "reopen_first_query_ms":reopen_query_ms,"final_full_rebuild_seconds":final_full_rebuild_seconds,
            "disk_bytes":disk_bytes,"linux_peak_rss_kib":memory_kib
        }))?
    );
    Ok(())
}
