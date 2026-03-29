# Comparative Evaluation Methodology

This paper cannot make strong competitive claims from SQLRite's internal release gates alone. This document defines the comparative benchmark protocol used to add independent, reproducible baselines.

## Competitor Set

The first comparative pass uses three systems that represent different design points:

- `SQLRite`: local-first retrieval engine built on SQLite
- `pgvector`: SQL-first vector search inside PostgreSQL
- `Qdrant`: network-native vector database with HNSW and filtered search support

This is not the full 2026 competitor landscape. It is the first benchmark set because these systems cover the most important comparison axes for SQLRite:

- embedded / local-first retrieval
- SQL-native retrieval
- networked vector database behavior
- filtered vector search under a common workload

## Common Workload

The benchmark uses a deterministic synthetic dataset so every system receives the same vectors, metadata filters, and queries.

Dataset parameters:

- corpus size: configurable
- query count: configurable
- embedding dimension: configurable
- similarity metric: cosine
- metadata filter: exact-match `tenant`
- top-k: configurable

Each query is evaluated only against items from the same tenant so the benchmark measures filtered vector retrieval, not just global nearest-neighbor search.

## Scenarios

### 1. Exact filtered cosine search

This scenario measures correctness-oriented retrieval.

- SQLRite: `brute_force`
- pgvector: exact scan without ANN index
- Qdrant: query with `params.exact=true`

### 2. Approximate filtered cosine search

This scenario measures latency / recall tradeoffs under ANN.

- SQLRite: `hnsw_baseline`
- pgvector: HNSW index with cosine ops
- Qdrant: default HNSW search

## Metrics

The harness records:

- QPS
- p50 latency
- p95 latency
- top-1 hit rate against exact ground truth
- recall@k against exact ground truth
- setup/load timing per system

Ground truth is computed in-process with exact cosine similarity over the generated dataset.

## Threats to Validity

This benchmark is still a single-host localhost experiment. It should be interpreted accordingly.

Main limitations:

- one hardware profile
- one synthetic workload family
- one filter type
- no sparse / lexical / hybrid benchmark parity across all systems yet
- no cluster-scale or distributed deployment evaluation yet

## Next Expansion

To strengthen the paper further, add:

1. a public dataset benchmark
2. a second local-first competitor such as `sqlite-vec` or LanceDB
3. hybrid retrieval experiments where a common workload definition is possible
4. cost and memory measurements
