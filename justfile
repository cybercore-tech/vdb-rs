# Project gates, compatible with the shared rustdev tasks.
default:
    @just --list

check:
    cargo check --locked --all-targets --all-features

lint:
    cargo clippy --locked --all-targets --all-features -- -D warnings

fmt-check:
    cargo fmt --all --check

test:
    cargo test --locked --all-features --lib --tests

doc-tests:
    cargo test --locked --all-features --doc

validate:
    ./scripts/validate.sh
    ./scripts/validate-features.sh

example:
    cargo run --locked --example basic

benchmark n="1000" dim="32" queries="50" batch_size="128":
    cargo run --locked --release --example benchmark -- {{n}} {{dim}} {{queries}} {{batch_size}}

semantic-eval corpus_dir="dist/semantic-search" db_path="dist/semantic-search/db":
    cargo run --locked --release --example semantic_search -- evaluate {{corpus_dir}}/corpus.jsonl {{corpus_dir}}/queries.jsonl {{db_path}}

semantic-search db_path="dist/semantic-search/db" queries="dist/semantic-search/search-queries.jsonl":
    cargo run --locked --release --example semantic_search -- search {{db_path}} {{queries}}

release-gates mode="full":
    ./scripts/release-gates {{mode}}
