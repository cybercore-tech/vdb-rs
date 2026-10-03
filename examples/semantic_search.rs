//! Local-note evaluation using real embeddings prepared outside the Rust library.
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::Instant;
use vdb::{Collection, Filter, Metric, ScoredVector, VectorDb};

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;
#[derive(Deserialize)]
struct Row {
    vector: Vec<f32>,
    metadata: Value,
}
#[derive(Deserialize)]
struct Query {
    text: String,
    vector: Vec<f32>,
    model: String,
    #[serde(default)]
    relevant_paths: Vec<String>,
    #[serde(default)]
    keyword_top_sources: Vec<String>,
    filter_tag: Option<String>,
}
fn read_jsonl<T: serde::de::DeserializeOwned>(path: &str) -> AnyResult<Vec<T>> {
    BufReader::new(File::open(path)?)
        .lines()
        .filter_map(|line| match line {
            Ok(line) if line.trim().is_empty() => None,
            other => Some(other),
        })
        .map(|line| Ok(serde_json::from_str(&line?)?))
        .collect()
}
fn source(row: &Row) -> AnyResult<&str> {
    row.metadata["source"]
        .as_str()
        .ok_or_else(|| "missing source metadata".into())
}
fn search(collection: &Collection, query: &Query, exact: bool) -> vdb::Result<Vec<ScoredVector>> {
    let mut builder = collection.query(&query.vector, 32).ef_search(128);
    if exact {
        builder = builder.exact();
    }
    if let Some(tag) = &query.filter_tag {
        builder = builder.filter(Filter {
            field: "tags".into(),
            contains: tag.clone(),
        });
    }
    builder.execute()
}
fn documents(hits: &[ScoredVector]) -> Vec<Value> {
    let mut seen = HashSet::new();
    hits.iter().filter_map(|hit| {
        let metadata = hit.metadata.as_ref()?;
        let source = metadata["source"].as_str()?;
        if !seen.insert(source) { return None; }
        Some(json!({"source":source,"title":metadata["title"],"score":hit.score,
            "chunk":metadata["chunk"],"snippet":metadata["text"].as_str().unwrap_or("").chars().take(240).collect::<String>()}))
    }).take(5).collect()
}
fn rank(paths: &[String], relevant: &[String]) -> Option<usize> {
    paths
        .iter()
        .take(5)
        .position(|path| relevant.contains(path))
        .map(|i| i + 1)
}
fn paths(documents: &[Value]) -> Vec<String> {
    documents
        .iter()
        .filter_map(|doc| doc["source"].as_str().map(String::from))
        .collect()
}
fn percentile(values: &mut [f64], p: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    values[((values.len() - 1) as f64 * p).round() as usize]
}
fn insert(collection: &Collection, rows: &[&Row], size: usize) -> AnyResult<Vec<u64>> {
    let mut ids = Vec::new();
    for batch in rows.chunks(size) {
        let inputs: Vec<_> = batch
            .iter()
            .map(|row| (row.vector.clone(), row.metadata.clone()))
            .collect();
        ids.extend(collection.upsert_batch(&inputs)?);
    }
    Ok(ids)
}
fn verify_model(collection: &Collection, queries: &[Query]) -> AnyResult<()> {
    let zero = vec![0.; collection.config()?.dim as usize];
    let probe = collection.query(&zero, 1).exact().execute()?;
    let model = probe
        .first()
        .and_then(|hit| hit.metadata.as_ref())
        .and_then(|meta| meta["model"].as_str())
        .ok_or("missing corpus model identity")?;
    if queries.iter().any(|query| query.model != model) {
        return Err("query and corpus embedding models differ".into());
    }
    Ok(())
}
fn evaluate(
    corpus_path: &str,
    query_path: &str,
    db_path: &str,
    batch_size: usize,
) -> AnyResult<Value> {
    if Path::new(db_path).exists() {
        return Err("evaluation requires a new database directory".into());
    }
    let rows: Vec<Row> = read_jsonl(corpus_path)?;
    let queries: Vec<Query> = read_jsonl(query_path)?;
    if rows.is_empty() || queries.is_empty() || batch_size == 0 {
        return Err("require corpus rows, queries and a positive batch size".into());
    }
    let dim = u32::try_from(rows[0].vector.len())?;
    let model = rows[0].metadata["model"]
        .as_str()
        .ok_or("missing corpus model identity")?;
    if rows.iter().any(|row| {
        row.vector.len() != dim as usize || row.metadata["model"].as_str() != Some(model)
    }) || queries
        .iter()
        .any(|query| query.model != model || query.vector.len() != dim as usize)
    {
        return Err("inconsistent corpus/query model or dimension".into());
    }
    let sources: BTreeSet<_> = rows.iter().map(source).collect::<AnyResult<_>>()?;
    let initial_sources: HashSet<_> = sources
        .iter()
        .copied()
        .take((sources.len() * 3 / 4).max(1))
        .collect();
    let (initial, added): (Vec<_>, Vec<_>) = rows
        .iter()
        .partition(|row| initial_sources.contains(source(row).unwrap()));
    let db = VectorDb::open(Path::new(db_path))?;
    let notes = db.create_collection("notes", dim, Metric::Cosine)?;
    let start = Instant::now();
    let initial_ids = insert(&notes, &initial, batch_size)?;
    let initial_ingest_seconds = start.elapsed().as_secs_f64();
    let start = Instant::now();
    notes
        .query(&queries[0].vector, 10)
        .ef_search(128)
        .execute()?;
    let initial_build_ms = start.elapsed().as_secs_f64() * 1000.;
    let start = Instant::now();
    let added_ids = insert(&notes, &added, batch_size)?;
    let incremental_ingest_ms = start.elapsed().as_secs_f64() * 1000.;
    drop(notes);
    drop(db);
    // Reopen with the old graph revision and newly committed real documents.
    let db = VectorDb::open(Path::new(db_path))?;
    let notes = db.collection("notes")?;
    let start = Instant::now();
    notes
        .query(&queries[0].vector, 10)
        .ef_search(128)
        .execute()?;
    let reopen_incremental_refresh_ms = start.elapsed().as_secs_f64() * 1000.;
    verify_model(&notes, &queries)?;
    let mut ann_times = Vec::new();
    let mut exact_times = Vec::new();
    let mut query_reports = Vec::new();
    let mut recalled = 0;
    let mut reference = 0;
    let mut labeled = 0;
    let (mut semantic_hits, mut exact_hits, mut keyword_hits) = (0, 0, 0);
    let (mut semantic_mrr, mut exact_mrr, mut keyword_mrr) = (0., 0., 0.);
    for _ in 0..3 {
        for query in &queries {
            let start = Instant::now();
            let ann = search(&notes, query, false)?;
            ann_times.push(start.elapsed().as_secs_f64() * 1000.);
            let start = Instant::now();
            let exact = search(&notes, query, true)?;
            exact_times.push(start.elapsed().as_secs_f64() * 1000.);
            if query_reports.len() == queries.len() {
                continue;
            }
            if query.filter_tag.is_none() {
                reference += exact.len().min(10);
                recalled += ann
                    .iter()
                    .take(10)
                    .filter(|a| exact.iter().take(10).any(|e| e.id == a.id))
                    .count();
            }
            let top = documents(&ann);
            let exact_top = documents(&exact);
            let semantic_rank = rank(&paths(&top), &query.relevant_paths);
            let exact_rank = rank(&paths(&exact_top), &query.relevant_paths);
            let keyword_rank = rank(&query.keyword_top_sources, &query.relevant_paths);
            if !query.relevant_paths.is_empty() {
                labeled += 1;
                for (rank, hits, mrr) in [
                    (semantic_rank, &mut semantic_hits, &mut semantic_mrr),
                    (exact_rank, &mut exact_hits, &mut exact_mrr),
                    (keyword_rank, &mut keyword_hits, &mut keyword_mrr),
                ] {
                    if let Some(rank) = rank {
                        *hits += 1;
                        *mrr += 1. / rank as f64;
                    }
                }
            }
            query_reports.push(json!({"text":query.text,"filter_tag":query.filter_tag,
                "relevant_paths":query.relevant_paths,"semantic_rank_at_5":semantic_rank,
                "exact_rank_at_5":exact_rank,"keyword_rank_at_5":keyword_rank,
                "keyword_top_sources":query.keyword_top_sources,"top_documents":top}));
        }
    }
    // Simulate deletion and reindexing of one whole note, not only one chunk.
    let deleted_source = source(&rows[0])?;
    let ordered_rows: Vec<_> = initial.iter().chain(&added).copied().collect();
    let ids: Vec<_> = initial_ids.into_iter().chain(added_ids).collect();
    let note_rows: Vec<_> = ordered_rows
        .iter()
        .copied()
        .filter(|row| source(row).unwrap() == deleted_source)
        .collect();
    let old_ids: HashSet<_> = ordered_rows
        .iter()
        .zip(&ids)
        .filter_map(|(row, id)| (source(row).unwrap() == deleted_source).then_some(*id))
        .collect();
    for id in &old_ids {
        if !notes.delete_vector(*id)? {
            return Err("note deletion lost an ID".into());
        }
    }
    let after_delete = notes
        .query(&rows[0].vector, rows.len())
        .ef_search(rows.len())
        .execute()?;
    if after_delete.len() != rows.len() - old_ids.len()
        || after_delete.iter().any(|hit| old_ids.contains(&hit.id))
    {
        return Err("deleted note remains visible or unrelated rows were lost".into());
    }
    let replacement_ids = insert(&notes, &note_rows, batch_size)?;
    let after_reindex = notes
        .query(&rows[0].vector, rows.len())
        .ef_search(rows.len())
        .execute()?;
    if after_reindex.len() != rows.len()
        || replacement_ids
            .iter()
            .any(|id| !after_reindex.iter().any(|hit| hit.id == *id))
        || after_reindex.iter().any(|hit| old_ids.contains(&hit.id))
    {
        return Err("note reindexing returned stale or incomplete IDs".into());
    }
    drop(notes);
    drop(db);
    let db = VectorDb::open(Path::new(db_path))?;
    let notes = db.collection("notes")?;
    let persisted = notes
        .query(&rows[0].vector, rows.len())
        .ef_search(rows.len())
        .execute()?;
    if persisted != after_reindex {
        return Err("reopen changed persisted query results".into());
    }
    let ratio = |value: f64| (labeled > 0).then(|| value / labeled as f64);
    Ok(
        json!({"model":model,"documents":sources.len(),"chunks":rows.len(),"dimensions":dim,
        "queries":queries.len(),"labeled_queries":labeled,"batch_size":batch_size,
        "initial_chunks":initial.len(),"incremental_chunks":added.len(),
        "initial_ingest_seconds":initial_ingest_seconds,"initial_build_and_query_ms":initial_build_ms,
        "incremental_ingest_ms":incremental_ingest_ms,"reopen_incremental_refresh_and_query_ms":reopen_incremental_refresh_ms,
        "ann_recall_at_10":(reference > 0).then(|| recalled as f64 / reference as f64),
        "semantic_document_hit_rate_at_5":ratio(semantic_hits as f64),"semantic_document_mrr_at_5":ratio(semantic_mrr),
        "exact_document_hit_rate_at_5":ratio(exact_hits as f64),"exact_document_mrr_at_5":ratio(exact_mrr),
        "bm25_document_hit_rate_at_5":ratio(keyword_hits as f64),"bm25_document_mrr_at_5":ratio(keyword_mrr),
        "warm_ann_p50_ms":percentile(&mut ann_times,0.5),"warm_ann_p95_ms":percentile(&mut ann_times,0.95),
        "warm_exact_p50_ms":percentile(&mut exact_times,0.5),"warm_exact_p95_ms":percentile(&mut exact_times,0.95),
        "note_delete_reindex_reopen_verified":true,"query_results":query_reports}),
    )
}
fn main() -> AnyResult<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("evaluate") if (4..=5).contains(&args.len()) => evaluate(&args[1], &args[2], &args[3], args.get(4).map(|s| s.parse()).transpose()?.unwrap_or(64))?,
        Some("search") if args.len() == 3 => {
            let db = VectorDb::open(Path::new(&args[1]))?;
            let notes = db.collection("notes")?;
            let queries: Vec<Query> = read_jsonl(&args[2])?;
            verify_model(&notes, &queries)?;
            let reports: AnyResult<Vec<_>> = queries.iter().map(|query| {
                let start = Instant::now();
                let hits = search(&notes, query, false)?;
                Ok(json!({"text":query.text,"query_ms":start.elapsed().as_secs_f64()*1000.,"top_documents":documents(&hits)}))
            }).collect();
            json!({"queries":reports?})
        }
        _ => return Err("usage: semantic_search evaluate CORPUS.jsonl QUERIES.jsonl NEW_DB [BATCH_SIZE] | search DB QUERIES.jsonl".into()),
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frozen_real_embeddings_support_search_and_note_lifecycle() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/semantic-search");
        let directory = tempfile::tempdir().unwrap();
        let report = evaluate(
            fixture.join("corpus.jsonl").to_str().unwrap(),
            fixture.join("queries.jsonl").to_str().unwrap(),
            directory.path().join("db").to_str().unwrap(),
            3,
        )
        .unwrap();
        assert_eq!(report["dimensions"], 384);
        assert_eq!(report["documents"], 8);
        assert_eq!(report["ann_recall_at_10"], 1.0);
        assert_eq!(report["note_delete_reindex_reopen_verified"], true);
        assert!(report["semantic_document_mrr_at_5"].as_f64().unwrap() >= 0.8);
        assert_eq!(
            report["query_results"][0]["top_documents"][0]["source"],
            "fixture/recovery.md"
        );
        // The evaluation must refuse to overwrite an existing application index.
        assert!(evaluate("missing", "missing", directory.path().to_str().unwrap(), 1).is_err());
    }

    #[test]
    fn search_rejects_a_query_from_another_embedding_model() {
        let directory = tempfile::tempdir().unwrap();
        let db = VectorDb::open(directory.path()).unwrap();
        let notes = db.create_collection("notes", 2, Metric::Cosine).unwrap();
        notes
            .upsert_vector(&[1., 0.], json!({"model":"model-a"}))
            .unwrap();
        let query = Query {
            text: "query".into(),
            vector: vec![1., 0.],
            model: "model-b".into(),
            relevant_paths: Vec::new(),
            keyword_top_sources: Vec::new(),
            filter_tag: None,
        };
        assert!(verify_model(&notes, &[query]).is_err());
    }
}
