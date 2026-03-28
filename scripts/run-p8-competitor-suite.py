#!/usr/bin/env python3
import argparse
import atexit
import csv
import http.client
import io
import json
import math
import random
import shutil
import sqlite3
import struct
import subprocess
import sys
import time
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Callable


ROOT = Path("/Users/jameskaranja/Developer/projects/SQLRight")
REPORT_DIR = ROOT / "project_plan" / "reports"
SQLRITE_PORT = 8099
QDRANT_PORT = 6333
PG_PORT = 55432
QDRANT_CONTAINER = "sqlrite-qdrant-p8-bench"
PG_CONTAINER = "sqlrite-pgvector-p8-bench"
SENTINEL = "__SQLRITE_DONE__"


@dataclass
class QuerySpec:
    vector: list[float]
    tenant: str
    ground_truth_ids: list[int]


def run(
    cmd: list[str],
    *,
    cwd: Path | None = None,
    check: bool = True,
    capture: bool = False,
    input_text: str | None = None,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=str(cwd) if cwd else None,
        check=check,
        text=True,
        capture_output=capture,
        input=input_text,
    )


def ensure_docker_available() -> None:
    result = run(["docker", "info"], check=False, capture=True)
    if result.returncode == 0:
        return
    stderr = (result.stderr or "").strip()
    stdout = (result.stdout or "").strip()
    detail = stderr or stdout or "docker info failed"
    raise RuntimeError(
        "docker daemon is unavailable; start Docker Desktop or another local Docker daemon before "
        f"running the competitor suite ({detail})"
    )


def wait_for_http(url: str, timeout_s: float = 60.0) -> None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=2.0) as resp:
                if 200 <= resp.status < 300:
                    return
        except Exception:
            time.sleep(0.5)
    raise RuntimeError(f"timed out waiting for {url}")


def http_json(method: str, url: str, payload: dict | None = None, timeout_s: float = 60.0) -> dict:
    data = None
    headers = {}
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
        headers["content-type"] = "application/json"
    request = urllib.request.Request(url, data=data, headers=headers, method=method)
    with urllib.request.urlopen(request, timeout=timeout_s) as response:
        return json.loads(response.read().decode("utf-8"))


def normalize(values: list[float]) -> list[float]:
    norm = math.sqrt(sum(value * value for value in values))
    if norm == 0.0:
        return values[:]
    return [value / norm for value in values]


def dot(left: list[float], right: list[float]) -> float:
    return sum(l * r for l, r in zip(left, right, strict=True))


def percentile_ms(values_seconds: list[float], percentile: float) -> float:
    if not values_seconds:
        return 0.0
    sorted_values = sorted(values_seconds)
    rank = (len(sorted_values) - 1) * (percentile / 100.0)
    lower = math.floor(rank)
    upper = math.ceil(rank)
    if lower == upper:
        return sorted_values[lower] * 1000.0
    weight = rank - lower
    value = sorted_values[lower] * (1.0 - weight) + sorted_values[upper] * weight
    return value * 1000.0


def generate_dataset(
    corpus_size: int,
    embedding_dim: int,
    query_count: int,
    tenants: int,
    top_k: int,
    seed: int,
) -> tuple[list[dict], list[QuerySpec]]:
    rng = random.Random(seed)
    records: list[dict] = []
    for idx in range(corpus_size):
        embedding = normalize([rng.gauss(0.0, 1.0) for _ in range(embedding_dim)])
        tenant = f"tenant-{idx % tenants}"
        records.append(
            {
                "id": idx + 1,
                "chunk_id": f"c{idx + 1:06d}",
                "doc_id": f"doc-{idx + 1:06d}",
                "tenant": tenant,
                "embedding": embedding,
                "content": f"synthetic content {idx + 1}",
            }
        )

    query_indices = rng.sample(range(corpus_size), query_count)
    queries: list[QuerySpec] = []
    for index in query_indices:
        base = records[index]
        noise = [rng.gauss(0.0, 0.01) for _ in range(embedding_dim)]
        query_vector = normalize(
            [value + delta for value, delta in zip(base["embedding"], noise, strict=True)]
        )
        tenant = base["tenant"]
        tenant_records = [record for record in records if record["tenant"] == tenant]
        scored = sorted(
            ((record["id"], dot(record["embedding"], query_vector)) for record in tenant_records),
            key=lambda item: item[1],
            reverse=True,
        )
        ground_truth = [record_id for record_id, _ in scored[:top_k]]
        queries.append(QuerySpec(vector=query_vector, tenant=tenant, ground_truth_ids=ground_truth))

    return records, queries


def sqlrite_command_prefix() -> list[str]:
    binary = ROOT / "target" / "debug" / "sqlrite"
    if binary.exists():
        return [str(binary)]
    return ["env", "RUSTC_WRAPPER=", "cargo", "run", "--bin", "sqlrite", "--"]


def init_sqlrite_db(sqlrite_cmd: list[str], db_path: Path) -> None:
    if db_path.exists():
        db_path.unlink()
    run([*sqlrite_cmd, "init", "--db", str(db_path)], cwd=ROOT)


def populate_sqlrite_db(db_path: Path, records: list[dict]) -> float:
    started = time.perf_counter()
    connection = sqlite3.connect(db_path)
    try:
        docs = [(record["doc_id"], f"synthetic/{record['doc_id']}.md") for record in records]
        connection.executemany(
            "INSERT OR REPLACE INTO documents (id, source, metadata) VALUES (?, ?, '{}')",
            docs,
        )

        chunk_rows = []
        for record in records:
            metadata = json.dumps({"tenant": record["tenant"]})
            embedding_blob = struct.pack("<" + "f" * len(record["embedding"]), *record["embedding"])
            chunk_rows.append(
                (
                    record["chunk_id"],
                    record["doc_id"],
                    record["content"],
                    metadata,
                    embedding_blob,
                    len(record["embedding"]),
                )
            )
        connection.executemany(
            "INSERT OR REPLACE INTO chunks (id, doc_id, content, metadata, embedding, embedding_dim) VALUES (?, ?, ?, ?, ?, ?)",
            chunk_rows,
        )
        connection.commit()
    finally:
        connection.close()
    return time.perf_counter() - started


def start_sqlrite_server(
    sqlrite_cmd: list[str], db_path: Path, index_mode: str, log_path: Path
) -> subprocess.Popen[str]:
    log_handle = open(log_path, "w")
    process = subprocess.Popen(
        [
            *sqlrite_cmd,
            "serve",
            "--db",
            str(db_path),
            "--bind",
            f"127.0.0.1:{SQLRITE_PORT}",
            "--index-mode",
            index_mode,
        ],
        cwd=ROOT,
        stdout=log_handle,
        stderr=subprocess.STDOUT,
        text=True,
    )
    wait_for_http(f"http://127.0.0.1:{SQLRITE_PORT}/readyz", timeout_s=120.0)
    return process


def stop_process(process: subprocess.Popen[str] | None) -> None:
    if process is None or process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=10.0)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5.0)


class JsonHttpClient:
    def __init__(self, host: str, port: int, timeout_s: float = 30.0):
        self.host = host
        self.port = port
        self.timeout_s = timeout_s
        self.connection: http.client.HTTPConnection | None = None

    def request_json(self, method: str, path: str, payload: dict | None = None) -> dict:
        if self.connection is None:
            self.connection = http.client.HTTPConnection(self.host, self.port, timeout=self.timeout_s)
        body = None
        headers = {}
        if payload is not None:
            body = json.dumps(payload).encode("utf-8")
            headers["content-type"] = "application/json"
        try:
            self.connection.request(method, path, body=body, headers=headers)
            response = self.connection.getresponse()
            payload_bytes = response.read()
            if response.status >= 400:
                raise RuntimeError(
                    f"HTTP {response.status}: {payload_bytes.decode('utf-8', errors='replace')}"
                )
            return json.loads(payload_bytes.decode("utf-8"))
        except Exception:
            self.close()
            raise

    def close(self) -> None:
        if self.connection is not None:
            try:
                self.connection.close()
            finally:
                self.connection = None


def sqlrite_query(
    client: JsonHttpClient,
    query: QuerySpec,
    top_k: int,
    candidate_limit: int,
    query_profile: str,
) -> list[int]:
    response = client.request_json(
        "POST",
        "/v1/query",
        {
            "query_embedding": query.vector,
            "top_k": top_k,
            "alpha": 0.0,
            "candidate_limit": candidate_limit,
            "query_profile": query_profile,
            "metadata_filters": {"tenant": query.tenant},
        },
    )
    output: list[int] = []
    for row in response.get("rows", []):
        chunk_id = row.get("chunk_id") or row.get("id")
        if isinstance(chunk_id, str) and chunk_id.startswith("c"):
            output.append(int(chunk_id[1:]))
        elif chunk_id is not None:
            output.append(int(chunk_id))
    return output


def docker_rm(name: str) -> None:
    subprocess.run(["docker", "rm", "-f", name], check=False, capture_output=True, text=True)


def start_qdrant() -> None:
    docker_rm(QDRANT_CONTAINER)
    run(
        [
            "docker",
            "run",
            "-d",
            "--rm",
            "--name",
            QDRANT_CONTAINER,
            "-p",
            f"{QDRANT_PORT}:6333",
            "qdrant/qdrant:latest",
        ]
    )
    wait_for_http(f"http://127.0.0.1:{QDRANT_PORT}/readyz", timeout_s=120.0)


def qdrant_create_collection(embedding_dim: int) -> None:
    http_json(
        "PUT",
        f"http://127.0.0.1:{QDRANT_PORT}/collections/sqlrite_bench",
        {"vectors": {"size": embedding_dim, "distance": "Cosine"}},
        timeout_s=120.0,
    )
    http_json(
        "PUT",
        f"http://127.0.0.1:{QDRANT_PORT}/collections/sqlrite_bench/index",
        {"field_name": "tenant", "field_schema": "keyword"},
        timeout_s=120.0,
    )


def qdrant_upload(records: list[dict], batch_size: int = 512) -> float:
    started = time.perf_counter()
    for offset in range(0, len(records), batch_size):
        batch = records[offset : offset + batch_size]
        points = [
            {"id": record["id"], "vector": record["embedding"], "payload": {"tenant": record["tenant"]}}
            for record in batch
        ]
        http_json(
            "PUT",
            f"http://127.0.0.1:{QDRANT_PORT}/collections/sqlrite_bench/points?wait=true",
            {"points": points},
            timeout_s=120.0,
        )
    return time.perf_counter() - started


def qdrant_query(client: JsonHttpClient, query: QuerySpec, top_k: int, exact: bool) -> list[int]:
    response = client.request_json(
        "POST",
        "/collections/sqlrite_bench/points/query",
        {
            "query": query.vector,
            "filter": {"must": [{"key": "tenant", "match": {"value": query.tenant}}]},
            "params": {"exact": exact},
            "limit": top_k,
        },
    )
    result = response.get("result", {})
    points = result.get("points") if isinstance(result, dict) else result
    if points is None:
        points = []
    return [int(point["id"]) for point in points]


def docker_exec(container: str, args: list[str], *, input_text: str | None = None, capture: bool = False) -> subprocess.CompletedProcess[str]:
    return run(["docker", "exec", "-i", container, *args], input_text=input_text, capture=capture)


def start_pgvector() -> None:
    docker_rm(PG_CONTAINER)
    run(
        [
            "docker",
            "run",
            "-d",
            "--rm",
            "--name",
            PG_CONTAINER,
            "-e",
            "POSTGRES_PASSWORD=postgres",
            "-e",
            "POSTGRES_DB=sqlritebench",
            "-p",
            f"{PG_PORT}:5432",
            "pgvector/pgvector:pg17",
        ]
    )
    deadline = time.time() + 120.0
    while time.time() < deadline:
        ready = subprocess.run(
            [
                "docker",
                "exec",
                PG_CONTAINER,
                "pg_isready",
                "-U",
                "postgres",
                "-d",
                "sqlritebench",
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        if ready.returncode == 0:
            return
        time.sleep(1.0)
    raise RuntimeError("timed out waiting for pgvector")


def pg_setup(embedding_dim: int) -> None:
    docker_exec(
        PG_CONTAINER,
        [
            "psql",
            "-U",
            "postgres",
            "-d",
            "sqlritebench",
            "-v",
            "ON_ERROR_STOP=1",
            "-c",
            "CREATE EXTENSION IF NOT EXISTS vector",
            "-c",
            "DROP TABLE IF EXISTS items",
            "-c",
            f"CREATE TABLE items (id BIGINT PRIMARY KEY, tenant TEXT NOT NULL, embedding vector({embedding_dim}) NOT NULL)",
        ],
    )


def pg_upload(records: list[dict]) -> float:
    started = time.perf_counter()
    buffer = io.StringIO()
    writer = csv.writer(buffer)
    for record in records:
        embedding = "[" + ",".join(f"{value:.7f}" for value in record["embedding"]) + "]"
        writer.writerow((record["id"], record["tenant"], embedding))

    docker_exec(
        PG_CONTAINER,
        [
            "psql",
            "-U",
            "postgres",
            "-d",
            "sqlritebench",
            "-v",
            "ON_ERROR_STOP=1",
            "-c",
            "COPY items (id, tenant, embedding) FROM STDIN WITH (FORMAT csv)",
        ],
        input_text=buffer.getvalue(),
    )
    return time.perf_counter() - started


def pg_create_hnsw_index() -> None:
    docker_exec(
        PG_CONTAINER,
        [
            "psql",
            "-U",
            "postgres",
            "-d",
            "sqlritebench",
            "-v",
            "ON_ERROR_STOP=1",
            "-c",
            "CREATE INDEX IF NOT EXISTS items_embedding_hnsw ON items USING hnsw (embedding vector_cosine_ops)",
            "-c",
            "ANALYZE items",
        ],
    )


def sql_quote(value: str) -> str:
    return "'" + value.replace("'", "''") + "'"


class PgQuerySession:
    def __init__(self, approximate: bool):
        self.process = subprocess.Popen(
            [
                "docker",
                "exec",
                "-i",
                PG_CONTAINER,
                "psql",
                "-U",
                "postgres",
                "-d",
                "sqlritebench",
                "-X",
                "-q",
                "-A",
                "-t",
                "-v",
                "ON_ERROR_STOP=1",
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1,
        )
        if self.process.stdin is None or self.process.stdout is None:
            raise RuntimeError("failed to start persistent psql session")

        if approximate:
            self.execute("SET enable_seqscan = off")
        else:
            self.execute("SET enable_indexscan = off")
            self.execute("SET enable_bitmapscan = off")

    def execute(self, sql: str) -> list[str]:
        if self.process.stdin is None or self.process.stdout is None:
            raise RuntimeError("psql session is not available")
        self.process.stdin.write(sql.strip() + ";\n")
        self.process.stdin.write(f"\\echo {SENTINEL}\n")
        self.process.stdin.flush()
        lines: list[str] = []
        while True:
            line = self.process.stdout.readline()
            if line == "":
                stderr = ""
                if self.process.stderr is not None:
                    stderr = self.process.stderr.read()
                raise RuntimeError(f"psql session ended unexpectedly: {stderr.strip()}")
            stripped = line.rstrip("\n")
            if stripped == SENTINEL:
                break
            if stripped:
                lines.append(stripped)
        return lines

    def query_ids(self, query: QuerySpec, top_k: int) -> list[int]:
        embedding = "[" + ",".join(f"{value:.7f}" for value in query.vector) + "]"
        rows = self.execute(
            f"SELECT id FROM items WHERE tenant = {sql_quote(query.tenant)} "
            f"ORDER BY embedding <=> {sql_quote(embedding)}::vector LIMIT {top_k}"
        )
        return [int(row) for row in rows if row]

    def close(self) -> None:
        if self.process.stdin is not None:
            try:
                self.process.stdin.close()
            except Exception:
                pass
        try:
            self.process.terminate()
            self.process.wait(timeout=5.0)
        except Exception:
            self.process.kill()
            self.process.wait(timeout=5.0)


def compute_metrics(results: list[list[int]], queries: list[QuerySpec], top_k: int) -> dict:
    if not queries:
        return {"top1_hit_rate": 0.0, "recall_at_k": 0.0}
    top1_hits = 0
    recall_total = 0.0
    for returned, query in zip(results, queries, strict=True):
        truth = query.ground_truth_ids[:top_k]
        if returned and truth and returned[0] == truth[0]:
            top1_hits += 1
        overlap = len(set(returned[:top_k]) & set(truth))
        recall_total += overlap / len(truth)
    return {
        "top1_hit_rate": top1_hits / len(queries),
        "recall_at_k": recall_total / len(queries),
    }


def benchmark(
    name: str,
    query_fn: Callable[[QuerySpec], list[int]],
    queries: list[QuerySpec],
    warmup: int,
    top_k: int,
) -> dict:
    for query in queries[:warmup]:
        query_fn(query)

    results: list[list[int]] = []
    latencies: list[float] = []
    started = time.perf_counter()
    for query in queries[warmup:]:
        query_started = time.perf_counter()
        results.append(query_fn(query))
        latencies.append(time.perf_counter() - query_started)
    elapsed = time.perf_counter() - started
    metrics = compute_metrics(results, queries[warmup:], top_k)
    return {
        "system": name,
        "qps": len(results) / elapsed if elapsed > 0 else 0.0,
        "p50_ms": percentile_ms(latencies, 50.0),
        "p95_ms": percentile_ms(latencies, 95.0),
        **metrics,
    }


def write_markdown(path: Path, report: dict) -> None:
    lines = [
        "# P8 Competitor Comparison Report",
        "",
        "This report compares SQLRite with external systems on the same deterministic filtered cosine workload.",
        "",
        "## Workload",
        "",
        f"- corpus size: `{report['workload']['corpus_size']}`",
        f"- query count: `{report['workload']['query_count']}`",
        f"- embedding dimension: `{report['workload']['embedding_dim']}`",
        f"- tenants: `{report['workload']['tenants']}`",
        f"- top-k: `{report['workload']['top_k']}`",
        f"- warmup: `{report['workload']['warmup']}`",
        f"- seed: `{report['workload']['seed']}`",
        "",
    ]
    for scenario_name in ("exact_filtered_cosine", "approx_filtered_cosine"):
        scenario = report["scenarios"][scenario_name]
        lines.extend(
            [
                f"## {scenario_name.replace('_', ' ').title()}",
                "",
                "| System | QPS | p50 ms | p95 ms | Top1 hit | Recall@k | Setup s |",
                "|---|---:|---:|---:|---:|---:|---:|",
            ]
        )
        for row in scenario["results"]:
            lines.append(
                f"| {row['system']} | {row['qps']:.2f} | {row['p50_ms']:.3f} | {row['p95_ms']:.3f} | {row['top1_hit_rate']:.4f} | {row['recall_at_k']:.4f} | {row['setup_seconds']:.3f} |"
            )
        lines.append("")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus-size", type=int, default=5000)
    parser.add_argument("--query-count", type=int, default=120)
    parser.add_argument("--embedding-dim", type=int, default=64)
    parser.add_argument("--tenants", type=int, default=8)
    parser.add_argument("--top-k", type=int, default=10)
    parser.add_argument("--warmup", type=int, default=16)
    parser.add_argument("--seed", type=int, default=20260328)
    parser.add_argument(
        "--output",
        type=Path,
        default=REPORT_DIR / "p8_competitor_comparison.json",
    )
    parser.add_argument(
        "--output-md",
        type=Path,
        default=REPORT_DIR / "P8_competitor_comparison.md",
    )
    args = parser.parse_args()

    ensure_docker_available()

    sqlrite_cmd = sqlrite_command_prefix()
    records, queries = generate_dataset(
        args.corpus_size,
        args.embedding_dim,
        args.query_count,
        args.tenants,
        args.top_k,
        args.seed,
    )

    temp_dir = Path("/tmp/sqlrite_p8_competitor")
    shutil.rmtree(temp_dir, ignore_errors=True)
    temp_dir.mkdir(parents=True, exist_ok=True)
    db_path = temp_dir / "sqlrite_p8_competitor.db"

    sqlrite_process = None
    try:
        init_sqlrite_db(sqlrite_cmd, db_path)
        sqlrite_setup_seconds = populate_sqlrite_db(db_path, records)

        start_qdrant()
        atexit.register(lambda: docker_rm(QDRANT_CONTAINER))
        qdrant_create_collection(args.embedding_dim)
        qdrant_setup_seconds = qdrant_upload(records)

        start_pgvector()
        atexit.register(lambda: docker_rm(PG_CONTAINER))
        pg_setup(args.embedding_dim)
        pg_setup_seconds = pg_upload(records)

        report = {
            "generated_at_unix": int(time.time()),
            "workload": {
                "corpus_size": args.corpus_size,
                "query_count": args.query_count,
                "embedding_dim": args.embedding_dim,
                "tenants": args.tenants,
                "top_k": args.top_k,
                "warmup": args.warmup,
                "seed": args.seed,
                "metric": "cosine",
                "filter": "tenant exact match",
            },
            "scenarios": {},
        }

        sqlrite_process = start_sqlrite_server(
            sqlrite_cmd,
            db_path,
            "brute_force",
            REPORT_DIR / "p8_competitor_sqlrite_bruteforce.log",
        )
        sqlrite_client = JsonHttpClient("127.0.0.1", SQLRITE_PORT)
        qdrant_client = JsonHttpClient("127.0.0.1", QDRANT_PORT)
        pg_exact = PgQuerySession(approximate=False)

        exact_results = [
            benchmark(
                "SQLRite brute_force",
                lambda query: sqlrite_query(
                    sqlrite_client, query, args.top_k, args.corpus_size, "recall"
                ),
                queries,
                args.warmup,
                args.top_k,
            ),
            benchmark(
                "Qdrant exact",
                lambda query: qdrant_query(qdrant_client, query, args.top_k, True),
                queries,
                args.warmup,
                args.top_k,
            ),
            benchmark(
                "pgvector exact",
                lambda query: pg_exact.query_ids(query, args.top_k),
                queries,
                args.warmup,
                args.top_k,
            ),
        ]
        sqlrite_client.close()
        qdrant_client.close()
        pg_exact.close()
        stop_process(sqlrite_process)
        sqlrite_process = None
        exact_results[0]["setup_seconds"] = sqlrite_setup_seconds
        exact_results[1]["setup_seconds"] = qdrant_setup_seconds
        exact_results[2]["setup_seconds"] = pg_setup_seconds
        report["scenarios"]["exact_filtered_cosine"] = {"results": exact_results}

        pg_create_hnsw_index()
        sqlrite_process = start_sqlrite_server(
            sqlrite_cmd,
            db_path,
            "hnsw_baseline",
            REPORT_DIR / "p8_competitor_sqlrite_hnsw.log",
        )
        sqlrite_client = JsonHttpClient("127.0.0.1", SQLRITE_PORT)
        qdrant_client = JsonHttpClient("127.0.0.1", QDRANT_PORT)
        pg_hnsw = PgQuerySession(approximate=True)
        approx_results = [
            benchmark(
                "SQLRite hnsw_baseline",
                lambda query: sqlrite_query(
                    sqlrite_client,
                    query,
                    args.top_k,
                    min(args.corpus_size, 500),
                    "balanced",
                ),
                queries,
                args.warmup,
                args.top_k,
            ),
            benchmark(
                "Qdrant HNSW",
                lambda query: qdrant_query(qdrant_client, query, args.top_k, False),
                queries,
                args.warmup,
                args.top_k,
            ),
            benchmark(
                "pgvector HNSW",
                lambda query: pg_hnsw.query_ids(query, args.top_k),
                queries,
                args.warmup,
                args.top_k,
            ),
        ]
        sqlrite_client.close()
        qdrant_client.close()
        pg_hnsw.close()
        stop_process(sqlrite_process)
        sqlrite_process = None
        approx_results[0]["setup_seconds"] = sqlrite_setup_seconds
        approx_results[1]["setup_seconds"] = qdrant_setup_seconds
        approx_results[2]["setup_seconds"] = pg_setup_seconds
        report["scenarios"]["approx_filtered_cosine"] = {"results": approx_results}

        args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        write_markdown(args.output_md, report)
        print(json.dumps(report, indent=2))
        return 0
    finally:
        stop_process(sqlrite_process)
        docker_rm(QDRANT_CONTAINER)
        docker_rm(PG_CONTAINER)


if __name__ == "__main__":
    raise SystemExit(main())
