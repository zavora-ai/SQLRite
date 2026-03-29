#!/usr/bin/env python3
import argparse
import atexit
import csv
import hashlib
import http.client
import json
import os
import re
import shutil
import sqlite3
import subprocess
import tarfile
import time
import urllib.request
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Callable

import numpy as np
import psycopg
import sqlite_vec

ROOT = Path('/Users/jameskaranja/Developer/projects/SQLRight')
PAPER_DIR = ROOT / 'papers' / 'sqlrite_arxiv'
DATA_DIR = Path('/tmp/sqlrite_arxiv_public_data')
SCIFACT_URL = 'https://public.ukp.informatik.tu-darmstadt.de/thakur/BEIR/datasets/scifact.zip'
SQLRITE_PORT = 8099
PG_PORT = 55433
DEFAULT_JSON = PAPER_DIR / 'public_dataset_results.json'
DEFAULT_MD = PAPER_DIR / 'public_dataset_results.md'
TOKEN_RE = re.compile(r"[A-Za-z0-9]+")


@dataclass
class PublicQuery:
    qid: str
    text: str
    embedding: np.ndarray
    relevant_doc_ids: list[str]


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
            body = json.dumps(payload).encode('utf-8')
            headers['content-type'] = 'application/json'
        try:
            conn = self._connect()
            conn.request(method, path, body=body, headers=headers)
            resp = conn.getresponse()
            data = resp.read()
            if resp.status >= 400:
                raise RuntimeError(f'HTTP {resp.status}: {data.decode("utf-8", errors="replace")}')
            return json.loads(data.decode('utf-8'))
        except Exception:
            self.close()
            raise

    def close(self) -> None:
        if self._conn is not None:
            try:
                self._conn.close()
            finally:
                self._conn = None


class PgClient:
    def __init__(self):
        self.conn = psycopg.connect(
            f'host=127.0.0.1 port={PG_PORT} dbname=sqlritebench user=postgres password=postgres',
            autocommit=True,
        )
        self.cur = self.conn.cursor()

    def exact_vector(self, embedding: np.ndarray, top_k: int) -> list[str]:
        self.cur.execute('SET enable_indexscan = off')
        self.cur.execute('SET enable_bitmapscan = off')
        emb = '[' + ','.join(f'{v:.7f}' for v in embedding) + ']'
        self.cur.execute(
            'SELECT id FROM items ORDER BY embedding <=> %s::vector LIMIT %s',
            (emb, top_k),
        )
        return [row[0] for row in self.cur.fetchall()]

    def hybrid(self, query_text: str, embedding: np.ndarray, top_k: int, alpha: float) -> list[str]:
        emb = '[' + ','.join(f'{v:.7f}' for v in embedding) + ']'
        self.cur.execute(
            '''
            SELECT id
            FROM items
            ORDER BY (
                %s * COALESCE(ts_rank_cd(ts, plainto_tsquery('english', %s)), 0.0) +
                (1.0 - %s) * GREATEST(0.0, 1.0 - (embedding <=> %s::vector))
            ) DESC,
            id ASC
            LIMIT %s
            ''',
            (alpha, query_text, alpha, emb, top_k),
        )
        return [row[0] for row in self.cur.fetchall()]

    def close(self) -> None:
        try:
            self.cur.close()
        finally:
            self.conn.close()


class SQLiteVecClient:
    def __init__(self, db_path: Path):
        self.conn = sqlite3.connect(db_path)
        self.conn.enable_load_extension(True)
        sqlite_vec.load(self.conn)
        self.conn.enable_load_extension(False)

    def exact_vector(self, embedding: np.ndarray, top_k: int) -> list[str]:
        rows = self.conn.execute(
            '''
            select d.doc_id
            from vec_items v
            join docs d on d.rowid = v.rowid
            where v.embedding match ?
              and k = ?
            order by distance
            ''',
            [embedding, top_k],
        ).fetchall()
        return [row[0] for row in rows]

    def close(self) -> None:
        self.conn.close()


def run(cmd: list[str], *, cwd: Path | None = None, check: bool = True, capture: bool = False):
    return subprocess.run(cmd, cwd=str(cwd) if cwd else None, check=check, text=True, capture_output=capture)


def wait_for_http(url: str, timeout_s: float = 60.0) -> None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=2.0) as resp:
                if 200 <= resp.status < 300:
                    return
        except Exception:
            time.sleep(0.5)
    raise RuntimeError(f'Timed out waiting for {url}')


def ensure_sqlrite_binary() -> Path:
    binary = ROOT / 'target' / 'debug' / 'sqlrite'
    if binary.exists():
        return binary
    run(['cargo', 'build', '--bin', 'sqlrite'], cwd=ROOT)
    return binary


def download_scifact() -> Path:
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    zip_path = DATA_DIR / 'scifact.zip'
    extract_dir = DATA_DIR / 'scifact'
    if not zip_path.exists():
        urllib.request.urlretrieve(SCIFACT_URL, zip_path)
    if not extract_dir.exists():
        with zipfile.ZipFile(zip_path) as zf:
            zf.extractall(extract_dir)
    return extract_dir / 'scifact'


def tokenize(text: str) -> list[str]:
    return [t.lower() for t in TOKEN_RE.findall(text)]


def hash_embed(text: str, dim: int) -> np.ndarray:
    vec = np.zeros(dim, dtype=np.float32)
    for token in tokenize(text):
        digest = hashlib.md5(token.encode('utf-8')).digest()
        idx = int.from_bytes(digest[:4], 'little') % dim
        sign = 1.0 if (digest[4] & 1) == 0 else -1.0
        weight = 1.0 + (int.from_bytes(digest[5:7], 'little') / 65535.0) * 0.05
        vec[idx] += sign * weight
    norm = float(np.linalg.norm(vec))
    if norm > 0:
        vec /= norm
    return vec


def load_scifact(max_queries: int, dim: int):
    root = download_scifact()
    corpus_path = root / 'corpus.jsonl'
    queries_path = root / 'queries.jsonl'
    qrels_path = root / 'qrels' / 'test.tsv'

    corpus = {}
    with corpus_path.open() as fh:
        for line in fh:
            row = json.loads(line)
            text = (row.get('title') or '').strip()
            body = (row.get('text') or '').strip()
            content = f'{text}. {body}'.strip()
            corpus[row['_id']] = {
                'id': row['_id'],
                'content': content,
                'embedding': hash_embed(content, dim),
            }

    queries = {}
    with queries_path.open() as fh:
        for line in fh:
            row = json.loads(line)
            queries[row['_id']] = row['text']

    qrels: dict[str, list[str]] = {}
    with qrels_path.open() as fh:
        reader = csv.DictReader(fh, delimiter='\t')
        for row in reader:
            if int(row['score']) > 0:
                qrels.setdefault(row['query-id'], []).append(row['corpus-id'])

    selected_queries = []
    for qid, rels in qrels.items():
        if qid not in queries:
            continue
        selected_queries.append(
            PublicQuery(
                qid=qid,
                text=queries[qid],
                embedding=hash_embed(queries[qid], dim),
                relevant_doc_ids=rels,
            )
        )
        if len(selected_queries) >= max_queries:
            break

    return list(corpus.values()), selected_queries


def init_sqlrite_db(binary: Path, db_path: Path) -> None:
    if db_path.exists():
        db_path.unlink()
    run([str(binary), 'init', '--db', str(db_path)], cwd=ROOT)


def populate_sqlrite(db_path: Path, corpus: list[dict]) -> float:
    started = time.perf_counter()
    conn = sqlite3.connect(db_path)
    try:
        docs = [(doc['id'], f'public/{doc["id"]}.txt') for doc in corpus]
        conn.executemany("INSERT OR REPLACE INTO documents (id, source, metadata) VALUES (?, ?, '{}')", docs)
        rows = []
        for doc in corpus:
            emb = np.asarray(doc['embedding'], dtype='<f4').tobytes()
            rows.append((doc['id'], doc['id'], doc['content'], '{}', emb, len(doc['embedding'])))
        conn.executemany(
            'INSERT OR REPLACE INTO chunks (id, doc_id, content, metadata, embedding, embedding_dim) VALUES (?, ?, ?, ?, ?, ?)',
            rows,
        )
        conn.commit()
    finally:
        conn.close()
    return time.perf_counter() - started


def start_sqlrite_server(binary: Path, db_path: Path, index_mode: str) -> subprocess.Popen:
    log = open(PAPER_DIR / f'public_sqlrite_{index_mode}.log', 'w')
    proc = subprocess.Popen(
        [str(binary), 'serve', '--db', str(db_path), '--bind', f'127.0.0.1:{SQLRITE_PORT}', '--index-mode', index_mode],
        cwd=ROOT,
        stdout=log,
        stderr=subprocess.STDOUT,
        text=True,
    )
    wait_for_http(f'http://127.0.0.1:{SQLRITE_PORT}/readyz', timeout_s=60)
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


def sqlrite_vector_query(client: JsonHttpClient, query: PublicQuery, top_k: int, candidate_limit: int) -> list[str]:
    payload = {
        'query_embedding': query.embedding.tolist(),
        'top_k': top_k,
        'alpha': 0.0,
        'candidate_limit': candidate_limit,
        'query_profile': 'latency',
        'include_payloads': False,
    }
    resp = client.request_json('POST', '/v1/query-compact', payload)
    return resp.get('chunk_ids', [])


def sqlrite_hybrid_query(client: JsonHttpClient, query: PublicQuery, top_k: int, alpha: float, candidate_limit: int) -> list[str]:
    payload = {
        'query_text': query.text,
        'query_embedding': query.embedding.tolist(),
        'top_k': top_k,
        'alpha': alpha,
        'candidate_limit': candidate_limit,
        'query_profile': 'balanced',
        'include_payloads': False,
    }
    resp = client.request_json('POST', '/v1/query-compact', payload)
    return resp.get('chunk_ids', [])


def setup_sqlite_vec(db_path: Path, corpus: list[dict], dim: int) -> float:
    if db_path.exists():
        db_path.unlink()
    started = time.perf_counter()
    conn = sqlite3.connect(db_path)
    conn.enable_load_extension(True)
    sqlite_vec.load(conn)
    conn.enable_load_extension(False)
    conn.execute(f'create virtual table vec_items using vec0(embedding float[{dim}])')
    conn.execute('create table docs(rowid integer primary key, doc_id text not null unique)')
    for idx, doc in enumerate(corpus, start=1):
        conn.execute('insert into docs(rowid, doc_id) values (?, ?)', (idx, doc['id']))
        conn.execute('insert into vec_items(rowid, embedding) values (?, ?)', (idx, doc['embedding']))
    conn.commit()
    conn.close()
    return time.perf_counter() - started


def docker_rm(name: str) -> None:
    subprocess.run(['docker', 'rm', '-f', name], check=False, capture_output=True, text=True)


def start_pgvector() -> None:
    docker_rm('sqlrite-pgvector-public')
    run([
        'docker', 'run', '-d', '--rm', '--name', 'sqlrite-pgvector-public',
        '-e', 'POSTGRES_PASSWORD=postgres', '-e', 'POSTGRES_DB=sqlritebench',
        '-p', f'{PG_PORT}:5432', 'pgvector/pgvector:pg17'
    ])
    deadline = time.time() + 120
    while time.time() < deadline:
        try:
            with psycopg.connect(f'host=127.0.0.1 port={PG_PORT} dbname=sqlritebench user=postgres password=postgres'):
                return
        except Exception:
            time.sleep(1)
    raise RuntimeError('Timed out waiting for pgvector')


def setup_pgvector(corpus: list[dict], dim: int) -> float:
    started = time.perf_counter()
    with psycopg.connect(f'host=127.0.0.1 port={PG_PORT} dbname=sqlritebench user=postgres password=postgres', autocommit=True) as conn:
        with conn.cursor() as cur:
            cur.execute('CREATE EXTENSION IF NOT EXISTS vector')
            cur.execute('DROP TABLE IF EXISTS items')
            cur.execute(f"CREATE TABLE items (id TEXT PRIMARY KEY, content TEXT NOT NULL, embedding vector({dim}) NOT NULL, ts tsvector GENERATED ALWAYS AS (to_tsvector('english', content)) STORED)")
            with cur.copy('COPY items (id, content, embedding) FROM STDIN') as copy:
                for doc in corpus:
                    emb = '[' + ','.join(f'{v:.7f}' for v in doc['embedding']) + ']'
                    copy.write_row((doc['id'], doc['content'], emb))
            cur.execute('CREATE INDEX IF NOT EXISTS items_ts_gin ON items USING gin(ts)')
            cur.execute('ANALYZE items')
    return time.perf_counter() - started


def recall_at_k(retrieved: list[str], relevant: list[str], k: int) -> float:
    rel = set(relevant)
    if not rel:
        return 0.0
    return len(set(retrieved[:k]) & rel) / len(rel)


def mrr_at_k(retrieved: list[str], relevant: list[str], k: int) -> float:
    rel = set(relevant)
    for idx, doc_id in enumerate(retrieved[:k], start=1):
        if doc_id in rel:
            return 1.0 / idx
    return 0.0


def ndcg_at_k(retrieved: list[str], relevant: list[str], k: int) -> float:
    rel = set(relevant)
    dcg = 0.0
    for idx, doc_id in enumerate(retrieved[:k], start=1):
        if doc_id in rel:
            dcg += 1.0 / np.log2(idx + 1)
    ideal_hits = min(len(rel), k)
    if ideal_hits == 0:
        return 0.0
    idcg = sum(1.0 / np.log2(idx + 1) for idx in range(1, ideal_hits + 1))
    return float(dcg / idcg)


def percentile_ms(values: list[float], pct: float) -> float:
    return float(np.percentile(np.array(values, dtype=np.float64), pct) * 1000.0) if values else 0.0


def benchmark(name: str, queries: list[PublicQuery], warmup: int, top_k: int, fn: Callable[[PublicQuery], list[str]]) -> dict:
    for query in queries[:warmup]:
        fn(query)
    latencies = []
    recall_sum = 0.0
    mrr_sum = 0.0
    ndcg_sum = 0.0
    started = time.perf_counter()
    for query in queries[warmup:]:
        q_started = time.perf_counter()
        docs = fn(query)
        latencies.append(time.perf_counter() - q_started)
        recall_sum += recall_at_k(docs, query.relevant_doc_ids, top_k)
        mrr_sum += mrr_at_k(docs, query.relevant_doc_ids, top_k)
        ndcg_sum += ndcg_at_k(docs, query.relevant_doc_ids, top_k)
    elapsed = time.perf_counter() - started
    count = max(len(queries[warmup:]), 1)
    return {
        'system': name,
        'qps': len(queries[warmup:]) / elapsed if elapsed > 0 else 0.0,
        'p50_ms': percentile_ms(latencies, 50),
        'p95_ms': percentile_ms(latencies, 95),
        'recall_at_k': recall_sum / count,
        'mrr_at_k': mrr_sum / count,
        'ndcg_at_k': ndcg_sum / count,
    }


def write_markdown(path: Path, report: dict) -> None:
    lines = [
        '# Public Dataset Benchmark Results',
        '',
        'Dataset: BEIR/SciFact with deterministic hashed embeddings and shared query relevance judgments.',
        '',
        '## Workload',
        '',
        f"- dataset: `{report['workload']['dataset']}`",
        f"- corpus size: `{report['workload']['corpus_size']}`",
        f"- query count: `{report['workload']['query_count']}`",
        f"- embedding dimension: `{report['workload']['embedding_dim']}`",
        f"- top-k: `{report['workload']['top_k']}`",
        f"- alpha: `{report['workload']['hybrid_alpha']}`",
        '',
        '## Vector Exact Benchmark',
        '',
        '| System | QPS | p50 ms | p95 ms | Recall@k | MRR@k | NDCG@k |',
        '|---|---:|---:|---:|---:|---:|---:|',
    ]
    for item in report['vector_exact']['results']:
        lines.append(f"| {item['system']} | {item['qps']:.2f} | {item['p50_ms']:.3f} | {item['p95_ms']:.3f} | {item['recall_at_k']:.4f} | {item['mrr_at_k']:.4f} | {item['ndcg_at_k']:.4f} |")
    lines.extend([
        '',
        '## Hybrid Lexical + Vector Benchmark',
        '',
        '| System | QPS | p50 ms | p95 ms | Recall@k | MRR@k | NDCG@k |',
        '|---|---:|---:|---:|---:|---:|---:|',
    ])
    for item in report['hybrid']['results']:
        lines.append(f"| {item['system']} | {item['qps']:.2f} | {item['p50_ms']:.3f} | {item['p95_ms']:.3f} | {item['recall_at_k']:.4f} | {item['mrr_at_k']:.4f} | {item['ndcg_at_k']:.4f} |")
    path.write_text('\n'.join(lines) + '\n')


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument('--max-queries', type=int, default=200)
    parser.add_argument('--embedding-dim', type=int, default=256)
    parser.add_argument('--top-k', type=int, default=10)
    parser.add_argument('--warmup', type=int, default=10)
    parser.add_argument('--hybrid-alpha', type=float, default=0.5)
    parser.add_argument('--output', type=Path, default=DEFAULT_JSON)
    parser.add_argument('--output-md', type=Path, default=DEFAULT_MD)
    args = parser.parse_args()

    binary = ensure_sqlrite_binary()
    corpus, queries = load_scifact(args.max_queries, args.embedding_dim)

    temp_dir = Path('/tmp/sqlrite_public_eval')
    shutil.rmtree(temp_dir, ignore_errors=True)
    temp_dir.mkdir(parents=True, exist_ok=True)
    sqlrite_db = temp_dir / 'sqlrite_public.db'
    sqlite_vec_db = temp_dir / 'sqlite_vec_public.db'

    sqlrite_proc = None
    sqlrite_client = None
    sqlite_vec_client = None
    pg_client = None
    try:
        init_sqlrite_db(binary, sqlrite_db)
        sqlrite_setup = populate_sqlrite(sqlrite_db, corpus)
        sqlite_vec_setup = setup_sqlite_vec(sqlite_vec_db, corpus, args.embedding_dim)
        sqlite_vec_client = SQLiteVecClient(sqlite_vec_db)
        start_pgvector()
        atexit.register(lambda: docker_rm('sqlrite-pgvector-public'))
        pg_setup = setup_pgvector(corpus, args.embedding_dim)

        report = {
            'generated_at_unix': int(time.time()),
            'workload': {
                'dataset': 'BEIR/SciFact',
                'corpus_size': len(corpus),
                'query_count': len(queries),
                'embedding_dim': args.embedding_dim,
                'top_k': args.top_k,
                'warmup': args.warmup,
                'hybrid_alpha': args.hybrid_alpha,
                'embedding_model': 'deterministic-hash-v1',
            },
        }

        sqlrite_proc = start_sqlrite_server(binary, sqlrite_db, 'brute_force')
        sqlrite_client = JsonHttpClient('127.0.0.1', SQLRITE_PORT)
        pg_client = PgClient()
        vector_results = [
            benchmark('SQLRite brute_force compact_http', queries, args.warmup, args.top_k, lambda q: sqlrite_vector_query(sqlrite_client, q, args.top_k, args.top_k)),
            benchmark('sqlite-vec exact', queries, args.warmup, args.top_k, lambda q: sqlite_vec_client.exact_vector(q.embedding, args.top_k)),
            benchmark('pgvector exact', queries, args.warmup, args.top_k, lambda q: pg_client.exact_vector(q.embedding, args.top_k)),
        ]
        sqlrite_client.close()
        sqlrite_client = None
        stop_process(sqlrite_proc)
        sqlrite_proc = None

        sqlrite_proc = start_sqlrite_server(binary, sqlrite_db, 'brute_force')
        sqlrite_client = JsonHttpClient('127.0.0.1', SQLRITE_PORT)
        hybrid_results = [
            benchmark('SQLRite hybrid compact_http', queries, args.warmup, args.top_k, lambda q: sqlrite_hybrid_query(sqlrite_client, q, args.top_k, args.hybrid_alpha, max(args.top_k * 20, 100))),
            benchmark('pgvector hybrid', queries, args.warmup, args.top_k, lambda q: pg_client.hybrid(q.text, q.embedding, args.top_k, args.hybrid_alpha)),
        ]
        sqlrite_client.close()
        sqlrite_client = None
        pg_client.close()
        pg_client = None
        stop_process(sqlrite_proc)
        sqlrite_proc = None

        for item in vector_results:
            if item['system'].startswith('SQLRite'):
                item['setup_seconds'] = sqlrite_setup
            elif item['system'].startswith('sqlite-vec'):
                item['setup_seconds'] = sqlite_vec_setup
            else:
                item['setup_seconds'] = pg_setup
        for item in hybrid_results:
            item['setup_seconds'] = sqlrite_setup if item['system'].startswith('SQLRite') else pg_setup

        report['vector_exact'] = {'results': vector_results}
        report['hybrid'] = {'results': hybrid_results}
        args.output.write_text(json.dumps(report, indent=2) + '\n')
        write_markdown(args.output_md, report)
        print(json.dumps(report, indent=2))
        return 0
    finally:
        if sqlrite_client is not None:
            sqlrite_client.close()
        if sqlite_vec_client is not None:
            sqlite_vec_client.close()
        if pg_client is not None:
            pg_client.close()
        stop_process(sqlrite_proc)
        docker_rm('sqlrite-pgvector-public')


if __name__ == '__main__':
    raise SystemExit(main())
