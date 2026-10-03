#!/usr/bin/env python3
"""Embed an explicitly selected Markdown corpus locally; write private JSONL inputs."""
import argparse
from collections import Counter
import hashlib
import importlib.metadata
import json
import math
import os
from pathlib import Path
import re
import time

# Model downloads are permitted; text is never sent to a remote inference API.
os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")
MODEL = "BAAI/bge-small-en-v1.5"
SKIP = {"target", "dist", "node_modules", "vendor", "Secure"}


def markdown_paths(root):
    if root.is_file():
        if root.suffix.lower() == ".md":
            yield root
        return
    for directory, names, files in os.walk(root, followlinks=False):
        names[:] = sorted(n for n in names if not n.startswith(".") and n not in SKIP)
        for name in sorted(files):
            path = Path(directory) / name
            if not name.startswith(".") and path.suffix.lower() == ".md" and not path.is_symlink():
                yield path


def chunks(text, tokenizer, size, overlap):
    """Use token offsets to preserve original Markdown and never silently truncate."""
    encoding = tokenizer.encode(text, add_special_tokens=False)
    for start in range(0, len(encoding.ids), size - overlap):
        end = min(start + size, len(encoding.ids))
        begin_char = encoding.offsets[start][0]
        end_char = encoding.offsets[end - 1][1]
        yield text[begin_char:end_char]
        if end == len(encoding.ids):
            break


def keyword_sources(rows, query, tag):
    """Small BM25 baseline (k1=1.2, b=.75), no stemming or learned model."""
    tokens = lambda s: re.findall(r"[a-z0-9]+", s.lower())
    eligible = [r for r in rows if tag is None or tag in r["metadata"]["tags"]]
    counts = [Counter(tokens(r["metadata"]["text"])) for r in eligible]
    if not counts:
        return []
    df = Counter(word for count in counts for word in count)
    average = sum(sum(count.values()) for count in counts) / len(counts)
    scored = []
    for row, count in zip(eligible, counts):
        length = sum(count.values())
        score = 0.0
        for word in set(tokens(query)):
            frequency = count[word]
            if frequency:
                idf = math.log(1 + (len(counts) - df[word] + .5) / (df[word] + .5))
                score += idf * frequency * 2.2 / (frequency + 1.2 * (.25 + .75 * length / average))
        scored.append((score, row["metadata"]["source"]))
    sources = []
    for score, source in sorted(scored, key=lambda pair: (-pair[0], pair[1])):
        if score > 0 and source not in sources:
            sources.append(source)
    return sources[:5]


def write_jsonl(path, records):
    with path.open("w", encoding="utf-8") as output:
        for record in records:
            output.write(json.dumps(record, ensure_ascii=False, allow_nan=False) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", action="append", default=[], metavar="LABEL=PATH")
    parser.add_argument("--queries", type=Path, help="JSON list of text/relevant_paths/filter_tag objects")
    parser.add_argument("--query", action="append", default=[], help="Additional unlabeled search query")
    parser.add_argument("--output", type=Path, default=Path("dist/semantic-search"))
    parser.add_argument("--cache", type=Path, default=Path.home() / ".cache/vdb-rs/embedding-models")
    parser.add_argument("--chunk-tokens", type=int, default=320)
    parser.add_argument("--overlap-tokens", type=int, default=48)
    parser.add_argument("--threads", type=int, default=4)
    parser.add_argument("--queries-only", action="store_true")
    parser.add_argument("--offline", action="store_true", help="Require cached model files; make no network requests")
    args = parser.parse_args()
    if not 0 <= args.overlap_tokens < args.chunk_tokens <= 384 or args.threads < 1:
        parser.error("require 0 <= overlap < chunk size <= 384 and positive threads")
    if not args.queries and not args.query:
        parser.error("provide --queries or --query")
    roots = []
    for item in args.root:
        label, separator, name = item.partition("=")
        path = Path(name).expanduser().resolve()
        if not separator or not re.fullmatch(r"[a-zA-Z0-9_-]+", label) or not path.exists():
            parser.error(f"invalid corpus root: {item}")
        roots.append((label, path))
    if len({label for label, _ in roots}) != len(roots):
        parser.error("root labels must be unique")
    if not roots and not args.queries_only:
        parser.error("provide at least one explicitly selected --root")
    queries = json.loads(args.queries.read_text()) if args.queries else []
    queries += [{"text": query, "relevant_paths": []} for query in args.query]
    if not queries or any(not isinstance(q.get("text"), str) or not q["text"].strip() for q in queries):
        parser.error("queries must be a nonempty list with nonempty text")

    from fastembed import TextEmbedding
    from tokenizers import Tokenizer
    started = time.perf_counter()
    model = TextEmbedding(MODEL, cache_dir=str(args.cache), threads=args.threads, local_files_only=args.offline)
    # FastEmbed 0.8.1 exposes the underlying tokenizer; clone so chunking does
    # not change inference padding/truncation settings.
    tokenizer = Tokenizer.from_str(model.model.tokenizer.to_str())
    tokenizer.no_truncation()
    tokenizer.no_padding()
    args.output.mkdir(parents=True, exist_ok=True)
    if args.queries_only:
        manifest = json.loads((args.output / "manifest.json").read_text())
        if manifest["model"] != MODEL:
            parser.error("cached corpus uses another embedding model")
        rows = [json.loads(line) for line in (args.output / "corpus.jsonl").read_text().splitlines()]
    else:
        rows = []
        files = []
        for label, root in roots:
            for path in markdown_paths(root):
                # Exclude symlinked ancestors escaping the explicitly chosen root.
                if root.is_dir() and not path.resolve().is_relative_to(root):
                    continue
                text = path.read_text(encoding="utf-8")
                relative = path.relative_to(root).as_posix() if root.is_dir() else path.name
                source = f"{label}/{relative}"
                heading = re.search(r"^# (.+)$", text, re.MULTILINE)
                title = heading.group(1) if heading else path.stem
                title = next(chunks(title, tokenizer, 48, 0), path.stem)
                passages = list(chunks(text, tokenizer, args.chunk_tokens, args.overlap_tokens))
                if passages:
                    files.append({"source": source, "sha256": hashlib.sha256(text.encode()).hexdigest()})
                for number, chunk in enumerate(passages):
                    passage = f"{title}\n\n{chunk}"
                    if len(tokenizer.encode(passage).ids) > 512:
                        raise ValueError(f"chunk exceeds model context: {source}/{number}")
                    rows.append({"metadata": {"source": source, "title": title, "chunk": number,
                                              "tags": [label], "text": passage, "model": MODEL}})
        if not rows:
            parser.error("selected roots contain no nonempty Markdown documents")
        embedding_started = time.perf_counter()
        vectors = model.passage_embed((row["metadata"]["text"] for row in rows), batch_size=32)
        for row, vector in zip(rows, vectors, strict=True):
            row["vector"] = vector.tolist()
        document_embedding_seconds = time.perf_counter() - embedding_started
        write_jsonl(args.output / "corpus.jsonl", rows)
        manifest = {"model": MODEL, "fastembed": importlib.metadata.version("fastembed"),
                    "dimensions": len(rows[0]["vector"]), "documents": len(files), "chunks": len(rows),
                    "chunk_tokens": args.chunk_tokens, "overlap_tokens": args.overlap_tokens,
                    "document_embedding_seconds": document_embedding_seconds,
                    "files": files}
        # Pin the downloaded artifact identity in the local manifest.
        artifacts = []
        for path in sorted(args.cache.rglob("*.onnx")):
            with path.open("rb") as stream:
                digest = hashlib.file_digest(stream, "sha256").hexdigest()
            artifacts.append({"file": str(path.relative_to(args.cache)), "sha256": digest})
        manifest["model_artifacts"] = artifacts
        (args.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    known = {row["metadata"]["source"] for row in rows}
    for query in queries:
        missing = set(query.get("relevant_paths", [])) - known
        if missing:
            parser.error(f"query relevance labels missing from corpus: {sorted(missing)}")
    query_started = time.perf_counter()
    query_vectors = list(model.query_embed([q["text"] for q in queries], batch_size=32))
    query_embedding_seconds = time.perf_counter() - query_started
    for query, vector in zip(queries, query_vectors, strict=True):
        query["vector"] = vector.tolist()
        query["model"] = MODEL
        query["keyword_top_sources"] = keyword_sources(rows, query["text"], query.get("filter_tag"))
    query_file = args.output / ("search-queries.jsonl" if args.queries_only else "queries.jsonl")
    write_jsonl(query_file, queries)
    print(json.dumps({"model": MODEL, "documents": manifest["documents"], "chunks": len(rows),
                      "queries": len(queries), "query_embedding_seconds": query_embedding_seconds,
                      "total_seconds": time.perf_counter() - started, "output": str(args.output), "query_file": str(query_file)}, indent=2))


if __name__ == "__main__":
    main()
