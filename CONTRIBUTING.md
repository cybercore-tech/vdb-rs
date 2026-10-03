# Contributing

Rust 1.88 or newer and a C compiler are required (LMDB is compiled from source).
Linux/macOS on local filesystems are the supported alpha targets. Python 3 is
needed for the standard-library helper checks in the full gates. Local embedding
preparation uses Python 3.11+ and the optional pinned requirements in
`scripts/semantic-search-requirements.txt`; model downloads are not needed in CI.

Run `./scripts/release-gates full` for development and `./scripts/release-gates release` before a release.
The latter requires a clean committed tree and performs a publish dry-run.
`just` exposes project recipes; your shared `rustdev` check/test/lint/doc-tests tasks also work. For focused development run
`./scripts/validate.sh` and `./scripts/validate-features.sh`. Add regressions for
observable correctness changes, especially collection isolation, transaction
boundaries, recovery, incremental index refresh, compaction and corrupted file input.
Run `cargo test --locked --all-features --example semantic_search` and
`python3 -m unittest discover -s scripts/tests` for the offline semantic-search
checks. Keep private note text, embeddings, databases and detailed reports in the
ignored `dist/` directory; public fixtures must contain only public material.
See [the semantic-search guide](docs/semantic-search.md) for preparation, evaluation,
readable output and model provenance. Document format
changes and compatibility requirements in the PR. Keep Cargo.lock committed.

The existing `scripts/monitor-gh-run.sh RUN_ID` and `scripts/show-gh-failure.sh
RUN_ID` help inspect CI. PRs target `main`; CI also runs on other pushed branches.

For a release, update the Cargo version, lockfile and CHANGELOG, run the gates on
Rust 1.88, then push a matching `vVERSION` tag. The release workflow verifies the
version, packages the crate, signs a SHA256 manifest with GitHub OIDC and creates
a draft release. Review that draft before publishing. There is no automatic
crates.io upload; establish name availability/ownership separately.
