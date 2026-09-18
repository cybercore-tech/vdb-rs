# Query Pipeline

A query flows through 4 stages:

## 1. Preprocessing

- Normalize query vector (L2 norm for cosine).
- Optional dimensionality reduction / quantization.
- Parse metadata filter expressions.

## 2. Candidate Retrieval

- Traverse the ANN index (HNSW in `.index` file via mmap).
- Collect `ef_search` candidate vector IDs.

## 3. Metadata Filtering

- For each candidate ID, look up metadata in LMDB `metadata` table.
- Apply filter expression (e.g., `tags CONTAINS 'rust'`).
- Drop non-matching candidates.

## 4. Scoring + Reranking

- Compute similarity scores (cosine, L2, dot product).
- Optionally combine with sparse BM25 scores (future).
- Rerank by combined score.
- Return top-k `(id, score, metadata)` results.

## Example

1. User sends query vector + filter (`tags CONTAINS 'rust'`).
2. Preprocess: normalize → unit vector.
3. Traverse HNSW → collect 200 candidates.
4. Filter metadata → 80 remain.
5. Score candidates → rerank by cosine similarity.
6. Return top-10 results with IDs + metadata.
