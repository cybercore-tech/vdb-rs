# AGENTS.md — vdb-rs Operating Contract

## Purpose

This file defines how coding agents must operate in the `vdb-rs` repository.

Primary rule:

> Preserve what works. Understand before changing. Make the smallest coherent change that fully solves the task, then prove it with the repository's validation gates.

This repository should always become easier to understand and continue after each agent session.

---

## Source of truth

When information conflicts, use this order:

1. Explicit current task instructions
2. `AGENTS.md`
3. `PROJECT_SPEC.md`
4. `PROJECT_STATE.md`
5. `AGENT_HANDOFF.md`
6. ADRs under `docs/adr/`
7. Passing tests
8. Public API contracts
9. User-facing docs (`README.md`, `docs/`)
10. Existing implementation
11. Recent Git history
12. Agent assumptions

Never silently override a higher-priority source with a lower-priority one.

---

## Required startup procedure

```bash
pwd
git status --short
git branch --show-current
git log --oneline -12
rustc --version
cargo --version
```

Then read, when present:

```text
AGENTS.md
PROJECT_SPEC.md
PROJECT_STATE.md
AGENT_HANDOFF.md
README.md
CHANGELOG.md
Cargo.toml
Cargo.lock
docs/
```

---

## Repository orientation

Identify:

- workspace members (`vdb`)
- public API surfaces (`VectorDb`, `Collection`, `QueryBuilder`, result types)
- feature flags (`storage`, `index-hnsw`, `metrics`, `serde-query` — current, isolation-tested; `quantization`, `index-ivf`, `async` — later-phase, not yet isolation-tested; see PROJECT_SPEC.md's Feature Matrix)
- LMDB table schemas (`src/storage/mod.rs`)
- `.vectors` and `.index` file layouts (`src/vector/layout.rs`, `src/index/file.rs`)
- integration tests (`tests/`)
- ADRs (`docs/adr/`)

---

## Rust project principles

Favor:

1. Explicit ownership and lifetimes.
2. Small, stable public APIs.
3. Strong typing (`CollectionId`, `VectorId`, `Metric`).
4. Useful error context.
5. Deterministic storage layout.
6. Feature isolation.
7. Backwards-compatible persistence where required.
8. Tests at public boundaries.
9. Documentation that compiles.
10. Clean CI and package output.

---

## Work classification

```text
BUG FIX
FEATURE
REFACTOR
TESTING
DOCS
CI
BUILD
PACKAGING
RELEASE
SECURITY
PERFORMANCE
MIGRATION
SERDE/PERSISTENCE
```

---

## Task normalization

```text
TASK:
GOAL:
REQUIREMENTS:
CONSTRAINTS:
DO NOT CHANGE:
ACCEPTANCE CRITERIA:
FOCUSED TESTS:
FULL VALIDATION:
```

---

## Scope discipline

Do not mix unrelated work into the current milestone.

Avoid: dependency upgrades, broad renames, formatting unrelated files,
reorganizing crates, renaming public APIs during refactors, changing wire
formats casually, weakening lints/tests/CI.

Prefer one coherent, reviewable change.

---

## Dirty worktree safety

Always start with `git status --short`. Do not reset/destroy unrelated user
changes. Do not run `git clean -fdx` or `git reset --hard` without explicit
authorization.

---

## Public API policy

Treat `pub` types, functions, traits, error enums, and feature-gated APIs as
compatibility-sensitive. Prefer additive APIs. Update `CHANGELOG.md`.

Protected surfaces for vdb-rs:

- `VectorDb`, `Collection`, `QueryBuilder`
- `CollectionConfig`, `IndexConfig`, `Metric`
- On-disk `.vectors` / `.index` formats

---

## SemVer discipline

Pre-1.0: minor bumps for feature additions, patch for fixes. No breaking changes
without ADR + changelog.

---

## Error handling

Use `thiserror`. Preserve cause chain + context. No generic errors.

---

## Panics

Library code must not panic on invalid user input. Use `expect` only for
internal invariants with explanatory messages.

---

## `unsafe`

Treat as security-sensitive. Document invariants. Minimize scope. Add tests.

> Note: `memmap2` handles mmap internally; our code should avoid `unsafe`
> unless absolutely necessary for index traversal performance.

---

## Serialization and persistence

- `.vectors` and `.index` formats are versioned with magic bytes.
- LMDB table schemas must be documented in ADRs.
- Never silently break on-disk formats.
- Add compatibility tests for persisted structures.

---

## Feature flags

Feature isolation is first-class. Validate:

```bash
cargo check --workspace --no-default-features
cargo test  --workspace --no-default-features
```

Then critical features independently.

---

## Testing rules

Every behavioral change gets coverage:

```text
unit
integration
regression
serde round-trip
compatibility
feature isolation
compile tests (doctests)
```

Bug fix: reproduce → add regression test → fix → verify.

Do not weaken assertions to make tests pass.

---

## Documentation rules

Public APIs documented. Examples compile. Rustdoc kept `-D warnings` clean.

Fix broken intra-doc links, don't suppress them.

---

## Standard full validation gate

```bash
cargo fmt --all --check

git diff --check

cargo check \
  --workspace \
  --all-targets \
  --all-features

cargo clippy \
  --workspace \
  --all-targets \
  --all-features \
  -- \
  -D warnings

cargo test \
  --workspace \
  --all-features \
  --lib \
  --tests

# Doc tests must pass explicitly:
cargo test --doc --workspace --all-features

RUSTDOCFLAGS="-D warnings" \
cargo doc \
  --workspace \
  --no-deps \
  --all-features
```

---

## Feature-isolation gate

```bash
cargo check --workspace --no-default-features
cargo test  --workspace --no-default-features
```

Record project-critical combinations in `PROJECT_SPEC.md`.

---

## Package validation gate

```bash
cargo package -p vdb --list
cargo package -p vdb
```

Do not use `--allow-dirty` as the normal fix.

---

## Cargo.lock policy

Treat as intentional state. Include only if tracked. Do not ignore package
errors from uncommitted lockfile.

---

## CI workflow handling

Inspect `.github/workflows/ci.yml`. Reproduce failures locally. Fix root cause.

---

## Baseline before changing code

Establish baseline with the full validation gate. Record pre-existing failures.

---

## Validation reporting

Report concrete checks:

```text
PASS cargo fmt --all --check
PASS git diff --check
PASS cargo check --workspace --all-targets --all-features
PASS cargo clippy --workspace --all-targets --all-features -- -D warnings
PASS cargo test --workspace --all-features --lib --tests
PASS cargo test --doc --workspace --all-features
PASS RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features
PASS feature-isolation checks
PASS package validation
```

---

## Failure reporting

```text
FAILED COMMAND:
ERROR:
ROOT CAUSE:
AFFECTED FILES:
NEXT FIX:
```

---

## Commit workflow

```bash
git status --short
git diff --check
git diff --stat
git diff
```

One logical commit per milestone. Verify `git log -1 --stat` after.

---

## Agent autonomy

**May:** fix task-caused compile errors, add necessary/regression tests, update
directly affected docs, small local refactors, fix lint/format.

**Should not:** redesign workspace, switch storage substrate, replace serde
formats, rename major public APIs, remove features, alter compat guarantees,
weaken CI/security validation, change release policy.

---

## Recovery after lost context

```bash
git status --short
git branch --show-current
git log --oneline -15
git diff
git diff --cached
```

Then read `PROJECT_STATE.md`, `AGENT_HANDOFF.md`. Continue from last green
milestone.

---

## Final checklist

```text
[ ] Task requirements met
[ ] Working behavior preserved
[ ] Public API impact reviewed
[ ] Persistence impact reviewed
[ ] Tests added/updated
[ ] Regression test added when appropriate
[ ] cargo fmt passes
[ ] git diff --check passes
[ ] cargo check passes
[ ] cargo clippy -D warnings passes
[ ] cargo test passes
[ ] rustdoc -D warnings passes
[ ] Feature isolation checked
[ ] Package validation checked when applicable
[ ] Docs updated
[ ] Changelog updated when applicable
[ ] Diff reviewed
[ ] Commit is detailed
[ ] Worktree status checked
[ ] PROJECT_STATE.md updated
[ ] AGENT_HANDOFF.md updated
```

---

## Final rule

Do not optimize for appearing finished. Optimize for leaving the repository
correct, green, documented, packageable, reproducible, and easy for the next
agent to continue.
