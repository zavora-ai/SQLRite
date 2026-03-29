#!/usr/bin/env python3
import argparse
import atexit
import http.client
import json
import os
import shutil
import signal
import sqlite3
import subprocess
import sys
import time
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

import numpy as np

try:
    import psycopg
except ImportError as exc:  # pragma: no cover
    raise SystemExit(
        "Missing dependency: psycopg. Install with `python3 -m pip install -r papers/sqlrite_arxiv/requirements.txt`."
    ) from exc


ROOT = Path("/Users/jameskaranja/Developer/projects/SQLRight")
PAPER_DIR = ROOT / "papers" / "sqlrite_arxiv"
DEFAULT_OUT = PAPER_DIR / "comparative_results.json"
DEFAULT_MD = PAPER_DIR / "comparative_results.md"
SQLRITE_PORT = 8099
QDRANT_PORT = 6333
PG_PORT = 55432


@dataclass
class QuerySpec:
    vector: np.ndarray
    tenant: str
    ground_truth_ids: list[int]


def run(cmd: list[str], *, cwd: Path | None = None, check: bool = True, capture: bool = False):
    return subprocess.run(
        cmd,
        cwd=str(cwd) if cwd else None,
        check=check,
        text=True,
        capture_output=capture,
    )


def http_json(method: str, url: str, payload: dict | None = None, timeout: float = 30.0) -> dict:
    data = None
    headers = {}
    if payload is not None:
        data = json.dumps(payload).encode("utf-8")
        headers["content-type"] = "application/json"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode("utf-8"))


def wait_for_http(url: str, timeout_s: float = 60.0) -> None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=2.0) as resp:
                if 200 <= resp.status < 300:
                    return
        except Exception:
            time.sleep(0.5)
    raise RuntimeError(f"Timed out waiting for {url}")


def normalize(vectors: np.ndarray) -> np.ndarray:
    norms = np.linalg.norm(vectors, axis=1, keepdims=True)
    norms[norms == 0] = 1.0
    return vectors / norms


class JsonHttpClient:
    def __init__(self, host: str, port: int, timeout: float = 30.0):
        self.host = host
        self.port = port
        self.timeout = timeout
        self._conn: http.client.HTTPConnection | None = None

    def _connect(self) -> http.client.HTTPConnection:
        if self._conn is None:
            self._conn = http.client.HTTPConnection(self.host, self.port, timeout=self.timeout)
        return self._conn

    def request_json(self, method: str, path: str, payload: dict | None = None) -> dict:
        body = None
        headers = {}
        if payload is not None:
            body = json.dumps(payload).encode("utf-8")
            headers["content-type"] = "application/json"
        try:
            conn = self._connect()
            conn.request(method, path, body=body, headers=headers)
            resp = conn.getresponse()
            data = resp.read()
            if resp.status >= 400:
                raise RuntimeError(f"HTTP {resp.status}: {data.decode('utf-8', errors='replace')}")
            return json.loads(data.decode("utf-8"))
        except Exception:
            self.close()
            raise

    def close(self) -> None:
        if self._conn is not None:
            try:
                self._conn.close()
            finally:
                self._conn = None


class PgQueryClient:
    def __init__(self, approximate: bool):
        self.conn = psycopg.connect(
            f"host=127.0.0.1 port={PG_PORT} dbname=sqlritebench user=postgres password=postgres",
            autocommit=True,
        )
        self.cur = self.conn.cursor()
        if approximate:
            self.cur.execute("SET enable_seqscan = off")
        else:
            self.cur.execute("SET enable_indexscan = off")
            self.cur.execute("SET enable_bitmapscan = off")

    def query(self, query: QuerySpec, top_k: int) -> list[int]:
        emb = "[" + ",".join(f"{v:.7f}" for v in query.vector) + "]"
        self.cur.execute(
            "SELECT id FROM items WHERE tenant = %s ORDER BY embedding <=> %s::vector LIMIT %s",
            (query.tenant, emb, top_k),
        )
        return [int(row[0]) for row in self.cur.fetchall()]

    def close(self) -> None:
        try:
            self.cur.close()
        finally:
            self.conn.close()


def generate_dataset(corpus_size: int, dim: int, query_count: int, tenants: int, top_k: int, seed: int):
    rng = np.random.default_rng(seed)
    corpus = normalize(rng.normal(size=(corpus_size, dim)).astype(np.float32))
    tenant_ids = np.array([f"tenant-{i % tenants}" for i in range(corpus_size)])
    query_indices = rng.choice(corpus_size, size=query_count, replace=False)
    query_vectors = corpus[query_indices] + rng.normal(scale=0.01, size=(query_count, dim)).astype(np.float32)
    query_vectors = normalize(query_vectors)

    queries: list[QuerySpec] = []
    for i, qv in enumerate(query_vectors):
        tenant = tenant_ids[query_indices[i]]
        mask = tenant_ids == tenant
        candidate_ids = np.nonzero(mask)[0] + 1
        scores = np.einsum("ij,j->i", corpus[mask].astype(np.float64), qv.astype(np.float64))
        top_idx = np.argsort(scores)[::-1][:top_k]
        ground_truth = candidate_ids[top_idx].tolist()
        queries.append(QuerySpec(vector=qv, tenant=tenant, ground_truth_ids=ground_truth))

    records = []
    for idx in range(corpus_size):
        records.append(
            {
                "id": idx + 1,
                "chunk_id": f"c{idx + 1:06d}",
                "doc_id": f"doc-{idx + 1:06d}",
                "tenant": tenant_ids[idx],
                "embedding": corpus[idx],
                "content": f"synthetic content {idx + 1}",
            }
        )

    return records, queries


def ensure_sqlrite_binary() -> Path:
    binary = ROOT / "target" / "debug" / "sqlrite"
    if binary.exists():
        return binary
    run(["cargo", "build", "--bin", "sqlrite"], cwd=ROOT)
    if not binary.exists():
        raise RuntimeError("sqlrite binary was not built")
    return binary


def init_sqlrite_db(binary: Path, db_path: Path) -> None:
    if db_path.exists():
        db_path.unlink()
    run([str(binary), "init", "--db", str(db_path)], cwd=ROOT)


def populate_sqlrite_db(db_path: Path, records: list[dict]) -> float:
    started = time.perf_counter()
    conn = sqlite3.connect(db_path)
    try:
        docs = [(r["doc_id"], f"synthetic/{r['doc_id']}.md") for r in records]
        conn.executemany(
            "INSERT OR REPLACE INTO documents (id, source, metadata) VALUES (?, ?, '{}')",
            docs,
        )
        chunk_rows = []
        for r in records:
            metadata = json.dumps({"tenant": r["tenant"]})
            emb = np.asarray(r["embedding"], dtype="<f4").tobytes()
            chunk_rows.append((r["chunk_id"], r["doc_id"], r["content"], metadata, emb, len(r["embedding"])))
        conn.executemany(
            "INSERT OR REPLACE INTO chunks (id, doc_id, content, metadata, embedding, embedding_dim) VALUES (?, ?, ?, ?, ?, ?)",
            chunk_rows,
        )
        conn.commit()
    finally:
        conn.close()
    return time.perf_counter() - started


def start_sqlrite_server(binary: Path, db_path: Path, index_mode: str) -> subprocess.Popen:
    log = open(PAPER_DIR / f"sqlrite_{index_mode}.log", "w")
    proc = subprocess.Popen(
        [
            str(binary),
            "serve",
            "--db",
            str(db_path),
            "--bind",
            f"127.0.0.1:{SQLRITE_PORT}",
            "--index-mode",
            index_mode,
        ],
        cwd=ROOT,
        stdout=log,
        stderr=subprocess.STDOUT,
        text=True,
    )
    wait_for_http(f"http://127.0.0.1:{SQLRITE_PORT}/readyz")
    return proc


def stop_process(proc: subprocess.Popen | None) -> None:
    if not proc or proc.poll() is not None:
        return
    proc.terminate()
    try:
        proc.wait(timeout=10)
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait(timeout=5)


def sqlrite_query(client: JsonHttpClient, query: QuerySpec, top_k: int, candidate_limit: int, query_profile: str) -> list[int]:
    payload = {
        "query_embedding": query.vector.tolist(),
        "top_k": top_k,
        "alpha": 0.0,
        "candidate_limit": candidate_limit,
        "query_profile": query_profile,
        "include_payloads": False,
        "metadata_filters": {"tenant": query.tenant},
    }
    resp = client.request_json("POST", "/v1/query-compact", payload)
    out = []
    for chunk_id in resp.get("chunk_ids", []):
        if isinstance(chunk_id, str) and chunk_id.startswith("c"):
            out.append(int(chunk_id[1:]))
        else:
            out.append(int(chunk_id))
    return out


def docker_rm(name: str) -> None:
    subprocess.run(["docker", "rm", "-f", name], check=False, capture_output=True, text=True)


def start_qdrant() -> None:
    docker_rm("sqlrite-qdrant-bench")
    run(
        [
            "docker",
            "run",
            "-d",
            "--rm",
            "--name",
            "sqlrite-qdrant-bench",
            "-p",
            f"{QDRANT_PORT}:6333",
            "qdrant/qdrant:latest",
        ]
    )
    wait_for_http(f"http://127.0.0.1:{QDRANT_PORT}/readyz", timeout_s=120)


def qdrant_create_collection(dim: int) -> None:
    http_json(
        "PUT",
        f"http://127.0.0.1:{QDRANT_PORT}/collections/sqlrite_bench",
        {"vectors": {"size": dim, "distance": "Cosine"}},
    )
    http_json(
        "PUT",
        f"http://127.0.0.1:{QDRANT_PORT}/collections/sqlrite_bench/index",
        {"field_name": "tenant", "field_schema": "keyword"},
    )


def qdrant_upload(records: list[dict], batch_size: int = 512) -> float:
    started = time.perf_counter()
    for i in range(0, len(records), batch_size):
        batch = records[i : i + batch_size]
        points = [
            {
                "id": r["id"],
                "vector": r["embedding"].tolist(),
                "payload": {"tenant": r["tenant"]},
            }
            for r in batch
        ]
        http_json(
            "PUT",
            f"http://127.0.0.1:{QDRANT_PORT}/collections/sqlrite_bench/points?wait=true",
            {"points": points},
            timeout=120.0,
        )
    return time.perf_counter() - started


def qdrant_query(client: JsonHttpClient, query: QuerySpec, top_k: int, exact: bool) -> list[int]:
    payload = {
        "query": query.vector.tolist(),
        "filter": {
            "must": [{"key": "tenant", "match": {"value": query.tenant}}],
        },
        "params": {"exact": exact},
        "limit": top_k,
    }
    resp = client.request_json("POST", "/collections/sqlrite_bench/points/query", payload)
    result = resp.get("result", {})
    points = result.get("points") if isinstance(result, dict) else result
    if points is None:
        points = []
    return [int(point["id"]) for point in points]


def start_pgvector() -> None:
    docker_rm("sqlrite-pgvector-bench")
    run(
        [
            "docker",
            "run",
            "-d",
            "--rm",
            "--name",
            "sqlrite-pgvector-bench",
            "-e",
            "POSTGRES_PASSWORD=postgres",
            "-e",
            "POSTGRES_DB=sqlritebench",
            "-p",
            f"{PG_PORT}:5432",
            "pgvector/pgvector:pg17",
        ]
    )

    deadline = time.time() + 120
    while time.time() < deadline:
        try:
            with psycopg.connect("host=127.0.0.1 port=%s dbname=sqlritebench user=postgres password=postgres" % PG_PORT):
                return
        except Exception:
            time.sleep(1.0)
    raise RuntimeError("Timed out waiting for pgvector")


def pg_setup(dim: int) -> None:
    with psycopg.connect(
        f"host=127.0.0.1 port={PG_PORT} dbname=sqlritebench user=postgres password=postgres",
        autocommit=True,
    ) as conn:
        with conn.cursor() as cur:
            cur.execute("CREATE EXTENSION IF NOT EXISTS vector")
            cur.execute("DROP TABLE IF EXISTS items")
            cur.execute(f"CREATE TABLE items (id BIGINT PRIMARY KEY, tenant TEXT NOT NULL, embedding vector({dim}) NOT NULL)")


def pg_upload(records: list[dict]) -> float:
    started = time.perf_counter()
    with psycopg.connect(
        f"host=127.0.0.1 port={PG_PORT} dbname=sqlritebench user=postgres password=postgres",
        autocommit=True,
    ) as conn:
        with conn.cursor() as cur:
            with cur.copy("COPY items (id, tenant, embedding) FROM STDIN") as copy:
                for r in records:
                    emb = "[" + ",".join(f"{v:.7f}" for v in r["embedding"]) + "]"
                    copy.write_row((r["id"], r["tenant"], emb))
    return time.perf_counter() - started


def pg_create_hnsw_index() -> None:
    with psycopg.connect(
        f"host=127.0.0.1 port={PG_PORT} dbname=sqlritebench user=postgres password=postgres",
        autocommit=True,
    ) as conn:
        with conn.cursor() as cur:
            cur.execute("CREATE INDEX IF NOT EXISTS items_embedding_hnsw ON items USING hnsw (embedding vector_cosine_ops)")
            cur.execute("ANALYZE items")


def percentile_ms(values: list[float], pct: float) -> float:
    if not values:
        return 0.0
    return float(np.percentile(np.array(values, dtype=np.float64), pct) * 1000.0)


def compute_metrics(results: list[list[int]], queries: list[QuerySpec], k: int) -> dict:
    top1_hits = 0
    recall_sum = 0.0
    for returned, query in zip(results, queries, strict=True):
        truth = query.ground_truth_ids[:k]
        if returned and truth and returned[0] == truth[0]:
            top1_hits += 1
        overlap = len(set(returned[:k]) & set(truth))
        recall_sum += overlap / len(truth)
    count = max(len(queries), 1)
    return {
        "top1_hit_rate": top1_hits / count,
        "recall_at_k": recall_sum / count,
    }


def benchmark(name: str, query_fn: Callable[[QuerySpec], list[int]], queries: list[QuerySpec], warmup: int, k: int) -> dict:
    for query in queries[:warmup]:
        query_fn(query)
    latencies = []
    results = []
    started = time.perf_counter()
    for query in queries[warmup:]:
        q_started = time.perf_counter()
        ids = query_fn(query)
        latencies.append(time.perf_counter() - q_started)
        results.append(ids)
    elapsed = time.perf_counter() - started
    metrics = compute_metrics(results, queries[warmup:], k)
    return {
        "system": name,
        "qps": len(results) / elapsed if elapsed > 0 else 0.0,
        "p50_ms": percentile_ms(latencies, 50),
        "p95_ms": percentile_ms(latencies, 95),
        **metrics,
    }


def write_markdown(path: Path, report: dict) -> None:
    lines = [
        "# Comparative Benchmark Results",
        "",
        "These results come from a single-host localhost benchmark using the same deterministic cosine+tenant-filter workload across all tested systems.",
        "",
        "## Workload",
        "",
        f"- corpus size: `{report['workload']['corpus_size']}`",
        f"- query count: `{report['workload']['query_count']}`",
        f"- embedding dimension: `{report['workload']['embedding_dim']}`",
        f"- tenants: `{report['workload']['tenants']}`",
        f"- top-k: `{report['workload']['top_k']}`",
        f"- seed: `{report['workload']['seed']}`",
        "",
    ]
    for scenario_name in ["exact_filtered_cosine", "approx_filtered_cosine"]:
        scenario = report["scenarios"][scenario_name]
        lines.extend([
            f"## {scenario_name.replace('_', ' ').title()}",
            "",
            "| System | QPS | p50 ms | p95 ms | Top1 hit | Recall@k | Setup s |",
            "|---|---:|---:|---:|---:|---:|---:|",
        ])
        for result in scenario["results"]:
            lines.append(
                f"| {result['system']} | {result['qps']:.2f} | {result['p50_ms']:.3f} | {result['p95_ms']:.3f} | {result['top1_hit_rate']:.4f} | {result['recall_at_k']:.4f} | {result['setup_seconds']:.3f} |"
            )
        lines.append("")
    lines.extend([
        "## Caveats",
        "",
        "- This is a single-host benchmark on one machine, not a cluster-scale study.",
        "- The workload measures filtered cosine vector search only; it does not yet cover lexical or hybrid retrieval parity.",
        "- SQLRite is compared here against one SQL-first competitor (`pgvector`) and one network-native vector database (`Qdrant`).",
    ])
    path.write_text("\n".join(lines) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus-size", type=int, default=20000)
    parser.add_argument("--query-count", type=int, default=200)
    parser.add_argument("--embedding-dim", type=int, default=32)
    parser.add_argument("--tenants", type=int, default=8)
    parser.add_argument("--top-k", type=int, default=10)
    parser.add_argument("--warmup", type=int, default=20)
    parser.add_argument("--seed", type=int, default=20260308)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--output-md", type=Path, default=DEFAULT_MD)
    args = parser.parse_args()

    binary = ensure_sqlrite_binary()
    records, queries = generate_dataset(
        args.corpus_size,
        args.embedding_dim,
        args.query_count,
        args.tenants,
        args.top_k,
        args.seed,
    )

    temp_dir = Path("/tmp/sqlrite_competitive_eval")
    shutil.rmtree(temp_dir, ignore_errors=True)
    temp_dir.mkdir(parents=True, exist_ok=True)
    db_path = temp_dir / "sqlrite_competitive.db"

    sqlrite_proc = None
    try:
        init_sqlrite_db(binary, db_path)
        sqlrite_setup = populate_sqlrite_db(db_path, records)

        start_qdrant()
        atexit.register(lambda: docker_rm("sqlrite-qdrant-bench"))
        qdrant_create_collection(args.embedding_dim)
        qdrant_setup = qdrant_upload(records)

        start_pgvector()
        atexit.register(lambda: docker_rm("sqlrite-pgvector-bench"))
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

        sqlrite_proc = start_sqlrite_server(binary, db_path, "brute_force")
        sqlrite_client = JsonHttpClient("127.0.0.1", SQLRITE_PORT)
        qdrant_client = JsonHttpClient("127.0.0.1", QDRANT_PORT)
        pg_exact_client = PgQueryClient(approximate=False)
        exact_results = [
            benchmark(
                "SQLRite brute_force compact_http",
                lambda q: sqlrite_query(sqlrite_client, q, args.top_k, args.top_k, "latency"),
                queries,
                args.warmup,
                args.top_k,
            ),
            benchmark(
                "Qdrant exact",
                lambda q: qdrant_query(qdrant_client, q, args.top_k, True),
                queries,
                args.warmup,
                args.top_k,
            ),
            benchmark(
                "pgvector exact",
                lambda q: pg_exact_client.query(q, args.top_k),
                queries,
                args.warmup,
                args.top_k,
            ),
        ]
        sqlrite_client.close()
        qdrant_client.close()
        pg_exact_client.close()
        stop_process(sqlrite_proc)
        sqlrite_proc = None
        exact_results[0]["setup_seconds"] = sqlrite_setup
        exact_results[1]["setup_seconds"] = qdrant_setup
        exact_results[2]["setup_seconds"] = pg_setup_seconds
        report["scenarios"]["exact_filtered_cosine"] = {"results": exact_results}

        pg_create_hnsw_index()
        sqlrite_proc = start_sqlrite_server(binary, db_path, "hnsw_baseline")
        sqlrite_client = JsonHttpClient("127.0.0.1", SQLRITE_PORT)
        qdrant_client = JsonHttpClient("127.0.0.1", QDRANT_PORT)
        pg_hnsw_client = PgQueryClient(approximate=True)
        approx_results = [
            benchmark(
                "SQLRite hnsw_baseline compact_http",
                lambda q: sqlrite_query(sqlrite_client, q, args.top_k, args.top_k, "latency"),
                queries,
                args.warmup,
                args.top_k,
            ),
            benchmark(
                "Qdrant HNSW",
                lambda q: qdrant_query(qdrant_client, q, args.top_k, False),
                queries,
                args.warmup,
                args.top_k,
            ),
            benchmark(
                "pgvector HNSW",
                lambda q: pg_hnsw_client.query(q, args.top_k),
                queries,
                args.warmup,
                args.top_k,
            ),
        ]
        sqlrite_client.close()
        qdrant_client.close()
        pg_hnsw_client.close()
        stop_process(sqlrite_proc)
        sqlrite_proc = None
        approx_results[0]["setup_seconds"] = sqlrite_setup
        approx_results[1]["setup_seconds"] = qdrant_setup
        approx_results[2]["setup_seconds"] = pg_setup_seconds
        report["scenarios"]["approx_filtered_cosine"] = {"results": approx_results}

        args.output.write_text(json.dumps(report, indent=2) + "\n")
        write_markdown(args.output_md, report)
        print(json.dumps(report, indent=2))
        return 0
    finally:
        stop_process(sqlrite_proc)
        docker_rm("sqlrite-qdrant-bench")
        docker_rm("sqlrite-pgvector-bench")


if __name__ == "__main__":
    raise SystemExit(main())
