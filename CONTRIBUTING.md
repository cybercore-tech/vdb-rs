# Contributing

Rust 1.88 or newer and a C compiler are required (LMDB is compiled from source).
Linux/macOS on local filesystems are the supported alpha targets.

Run `./scripts/release-gates full` for development and `./scripts/release-gates release` before a release.
The latter requires a clean committed tree and performs a publish dry-run.
`just` exposes project recipes; your shared `rustdev` check/test/lint/doc-tests tasks also work. For focused development run
`./scripts/validate.sh` and `./scripts/validate-features.sh`. Add regressions for
observable correctness changes, especially collection isolation, transaction
boundaries, recovery, index invalidation and corrupted file input. Document format
changes and compatibility requirements in the PR. Keep Cargo.lock committed.

The existing `scripts/monitor-gh-run.sh RUN_ID` and `scripts/show-gh-failure.sh
RUN_ID` help inspect CI. PRs target `main`; CI also runs on other pushed branches.

For a release, update the Cargo version, lockfile and CHANGELOG, run the gates on
Rust 1.88, then push a matching `vVERSION` tag. The release workflow verifies the
version, packages the crate, signs a SHA256 manifest with GitHub OIDC and creates
a draft release. Review that draft before publishing. There is no automatic
crates.io upload; establish name availability/ownership separately.
