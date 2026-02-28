# Changelog

All notable changes to SQLRite are documented in this file.

The format is based on Keep a Changelog.

## [Unreleased]

### Added

- Unified `sqlrite` umbrella CLI with subcommands:
  - `init`, `sql`, `ingest`, `query`, `quickstart`, `serve`, `backup`, `compact`, `benchmark`, `doctor`
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
- Hybrid planner fallback hardening:
  - vector retrieval now falls back to brute-force scoring when ANN/index candidates are absent or unhealthy
  - deterministic repeated-run ordering validated for fixed data/version
- Sprint execution reports and evidence artifacts under `project_plan/reports/`.
- Sprint 5 evidence artifacts:
  - `project_plan/reports/S05.md`
  - `project_plan/reports/s05_sql_smoke.log`
  - `project_plan/reports/s05_benchmark.json`
  - `project_plan/reports/s05_eval.json`
- Sprint 6 evidence artifacts:
  - `project_plan/reports/S06.md`
  - `project_plan/reports/s06_sql_smoke.log`
  - `project_plan/reports/s06_benchmark.json`
  - `project_plan/reports/s06_eval.json`
- Retrieval explainability support in SQL mode:
  - `EXPLAIN RETRIEVAL <query>` with execution-path, score attribution, determinism hints, and `EXPLAIN QUERY PLAN` rows
- SQL cookbook and migration documentation:
  - `docs/sql_cookbook.md`
  - `docs/migrations/sqlite_to_sqlrite.md`
  - `docs/migrations/pgvector_to_sqlrite.md`
- SQL-only conformance runner:
  - `scripts/run-sql-cookbook-conformance.sh`
- Sprint 7 evidence artifacts:
  - `project_plan/reports/S07.md`
  - `project_plan/reports/s07_sql_conformance.json`
  - `project_plan/reports/s07_bench_matrix.json`
  - `project_plan/reports/s07_eval.json`
  - `project_plan/reports/s07_release_gate.md`
  - `project_plan/reports/s07_competitor_review.md`
- ANN baseline and tuning expansions:
  - new index mode `hnsw_baseline`
  - ANN runtime tuning config (`min_candidates`, hamming radius, candidate multiplier)
  - vector storage options (`f32`, `f16`, `int8`)
- ANN persistence and quantization support:
  - persisted ANN snapshot files for ANN modes
  - snapshot encoding variants (`f32`, `f16`, `int8`)
  - quantization/dequantization regression tests
- Sprint 8 evidence artifacts:
  - `project_plan/reports/S08.md`
  - `project_plan/reports/s08_bench_matrix.json`
  - `project_plan/reports/s08_eval.json`
- Sprint 9 evidence artifacts:
  - `project_plan/reports/S09.md`
  - `project_plan/reports/s09_benchmark_f32.json`
  - `project_plan/reports/s09_benchmark_f16.json`
  - `project_plan/reports/s09_benchmark_int8.json`
  - `project_plan/reports/s09_eval_int8.json`
  - `project_plan/reports/s09_persistence_doctor.json`
- Sprint 10 evidence artifacts:
  - `project_plan/reports/S10.md`
  - `project_plan/reports/s10_benchmark_default.json`
  - `project_plan/reports/s10_benchmark_tuned.json`
  - `project_plan/reports/s10_quickstart_default.json`
  - `project_plan/reports/s10_quickstart_tuned.json`
  - `project_plan/reports/s10_doctor_tuned.json`
  - `project_plan/reports/s10_memory_dashboard.md`
- Ingestion throughput optimizer and telemetry:
  - adaptive batch tuning (`IngestionBatchTuning`)
  - ingestion report fields for throughput/duration/batch profile
  - `sqlrite-ingest` flags: adaptive toggle, max batch, target batch latency, JSON output
- Compaction tooling:
  - `SqlRite::compact(CompactionOptions)` with dedupe + orphan prune + maintenance actions
  - umbrella CLI command: `sqlrite compact`
  - ops CLI command: `sqlrite-ops compact`
- Sprint 11 evidence artifacts:
  - `project_plan/reports/S11.md`
  - `project_plan/reports/s11_ingest_no_adaptive.json`
  - `project_plan/reports/s11_ingest_adaptive.json`
  - `project_plan/reports/s11_compaction.json`
  - `project_plan/reports/s11_compaction_dedupe.json`
  - `project_plan/reports/s11_benchmark.json`
  - `project_plan/reports/s11_eval.json`

### Changed

- `sqlrite doctor` diagnostics now include structured JSON output and improved writable-path detection.
- CLI help and README now document SQL-native retrieval usage and quickstart gates.
- `sqlrite sql` now bootstraps database migrations before executing statements, ensuring catalog tables/views exist in non-init SQL sessions.
- Search planner behavior now guarantees vector brute-force fallback semantics when ANN/index paths are unavailable, with deterministic tie-break ordering.
- SQL REPL example catalog now includes additional SQL-only retrieval cookbook patterns (`filter`, `doc_scope`, `rerank_ready`, `explain`).
- Runtime profiles now include sqlite tuning controls and report them in benchmark/doctor output:
  - `sqlite_mmap_size_bytes`
  - `sqlite_cache_size_kib`
- `sqlrite-bench-matrix` now includes `weighted + hnsw_baseline` scenario.
- Benchmark CLI human output now includes runtime storage/cache settings and ingestion payload/index footprint metrics.

## [0.1.0] - 2026-02-28

### Added

- SQLite-backed RAG storage and retrieval core in Rust.
- Hybrid retrieval pipeline (vector + lexical), weighted and RRF fusion.
- Runtime profiles and vector index modes (`brute_force`, `lsh_ann`, `disabled`).
- Ingestion, reindex, security, eval, benchmark, server health endpoints, and backup/verify tooling.
