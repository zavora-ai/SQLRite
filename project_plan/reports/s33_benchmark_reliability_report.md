# SQLRite v1.0.0 Benchmark And Reliability Report

Generated: `2026-03-07`
Host target archive: `dist/sqlrite-v1.0.0-aarch64-apple-darwin.tar.gz`

## Benchmark Publication

- quick profile weighted/brute_force qps: `162.57`
- quick profile weighted/brute_force p95 ms: `6.4154`
- 10k profile weighted/brute_force qps: `89.58`
- 10k profile weighted/brute_force p95 ms: `11.4753`
- 10k approx working set bytes: `11300140`
- 10k vector index estimated memory bytes: `5660000`

## Retrieval Quality Publication

- brute_force @k=5: recall=`1.0000`, mrr=`1.0000`, ndcg=`0.9732`
- lsh_ann @k=5: recall=`1.0000`, mrr=`1.0000`, ndcg=`0.9732`
- hnsw_baseline @k=5: recall=`1.0000`, mrr=`1.0000`, ndcg=`0.9732`

## Reliability Publication

- monthly availability: `100.00%`
- availability target: `99.95%`
- observed RPO seconds: `0.0050`
- RPO target seconds: `60.0000`
- DR benchmark qps: `68.53`
- DR benchmark p95 ms: `6.5572`
- restore benchmark qps: `286.51`
- restore benchmark p95 ms: `4.4195`
- observability benchmark qps: `73.02`
- observability benchmark p95 ms: `6.3222`

## Reproducibility

- benchmark runner: `src/bin/sqlrite-bench-suite.rs`
- release audit runner: `scripts/run-s32-release-candidate-audit.sh`
- GA release runner: `scripts/run-s33-ga-release-train.sh`
- release archive builder: `scripts/create-release-archive.sh`
- dataset: `/Users/jameskaranja/Developer/projects/SQLRight/examples/eval_dataset.json`
- dataset_id: `s32_release_candidate_v1`
- embedding_model: `deterministic-local-v1`
- hardware_class: `darwin-arm64-10cpu`
