# SQLRite HA Control-Plane Runbook

Status: S14-S19 protocol runbook  
Date: March 1, 2026

## Pre-Flight

1. Initialize or identify a shared database path per node.
2. Confirm node IDs and addresses for all members.
3. Choose control token and distribute securely.
4. Use `--failover automatic` only when election timeout policy and recovery drills are validated.

## Start a Primary

```bash
sqlrite serve \
  --db /var/lib/sqlrite/sqlrite.db \
  --bind 0.0.0.0:8099 \
  --ha-role primary \
  --cluster-id prod-a \
  --node-id node-a \
  --advertise node-a.internal:8099 \
  --peer node-b.internal:8099 \
  --peer node-c.internal:8099 \
  --sync-ack-quorum 2 \
  --failover manual \
  --control-token "$SQLRITE_CONTROL_TOKEN"
```

## Start a Replica

```bash
sqlrite serve \
  --db /var/lib/sqlrite/sqlrite.db \
  --bind 0.0.0.0:8099 \
  --ha-role replica \
  --cluster-id prod-a \
  --node-id node-b \
  --advertise node-b.internal:8099 \
  --peer node-a.internal:8099 \
  --peer node-c.internal:8099 \
  --sync-ack-quorum 2 \
  --failover manual \
  --control-token "$SQLRITE_CONTROL_TOKEN"
```

## Health and Readiness

```bash
curl -fsS http://127.0.0.1:8099/healthz | jq
curl -fsS http://127.0.0.1:8099/readyz | jq
curl -fsS http://127.0.0.1:8099/metrics
```

## Inspect Control Plane

```bash
curl -fsS http://127.0.0.1:8099/control/v1/profile | jq
curl -fsS http://127.0.0.1:8099/control/v1/state | jq
curl -fsS http://127.0.0.1:8099/control/v1/peers | jq
```

## Manual Failover Drill

1. Mark failover start:

```bash
curl -fsS -X POST \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  http://127.0.0.1:8099/control/v1/failover/start | jq
```

2. Promote candidate node:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"leader_id":"node-b"}' \
  http://127.0.0.1:8099/control/v1/failover/promote | jq
```

3. Step down previous primary:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"leader_id":"node-b"}' \
  http://127.0.0.1:8099/control/v1/failover/step-down | jq
```

## Automatic Failover Drill (S16)

1. Inspect failover status:

```bash
curl -fsS http://127.0.0.1:8099/control/v1/failover/status | jq
```

2. Trigger a controller evaluation without waiting for wall-clock timeout:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"simulate_elapsed_ms":5000,"reason":"drill_timeout"}' \
  http://127.0.0.1:8099/control/v1/failover/auto-check | jq
```

3. Confirm resilience counters updated:

```bash
curl -fsS http://127.0.0.1:8099/control/v1/resilience | jq
```

## Recovery Marker (Backup Restore/PITR Drill)

Start recovery timing window:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"note":"restore_drill_start"}' \
  http://127.0.0.1:8099/control/v1/recovery/start | jq
```

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"backup_artifact":"/var/backups/sqlrite-2026-03-01.db","note":"restore drill"}' \
  http://127.0.0.1:8099/control/v1/recovery/mark-restored | jq
```

Create and inspect snapshots:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"note":"scheduled_snapshot"}' \
  http://127.0.0.1:8099/control/v1/recovery/snapshot | jq

curl -fsS \
  "http://127.0.0.1:8099/control/v1/recovery/snapshots?limit=20" | jq
```

Verify restore from latest/selected snapshot:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"keep_artifact":false,"note":"verify_restore_drill"}' \
  http://127.0.0.1:8099/control/v1/recovery/verify-restore | jq
```

Prune old snapshots by retention:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"retention_seconds":86400}' \
  http://127.0.0.1:8099/control/v1/recovery/prune-snapshots | jq
```

## Chaos Harness Drill (S16)

Inject partial-partition fault (blocks heartbeat/replication-receive paths):

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"scenario":"partition_subset","note":"drill_partition"}' \
  http://127.0.0.1:8099/control/v1/chaos/inject | jq
```

Inject disk-full fault (blocks write/control mutation paths with `507`):

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"scenario":"disk_full","note":"drill_disk_full"}' \
  http://127.0.0.1:8099/control/v1/chaos/inject | jq
```

Inject node-crash fault (returns `503` on non-chaos endpoints):

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"scenario":"node_crash","note":"drill_node_crash"}' \
  http://127.0.0.1:8099/control/v1/chaos/inject | jq
```

Inspect and clear faults:

```bash
curl -fsS http://127.0.0.1:8099/control/v1/chaos/status | jq

curl -fsS -X POST \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  http://127.0.0.1:8099/control/v1/chaos/clear | jq
```

## Observability and SLO Operations (S18-S19)

Inspect metrics map and recent traces:

```bash
curl -fsS http://127.0.0.1:8099/control/v1/observability/metrics-map | jq
curl -fsS "http://127.0.0.1:8099/control/v1/traces/recent?limit=25" | jq
```

Inspect alert templates and run a simulation:

```bash
curl -fsS http://127.0.0.1:8099/control/v1/alerts/templates | jq

curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"sql_error_rate":0.2,"sql_avg_latency_ms":75.0,"replication_lag_ms":1000}' \
  http://127.0.0.1:8099/control/v1/alerts/simulate | jq
```

Reset observability window before soak and validate SLOs:

```bash
curl -fsS -X POST \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  http://127.0.0.1:8099/control/v1/observability/reset | jq

curl -fsS http://127.0.0.1:8099/control/v1/slo/report | jq
```

Run full DR game-day harness:

```bash
cargo build --bin sqlrite
scripts/run-s19-dr-gameday.sh
```

Expected artifacts:

1. `project_plan/reports/s19_dr_gameday.log`
2. `project_plan/reports/s19_soak_slo_summary.json`

## Replication Log Protocol Drill

1. Append replicated event on primary:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"operation":"ingest_chunk","payload":{"chunk_id":"c1","doc_id":"d1"}}' \
  http://127.0.0.1:8099/control/v1/replication/append | jq
```

2. ACK replicated index from a replica:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"node_id":"node-b","index":1}' \
  http://127.0.0.1:8099/control/v1/replication/ack | jq
```

3. Inspect replication log:

```bash
curl -fsS "http://127.0.0.1:8099/control/v1/replication/log?from=1&limit=100" | jq
```

4. Request reconciliation for lagging replica:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"node_id":"node-c","last_applied_index":0}' \
  http://127.0.0.1:8099/control/v1/replication/reconcile | jq
```

## Election Reliability Drill

1. Request vote:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"term":2,"candidate_id":"node-b","candidate_last_log_index":1,"candidate_last_log_term":1}' \
  http://127.0.0.1:8099/control/v1/election/request-vote | jq
```

2. Send heartbeat from elected leader:

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -H "x-sqlrite-control-token: $SQLRITE_CONTROL_TOKEN" \
  -d '{"term":2,"leader_id":"node-b","commit_index":1,"leader_last_log_index":1}' \
  http://127.0.0.1:8099/control/v1/election/heartbeat | jq
```

## SQL Endpoint Usage

```bash
curl -fsS -X POST \
  -H "content-type: application/json" \
  -d '{"statement":"SELECT id, embedding <=> vector(\"1,0,0\") AS cosine_distance FROM chunks ORDER BY cosine_distance ASC, id ASC LIMIT 3;"}' \
  http://127.0.0.1:8099/v1/sql | jq
```

## Troubleshooting

1. `401 unauthorized` on control endpoint:
- validate `x-sqlrite-control-token` and server `--control-token` alignment.

2. `503` on `/readyz` for replica:
- check control-plane leader state (`/control/v1/state`) and failover role transitions.
 - check `GET /control/v1/chaos/status` for active injected faults.

3. SQL endpoint disabled:
- remove `--disable-sql-endpoint` or route SQL via local CLI.

4. Configuration validation failure at startup:
- ensure HA role and replication settings are consistent (e.g., quorum <= cluster size).

5. `507` on control write endpoints:
- active `disk_full` chaos scenario is blocking writes; clear via `/control/v1/chaos/clear`.

6. availability appears low immediately after game-day:
- clear measurement window via `POST /control/v1/observability/reset` before soak/SLO validation.
