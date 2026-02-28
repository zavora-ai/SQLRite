# SQLRite Product and Engineering Roadmap

Last updated: February 21, 2026

## 1. Vision

SQLRite is a Rust-native, SQLite-compatible retrieval database optimized for AI agents and RAG workloads.
It should combine:

- SQLite simplicity (single-file deployment, local-first operation, SQL ergonomics)
- Retrieval-native capabilities (vector similarity, keyword search, metadata filtering, hybrid ranking)
- Agent-friendly reliability (deterministic reads, low latency, concurrency controls, auditability)

Target outcome: a production-grade embedded and server deployable store for agent memory, knowledge grounding, and context retrieval.

## 2. Product Thesis

Most agent systems need a store that is:

- easier to operate than distributed vector databases
- more retrieval-aware than vanilla SQLite
- predictable enough for deterministic agent behavior in production

SQLRite will win by owning this middle layer:

- portable and embeddable like SQLite
- RAG feature set expected from modern retrieval databases
- Rust performance and safety for sustained production usage

## 3. Current Baseline (as of February 21, 2026)

Current repository state includes:

- Rust crate `sqlrite` with SQLite-backed chunk/document persistence
- embedding storage as BLOB (`Vec<f32>`)
- metadata JSON storage and equality filters
- optional FTS5 text lookup with lexical fallback
- hybrid scoring (vector + text)
- unit tests for vector ranking, hybrid retrieval, and metadata filtering

This is a strong MVP foundation but still pre-production.

## 4. Strategic Goals (12-18 months)

### 4.1 Product Goals

- Ship a stable v1.0 retrieval engine for single-node production use
- Provide SDK and API surface that AI frameworks can integrate quickly
- Support tenant isolation and compliance-ready logging
- Deliver benchmarked retrieval quality and latency under realistic corpus sizes

### 4.2 Technical Goals

- Deterministic ingestion and query semantics
- Pluggable ANN indexing with graceful brute-force fallback
- Safe schema migrations and backward compatibility policy
- End-to-end observability for query quality and system health
- High confidence release pipeline with automated gates

### 4.3 Business/Adoption Goals

- Developer-first onboarding in under 10 minutes
- 3+ framework integrations (MCP, LangChain-like adapters, custom Rust workflows)
- Reference architectures for local app, SaaS backend, and edge deployment

## 5. Non-Goals (for v1.0)

- Building a globally distributed, multi-region consensus database
- Full SQL feature parity beyond SQLite compatibility baseline
- Training or serving foundation models
- Replacing full OLTP warehouses for analytics-heavy workloads

## 6. Personas and Primary Use Cases

### 6.1 Personas

- Agent platform engineer building retrieval and memory into products
- ML engineer running RAG pipelines that need deterministic behavior
- SaaS backend engineer needing multi-tenant retrieval with predictable ops
- Desktop/edge developer needing local-first AI memory without infra overhead

### 6.2 Core Use Cases

- knowledge grounding for LLM responses
- long-term agent memory with metadata filtering and recency control
- tool-augmented agent workflows requiring query traceability
- tenant-scoped retrieval in production SaaS environments
- offline or constrained-network deployments

## 7. Product Principles

- Local-first by default, server-capable by design
- Determinism over heuristics for core data correctness
- Explicit quality metrics for retrieval (not only latency)
- Progressive complexity: simple APIs for default use, deep controls for advanced users
- Safe failure modes with clear observability signals

## 8. Delivery Model

- Planning cadence: 2-week sprints, 6-week roadmap checkpoints
- Release cadence: monthly pre-1.0 minor releases, quarterly stability review
- Quality gates: compile, tests, lint, performance baseline, retrieval eval baseline
- Change policy: no breaking schema/API changes in stable line without migration path

## 9. Roadmap Timeline (Proposed)

This timeline assumes active development beginning March 2026.

### Milestone Schedule

1. Phase 0 (Complete): MVP Foundation
   - Status: done (February 2026)
2. Phase 1: Core Hardening and API Stabilization
   - Target window: March 1, 2026 to April 15, 2026
   - Target release: v0.2.0
3. Phase 2: ANN and Retrieval Quality System
   - Target window: April 16, 2026 to June 15, 2026
   - Target release: v0.3.0
4. Phase 3: Ingestion Pipeline and Embedding Integrations
   - Target window: June 16, 2026 to August 15, 2026
   - Target release: v0.4.0
5. Phase 4: Multi-Tenant Security and Governance
   - Target window: August 16, 2026 to October 15, 2026
   - Target release: v0.6.0
6. Phase 5: Operability, Reliability, and Disaster Readiness
   - Target window: October 16, 2026 to December 15, 2026
   - Target release: v0.8.0
7. Phase 6: Ecosystem Integrations and v1.0 Readiness
   - Target window: December 16, 2026 to February 15, 2027
   - Target release: v1.0.0
8. Phase 7: Post-v1 Expansion
   - Target window: February 16, 2027 onward
   - Target release: v1.1+

## 10. Phase-by-Phase Execution Plan

## Phase 1: Core Hardening and API Stabilization (v0.2.0)

### Objectives

- make storage and retrieval behavior production-safe for small to medium deployments
- lock down a semver-friendly public API contract
- improve query determinism and observability hooks

### Scope

1. Storage and schema
   - add schema version table (`schema_migrations`)
   - implement migration runner with idempotent upgrades
   - enforce constraints for embedding dimensions and key fields
2. Runtime and concurrency
   - configure WAL mode and synchronous settings profiles
   - add connection management strategy (single connection plus pooled server mode)
   - define transactional boundaries for ingest and index updates
3. Search engine
   - make ranking normalization explicit and testable
   - support weighted fields (content title/source metadata where available)
   - add deterministic tie-break rules for equal score results
4. API
   - introduce builder-style query API with validation
   - document error taxonomy and retry guidance
   - mark unstable APIs clearly
5. Testing and docs
   - add integration tests on temporary on-disk databases
   - add compatibility matrix for SQLite features (FTS5 present/absent)
   - publish architecture and data model docs

### Deliverables

- v0.2.0 crate release with migration system
- documented query API contract
- WAL and durability configuration guide
- expanded integration test suite

### Exit Criteria

- 0 data corruption defects in stress test suite
- deterministic ranking tests pass across repeated runs
- p95 query latency under 50 ms on 100k chunk corpus (single-node benchmark profile)
- migration tests pass from v0.1.0 fixtures to v0.2.0

### Risks

- schema evolution introduces breaking behavior
- WAL tuning differs by operating environment

### Mitigations

- golden database fixtures for migration tests
- profile-based runtime defaults and explicit override docs

## Phase 2: ANN and Retrieval Quality System (v0.3.0)

### Objectives

- move beyond brute-force vector scoring for larger corpora
- establish measurable retrieval quality framework
- improve hybrid scoring controls for agent workloads

### Scope

1. ANN abstraction layer
   - create `VectorIndex` trait with unified query/insert/delete interface
   - implement baseline brute-force backend
   - integrate one ANN backend (HNSW or USearch)
   - support index persistence and rebuild workflows
2. Retrieval quality framework
   - build offline eval harness (Recall@k, MRR, nDCG)
   - add judgment dataset format for domain-specific evals
   - create regression thresholds in CI
3. Hybrid ranking improvements
   - configurable score fusion methods (weighted sum, reciprocal rank fusion)
   - query-adaptive alpha tuning hook
   - better lexical ranking fallback behavior
4. Performance engineering
   - benchmark suite by corpus size (10k, 100k, 1M)
   - memory footprint measurements for index strategies
   - cold-start and warm-cache benchmark reporting

### Deliverables

- pluggable ANN index with persistence
- retrieval eval CLI and baseline datasets
- benchmark report in repository docs

### Exit Criteria

- Recall@10 improvement >= 15 percent over brute-force baseline on evaluation corpus
- p95 vector query latency under 80 ms on 1M embeddings (target hardware profile)
- no correctness regressions in hybrid ranking tests

### Risks

- ANN dependency portability issues across platforms
- quality gains may not generalize across domains

### Mitigations

- maintain brute-force fallback backend
- ship domain-agnostic and domain-specific benchmark profiles

## Phase 3: Ingestion Pipeline and Embedding Integrations (v0.4.0)

### Objectives

- provide production ingestion workflows, not only low-level chunk APIs
- make embedding generation pluggable and reliable
- support idempotent, incremental corpus updates

### Scope

1. Ingestion orchestration
   - add ingestion jobs with durable checkpoints
   - support file, URL, and direct payload ingestion modes
   - enforce idempotent upsert semantics with content hashes
2. Chunking pipeline
   - implement chunking strategies (fixed, semantic, heading-aware)
   - preserve source offsets for citation rendering
   - configurable overlap and max token policies
3. Embedding providers
   - provider trait and adapters (local model, OpenAI-compatible endpoint, custom HTTP)
   - batch embed with retry/backoff and partial failure handling
   - embedding version management for reindex events
4. Reindex and compaction
   - trigger re-embedding by provider or model version change
   - background index rebuild with progress reporting
   - garbage collect stale embeddings/chunks

### Deliverables

- ingestion worker module
- embedding provider abstraction and adapters
- reindex command and lifecycle docs

### Exit Criteria

- successful ingestion of 1M chunks with zero duplicate IDs in idempotent mode
- recoverable restart from ingestion checkpoint after simulated process crash
- full re-embedding and reindex flow validated in staging benchmark scenario

### Risks

- third-party embedding API variability causes unstable throughput
- chunking defaults may over/under-segment documents

### Mitigations

- queue-based retry and circuit breaker policies
- profile presets and dataset-specific chunking validation

## Phase 4: Multi-Tenant Security and Governance (v0.6.0)

### Objectives

- make SQLRite safe for shared SaaS usage
- add policy and audit capabilities expected in production systems
- enforce tenant-aware retrieval boundaries

### Scope

1. Tenant isolation model
   - required tenant ID on ingest and query paths
   - tenant-aware indexes and query guards
   - optional per-tenant encryption keys for stored embeddings
2. Authentication and authorization hooks
   - API-level authn/authz interceptors
   - policy engine integration points
   - allow/deny audit trail for sensitive operations
3. Audit logging
   - structured query and ingestion event logs
   - redaction controls for sensitive content
   - retention policy hooks
4. Compliance readiness
   - data deletion workflows (subject delete)
   - key rotation and credential management guidelines
   - security hardening checklist and threat model documentation

### Deliverables

- tenant-aware schema and query enforcement
- auth hooks and policy reference implementation
- audit log format and retention controls
- security threat model document

### Exit Criteria

- tenant escape tests demonstrate zero cross-tenant retrieval leakage
- all critical threat model items mitigated or accepted with explicit owner sign-off
- deletion and key rotation workflows pass integration tests

### Risks

- policy flexibility introduces performance overhead
- encryption strategy can complicate portability and recovery

### Mitigations

- benchmark secure and non-secure profiles
- explicit key management runbooks

## Phase 5: Operability, Reliability, and Disaster Readiness (v0.8.0)

### Objectives

- ensure SQLRite is operable at scale in real environments
- support backup, restore, and failure recovery workflows
- provide SRE-friendly observability and runbooks

### Scope

1. Reliability engineering
   - fault-injection tests for process crashes and IO failures
   - startup recovery validation for partial writes
   - consistency checks and automated repair tooling
2. Backup and restore
   - full and incremental backup workflows
   - restore verification command
   - point-in-time restore guidance where feasible
3. Observability
   - metrics (query latency, recall proxy metrics, ingest throughput, queue depth)
   - tracing around ingest and query execution stages
   - health endpoints and readiness probes for server mode
4. Performance and capacity planning
   - hardware profile guidance by corpus size and QPS
   - capacity estimation tooling
   - compaction and maintenance schedule recommendations

### Deliverables

- reliability test harness and chaos scenarios
- backup/restore commands and docs
- dashboard templates for metrics and tracing
- SRE runbook with incident playbooks

### Exit Criteria

- disaster-recovery game day completed with successful restore
- p99 query latency and error budget meet SLO targets in soak tests
- observability dashboards cover all critical user journeys

### Risks

- hidden filesystem assumptions in different cloud/storage environments
- restore-time objectives may not meet strict enterprise needs

### Mitigations

- test matrix across local SSD, network volumes, and containerized environments
- publish realistic RTO/RPO envelopes by dataset size

## Phase 6: Ecosystem Integrations and v1.0 Readiness (v1.0.0)

### Objectives

- finalize API and stability guarantees
- ensure smooth integration into agent ecosystems
- complete release readiness evidence and governance

### Scope

1. API and compatibility
   - freeze stable API modules
   - formal deprecation policy and migration notes
   - long-term support branch policy definition
2. Ecosystem adapters
   - MCP-compatible tool adapter
   - framework adapters for common RAG stacks
   - reference code samples for Rust and Python consumers
3. Documentation and developer experience
   - end-to-end guides: quickstart, production setup, troubleshooting
   - architecture deep-dive and performance tuning guide
   - cookbook examples for agent memory and retrieval patterns
4. Release quality
   - full regression matrix (platforms, SQLite variants, FTS availability)
   - security review and dependency audit
   - v1.0 launch checklist with release notes

### Deliverables

- v1.0.0 stable release
- adapter packages and sample apps
- production documentation set
- compatibility and quality report

### Exit Criteria

- zero P0/P1 defects open at release cut
- API freeze validation completed and documented
- sample integrations verified in CI against supported versions

### Risks

- integration surface area increases maintenance overhead
- final stabilization can delay release if regression rate spikes

### Mitigations

- define strict adapter maintenance boundaries
- pre-release canary cycle before final v1.0 cut

## Phase 7: Post-v1 Expansion (v1.1+)

### Candidate Themes

- optional server cluster patterns (read replicas and async sync)
- advanced reranking (cross-encoder hooks)
- multimodal retrieval (text and image embeddings)
- enterprise controls (policy packs, extended audit exports)
- optional SQL extensions for retrieval-native query syntax

### Decision Gates

- only prioritize features with measurable user adoption or enterprise demand
- preserve embedded-local-first simplicity as a permanent product constraint

## 11. Workstream Backlog (Cross-Phase)

### A. Data Model and Storage

- schema versioning and forward-only migrations
- optimized metadata indexing strategy
- vector storage format evolution (f32/f16 options)
- document/chunk lifecycle states and tombstones
- provenance model for chunk origin and transformation lineage

### B. Retrieval Core

- ANN backend abstraction and lifecycle
- hybrid ranking fusion methods
- advanced filter planner
- query-level controls (timeout, recall mode, precision mode)
- citation-ready source span return types

### C. Ingestion and Pipeline

- connector framework (filesystem, HTTP, queue)
- chunking strategy registry
- embedding job queue and retries
- idempotent ingestion and dedupe
- reindex orchestration

### D. Security and Governance

- tenant partitioning primitives
- auth hooks
- audit schema and retention
- encryption and key rotation
- secure defaults and configuration profiles

### E. Operability

- metrics, traces, logs
- backup/restore
- consistency tooling
- maintenance operations and automation
- deployment reference architectures

### F. Developer Experience

- SDK ergonomics and examples
- error messages and troubleshooting docs
- compatibility matrix and upgrade guides
- templates for common agent architectures

## 12. Detailed Quality Gates

Every release candidate must pass all gates below.

### Gate 1: Build and Static Checks

- `cargo fmt --check`
- `cargo clippy` with defined lint baseline
- dependency security audit

### Gate 2: Correctness

- full unit and integration test suite
- migration round-trip tests
- deterministic ranking tests with fixed seeds

### Gate 3: Retrieval Quality

- offline evaluation suite with minimum thresholds
- no degradation beyond allowed deltas versus previous release

### Gate 4: Performance

- benchmark suite by corpus size profile
- no regression beyond budgeted tolerance

### Gate 5: Reliability

- crash recovery tests
- backup/restore verification
- soak test completion

### Gate 6: Documentation and Release Readiness

- updated API docs and migration notes
- release notes with known limitations
- runbooks and operator guidance updates

## 13. Success Metrics and Targets

## 13.1 Retrieval Quality KPIs

- Recall@10
  - v0.2 baseline: establish
  - v0.3 target: +15 percent relative improvement
  - v1.0 target: +25 percent relative improvement over baseline
- MRR@n
  - continuous improvement target each minor release
- nDCG@10
  - minimum non-regression policy after v0.3

## 13.2 Latency and Throughput KPIs

- p95 query latency
  - <= 50 ms at 100k chunks (v0.2 target)
  - <= 80 ms at 1M chunks with ANN (v0.3 target)
  - <= 60 ms at 1M chunks optimized profile (v1.0 target)
- ingest throughput
  - >= 5k chunks/min baseline batch mode (v0.4 target)
  - >= 20k chunks/min optimized path (v1.0 target)

## 13.3 Reliability KPIs

- successful crash recovery rate: 100 percent in harness scenarios
- restore success rate: 100 percent in DR game days
- error budget adherence per release cycle

## 13.4 Adoption KPIs

- time to first successful query under 10 minutes
- documentation task completion rate in onboarding tests
- number of active integration paths in real projects

## 14. Risks and Decision Register

### High-Risk Items

1. Scope creep from feature requests outside core retrieval mission
2. ANN backend lock-in or unstable dependency landscape
3. Security requirements arriving late in design lifecycle
4. Incomplete retrieval evaluation leading to misleading performance-only optimization

### Major Decision Checkpoints

1. By April 2026: ANN backend selection for default distribution
2. By June 2026: ingestion architecture (embedded only vs optional worker service)
3. By August 2026: tenant isolation depth for v1.0 (schema-level vs file-level partitioning)
4. By November 2026: v1 API freeze scope and deprecation budget

## 15. Team and Resourcing Model (Recommended)

Minimal staffing for roadmap confidence:

- 1 product/technical lead
- 2 Rust database engineers
- 1 retrieval/ML quality engineer
- 1 DevOps/SRE engineer (shared acceptable early, dedicated by Phase 5)
- 1 developer experience writer/engineer (part-time early, dedicated by Phase 6)

## 16. Dependency Plan

### External Dependencies

- SQLite capabilities and extension availability by platform
- ANN library ecosystem maturity and licensing
- embedding provider APIs and rate limits
- observability stack choices for server deployments

### Internal Dependencies

- benchmark corpus and relevance labels for eval loop
- CI capacity for performance and soak workloads
- reproducible test environments

## 17. Governance and Operating Rhythm

- Weekly:
  - engineering triage and blocker review
  - retrieval quality trend review
- Bi-weekly:
  - sprint planning, demo, and retro
  - risk register refresh
- Every 6 weeks:
  - roadmap checkpoint and re-prioritization
- Quarterly:
  - architecture review and technical debt reconciliation

## 18. Definition of Done (Project-Level)

SQLRite reaches roadmap completion for v1.0 when:

- stable APIs and migration policy are published and enforced
- retrieval quality and latency targets are met across benchmark profiles
- tenant isolation and audit controls pass security acceptance criteria
- backup/restore and incident runbooks are validated in practice
- ecosystem integrations and docs enable independent adoption

## 19. Immediate Next 30-Day Action Plan

1. Phase 1 kickoff (Week 1)
   - define migration format and schema change policy
   - introduce runtime configuration profile system
2. Determinism and quality baseline (Week 2)
   - add ranking normalization tests and tie-break behavior
   - set baseline retrieval metrics dataset and reporting format
3. Runtime hardening (Week 3)
   - implement WAL profile defaults and connection strategy
   - add integration tests for on-disk behavior under concurrency
4. v0.2 release prep (Week 4)
   - finalize API docs and migration notes
   - release candidate testing and benchmark snapshot

## 20. Open Questions to Resolve Early

1. Should SQLRite remain purely embedded-first, or should server mode be first-class in v1?
2. Which ANN backend best balances portability, quality, and operational simplicity?
3. Is per-tenant encryption in scope for v1 core, or delivered as enterprise profile extension?
4. What are the minimum framework adapters required to maximize adoption at launch?
5. What benchmark datasets best represent target production workloads?
