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
    if n < 10 || queries == 0 {
        return Err("use at least 10 vectors and one query".into());
    }
    let dir = tempfile::tempdir()?;
    let db = VectorDb::open(dir.path())?;
    let docs = db.create_collection("bench", dim, Metric::Cosine)?;
    let mut rng = 0x123456789abcdef;
    let ingestion = Instant::now();
    for _ in 0..n {
        let vector: Vec<_> = (0..dim).map(|_| random(&mut rng)).collect();
        docs.upsert_vector(&vector, json!(null))?;
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
            "vectors":n,"dimensions":dim,"queries":queries,"metric":"cosine","k":10,"ef_search":128,
            "ingest_seconds":ingest_seconds,"vectors_per_second":n as f64/ingest_seconds,
            "first_build_and_query_seconds":build_seconds,"recall_at_10":hits as f64/(queries*10) as f64,
            "ann_p50_ms":percentile(&mut ann_times,0.5),"ann_p95_ms":percentile(&mut ann_times,0.95),
            "exact_p50_ms":percentile(&mut exact_times,0.5),"exact_p95_ms":percentile(&mut exact_times,0.95),
            "disk_bytes":disk_bytes,"linux_peak_rss_kib":memory_kib
        }))?
    );
    Ok(())
}
