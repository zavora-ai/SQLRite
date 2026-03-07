"""HTTP JSON client for SQLRite server surfaces."""

from __future__ import annotations

import json
import urllib.error
import urllib.request
from dataclasses import dataclass
from typing import Any


@dataclass
class SqlRiteApiError(Exception):
    """Raised when SQLRite returns an error response."""

    status_code: int
    message: str
    payload: Any | None = None

    def __str__(self) -> str:
        return f"sqlrite api error (status={self.status_code}): {self.message}"


class SqlRiteClient:
    """Minimal Python SDK for SQLRite query and SQL endpoints."""

    def __init__(self, base_url: str = "http://127.0.0.1:8099", timeout_s: float = 10.0) -> None:
        self.base_url = base_url.rstrip("/")
        self.timeout_s = timeout_s

    def health(self) -> dict[str, Any]:
        return self._request_json("GET", "/healthz")

    def ready(self) -> dict[str, Any]:
        return self._request_json("GET", "/readyz")

    def openapi(self) -> dict[str, Any]:
        return self._request_json("GET", "/v1/openapi.json")

    def sql(self, statement: str) -> dict[str, Any]:
        payload = {"statement": statement}
        return self._request_json("POST", "/v1/sql", payload)

    def query(
        self,
        query_text: str | None = None,
        query_embedding: list[float] | None = None,
        top_k: int | None = None,
        alpha: float | None = None,
        candidate_limit: int | None = None,
        query_profile: str | None = None,
        metadata_filters: dict[str, str] | None = None,
        doc_id: str | None = None,
    ) -> dict[str, Any]:
        payload: dict[str, Any] = {}
        if query_text is not None:
            payload["query_text"] = query_text
        if query_embedding is not None:
            payload["query_embedding"] = query_embedding
        if top_k is not None:
            payload["top_k"] = top_k
        if alpha is not None:
            payload["alpha"] = alpha
        if candidate_limit is not None:
            payload["candidate_limit"] = candidate_limit
        if query_profile is not None:
            payload["query_profile"] = query_profile
        if metadata_filters is not None:
            payload["metadata_filters"] = metadata_filters
        if doc_id is not None:
            payload["doc_id"] = doc_id
        return self._request_json("POST", "/v1/query", payload)

    def _request_json(
        self,
        method: str,
        path: str,
        payload: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        body = None
        headers = {"accept": "application/json"}

        if payload is not None:
            body = json.dumps(payload).encode("utf-8")
            headers["content-type"] = "application/json"

        request = urllib.request.Request(
            f"{self.base_url}{path}",
            data=body,
            headers=headers,
            method=method,
        )

        try:
            with urllib.request.urlopen(request, timeout=self.timeout_s) as response:
                raw = response.read().decode("utf-8")
                if not raw.strip():
                    return {}
                return json.loads(raw)
        except urllib.error.HTTPError as error:
            raw = error.read().decode("utf-8") if error.fp is not None else ""
            parsed: Any | None = None
            message = raw
            if raw:
                try:
                    parsed = json.loads(raw)
                    if isinstance(parsed, dict) and "error" in parsed:
                        message = str(parsed["error"])
                except json.JSONDecodeError:
                    pass
            raise SqlRiteApiError(error.code, message, parsed) from error
        except urllib.error.URLError as error:
            raise SqlRiteApiError(0, f"connection error: {error}") from error
