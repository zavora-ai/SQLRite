#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

BIN="${BIN:-target/debug/sqlrite}"
DB_PATH="${DB_PATH:-project_plan/reports/s26_api_compat.db}"
BIND_ADDR="${BIND_ADDR:-127.0.0.1:8346}"
LOG_PATH="${LOG_PATH:-project_plan/reports/s26_api_compatibility.log}"
REPORT_PATH="${REPORT_PATH:-project_plan/reports/s26_api_compatibility_report.json}"
CURRENT_PATH="${CURRENT_PATH:-project_plan/reports/s26_api_current_manifest.json}"
FROZEN_MANIFEST="${FROZEN_MANIFEST:-docs/contracts/api_freeze_v1.json}"
PROTO_PATH="${PROTO_PATH:-proto/sqlrite/v1/query_service.proto}"
KEEP_DB="${KEEP_DB:-0}"

mkdir -p "$(dirname "$DB_PATH")" "$(dirname "$LOG_PATH")" "$(dirname "$REPORT_PATH")" "$(dirname "$CURRENT_PATH")"
rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm" "$LOG_PATH" "$REPORT_PATH" "$CURRENT_PATH" \
  /tmp/s26_help.txt /tmp/s26_openapi.json /tmp/s26_mcp_manifest.json /tmp/s26_server.log

echo "[build] cargo build --bin sqlrite" | tee -a "$LOG_PATH"
cargo build --bin sqlrite >/dev/null

echo "[seed-db] $DB_PATH" | tee -a "$LOG_PATH"
"$BIN" init --db "$DB_PATH" --seed-demo >/tmp/s26_init.log 2>&1

echo "[capture-cli-help]" | tee -a "$LOG_PATH"
"$BIN" --help > /tmp/s26_help.txt

echo "[capture-mcp-manifest]" | tee -a "$LOG_PATH"
"$BIN" mcp --db "$DB_PATH" --print-manifest > /tmp/s26_mcp_manifest.json

echo "[start-server] bind=$BIND_ADDR" | tee -a "$LOG_PATH"
"$BIN" serve --db "$DB_PATH" --bind "$BIND_ADDR" >/tmp/s26_server.log 2>&1 &
SERVER_PID=$!

cleanup() {
  if kill -0 "$SERVER_PID" >/dev/null 2>&1; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
    wait "$SERVER_PID" >/dev/null 2>&1 || true
  fi
  if [[ "$KEEP_DB" != "1" ]]; then
    rm -f "$DB_PATH" "$DB_PATH-wal" "$DB_PATH-shm"
  fi
}
trap cleanup EXIT

for _ in $(seq 1 120); do
  if curl -fsS "http://$BIND_ADDR/readyz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

if ! curl -fsS "http://$BIND_ADDR/readyz" >/dev/null 2>&1; then
  echo "server did not become ready" | tee -a "$LOG_PATH"
  tail -n 120 /tmp/s26_server.log | tee -a "$LOG_PATH" >/dev/null
  exit 1
fi

echo "[capture-openapi]" | tee -a "$LOG_PATH"
curl -fsS "http://$BIND_ADDR/v1/openapi.json" > /tmp/s26_openapi.json

echo "[build-current-manifest]" | tee -a "$LOG_PATH"
python3 - <<'PY' /tmp/s26_help.txt /tmp/s26_openapi.json /tmp/s26_mcp_manifest.json "$PROTO_PATH" "$CURRENT_PATH" | tee -a "$LOG_PATH"
import json
import pathlib
import re
import sys

help_text = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
openapi = json.load(open(sys.argv[2], "r", encoding="utf-8"))
mcp = json.load(open(sys.argv[3], "r", encoding="utf-8"))
proto_text = pathlib.Path(sys.argv[4]).read_text(encoding="utf-8")
output_path = pathlib.Path(sys.argv[5])

def parse_help_sections(text: str):
    commands = []
    envs = []

    section = None
    for raw in text.splitlines():
        line = raw.rstrip("\n")
        stripped = line.strip()
        if stripped == "commands:":
            section = "commands"
            continue
        if stripped == "env overrides:":
            section = "env"
            continue
        if stripped == "examples:":
            section = None
            continue

        if section is None:
            continue
        if not stripped:
            section = None
            continue

        if section == "commands":
            if line.startswith("  "):
                token = stripped.split()[0]
                if token and token not in commands:
                    commands.append(token)
        elif section == "env":
            if line.startswith("  "):
                token = stripped.split("=", 1)[0]
                if token and token not in envs:
                    envs.append(token)

    return commands, envs

commands, env_overrides = parse_help_sections(help_text)

http_methods = {"get", "post", "put", "patch", "delete", "head", "options"}
paths = {}
for path, methods in openapi.get("paths", {}).items():
    method_set = []
    if isinstance(methods, dict):
        for method in methods.keys():
            if method.lower() in http_methods:
                method_set.append(method.upper())
    paths[path] = sorted(set(method_set))

components = sorted(openapi.get("components", {}).get("schemas", {}).keys())

package_match = re.search(r"\bpackage\s+([a-zA-Z0-9_.]+)\s*;", proto_text)
service_match = re.search(r"\bservice\s+([a-zA-Z0-9_]+)\s*\{", proto_text)
methods = sorted(set(re.findall(r"\brpc\s+([a-zA-Z0-9_]+)\s*\(", proto_text)))

manifest = {
    "cli": {
        "command": "sqlrite",
        "commands": commands,
        "env_overrides": env_overrides,
    },
    "http_openapi": {
        "openapi_version": openapi.get("openapi"),
        "paths": paths,
        "components": components,
    },
    "grpc": {
        "package": package_match.group(1) if package_match else None,
        "service": service_match.group(1) if service_match else None,
        "methods": methods,
    },
    "mcp": {
        "manifest_name": mcp.get("name"),
        "auth_argument": mcp.get("auth", {}).get("argument"),
        "auth_type": mcp.get("auth", {}).get("type"),
        "transport": {
            "type": mcp.get("transport", {}).get("type"),
            "command": mcp.get("transport", {}).get("command"),
            "args": mcp.get("transport", {}).get("args", []),
        },
        "tools": sorted(
            [
                tool.get("name")
                for tool in mcp.get("tools", [])
                if isinstance(tool, dict) and tool.get("name")
            ]
        ),
    },
}

output_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
print(f"current_commands={len(commands)}")
print(f"current_env_overrides={len(env_overrides)}")
print(f"current_openapi_paths={len(paths)}")
print(f"current_mcp_tools={len(manifest['mcp']['tools'])}")
print(f"current_grpc_methods={len(methods)}")
PY

echo "[compat-assertions]" | tee -a "$LOG_PATH"
python3 - <<'PY' "$FROZEN_MANIFEST" "$CURRENT_PATH" "$REPORT_PATH" | tee -a "$LOG_PATH"
import json
import pathlib
import sys
import time

frozen = json.load(open(sys.argv[1], "r", encoding="utf-8"))
current = json.load(open(sys.argv[2], "r", encoding="utf-8"))
report_path = pathlib.Path(sys.argv[3])

failures = []
checks = {}

def record(name: str, passed: bool, detail: str):
    checks[name] = {"pass": bool(passed), "detail": detail}
    if not passed:
        failures.append({"check": name, "detail": detail})

required_commands = sorted(frozen["cli"]["required_commands"])
current_commands = sorted(current["cli"]["commands"])
missing_commands = sorted(set(required_commands) - set(current_commands))
record("cli_required_commands", len(missing_commands) == 0, f"missing={missing_commands}")

required_env = sorted(frozen["cli"]["required_env_overrides"])
current_env = sorted(current["cli"]["env_overrides"])
missing_env = sorted(set(required_env) - set(current_env))
record("cli_required_env_overrides", len(missing_env) == 0, f"missing={missing_env}")

expected_openapi_version = frozen["http_openapi"]["openapi_version"]
current_openapi_version = current["http_openapi"]["openapi_version"]
record(
    "openapi_version",
    expected_openapi_version == current_openapi_version,
    f"expected={expected_openapi_version} observed={current_openapi_version}",
)

required_paths = frozen["http_openapi"]["required_paths"]
current_paths = current["http_openapi"]["paths"]
missing_paths = []
method_mismatches = []
for path, methods in required_paths.items():
    observed = current_paths.get(path)
    if observed is None:
        missing_paths.append(path)
        continue
    missing_methods = sorted(set(methods) - set(observed))
    if missing_methods:
        method_mismatches.append({"path": path, "missing_methods": missing_methods, "observed": observed})
record("openapi_required_paths", len(missing_paths) == 0, f"missing={missing_paths}")
record("openapi_required_methods", len(method_mismatches) == 0, f"mismatches={method_mismatches}")

required_components = sorted(frozen["http_openapi"]["required_components"])
current_components = sorted(current["http_openapi"]["components"])
missing_components = sorted(set(required_components) - set(current_components))
record("openapi_required_components", len(missing_components) == 0, f"missing={missing_components}")

record(
    "grpc_package",
    frozen["grpc"]["package"] == current["grpc"]["package"],
    f"expected={frozen['grpc']['package']} observed={current['grpc']['package']}",
)
record(
    "grpc_service",
    frozen["grpc"]["service"] == current["grpc"]["service"],
    f"expected={frozen['grpc']['service']} observed={current['grpc']['service']}",
)
required_grpc_methods = sorted(frozen["grpc"]["required_methods"])
current_grpc_methods = sorted(current["grpc"]["methods"])
missing_grpc_methods = sorted(set(required_grpc_methods) - set(current_grpc_methods))
record("grpc_required_methods", len(missing_grpc_methods) == 0, f"missing={missing_grpc_methods}")

record(
    "mcp_manifest_name",
    frozen["mcp"]["manifest_name"] == current["mcp"]["manifest_name"],
    f"expected={frozen['mcp']['manifest_name']} observed={current['mcp']['manifest_name']}",
)
record(
    "mcp_auth_argument",
    frozen["mcp"]["auth_argument"] == current["mcp"]["auth_argument"],
    f"expected={frozen['mcp']['auth_argument']} observed={current['mcp']['auth_argument']}",
)
record(
    "mcp_auth_type",
    frozen["mcp"]["auth_type"] == current["mcp"]["auth_type"],
    f"expected={frozen['mcp']['auth_type']} observed={current['mcp']['auth_type']}",
)

for key in ["type", "command", "args"]:
    record(
        f"mcp_transport_{key}",
        frozen["mcp"]["transport"][key] == current["mcp"]["transport"][key],
        f"expected={frozen['mcp']['transport'][key]} observed={current['mcp']['transport'][key]}",
    )

required_tools = sorted(frozen["mcp"]["required_tools"])
current_tools = sorted(current["mcp"]["tools"])
missing_tools = sorted(set(required_tools) - set(current_tools))
record("mcp_required_tools", len(missing_tools) == 0, f"missing={missing_tools}")

report = {
    "generated_unix_ms": int(time.time() * 1000),
    "frozen_manifest": sys.argv[1],
    "current_manifest": sys.argv[2],
    "pass": len(failures) == 0,
    "checks": checks,
    "failures": failures,
    "summary": {
        "frozen_required_command_count": len(required_commands),
        "observed_command_count": len(current_commands),
        "frozen_required_path_count": len(required_paths),
        "observed_path_count": len(current_paths),
        "frozen_required_grpc_method_count": len(required_grpc_methods),
        "observed_grpc_method_count": len(current_grpc_methods),
        "frozen_required_tool_count": len(required_tools),
        "observed_tool_count": len(current_tools),
    },
}

report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
print(f"pass={report['pass']}")
print(f"failures={len(report['failures'])}")

if not report["pass"]:
    raise SystemExit("api compatibility assertions failed")
PY

echo "[s26-api-compat-suite-complete] report=$REPORT_PATH log=$LOG_PATH" | tee -a "$LOG_PATH"
