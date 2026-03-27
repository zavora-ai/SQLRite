# SQLRite Performance V2

This document is the execution plan for moving SQLRite from a solid local-first retrieval engine to a top-tier performance product.

The target is not vague "optimization." The target is to make SQLRite competitive on measured retrieval throughput and latency while keeping its SQL-native and local-first strengths intact.

## Current Position

The paper exposed the current reality clearly:

| Area | Current state | Main issue |
|---|---|---|
| Exact vector retrieval | materially improved, still behind the leaders | hot path now avoids most row materialization, but exact search still lacks a persisted/mmapped vector data plane |
| ANN retrieval | improved, but still behind the leaders | `hnsw_baseline` is now real HNSW, but graph storage and query kernels are still not specialized enough |
| Hybrid retrieval | Stronger quality on public data | Latency is too high because the execution path materializes and scores too much too early |
| Filtering | Correct | Filters are not yet a first-class vector index primitive |
| Deployment model | Strong | SQLite is acting as both control plane and hot-path data plane |

## North-Star Goal

SQLRite should aim to become:

**The fastest local-first SQL-native retrieval engine**

That is the right competitive frame.

Trying to beat every distributed vector database on every raw-QPS benchmark immediately is not realistic. Beating the local-first class while closing the gap on service-oriented systems is realistic.

## Performance Principles

1. SQLite stays the control plane.
2. Vector search becomes a specialized data plane.
3. Query execution must use late materialization.
4. Filters must prune before scoring.
5. Hybrid retrieval must fuse compact candidate sets, not full payload rows.
6. SIMD and memory layout matter more than language choice.

## Phase Plan

## Phase 1: Real ANN Core

### Objective

Replace the current pseudo-HNSW path with a real ANN graph.

### Scope

- keep the public `hnsw_baseline` mode name stable for compatibility
- replace the current LSH-backed implementation with a real HNSW backend
- preserve correctness for ingest, remove, reset, and query flows
- use lazy graph rebuilds after mutations so the mutable SQLRite API remains intact

### Success Criteria

- `hnsw_baseline` is backed by a real HNSW graph
- ANN recall is measurably higher than the prior LSH-backed path at similar candidate limits
- benchmark results improve enough to justify the architectural step

### Delivered in this phase

- real HNSW backend integration
- dirty-graph rebuild model after upserts/removals
- no public API breakage
- filtered HNSW search in the planner
- binary ANN entry sidecars for file-backed ANN modes, with binary-first reload and JSON fallback
- on-disk HNSW graph snapshots with eager reload for file-backed reopen
- adaptive exact-scan crossover for `hnsw_baseline` on small corpora and small filtered subsets

### Deferred

- filtered HNSW search integrated into the request planner
- quantized ANN storage

## Phase 2: Late Materialization Query Engine

### Objective

Stop materializing full rows during candidate generation.

### Work

- split query execution into:
  - candidate generation
  - scoring/fusion
  - payload fetch
- fetch only chunk ids and compact scores until final top-k
- load `content` and `metadata` only for the winning rows

### Expected impact

- lower CPU and memory pressure
- lower hybrid latency
- better scaling at larger `candidate_limit`

### Delivered so far

- candidate fetches now materialize ids first for the common vector/text path
- `content` is fetched only for lexical fallback and final top-k payloads
- embeddings are fetched on demand only for candidate ids that need fallback vector scoring
- `doc_id` and `metadata` are now fetched only for the final top-k rows after hybrid/vector/text ranking

## Phase 3: Filter-First Retrieval

### Objective

Make metadata and tenant filters prune before vector scoring.

### Work

- add fast filter bitmaps or sorted allow-lists by:
  - tenant
  - doc_id
  - common metadata keys
- pass allow-lists into ANN queries where the backend supports filtered search
- avoid broad candidate fetches followed by post-filtering

### Expected impact

- major gains on multi-tenant workloads
- much fairer comparisons against systems optimized for filtered vector search

### Delivered so far

- allow-list planning for `doc_id` and metadata filters
- filtered vector query path for brute force, LSH, and HNSW modes
- end-to-end filtered HNSW validation

## Phase 4: SIMD Vector Kernels

### Objective

Replace scalar vector math with architecture-aware kernels.

### Work

- SIMD dot product
- SIMD cosine / normalization
- optimized rerank kernels for top-k candidate sets
- architecture-specific tuning for:
  - Apple Silicon / ARM NEON
  - x86_64 AVX2
  - x86_64 AVX-512 where available

### Expected impact

- better exact-search throughput
- better rerank performance
- lower hybrid latency

### Delivered so far

- unrolled dot-product and norm helpers in the exact-search hot path
- query normalization reuse to avoid repeated work during fallback scoring
- AVX2 kernels on x86/x86_64, with the faster scalar path retained on Apple Silicon until a NEON path benchmarks positively

## Phase 5: Vector Segment Store

### Objective

Move vectors out of the SQLite BLOB hot path.

### Work

- keep SQLite for metadata, SQL semantics, transactions, and ops
- store vectors in a dedicated segment data plane
- start with contiguous in-process segment storage for exact search
- move to memory-mapped segment files and row-to-segment persistence next
- support aligned contiguous vector blocks for efficient scan and ANN build

### Expected impact

- eliminate repeated embedding decode overhead
- increase cache locality
- reduce per-query data movement

### Delivered so far

- brute-force exact search now stores normalized vectors in a contiguous segment store instead of one heap allocation per vector
- exact-search scans now operate over contiguous vector slices keyed by stable chunk positions
- exact-search latency improved further after the segment-store refactor in the internal 5k/150 harness
- file-backed brute-force mode now persists a binary exact-vector sidecar so startup can reload the exact-search segment store without decoding every embedding blob from SQLite rows
- file-backed brute-force `f32` reopen now prefers an mmap-backed exact sidecar instead of eagerly decoding a copied snapshot
- `hnsw_baseline` now stores normalized vectors in the same contiguous segment layout instead of one heap allocation per entry

## Phase 6: Quantization

### Objective

Improve memory density and search speed.

### Work

- int8 exact-search path
- product quantization or residual quantization for ANN
- fp32 rerank over compressed candidates

### Expected impact

- better memory utilization
- higher QPS
- improved scale on laptop and edge-class hardware

### Delivered in this phase

- `VectorStorageKind` now changes live in-memory storage, not only snapshots
- brute-force exact search now supports:
  - `f32`
  - `f16`
  - `int8`
- `hnsw_baseline` now stores its segment/rerank data in:
  - `f32`
  - `f16`
  - `int8`
- `lsh_ann` now stores normalized vectors in the selected storage format instead of always keeping fp32 payloads
- the benchmark CLI now accepts `--storage-kind f32|f16|int8`
- the Phase 6 quantization suite is now reproducible via:
  - `/Users/jameskaranja/Developer/projects/SQLRight/scripts/run-p6-quantization-suite.sh`

### Measured results

On the internal 5k/150 weighted-hybrid benchmark:

- brute-force:
  - `f32`: `429.47 QPS`, `p95=2.5240 ms`, `1,550,000` estimated bytes
  - `f16`: `413.50 QPS`, `p95=2.6368 ms`, `910,000` estimated bytes
  - `int8`: `426.74 QPS`, `p95=2.5362 ms`, `610,000` estimated bytes
- `hnsw_baseline`:
  - `f32`: `423.40 QPS`, `p95=2.5516 ms`, `2,830,000` estimated bytes
  - `f16`: `404.63 QPS`, `p95=2.7055 ms`, `2,190,000` estimated bytes
  - `int8`: `420.87 QPS`, `p95=2.6683 ms`, `1,890,000` estimated bytes

### Conclusion

- `int8` is the best current tradeoff:
  - near-f32 throughput
  - materially lower estimated memory
- `f16` reduces memory but costs more throughput than `int8` on the current workload
- the next bottleneck after Phase 6 is ANN graph/query specialization, not raw exact-search storage size

## Phase 7: Planner Rewrite for Hybrid Retrieval

### Objective

Make hybrid retrieval efficient enough to be a headline differentiator.

### Work

- vector top-N
- text top-N
- id-only fusion
- selective rerank
- final payload fetch
- explicit planner modes:
  - vector-first
  - text-first
  - balanced hybrid

### Expected impact

- preserve quality gains from hybrid retrieval
- reduce the current severe latency penalty

### Delivered so far

- hybrid/vector/text candidate fusion now ranks id-only candidates first and fetches row metadata only for the final winners
- explicit hybrid planner modes now exist:
  - `vector-first`
  - `text-first`
  - `balanced hybrid`
- planner mode selection is now driven by:
  - query profile
  - alpha
  - FTS availability
  - vector-index availability
- `vector-first` and `text-first` now use tighter candidate budgets instead of always expanding to the full SQL candidate limit
- staged hybrid rerank now uses provisional hybrid scores from already-known vector/FTS signals before it fetches missing embeddings or lexical fallback content
- partial FTS score lookups now fill only missing candidate ids instead of redoing the whole candidate set
- lexical fallback content fetch now loads only candidates that are actually missing a usable FTS score
- filtered vector search now resolves common `doc_id` and top-level string metadata filters through an in-memory chunk filter index before it falls back to SQLite scans
- the Phase 7 planner suite is reproducible via:
  - `/Users/jameskaranja/Developer/projects/SQLRight/scripts/run-p7-hybrid-planner-suite.sh`

### Current measured result

On the internal 5k/150 weighted-hybrid benchmark with `f32` storage:

- `brute_force`: `600.14 QPS`, `p95=1.8118 ms`, `top1=1.0`
- `hnsw_baseline`: `589.01 QPS`, `p95=1.8907 ms`, `top1=1.0`

Relative to the Phase 6 `f32` baseline:

- `brute_force` improved by `+170.67 QPS`
- `hnsw_baseline` improved by `+165.62 QPS`

### Conclusion

- Phase 7 has already materially reduced hybrid-path overhead
- the planner work is directionally correct
- `hnsw_baseline` is now in the same performance band as `brute_force`, but it is still not decisively ahead on this workload
- the next clean win is no longer broad hybrid-planner overhead; it is filtered workloads and ANN-specific specialization

## Phase 8: Benchmark Discipline

### Objective

Make performance work measurable and defensible.

### Required benchmark matrix

| Category | Workloads |
|---|---|
| Exact vector | small, medium, large corpora |
| ANN | recall/latency sweeps |
| Filtered ANN | tenant and metadata filter workloads |
| Hybrid | lexical+vector public datasets |
| Embedded | direct library-to-library comparison |
| Service | HTTP/gRPC to HTTP/gRPC comparison |

### Comparator set

- sqlite-vec
- pgvector
- Qdrant
- LanceDB

### Public datasets

- SciFact
- one larger BEIR dataset
- one document-heavy hybrid retrieval dataset

## Engineering Priorities

If only three things happen next, they should be:

1. persisted/mmapped vector segment store
2. real SIMD kernels
3. ANN graph storage and query-path specialization

That is the shortest path to a meaningful leaderboard change.

## Risks

| Risk | Why it matters | Mitigation |
|---|---|---|
| SQLite stays in the hot path too long | Throughput ceiling remains low | move vectors to segment storage |
| ANN gains hurt correctness | Search regressions damage trust | keep exact fallback and recall benchmarks |
| Hybrid remains high-latency | Strong quality still won’t convert to adoption | planner rewrite and late materialization |
| Benchmark design is not apples-to-apples | performance claims won’t hold up | embedded-vs-embedded and service-vs-service discipline |

## Immediate Work Order

### Now

- memory-map the exact segment sidecar instead of eagerly decoding it into heap vectors
- benchmark startup and steady-state exact search again
- extend the segment data plane beyond brute-force exact search

### Next

- add real SIMD kernels
- reduce HNSW graph/query overhead
- benchmark against sqlite-vec and pgvector again

## Definition of "No. 1"

SQLRite is "No. 1" when it wins the local-first class on:

- exact vector throughput
- ANN recall/latency balance
- hybrid retrieval quality/performance balance
- developer-facing SQL retrieval ergonomics
- operational completeness in one product

That is a stronger and more defensible goal than claiming universal dominance on every vector benchmark.
