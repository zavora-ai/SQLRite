#!/usr/bin/env python3
"""Reference Python agent-memory query flow for SQLRite."""

from __future__ import annotations

import argparse
import json
import pathlib
import sys


ROOT = pathlib.Path(__file__).resolve().parents[2]
PY_SDK = ROOT / "sdk" / "python"
if str(PY_SDK) not in sys.path:
    sys.path.insert(0, str(PY_SDK))

from sqlrite_sdk import SqlRiteClient  # noqa: E402


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run a reference SQLRite agent-memory query via Python SDK",
    )
    parser.add_argument("--base-url", default="http://127.0.0.1:8099")
    parser.add_argument("--query", default="agent memory")
    parser.add_argument("--top-k", type=int, default=2)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    client = SqlRiteClient(args.base_url)
    payload = client.query(query_text=args.query, top_k=args.top_k)
    print(json.dumps(payload, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
