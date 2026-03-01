# SQLRite HA Replication Profile (S14-S19)

Status: Active scaffold + protocol reliability + resilience + DR/SLO operations  
Date: March 1, 2026

## Purpose

Define the reference high-availability profile for SQLRite server mode (`v0.8.0` target) with:

1. multi-node topology primitives
2. replication and failover configuration surface
3. backup/restore/PITR lifecycle
4. observability and SLO control-plane contract
5. deployment references for Docker Compose and Kubernetes

S14 delivered control-plane scaffolding and deployment references. S15 extended this with replication log protocol and leader-election reliability primitives. S16 added automatic failover controller checks and chaos scenarios. S17 added snapshot/restore/PITR tooling. S18 added observability/alerts/SLO endpoints. S19 added DR game-day + soak gate automation.

## Reference Topology

Recommended baseline cluster:

1. `1x primary`
2. `2x replicas`
3. shared control-plane contract (`/control/v1/*`)
4. metrics scraped from `/metrics`

Role model:

1. `standalone` for non-HA/local development
2. `primary` for write leadership
3. `replica` for follower nodes

## Runtime Profile Contract

`sqlrite serve` accepts HA profile controls:

1. identity and topology
- `--ha-role standalone|primary|replica`
- `--cluster-id <id>`
- `--node-id <id>`
- `--advertise <host:port>`
- `--peer <host:port>` (repeatable)

2. replication/failover tuning
- `--sync-ack-quorum <n>`
- `--heartbeat-ms <n>`
- `--election-timeout-ms <n>`
- `--max-replication-lag-ms <n>`
- `--failover manual|automatic`

3. recovery profile
- `--backup-dir <dir>`
- `--snapshot-interval-s <n>`
- `--pitr-retention-s <n>`

4. control-plane hardening
- `--control-token <token>` protects mutating control endpoints

5. data-plane SQL endpoint control
- `--disable-sql-endpoint` disables `/v1/sql`

## Control-Plane Endpoints

Read endpoints:

1. `GET /control/v1/profile`
2. `GET /control/v1/state`
3. `GET /control/v1/peers`
4. `GET /control/v1/failover/status`
5. `GET /control/v1/resilience`
6. `GET /control/v1/chaos/status`
7. `GET /control/v1/replication/log?from=<index>&limit=<n>`
8. `GET /control/v1/recovery/snapshots?limit=<n>`
9. `GET /control/v1/observability/metrics-map`
10. `GET /control/v1/traces/recent?limit=<n>`
11. `GET /control/v1/alerts/templates`
12. `GET /control/v1/slo/report`

Mutation endpoints (`POST`, optional token auth via `x-sqlrite-control-token`):

1. `/control/v1/failover/start`
2. `/control/v1/failover/promote`
3. `/control/v1/failover/step-down`
4. `/control/v1/failover/auto-check`
5. `/control/v1/recovery/start`
6. `/control/v1/recovery/mark-restored`
7. `/control/v1/recovery/snapshot`
8. `/control/v1/recovery/verify-restore`
9. `/control/v1/recovery/prune-snapshots`
10. `/control/v1/observability/reset`
11. `/control/v1/alerts/simulate`
12. `/control/v1/replication/append`
13. `/control/v1/replication/receive`
14. `/control/v1/replication/ack`
15. `/control/v1/replication/reconcile`
16. `/control/v1/election/request-vote`
17. `/control/v1/election/heartbeat`
18. `/control/v1/chaos/inject`
19. `/control/v1/chaos/clear`

## S15 Protocol Primitives

1. Replication log structure:
- ordered entries (`index`, `term`, `leader_id`, `operation`, `payload`, `checksum`)
- checksum validation and conflict truncation on receive
- quorum-based commit advancement via replica acknowledgements

2. Leader election reliability:
- vote handling guards against stale terms
- candidate log freshness checks (`last_log_term`, `last_log_index`)
- one-vote-per-term behavior via `voted_for` tracking
- heartbeat acceptance updates leader term/role and commit progress

3. State reconciliation:
- replica progress tracking per node
- reconcile endpoint emits missing entries from requested index
- lag reporting by index delta or explicit lag value

4. Persistence scaffolding (SQLite metadata tables):
- `replication_log`
- `election_votes`
- `replication_reconcile_events`
- `ha_runtime_markers`

## S16 Resilience and Chaos Primitives

1. Automatic failover controller:
- `failover_mode=automatic` enables periodic timeout-based promotion checks.
- `POST /control/v1/failover/auto-check` supports force/simulated elapsed inputs.

2. Recovery timing instrumentation:
- `POST /control/v1/recovery/start` starts restore timing windows.
- `POST /control/v1/recovery/mark-restored` closes restore timing windows.

3. Chaos scenarios:
- `node_crash`: blocks non-chaos endpoints (`503`).
- `disk_full`: blocks write/control mutation paths (`507`).
- `partition_subset`: blocks heartbeat and replication-receive paths (`503`).

4. Operational resilience persistence:
- `ha_resilience_events` for failover/restore lifecycle timing.
- `ha_chaos_events` for chaos drill audits.

## S17 Backup and PITR Primitives

1. Snapshot lifecycle operations:
- create/list snapshots
- select snapshot for target timestamp
- restore and verify restore artifacts
- prune old snapshots by retention window

2. Snapshot catalog:
- persisted `backup_catalog.jsonl`
- snapshot metadata for path, note, size, integrity, and schema/chunk summary

3. CLI lifecycle:
- `sqlrite backup snapshot`
- `sqlrite backup list`
- `sqlrite backup restore`
- `sqlrite backup pitr-restore`
- `sqlrite backup prune`

## S18 Observability and SLO Primitives

1. Observability state:
- request totals and error classes
- SQL request/error counters
- SQL avg/max latency
- bounded recent trace buffer

2. Alerting/SLO surfaces:
- metric coverage map endpoint
- alert template discovery and simulation endpoint
- SLO report endpoint with availability/RPO targets and pass/fail state

3. Metrics export:
- request/error counters
- SQL latency gauges
- trace-buffer gauge
- alert simulation counter

## S19 DR Game-Day and Soak Gate

1. DR harness:
- `scripts/run-s19-dr-gameday.sh` orchestrates failover, backup/restore verification, chaos injection, observability reset, and soak loop.

2. Gate targets validated in harness:
- availability >= `99.95%` (`PD-G01`, `SD-16`)
- RPO <= `60s` (`PD-G02`)
- validated chaos scenarios (`PD-G03`):
  - `node_crash`
  - `disk_full`
  - `partition_subset`

3. Produced artifacts:
- `project_plan/reports/s19_dr_gameday.log`
- `project_plan/reports/s19_soak_slo_summary.json`

## SQL Semantics Parity

Server mode exposes `POST /v1/sql` so retrieval SQL works with the same embedded SQL surface:

1. vector helpers: `vector`, `vec_dims`, `vec_to_json`
2. distance functions: `l2_distance`, `cosine_distance`, `neg_inner_product`
3. retrieval functions: `embed`, `bm25_score`, `hybrid_score`
4. operator rewrite: `<->`, `<=>`, `<#>`

## Failure Modes and Fallback Behavior

Current S19 behavior:

1. readiness gates on storage integrity and HA leader visibility for replica-mode nodes
2. replicated events are checksummed and quorum-committed
3. vote/heartbeat flows enforce term monotonicity and log freshness checks
4. recovery endpoints support snapshot/verify/prune lifecycle
5. observability reset enables clean soak windows after planned chaos drills
6. SLO report surfaces pass/fail state for availability and RPO targets

Planned in follow-up sprints:

1. multi-node transport and automated replication fan-out between servers
2. longer-duration monthly soak automation and release-train reliability reporting

## Deployment References

1. Docker Compose reference: `/Users/jameskaranja/Developer/projects/SQLRight/deploy/ha/docker-compose.reference.yml`
2. Prometheus scrape config: `/Users/jameskaranja/Developer/projects/SQLRight/deploy/ha/prometheus.yml`
3. Kubernetes StatefulSet: `/Users/jameskaranja/Developer/projects/SQLRight/deploy/ha/k8s-statefulset.yaml`
4. Kubernetes service objects: `/Users/jameskaranja/Developer/projects/SQLRight/deploy/ha/k8s-service.yaml`

## S14-S19 Verification Summary

1. HA profile/state and replication log tests in `src/ha.rs`
2. control-plane, replication, failover, recovery, observability, and chaos endpoint tests in `src/server.rs`
3. protocol smoke and benchmark artifacts in `project_plan/reports/`
4. end-to-end build/test gates: `cargo test`
