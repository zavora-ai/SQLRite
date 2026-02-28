# Changelog

All notable changes to SQLRite are documented in this file.

The format is based on Keep a Changelog.

## [Unreleased]

### Added

- Unified `sqlrite` umbrella CLI with subcommands:
  - `init`, `sql`, `ingest`, `query`, `quickstart`, `serve`, `backup`, `benchmark`, `doctor`
- Cross-platform installation and packaging channels:
  - source install/update scripts, release installer script, Homebrew/winget/nfpm assets, Docker image workflow
- Interactive SQL shell helpers:
  - `.help`, `.tables`, `.schema`, `.example`
- Quickstart UX gate command:
  - `sqlrite quickstart` with timing/success telemetry (`--json`, `--output`, threshold gates)
- SQL-native vector retrieval syntax in `sqlrite sql`:
  - operators: `<->`, `<=>`, `<#>`
  - functions: `vector`, `l2_distance`, `cosine_distance`, `neg_inner_product`, `vec_dims`, `vec_to_json`
- SQL retrieval function set in `sqlrite sql`:
  - `embed(text)`, `bm25_score(query, document)`, `hybrid_score(vector_score, text_score, alpha)`
- Retrieval index DDL support in SQL mode:
  - `CREATE VECTOR INDEX ... USING HNSW`
  - `CREATE TEXT INDEX ... USING FTS5`
  - `DROP VECTOR INDEX ...`, `DROP TEXT INDEX ...`
- Retrieval index metadata catalog migration:
  - schema version `3`
  - `retrieval_indexes` table + `retrieval_index_catalog` view
- Sprint execution reports and evidence artifacts under `project_plan/reports/`.
- Sprint 5 evidence artifacts:
  - `project_plan/reports/S05.md`
  - `project_plan/reports/s05_sql_smoke.log`
  - `project_plan/reports/s05_benchmark.json`
  - `project_plan/reports/s05_eval.json`

### Changed

- `sqlrite doctor` diagnostics now include structured JSON output and improved writable-path detection.
- CLI help and README now document SQL-native retrieval usage and quickstart gates.
- `sqlrite sql` now bootstraps database migrations before executing statements, ensuring catalog tables/views exist in non-init SQL sessions.

## [0.1.0] - 2026-02-28

### Added

- SQLite-backed RAG storage and retrieval core in Rust.
- Hybrid retrieval pipeline (vector + lexical), weighted and RRF fusion.
- Runtime profiles and vector index modes (`brute_force`, `lsh_ann`, `disabled`).
- Ingestion, reindex, security, eval, benchmark, server health endpoints, and backup/verify tooling.
