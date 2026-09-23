# AGENTS.md

## Project

Rust CLI (`rep`, package `agent-repertoire`): reusable tools for AI agents over MCP + CLI, backed by SQLite/FTS5.

## Validation

- `cargo test` — the full suite must pass before merging.
- `cargo build --release --locked` — CI releases build locked; keep `Cargo.lock` in sync when editing `Cargo.toml` (after a version bump: `cargo update -p agent-repertoire`).

## Release

Releases run via the manual `Release` workflow (`.github/workflows/release.yml`), never by hand-tagging:

1. Bump `version` in `Cargo.toml` + `cargo update -p agent-repertoire`; commit both.
2. Write release notes under `## [Unreleased]` in `CHANGELOG.md` (the workflow promotes them into `## [<version>]` and auto-commits).
3. Push, then run Actions → Release → Run workflow.

The workflow builds 5 platform binaries, publishes them as GitHub Release assets with `SHA256SUMS` + provenance attestations, and is safe to re-run for the same version.
