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
- Sprint execution reports and evidence artifacts under `project_plan/reports/`.

### Changed

- `sqlrite doctor` diagnostics now include structured JSON output and improved writable-path detection.
- CLI help and README now document SQL-native retrieval usage and quickstart gates.

## [0.1.0] - 2026-02-28

### Added

- SQLite-backed RAG storage and retrieval core in Rust.
- Hybrid retrieval pipeline (vector + lexical), weighted and RRF fusion.
- Runtime profiles and vector index modes (`brute_force`, `lsh_ann`, `disabled`).
- Ingestion, reindex, security, eval, benchmark, server health endpoints, and backup/verify tooling.
