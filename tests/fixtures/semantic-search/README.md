# Offline semantic-search regression fixture

Eight short, public notes describe vdb-rs behavior documented in README.md,
docs/arch.md, docs/recovery.md, and docs/storage_layout.md. The note text is included
in corpus.jsonl metadata. Five natural-language questions carry known relevant
source paths in queries.jsonl. No private notes or machine configuration are included.

Vectors were generated locally with FastEmbed 0.8.1, BAAI/bge-small-en-v1.5,
384 dimensions, using passage_embed for documents and query_embed for questions.
The model artifact identity and content hashes are recorded in manifest.json.
The ONNX weights are not distributed. Public model information:
https://huggingface.co/Qdrant/bge-small-en-v1.5-onnx-Q

CI reads the frozen vectors directly, without Python embedding packages, model
downloads, GPU access, or an external inference service. The example's tests verify
relevance, persistence, incremental insertion, whole-note deletion/reindexing and
model mismatch handling. This tiny fixture is a regression check, not a retrieval
quality benchmark. The larger local-note evaluation is described in
[the run guide](../../../docs/semantic-search.md).

To regenerate: extract each row's text after its first title prefix into the
corresponding fixture Markdown file in a scratch directory, extract text and
relevant_paths from queries.jsonl into a JSON question list, then run
scripts/prepare-semantic-search.py with that directory as --root fixture=PATH.
The first line of each note is its title, so embedding preparation adds the same
prefix again. Record and review any model-artifact or ranking changes before
replacing the checked-in vectors.
