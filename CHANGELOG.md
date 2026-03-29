# Changelog

All notable changes to SQLRite are documented here.

The changelog is intentionally product-facing. Internal sprint reports, benchmark artifacts, and release-train evidence remain in the repository for maintainers, but they are not duplicated here.

## Unreleased

No unreleased changes yet.

## 1.0.2 - 2026-03-29

Patch release focused on embedded integration ergonomics, publication quality, install reliability, and Rust 2024 compatibility.

### Added

- Added `SqlRiteHandle`, a cloneable, `Send` + `Sync` connection-opening handle for concurrent async integration without wrapping `SqlRite` itself in a process-wide mutex.
- Added text-first ingestion APIs:
  - `TextChunkInput`
  - `SqlRite::ingest_text_chunk`
  - `SqlRite::ingest_text_chunks`
  - `SqlRite::ingest_document_text`
  - `SqlRite::update_chunk_embedding`
- Added built-in chunking helpers through `DocumentIngestOptions` and `SqlRite::chunk_text`.
- Added public `document_count`, `delete_by_doc_id`, and `diagnostics` APIs for dashboards and health endpoints.
- Added discoverable search convenience constructors `SearchRequest::text_only` and `SearchRequest::vector_only`.

### Changed

- Repositioned SQLRite around its primary embedded use case.
- Consolidated the public documentation into a single `docs/` tree.
- Streamlined `README.md` so install, embedded usage, querying, SQL, server mode, security, and distribution are current and easier to follow.
- Removed duplicate documentation trees and old public-facing project-history references.
- Updated example defaults so security rotation fixtures write to neutral local paths instead of internal report folders.
- Published crates.io-ready package metadata for `sqlrite` and `sqlrite-sdk-core`.
- Added docs.rs metadata and crate-level documentation for both published crates.
- Added a dedicated `README.md` for `sqlrite-sdk-core`.
- Updated public install and distribution examples to `1.0.2`.
- Vector-index rebuild and load paths now skip text-only chunks with no embedding material instead of treating them as invalid vectors.

### Performance

- Improved embedded filtered retrieval performance substantially through compact numeric filter paths, sidecar-backed vector persistence, and lower-overhead compact HTTP responses.
- Current benchmark snapshot is documented in `/Users/jameskaranja/Developer/projects/SQLRight/docs/performance.md`.

### Fixed

- Vendored `protoc` in the build so CI and docs.rs-style package verification do not rely on a system protobuf installation.
- Simplified CI to one practical quality job with lint, test, CLI smoke, and benchmark smoke coverage.
- Fixed Rust 2024 `unsafe_op_in_unsafe_fn` failures in AVX2 hot paths.
- Removed `ripgrep` as an implicit dependency from install smoke scripts.
- Fixed `sqlrite-global-update.sh` quick mode on macOS Bash when no extra installer arguments are passed.

## 1.0.0 - 2026-03-28

First public production release.

### Added

- Embedded SQLite-based retrieval engine for AI-agent and RAG workloads.
- Hybrid retrieval with text, vector, weighted fusion, and reciprocal-rank fusion.
- SQL-native retrieval operators, helper functions, and `SEARCH(...)` support.
- CLI workflows for init, query, ingest, migrate, backup, compact, benchmark, doctor, and SQL shell usage.
- Optional service surfaces: HTTP, compact HTTP, gRPC, and MCP.
- Migration tooling for SQLite, libSQL, pgvector JSONL, and API-first vector export patterns.
- Security features including RBAC, tenant key registries, audit export, metadata encryption rotation, and secure server defaults.
- Operations tooling for health checks, backups, snapshots, PITR restore, and compaction.
- Packaging flows for Cargo installs, GitHub release archives, and Docker.

### Notes

- Embedded mode is the primary SQLRite deployment model.
- Release installers currently install `sqlrite`; source installs remain the best path when you want the full companion CLI toolchain.
