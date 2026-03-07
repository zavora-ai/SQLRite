# API Compatibility Freeze Runbook (Sprint 26)

Objective: enforce the frozen API contract for v1 surfaces (CLI, OpenAPI, gRPC, MCP) and catch breaking drift before merge/release.

## Frozen Contract Source

- `/Users/jameskaranja/Developer/projects/SQLRight/docs/contracts/api_freeze_v1.json`

## Compatibility Suite

Run from repository root:

```bash
bash scripts/run-s26-api-compat-suite.sh
```

The suite performs:

1. Build SQLRite binary.
2. Capture current CLI command/env contract from `sqlrite --help`.
3. Capture current OpenAPI document from running server.
4. Capture current MCP manifest.
5. Parse current gRPC service contract from `proto/sqlrite/v1/query_service.proto`.
6. Compare observed contract to frozen requirements.

## Artifacts

- `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s26_api_compatibility.log`
- `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s26_api_current_manifest.json`
- `/Users/jameskaranja/Developer/projects/SQLRight/project_plan/reports/s26_api_compatibility_report.json`

`pass=true` in `s26_api_compatibility_report.json` is required for sprint gate success.

## CI Validation

- Workflow: `/Users/jameskaranja/Developer/projects/SQLRight/.github/workflows/api-compatibility.yml`

CI runs the same suite and uploads the same artifacts.

## Troubleshooting

1. Read `s26_api_compatibility_report.json` `failures[]` for exact drift.
2. If drift is unintended, restore compatibility in code.
3. If drift is intended and approved as a breaking change:
- create a new freeze manifest file (do not silently mutate `api_freeze_v1.json`)
- add compatibility note in `CHANGELOG.md`
- update migration documentation and examples

## Edge/WASM linkage

This runbook validates the API surfaces used by the edge-read story:

- `/v1/query`
- `/v1/sql`
- `/v1/openapi.json`
- gRPC QueryService bridge routes
- MCP tool contract

Reference design: `/Users/jameskaranja/Developer/projects/SQLRight/docs/rfcs/0002-edge-read-query-wasm.md`
