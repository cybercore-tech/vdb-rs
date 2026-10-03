# Semantic search over local technical notes

This is a working standalone vdb-rs evaluation and search example. It reads an
explicitly selected Markdown corpus, creates real embeddings locally, persists
chunks with source/title/tag/snippet metadata, and returns ranked documents.
It does not modify original notes or install search into Cyberdesk.

Cyberdesk currently uses substring matching over note titles, bodies and tags.
A semantic index can run beside its filesystem/git model: Markdown stays the
source of truth and vdb-rs becomes a derived search index.

## Local pilot results

Measured on 2026-10-02, Linux, Rust 1.98.1 release build. The corpus contained
**122 nonempty documents and 426 chunks**: eight local virtualization/setup notes
plus documentation from Cyberdesk, vdb-rs, Gateflow and Cyberdeck Hub. The local
Darknotes directory was empty. One empty Markdown file was excluded from the
document count. Private text, corpus embeddings and detailed result snippets
remain under ignored dist/semantic-search; they are not distributed in this repo.

Embedding model: BAAI/bge-small-en-v1.5, FastEmbed 0.8.1, CPU inference, 384-dimension
f32 vectors. Chunking uses tokenizer offsets, a 320-token window and 48-token
overlap, preserving original Markdown. Titles are prefixed; passages exceeding
the 512-token context are rejected. Root labels become filterable project tags.
The local manifest records source hashes and the downloaded ONNX artifact hash.

Eleven questions and their known relevant document paths were chosen before
searching, including guest installation, missing VM listings, workload budgets,
browser note editing, interrupted writes, incremental indexing, compaction,
directory locking and network namespaces. One question was project-filtered.
These path labels are a small, manually selected set of known answers, not
exhaustive relevance judgments or a held-out benchmark dataset.

| Measurement | Result |
|---|---:|
| Initial batch ingestion: 324 chunks, batch size 64 | 89.8 ms |
| Initial HNSW build + first query | 2,996.9 ms |
| Incremental batch ingestion: 102 chunks | 37.6 ms |
| Reopen with old graph, incremental refresh + query | 1,124.1 ms |
| Warm ANN p50 / p95, 33 query samples | 1.373 / 1.510 ms |
| Warm exact p50 / p95 | 2.477 / 2.969 ms |
| ANN chunk recall@10 against exact vectors, unfiltered questions | 100% |
| Semantic known-answer document hit rate@5 | 10 / 11 (90.9%) |
| Exact-vector known-answer document hit rate@5 | 10 / 11 (90.9%) |
| BM25 known-answer document hit rate@5 | 10 / 11 (90.9%) |
| Semantic / exact-vector MRR@5 | 0.568 / 0.568 |
| BM25 MRR@5 | 0.720 |
| Whole-note delete, reindex and persisted reopen | Passed |

Recall compares the first ten chunk IDs with exact cosine results. Document
ranking deduplicates source paths within a 32-chunk candidate window and displays
five documents. Hit rate asks whether a known relevant source appears; MRR is the
reciprocal of its first document rank, with zero for misses. BM25 uses k1=1.2,
b=.75, lowercase alphanumeric tokens, no stemming, and the maximum chunk score
per source. Project-filtered queries use exact search and are excluded from ANN
recall. They are included in relevance and timing measurements.

The index preserved the sampled exact-vector neighbors. Embeddings did not rank
known answers as highly as BM25 overall, and both methods missed one labeled
question. For example, the virtualization questions found relevant setup notes,
but the question about a second process opening a database retrieved namespace
and directory-related distractors. This points to embedding/ranking quality and
relevance judgments, rather than HNSW losing the exact neighbors. Keep keyword
search for exact identifiers and evaluate a hybrid ranking before replacing
Cyberdesk's current search. Do not tune against this small question set and claim
that the resulting score generalizes.

Timings exclude model inference and Markdown preparation. Preparing the original
corpus took about 41.6 seconds including model setup, embedding and output writes.
A separate single-question run measured 7.66 ms of embedding inference, 0.353
seconds for the preparation process including cold model setup and BM25, then
2.72 ms for the database query. Those separate measurements are not an end-to-end
service latency promise. Startup/refresh still copies graph vectors and rewrites
a full snapshot. Local validation overlapped parts of these observations.

## Prepare your own corpus

The embedding dependency belongs to the example tooling, not the Rust library.
[FastEmbed documentation](https://qdrant.github.io/fastembed/Getting%20Started/)
describes local ONNX inference; its [supported models](https://qdrant.github.io/fastembed/examples/Supported_Models/)
list the selected model. Initial setup downloads packages/model files. Document
text is never sent to a remote embedding API; use --offline after caching weights.

```sh
python3 -m venv /tmp/vdb-semantic-venv
/tmp/vdb-semantic-venv/bin/python -m pip install -r scripts/semantic-search-requirements.txt
```

Choose specific directories or Markdown files. The scanner skips hidden paths,
symlinked files/directories, Secure, dist, target, node_modules and vendor folders.
It reads UTF-8 Markdown. Choose roots that contain only the notes you want indexed;
the scanner is not a secret detector. Root labels are unique project/tag names.

```sh
/tmp/vdb-semantic-venv/bin/python scripts/prepare-semantic-search.py \
  --root notes=/path/to/technical-notes \
  --root vdb=docs \
  --queries /path/to/questions.json \
  --output dist/my-notes
```

Example questions.json (expected source paths are optional for interactive use):

```json
[
  {
    "text": "What happens if a vector write is interrupted before file synchronization?",
    "relevant_paths": ["vdb/recovery.md"],
    "filter_tag": "vdb"
  }
]
```

The output contains corpus.jsonl, queries.jsonl and manifest.json. Source names
are LABEL/relative/path.md. Keep vectors, reports, source snippets and databases
local. The default dist directory is ignored by git. The helper and CLI load this
pilot corpus into memory; streaming at much larger scales is future work.

## Evaluate and search

Evaluation requires a new database directory and refuses to overwrite one.
It indexes 75% of source documents, publishes a graph, indexes the remainder,
closes/reopens with the older graph and refreshes incrementally. It compares ANN,
exact-vector and BM25 rankings, deletes every chunk of one note, reindexes that
note under fresh IDs and verifies persisted results after another reopen.

```sh
cargo run --locked --release --example semantic_search -- \
  evaluate dist/my-notes/corpus.jsonl dist/my-notes/queries.jsonl \
  dist/my-notes/db 64 > dist/my-notes/report.json
# Same recipe, with default batch size 64:
just semantic-eval dist/my-notes dist/my-notes/another-new-db
```

Embed another question against the cached model and reuse the persisted database:

```sh
/tmp/vdb-semantic-venv/bin/python scripts/prepare-semantic-search.py \
  --offline --queries-only --output dist/my-notes \
  --query "Why does the graphical VM manager disagree with the terminal?"
just semantic-search dist/my-notes/db dist/my-notes/search-queries.jsonl
```

Queries-only mode preserves the original evaluation questions and writes
search-queries.jsonl. The CLI verifies the stored model name against the query
model; callers must also keep model artifact version and preprocessing consistent.
Changing embedding models requires reindexing. Metadata tags currently use root
labels; adapting note frontmatter tags is an application integration step.

## Regression and application integration

CI/MSRV/release gates run the example tests against a small checked-in fixture of
real embeddings from eight public vdb-rs notes. No model download or private corpus
is needed in CI. Standard-library Python tests check corpus selection and the
BM25 baseline. Run them with:

```sh
cargo test --locked --all-features --example semantic_search
python3 -m unittest discover -s scripts/tests
```

A Cyberdesk integration should map each source file to its chunk IDs and content
hash, batch-index new/changed notes, delete obsolete chunks and preserve metadata
links to /n/<path>. Because vdb-rs currently inserts under new IDs, edits require
an application-managed replace cycle. A crash-safe source manifest, stale-version
filtering, background embedding work and reconciliation against the filesystem
are still needed; this example's delete/reinsert exercise is not an atomic
application-level edit. Add keyword/vector fusion, relevance tests with more
questions and real note tags, then expose a semantic search mode alongside the
existing substring search.
