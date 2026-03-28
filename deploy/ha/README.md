# SQLRite HA Deployment References

This folder contains reference manifests for high-availability SQLRite deployments.

Use these files as starting points for development, evaluation, and operator adaptation. They are examples, not one-click production manifests.

## Files

1. `docker-compose.reference.yml`
- three SQLRite nodes (`primary` plus two `replica` nodes)
- Prometheus for local metrics scraping
- useful for local HA smoke testing

2. `prometheus.yml`
- scrape configuration for each SQLRite node `/metrics` endpoint

3. `k8s-service.yaml`
- headless service for peer discovery
- cluster service for API access

4. `k8s-statefulset.yaml`
- StatefulSet layout with per-pod persistent storage
- pod `0` starts as `primary`; pods `1-2` start as `replica`

## Docker Compose Quick Start

```bash
docker build -t sqlrite:local .
export SQLRITE_CONTROL_TOKEN=dev-token
cd deploy/ha
docker compose -f docker-compose.reference.yml up -d
```

Check the cluster state:

```bash
curl -fsS http://127.0.0.1:8099/readyz
curl -fsS http://127.0.0.1:8099/control/v1/state
curl -fsS -X POST \
  -H "x-sqlrite-control-token: ${SQLRITE_CONTROL_TOKEN}" \
  http://127.0.0.1:8099/control/v1/failover/start
```

## Before production use

Validate these areas in your own environment:

- persistent volume behavior
- token and secret management
- backup and restore flows
- metrics, alerting, and log collection
- failover and recovery drills
