# Public Dataset Evaluation Methodology

This benchmark extends the paper beyond synthetic workloads.

## Dataset

- Dataset: `BEIR/SciFact`
- Source: the public BEIR dataset release
- Corpus fields used: title + abstract text
- Query relevance: official SciFact qrels

## Embeddings

To keep the benchmark fully local and reproducible, the benchmark uses a deterministic hashed embedding function instead of a hosted or heavyweight neural embedding model.

Properties:

- token hashing into a fixed-dimensional dense vector
- L2 normalization
- same embedding function for corpus and query text
- no external API dependency

This is not intended to represent state-of-the-art semantic embedding quality. It is intended to create a reproducible shared dense representation so systems can be compared on the same public corpus.

## Benchmarks

### 1. Vector Exact Benchmark

Compared systems:

- `SQLRite brute_force`
- `sqlite-vec exact`
- `pgvector exact`

Metrics:

- QPS
- p50 latency
- p95 latency
- recall@k
- MRR@k
- NDCG@k

### 2. Hybrid Lexical + Vector Benchmark

Compared systems:

- `SQLRite hybrid`
- `pgvector hybrid`

Reason for this narrower set:

- both systems support a meaningful lexical + vector query path on the same data
- `sqlite-vec` is vector-only, so hybrid parity is not available there

## Interpretation

This benchmark is stronger than a synthetic-only benchmark because it uses public queries and qrels. It is still limited by the deterministic local embedding function and a single-host setup.
