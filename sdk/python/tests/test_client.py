from __future__ import annotations

import pathlib
import socket
import subprocess
import tempfile
import time
import unittest
import urllib.request

import sys

THIS_FILE = pathlib.Path(__file__).resolve()
PY_SDK_ROOT = THIS_FILE.parents[1]
REPO_ROOT = THIS_FILE.parents[3]
if str(PY_SDK_ROOT) not in sys.path:
    sys.path.insert(0, str(PY_SDK_ROOT))

from sqlrite_sdk import SqlRiteApiError, SqlRiteClient


def _pick_free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return int(sock.getsockname()[1])


def _wait_http_ready(url: str, timeout_s: float = 8.0) -> None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=0.5) as response:
                if response.status == 200:
                    return
        except Exception:
            time.sleep(0.1)
    raise RuntimeError(f"timed out waiting for {url}")


class SqlRiteClientIntegrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.tmpdir = tempfile.TemporaryDirectory(prefix="sqlrite-py-sdk-")
        cls.db_path = pathlib.Path(cls.tmpdir.name) / "sdk_integration.db"
        cls.port = _pick_free_port()
        cls.base_url = f"http://127.0.0.1:{cls.port}"

        cls.sqlrite_bin = REPO_ROOT / "target" / "debug" / "sqlrite"
        if not cls.sqlrite_bin.exists():
            subprocess.run(
                ["cargo", "build", "--bin", "sqlrite"],
                cwd=REPO_ROOT,
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
            )

        subprocess.run(
            [
                str(cls.sqlrite_bin),
                "init",
                "--db",
                str(cls.db_path),
                "--seed-demo",
                "--profile",
                "balanced",
                "--index-mode",
                "brute_force",
            ],
            cwd=REPO_ROOT,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )

        cls.server = subprocess.Popen(
            [
                str(cls.sqlrite_bin),
                "serve",
                "--db",
                str(cls.db_path),
                "--bind",
                f"127.0.0.1:{cls.port}",
            ],
            cwd=REPO_ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )

        _wait_http_ready(f"{cls.base_url}/readyz")
        cls.client = SqlRiteClient(cls.base_url)

    @classmethod
    def tearDownClass(cls) -> None:
        if getattr(cls, "server", None) is not None:
            cls.server.terminate()
            try:
                cls.server.wait(timeout=3)
            except subprocess.TimeoutExpired:
                cls.server.kill()

        if getattr(cls, "tmpdir", None) is not None:
            cls.tmpdir.cleanup()

    def test_openapi_contains_query_path(self) -> None:
        payload = self.client.openapi()
        self.assertIn("paths", payload)
        self.assertIn("/v1/query", payload["paths"])

    def test_query_returns_rows(self) -> None:
        payload = self.client.query(query_text="agent memory", top_k=2)
        self.assertEqual(payload.get("kind"), "query")
        self.assertGreaterEqual(payload.get("row_count", 0), 1)

    def test_sql_returns_rows(self) -> None:
        payload = self.client.sql("SELECT id, doc_id FROM chunks ORDER BY id ASC LIMIT 2;")
        self.assertEqual(payload.get("kind"), "query")
        self.assertEqual(payload.get("row_count"), 2)

    def test_query_validation_error_is_mapped(self) -> None:
        with self.assertRaises(SqlRiteApiError) as raised:
            self.client.query(top_k=2)

        self.assertEqual(raised.exception.status_code, 400)
        self.assertIn("query_text or query_embedding", raised.exception.message)


if __name__ == "__main__":
    unittest.main()
