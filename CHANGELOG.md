# Changelog

All notable changes to SQLRite are documented here.

The changelog is intentionally product-facing. Internal sprint reports, benchmark artifacts, and release-train evidence remain in the repository for maintainers, but they are not duplicated here.

## Unreleased

### Changed

- Repositioned SQLRite around its primary embedded use case.
- Consolidated the public documentation into a single `docs/` tree.
- Streamlined `README.md` so install, embedded usage, querying, SQL, server mode, security, and distribution are current and easier to follow.
- Removed duplicate documentation trees and old public-facing project-history references.
- Updated example defaults so security rotation fixtures write to neutral local paths instead of internal report folders.

### Performance

- Improved embedded filtered retrieval performance substantially through compact numeric filter paths, sidecar-backed vector persistence, and lower-overhead compact HTTP responses.
- Current benchmark snapshot is documented in `/Users/jameskaranja/Developer/projects/SQLRight/docs/performance.md`.

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
