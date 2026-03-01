# SQLRite HA Deployment References

This folder provides S14 reference manifests for HA scaffolding.

## Files

1. `docker-compose.reference.yml`:
- 3 SQLRite nodes (`primary` + 2 `replica`) and Prometheus
- local development and smoke validation

2. `prometheus.yml`:
- scrape jobs for each SQLRite node `/metrics` endpoint

3. `k8s-service.yaml`:
- headless service for StatefulSet peer discovery
- cluster service for API access

4. `k8s-statefulset.yaml`:
- 3-node StatefulSet with per-pod persistent storage
- pod 0 starts as `primary`; pods 1-2 start as `replica`

## Docker Compose Quick Start

```bash
docker build -t sqlrite:local .
export SQLRITE_CONTROL_TOKEN=dev-token
cd deploy/ha
docker compose -f docker-compose.reference.yml up -d
```

Check endpoints:

```bash
curl -fsS http://127.0.0.1:8099/readyz | jq
curl -fsS http://127.0.0.1:8099/control/v1/state | jq
curl -fsS -X POST \
  -H "x-sqlrite-control-token: ${SQLRITE_CONTROL_TOKEN}" \
  http://127.0.0.1:8099/control/v1/failover/start | jq
```
