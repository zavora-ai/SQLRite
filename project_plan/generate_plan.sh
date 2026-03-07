#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
SPRINT_DIR="$ROOT_DIR/sprints"
REQ_FILE="$ROOT_DIR/requirements_catalog.tsv"
META_FILE="$ROOT_DIR/sprint_metadata.tsv"
COVERAGE_FILE="$ROOT_DIR/coverage_matrix.md"
README_FILE="$ROOT_DIR/README.md"

mkdir -p "$SPRINT_DIR"
mkdir -p "$ROOT_DIR/reports"

cat > "$REQ_FILE" <<'__REQ__'
ID|Source|Description
SD-01|3.1|Ship a single sqlrite umbrella CLI with init/sql/ingest/query/serve/backup/benchmark/doctor commands.
SD-02|3.1|Ship installers for Homebrew, winget, apt/rpm, curl install script, and Docker image.
SD-03|3.1|Ship first-class SDKs for Rust, Python, and TypeScript.
SD-04|3.1|Ship interactive SQL shell with retrieval-aware helpers and examples.
SD-05|3.2|Support vector distance operators (<->, <=>, <#>) in SQL.
SD-06|3.2|Support retrieval SQL functions (vector, embed, bm25_score, hybrid_score).
SD-07|3.2|Support retrieval index DDL for vector and text indexes.
SD-08|3.2|Planner fallback to brute-force when ANN is absent or unhealthy.
SD-09|3.2|Provide EXPLAIN RETRIEVAL with score and execution breakdown.
SD-10|3.2|Provide SEARCH table-valued function for concise hybrid queries.
SD-11|3.2|Provide reranking hooks (cross-encoder optional).
SD-12|3.2|Provide query profile hints for recall/latency tradeoffs.
SD-13|3.3|Embedded mode default: single-file DB, WAL profile, local-first performance.
SD-14|3.3|Server HA profile: multi-node deployment, replicated writes, automatic failover.
SD-15|3.3|Embedded durability target: crash-safe restart with verified recovery.
SD-16|3.3|HA availability target: 99.95% monthly in reference profile.
MR-01|4.1|Install and first query in under five minutes.
MR-02|4.1|SQL cookbook covers at least 80 percent of RAG retrieval patterns.
MR-03|4.1|Built-in agent interoperability (MCP manifest and tool server mode).
MR-04|4.1|Reproducible benchmark and evaluation tooling.
MR-05|4.1|Migration paths from SQLite, pgvector, and API-first vector databases.
MR-06|4.1|Built-in observability: health/readiness/metrics/query traces.
MR-07|4.1|Security defaults: tenant isolation, encryption options, audit logs.
XP-01|4.2|Linux x86_64 and arm64 distribution support.
XP-02|4.2|macOS universal binary distribution support.
XP-03|4.2|Windows x64 and arm64 distribution support.
XP-04|4.2|Docker image support for server mode.
XP-05|4.2|WASM or edge read/query support story.
XP-06|4.2|SDK test matrix green on all supported platforms.
SQ-01|4.3|Hybrid retrieval expressible in one SQL statement.
SQ-02|4.3|Same retrieval SQL semantics in embedded and server modes.
SQ-03|4.3|EXPLAIN indicates ANN or brute-force execution path.
SQ-04|4.3|Deterministic ordering for repeated runs on fixed data/version.
PA-D01|Phase A|Consolidated sqlrite binary with subcommands.
PA-D02|Phase A|Packaging pipeline for Homebrew/winget/apt.
PA-D03|Phase A|sqlrite doctor environment diagnostics.
PA-D04|Phase A|Quickstart path (init then query).
PA-G01|Phase A|Median time-to-first-query under 3 minutes.
PA-G02|Phase A|Install success rate greater than 95 percent across OS matrix.
PB-D01|Phase B|Distance operators and vector helper functions.
PB-D02|Phase B|CREATE VECTOR INDEX USING HNSW support.
PB-D03|Phase B|Hybrid scoring with deterministic tie-breaks.
PB-D04|Phase B|Retrieval EXPLAIN output with score attribution.
PB-D05|Phase B|SQL cookbook for semantic/lexical/hybrid/filter/tenant/rerank-ready patterns.
PB-G01|Phase B|All documented retrieval patterns runnable via SQL only.
PB-G02|Phase B|Planner correctness across indexed and non-indexed paths.
PC-D01|Phase C|ANN tuning controls with brute-force fallback.
PC-D02|Phase C|Vector datatype options and quantization controls.
PC-D03|Phase C|Memory-mapped index/page optimizations.
PC-D04|Phase C|Batch ingestion optimizer and compaction tooling.
PC-D05|Phase C|Public benchmark harness for 10k/100k/1M/10M profiles.
PC-G01|Phase C|p95 hybrid latency under 40ms at 100k profile.
PC-G02|Phase C|p95 hybrid latency under 90ms at 1M profile.
PC-G03|Phase C|Ingestion throughput at or above 50k chunks/min on 8 vCPU reference.
PD-D01|Phase D|Server replication profile and reference architecture.
PD-D02|Phase D|Automatic leader failover test harness.
PD-D03|Phase D|Backup/restore and snapshot policy tooling.
PD-D04|Phase D|SLO dashboards and alert templates.
PD-D05|Phase D|Disaster recovery game-day scripts.
PD-G01|Phase D|Monthly availability at or above 99.95 percent in soak test.
PD-G02|Phase D|RPO at or below 60 seconds in HA reference profile.
PD-G03|Phase D|Chaos scenarios validated (node crash, disk-full, partition subset).
PE-D01|Phase E|Built-in MCP tool server mode.
PE-D02|Phase E|OpenAPI and gRPC query endpoints.
PE-D03|Phase E|Python and TypeScript SDK feature parity with Rust core.
PE-D04|Phase E|First-party integrations and examples for common agent stacks.
PE-D05|Phase E|Deterministic tool contract tests for agent workflows.
PE-G01|Phase E|Reference integrations validated in CI.
PE-G02|Phase E|Agent memory sample setup under 15 minutes.
PF-D01|Phase F|API freeze and compatibility contract.
PF-D02|Phase F|Secure multi-tenant policy framework (RBAC hooks/audit/key hardening).
PF-D03|Phase F|Compliance documentation and updated threat model.
PF-D04|Phase F|Long-term support branch and release policy.
PF-D05|Phase F|Migration guides from pgvector/libSQL/Qdrant/Weaviate/Milvus patterns.
PF-G01|Phase F|Zero open P0/P1 defects at release cut.
PF-G02|Phase F|Full release quality gates green.
PF-G03|Phase F|Published v1.0 benchmark and reliability report.
BE-01|6.1|Track latency (p50/p95/p99) by workload profile for each release.
BE-02|6.1|Track throughput (QPS) by concurrency level for each release.
BE-03|6.1|Track retrieval quality metrics (Recall@k, MRR, nDCG).
BE-04|6.1|Track cost efficiency (memory and storage overhead).
BE-05|6.1|Track operational resilience (failover and restore times).
BE-06|6.2|Use the same embedding model and datasets for competitor comparisons.
BE-07|6.2|Use the same hardware classes for competitor comparisons.
BE-08|6.2|Publish benchmark configurations and scripts for reproducibility.
N90-01|11|Deliver unified sqlrite CLI and packaging pipeline in first 90 days.
N90-02|11|Implement SQL operators/functions and retrieval index DDL in first 90 days.
N90-03|11|Publish SQL cookbook with real output examples in first 90 days.
N90-04|11|Stand up benchmark harness and initial competitor baseline runs in first 90 days.
N90-05|11|Ship SQLite and pgvector migration guides in first 90 days.
GV-01|9|Run weekly roadmap burn-down, benchmark drift review, and bug triage.
GV-02|9|Run monthly release-gate review across performance, quality, and security.
GV-03|9|Run quarterly competitor baseline review and target recalibration.
__REQ__

cat > "$META_FILE" <<'__META__'
Sprint|Start|End|Phase|Release|Goal|ScopeIDs|SprintDeliverables
S01|2026-03-02|2026-03-15|A|v0.5.0|Unified CLI architecture and command contract|SD-01,SD-13,PA-D01,N90-01,GV-01|Command map RFC, CLI skeleton, config profile contract, CI bootstrap
S02|2026-03-16|2026-03-29|A|v0.5.0|Packaging and install channels plus doctor diagnostics|SD-02,SD-04,PA-D02,PA-D03,XP-01,XP-02,XP-03,XP-04,N90-01,GV-01|Packager workflows, platform installers, doctor command, install smoke tests
S03|2026-03-30|2026-04-12|A|v0.5.0|Quickstart UX hardening and phase A gate closure|PA-D04,PA-G01,PA-G02,MR-01,SD-04,GV-01,GV-02|Init-to-query happy path, telemetry instrumentation, rollout checklist
S04|2026-04-13|2026-04-26|B|v0.6.0|SQL parser extension for vector operators and literal helpers|SD-05,PB-D01,N90-02,GV-01|Parser and planner updates, operator tests, SQL compatibility docs
S05|2026-04-27|2026-05-10|B|v0.6.0|Vector index DDL and metadata catalog|SD-06,SD-07,PB-D02,N90-02,GV-01|Index DDL implementation, schema metadata, migration scripts
S06|2026-05-11|2026-05-24|B|v0.6.0|Hybrid scoring engine and deterministic fallback behavior|SD-06,SD-08,SQ-01,SQ-04,PB-D03,N90-02,GV-01|Hybrid scoring APIs, deterministic tie-break tests, fallback planner rules
S07|2026-05-25|2026-06-07|B|v0.6.0|Retrieval explainability and SQL cookbook completion|SD-09,PB-D04,PB-D05,PB-G01,PB-G02,MR-02,SQ-03,N90-03,N90-04,N90-05,GV-01,GV-02,GV-03|EXPLAIN RETRIEVAL output, cookbook examples, SQL-only conformance report
S08|2026-06-08|2026-06-21|C|v0.7.0|ANN abstraction refactor and HNSW baseline|PC-D01,GV-01|ANN trait layer, HNSW adapter baseline, fallback parity checks
S09|2026-06-22|2026-07-05|C|v0.7.0|Index persistence and datatype/quantization options|PC-D01,PC-D02,GV-01|Persisted ANN format, f16/int8 support, quantization toggles
S10|2026-07-06|2026-07-19|C|v0.7.0|Memory-mapped index pages and cache tuning|PC-D03,BE-04,GV-01|Mmap-backed index mode, cache profiles, memory footprint dashboard
S11|2026-07-20|2026-08-02|C|v0.7.0|Ingestion throughput optimization and compaction|PC-D04,PC-G03,GV-01|Batch scheduler upgrades, compaction jobs, throughput profiling
S12|2026-08-03|2026-08-16|C|v0.7.0|Benchmark harness at 10k/100k/1M and platform test matrix|PC-D05,BE-01,BE-02,BE-03,BE-06,BE-07,MR-04,XP-01,XP-02,XP-03,GV-01,GV-02|Benchmark runner, standard datasets, cross-platform benchmark jobs
S13|2026-08-17|2026-08-30|C|v0.7.0|10M profile hardening and phase C gate closure|PC-D05,PC-G01,PC-G02,PC-G03,BE-01,BE-02,BE-03,BE-04,BE-08,MR-04,GV-01,GV-02|10M perf profile, release candidate report, reproducible benchmark bundle
S14|2026-08-31|2026-09-13|D|v0.8.0|HA server architecture and replication profile scaffolding|SD-14,PD-D01,SQ-02,XP-04,GV-01|Replication architecture doc, control-plane scaffolding, deployment manifests
S15|2026-09-14|2026-09-27|D|v0.8.0|Replication log and leader election reliability|PD-D01,SQ-02,GV-01|Replication log protocol, election logic, state reconciliation tests
S16|2026-09-28|2026-10-11|D|v0.8.0|Automatic failover and chaos harness|PD-D02,PD-G03,BE-05,GV-01|Failover controller, chaos scenarios, recovery-time instrumentation
S17|2026-10-12|2026-10-25|D|v0.8.0|Backup/restore and point-in-time recovery tooling|PD-D03,SD-15,GV-01|Snapshot orchestration, restore verification, PITR operator runbook
S18|2026-10-26|2026-11-08|D|v0.8.0|Observability dashboards and alert policy templates|PD-D04,MR-06,SD-15,GV-01|Metrics coverage map, tracing spans, SLO alerts and runbooks
S19|2026-11-09|2026-11-22|D|v0.8.0|DR game-day, soak tests, and phase D gate closure|PD-D05,PD-G01,PD-G02,PD-G03,MR-06,SD-16,BE-05,GV-01,GV-02,GV-03|Game-day outcomes, SLO evidence, HA readiness sign-off
S20|2026-11-23|2026-12-06|E|v0.9.0|MCP tool server mode baseline|PE-D01,MR-03,GV-01|MCP server runtime, manifest generation, tool auth baseline
S21|2026-12-07|2026-12-20|E|v0.9.0|OpenAPI query surface and cookbook parity|PE-D02,MR-02,SQ-03,GV-01|OpenAPI schemas, query endpoint parity tests, cookbook sync
S22|2026-12-21|2027-01-03|E|v0.9.0|gRPC service and shared SDK runtime core|PE-D02,PE-D03,SD-03,GV-01|gRPC APIs, shared protocol models, SDK runtime core crate
S23|2027-01-04|2027-01-17|E|v0.9.0|Python SDK parity and integration test matrix|PE-D03,SD-03,GV-01|Python client, parity tests, package release pipeline
S24|2027-01-18|2027-01-31|E|v0.9.0|TypeScript SDK parity and cross-platform SDK CI|PE-D03,XP-06,SD-03,GV-01|TypeScript client, contract tests, CI matrix expansion
S25|2027-02-01|2027-02-14|E|v0.9.0|Reference integrations and phase E gate closure|PE-D04,PE-D05,PE-G01,PE-G02,MR-03,XP-06,GV-01,GV-02|Reference apps, deterministic contract reports, release evidence pack
S26|2027-02-15|2027-02-28|F|v1.0.0|API freeze and compatibility suite kickoff|PF-D01,XP-05,GV-01|Frozen API manifest, compatibility tests, edge-read design RFC
S27|2027-03-01|2027-03-14|F|v1.0.0|RBAC policy framework and secure defaults|PF-D02,MR-07,XP-05,GV-01|RBAC hooks, tenant policy enforcement, secure config defaults
S28|2027-03-15|2027-03-28|F|v1.0.0|Audit export and key-rotation hardening|PF-D03,MR-07,SD-11,GV-01|Audit export pipeline, key rotation workflows, rerank hook security review
S29|2027-03-29|2027-04-11|F|v1.0.0|Compliance documentation and query hint design|PF-D03,SD-12,GV-01|Compliance docs, threat model delta, query hints proposal and tests
S30|2027-04-12|2027-04-25|F|v1.0.0|Migration toolchain from SQLite/pgvector/libSQL|PF-D05,MR-05,BE-06,BE-07,GV-01|Migration CLI tools, conversion docs, baseline migration benchmark
S31|2027-04-26|2027-05-09|F|v1.0.0|Migration from API-first vector DB patterns and SQL v2 design|PF-D05,SD-10,SD-11,SD-12,MR-05,GV-01,GV-03|Qdrant/Weaviate/Milvus mapping guides, SEARCH TVF prototype, gap analysis
S32|2027-05-10|2027-05-23|F|v1.0.0|Final quality audit and release blocker burn-down|PF-D04,PF-G01,BE-01,BE-02,BE-03,BE-04,BE-05,GV-01,GV-02|Full quality report, blocker closure list, release candidate hardening
S33|2027-05-24|2027-05-31|F|v1.0.0|GA release train and publication of benchmark/reliability reports|PF-G01,PF-G02,PF-G03,SD-16,BE-08,GV-01,GV-02|v1.0 GA checklist, benchmark publication, final release sign-off
__META__

phase_name() {
  case "$1" in
    A) echo "Phase A - Productization and Distribution" ;;
    B) echo "Phase B - SQL-Native Retrieval Core" ;;
    C) echo "Phase C - Performance and Scalability" ;;
    D) echo "Phase D - High Availability and Operations" ;;
    E) echo "Phase E - Agent-First Ecosystem" ;;
    F) echo "Phase F - Enterprise Trust and v1.0" ;;
    *) echo "Unknown Phase" ;;
  esac
}

phase_tracks() {
  case "$1" in
    A) cat <<'__T_A__'
1. Implement one-command developer and operator workflow (`sqlrite` umbrella CLI).
2. Make installation deterministic across Linux, macOS, Windows, and Docker images.
3. Produce first-run onboarding path with diagnostics and failure remediation.
__T_A__
      ;;
    B) cat <<'__T_B__'
1. Implement retrieval-native SQL syntax and operators with SQLite-friendly behavior.
2. Keep planner behavior deterministic across indexed and non-indexed execution paths.
3. Publish SQL-only retrieval playbook with explainability and conformance tests.
__T_B__
      ;;
    C) cat <<'__T_C__'
1. Scale vector retrieval with ANN while preserving deterministic fallbacks.
2. Improve latency, throughput, and memory efficiency through profiling and tuning.
3. Produce reproducible benchmark assets for internal and external comparison.
__T_C__
      ;;
    D) cat <<'__T_D__'
1. Add HA runtime profile with replication, failover, and recovery primitives.
2. Build disaster-readiness tooling (backup/restore/PITR) and validate via drills.
3. Establish observability stack and SLO-driven operations workflow.
__T_D__
      ;;
    E) cat <<'__T_E__'
1. Expose SQLRite as a native tool surface for agent runtimes.
2. Keep API contracts consistent across MCP, OpenAPI, gRPC, and SDKs.
3. Validate integration reliability with deterministic contract testing in CI.
__T_E__
      ;;
    F) cat <<'__T_F__'
1. Freeze API and harden enterprise governance and security controls.
2. Deliver migration tooling from SQL and API-first competitor ecosystems.
3. Close all release quality gates and publish v1.0 benchmark/reliability evidence.
__T_F__
      ;;
  esac
}

phase_validation() {
  case "$1" in
    A) cat <<'__V_A__'
- `cargo fmt --all --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test`
- installer smoke checks on Linux/macOS/Windows runners
- quickstart timing script (`init -> query`) across three clean environments
__V_A__
      ;;
    B) cat <<'__V_B__'
- SQL parser/operator regression suite
- deterministic ranking/tie-break suite across repeated seeds
- `EXPLAIN RETRIEVAL` structure snapshot tests
- SQL-only retrieval pattern conformance run
- SQLite compatibility and migration tests
__V_B__
      ;;
    C) cat <<'__V_C__'
- benchmark harness profiles (10k/100k/1M/10M)
- ANN vs brute-force recall regression tests
- memory and storage overhead measurement run
- ingestion throughput stress tests with compaction enabled
- performance drift check against prior sprint baseline
__V_C__
      ;;
    D) cat <<'__V_D__'
- replication consistency tests under concurrent writes
- failover and recovery timing tests
- backup/restore/PITR verification run
- chaos scenarios: node crash, disk-full, partial partition
- SLO dashboard signal and alert simulation tests
__V_D__
      ;;
    E) cat <<'__V_E__'
- MCP tool contract tests with deterministic request/response fixtures
- OpenAPI and gRPC compatibility tests
- cross-language SDK parity matrix (Rust/Python/TypeScript)
- reference integration end-to-end tests in CI
- setup-time test for agent memory reference app (<15 min target)
__V_E__
      ;;
    F) cat <<'__V_F__'
- API compatibility freeze diff checks
- policy/audit/encryption security regression tests
- migration tests from SQLite/pgvector/API-first query models
- full release gate run (build/test/lint/bench/security)
- final P0/P1 defect audit and sign-off
__V_F__
      ;;
  esac
}

while IFS='|' read -r sprint start end phase release goal scope_ids sprint_deliverables; do
  if [[ "$sprint" == "Sprint" ]]; then
    continue
  fi

  phase_title="$(phase_name "$phase")"
  tracks="$(phase_tracks "$phase")"
  validation="$(phase_validation "$phase")"

  scope_lines=""
  IFS=',' read -ra id_array <<< "$scope_ids"
  for raw_id in "${id_array[@]}"; do
    id="$(echo "$raw_id" | xargs)"
    source="$(awk -F'|' -v rid="$id" '$1==rid {print $2}' "$REQ_FILE")"
    desc="$(awk -F'|' -v rid="$id" '$1==rid {print $3}' "$REQ_FILE")"
    scope_lines+="- ${id} (${source}): ${desc}\n"
  done

  output_file="$SPRINT_DIR/${sprint}_${start}_to_${end}.md"

  cat > "$output_file" <<__SPRINT__
# ${sprint} Plan (${start} to ${end})

Phase: ${phase_title}
Release target: ${release}

## Sprint Goal
${goal}

## Roadmap Scope Coverage
$(printf "%b" "$scope_lines")

## Sprint Outcomes (Must Ship)
- ${sprint_deliverables}
- Demonstrable progress against all scope IDs listed above.
- Updated changelog entry for sprint artifacts and verification evidence.

## Execution Tracks
${tracks}

## Detailed Work Backlog
1. Architecture and design
- Finalize design notes and implementation boundaries for sprint goal.
- Define failure modes, fallback behavior, and data migration impacts.

2. Core implementation
- Implement feature-complete code path for sprint goal in production modules.
- Add interfaces and flags needed for backward-compatible rollout.

3. Test and validation
- Add or update unit, integration, and regression tests for all touched components.
- Ensure deterministic behavior where ranking, planning, or failover is involved.

4. Benchmark and profiling
- Capture before/after metrics for latency, throughput, and reliability where relevant.
- Record benchmark profile configuration so results are reproducible.

5. Documentation and usability
- Update README/cookbook/examples/CLI help for user-visible changes.
- Document operational runbooks for installation, rollback, and troubleshooting.

6. Release readiness
- Run release quality gates and record evidence links in sprint report.
- Prepare risk register updates, rollback plan, and release notes draft.

## Verification Plan
${validation}

## Definition of Done
- Every scope ID in this sprint has implementation, tests, and docs committed.
- CI pipeline for this sprint scope is green with no critical regressions.
- Sprint demo includes command/query examples and measurable outcome evidence.
- Rollback and recovery instructions are validated for changed surfaces.

## Dependencies and Risks
- External dependency updates are pinned and compatibility-checked before merge.
- Platform-specific differences are tested on target OS matrix before release tagging.
- Any unmet gate blocks sprint closure and creates carry-over issue in next sprint.

## Artifacts to Produce
- Sprint report (project_plan/reports/${sprint}.md) with evidence links.
- Benchmark/eval outputs for changed query or ingestion paths.
- Updated roadmap status row in project_plan/coverage_matrix.md.
__SPRINT__

done < "$META_FILE"

TOTAL_REQS=$(awk 'END{print NR-1}' "$REQ_FILE")
MAPPED_REQS=$(awk -F'|' 'NR==FNR && FNR>1 {
  split($7, ids, ",")
  for (i in ids) {
    gsub(/^ +| +$/, "", ids[i])
    if (ids[i] != "") {
      if (cov[ids[i]] == "") cov[ids[i]] = $1
      else cov[ids[i]] = cov[ids[i]] ", " $1
    }
  }
  next
}
FNR>1 {
  if ($1 in cov) mapped++
}
END { print mapped+0 }' "$META_FILE" "$REQ_FILE")

UNMAPPED_REQS=$((TOTAL_REQS - MAPPED_REQS))

{
  echo "# SQLRite Roadmap Coverage Matrix"
  echo
  echo "Source roadmap: \`/Users/jameskaranja/Developer/projects/SQLRight/project_plan/strategy/ROADMAP_COMPETITIVE_2026.md\`"
  echo
  echo "Coverage status: **${MAPPED_REQS}/${TOTAL_REQS} requirements mapped**."
  if [[ "$UNMAPPED_REQS" -eq 0 ]]; then
    echo "Coverage guarantee: **100%** (no unmapped roadmap requirements)."
  else
    echo "Coverage guarantee: **incomplete** (${UNMAPPED_REQS} requirement(s) unmapped)."
  fi
  echo
  echo "| Requirement ID | Roadmap Source | Description | Planned Sprint Coverage |"
  echo "| --- | --- | --- | --- |"

  awk -F'|' '
  NR==FNR && FNR>1 {
    split($7, ids, ",")
    for (i in ids) {
      gsub(/^ +| +$/, "", ids[i])
      if (ids[i] != "") {
        if (cov[ids[i]] == "") cov[ids[i]] = $1
        else cov[ids[i]] = cov[ids[i]] ", " $1
      }
    }
    next
  }
  FNR>1 {
    rid=$1
    source=$2
    desc=$3
    sprints=(rid in cov)?cov[rid]:"UNMAPPED"
    printf("| %s | %s | %s | %s |\n", rid, source, desc, sprints)
  }' "$META_FILE" "$REQ_FILE"

} > "$COVERAGE_FILE"

if [[ "$UNMAPPED_REQS" -ne 0 ]]; then
  echo "Coverage matrix generation failed: ${UNMAPPED_REQS} requirement(s) are unmapped." >&2
  exit 1
fi

SPRINT_COUNT=$(awk 'END{print NR-1}' "$META_FILE")

{
  echo "# SQLRite Project Plan"
  echo
  echo "This folder contains executable sprint-level planning for the competitive roadmap."
  echo
  echo "Roadmap source: \`/Users/jameskaranja/Developer/projects/SQLRight/project_plan/strategy/ROADMAP_COMPETITIVE_2026.md\`"
  echo
  echo "## Coverage Commitments"
  echo
  echo "- Sprint cadence: 2 weeks (with release-close short sprint for S33)."
  echo "- Planned sprint files: ${SPRINT_COUNT}"
  echo "- Requirement catalog: ${TOTAL_REQS} roadmap requirements"
  echo "- Coverage matrix: \`coverage_matrix.md\` (auto-generated; must remain 100%)."
  echo "- Coverage status at generation time: ${MAPPED_REQS}/${TOTAL_REQS} mapped."
  echo
  echo "## Folder Layout"
  echo
  echo "- \`requirements_catalog.tsv\`: normalized requirement IDs extracted from roadmap"
  echo "- \`sprint_metadata.tsv\`: sprint schedule, scope IDs, and sprint deliverables"
  echo "- \`coverage_matrix.md\`: requirement-to-sprint mapping (100% required)"
  echo "- \`sprints/\`: detailed sprint execution plans (S01..S33)"
  echo "- \`generate_plan.sh\`: generator script for all plan artifacts"
  echo
  echo "## Sprint Schedule"
  echo
  echo "| Sprint | Dates | Phase | Goal |"
  echo "| --- | --- | --- | --- |"
  awk -F'|' 'FNR>1 { printf("| %s | %s to %s | %s | %s |\n", $1, $2, $3, $4, $6) }' "$META_FILE"
  echo
  echo "## How to Regenerate"
  echo
  echo "\`bash /Users/jameskaranja/Developer/projects/SQLRight/project_plan/generate_plan.sh\`"
  echo
  echo "The script fails if any roadmap requirement is not mapped to at least one sprint."
} > "$README_FILE"

echo "Generated project plan artifacts in: $ROOT_DIR"
