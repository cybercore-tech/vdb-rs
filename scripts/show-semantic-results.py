#!/usr/bin/env python3
"""Display local semantic-search results and evaluation reports as readable text."""
import argparse
import json
from pathlib import Path
import sys


def search_text(result):
    lines = []
    for query in result.get("queries", []):
        lines.append(f"Query: {query['text']}")
        lines.append(f"Database search: {query['query_ms']:.2f} ms")
        hits = query.get("top_documents", [])
        if not hits:
            lines.append("No matching documents.")
        for rank, hit in enumerate(hits, 1):
            lines.append(f"\n{rank}. {hit.get('title') or hit['source']}")
            lines.append(f"   Source: {hit['source']}")
            lines.append(f"   Cosine distance: {hit['score']:.4f}")
            snippet = " ".join(hit.get("snippet", "").split())
            if snippet:
                lines.append(f"   {snippet}")
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def report_text(report):
    recall = report.get("ann_recall_at_10")
    recall_text = "not measured" if recall is None else f"{recall:.1%}"
    lines = ["# Local semantic-search evaluation", "",
             f"Corpus: {report['documents']} documents, {report['chunks']} chunks, {report['dimensions']} dimensions.",
             f"Model: {report['model']}. Questions: {report['queries']} ({report['labeled_queries']} labeled).", "",
             f"- Sampled ANN recall@10 against exact vectors: {recall_text}.",
             f"- Warm database search p50 / p95: {report['warm_ann_p50_ms']:.3f} / {report['warm_ann_p95_ms']:.3f} ms (embedding excluded).",
             f"- Whole-note delete/reindex/reopen: {'passed' if report['note_delete_reindex_reopen_verified'] else 'failed'}.", "",
             "| Ranking | Known-answer hit rate@5 | MRR@5 |", "|---|---:|---:|"]
    for title, prefix in [("Semantic", "semantic"), ("Exact vectors", "exact"), ("BM25 keywords", "bm25")]:
        rate = report.get(f"{prefix}_document_hit_rate_at_5")
        mrr = report.get(f"{prefix}_document_mrr_at_5")
        lines.append(f"| {title} | {'not measured' if rate is None else f'{rate:.1%}'} | {'not measured' if mrr is None else f'{mrr:.3f}'} |")
    lines.extend(["", "These are a small local pilot with manually selected expected paths, not production accuracy guarantees.", "", "## Questions and retrieved documents", ""])
    for number, query in enumerate(report.get("query_results", []), 1):
        lines.extend([f"### {number}. {query['text']}", ""])
        if query.get("filter_tag"):
            lines.append(f"Project filter: {query['filter_tag']}.")
        for title, key in [("Semantic", "semantic_rank_at_5"), ("Exact vectors", "exact_rank_at_5"), ("BM25", "keyword_rank_at_5")]:
            rank = query.get(key)
            lines.append(f"- {title} expected-answer rank: {rank if rank is not None else 'outside top five / unlabeled'}.")
        if query.get("relevant_paths"):
            lines.append("- Expected sources: " + ", ".join(query["relevant_paths"]) + ".")
        lines.append("")
        for rank, hit in enumerate(query.get("top_documents", []), 1):
            lines.append(f"{rank}. {hit['source']} — cosine distance {hit['score']:.4f}")
            snippet = " ".join(hit.get("snippet", "").split())
            if snippet:
                lines.extend(["", f"   {snippet}", ""])
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["search", "report"])
    parser.add_argument("path", nargs="?", type=Path, help="JSON file; defaults to stdin")
    parser.add_argument("--markdown", nargs="?", const=Path("-"), type=Path, help="Save a Markdown copy; default is beside the input JSON")
    args = parser.parse_args()
    try:
        result = json.loads(args.path.read_text()) if args.path else json.load(sys.stdin)
        text = search_text(result) if args.mode == "search" else report_text(result)
        if args.markdown:
            output = args.markdown
            if output == Path("-"):
                if args.path is None:
                    raise ValueError("provide an input file or explicit Markdown output path")
                output = args.path.with_suffix(".md")
            if args.path and output.resolve() == args.path.resolve():
                raise ValueError("Markdown output must differ from the input JSON file")
            output.write_text(text, encoding="utf-8")
        print(text, end="")
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.exit(1, f"Cannot display semantic results: {error}\n")


if __name__ == "__main__":
    main()
