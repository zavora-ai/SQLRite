# S31 Competitor Gap Analysis

Date: March 7, 2026

## Focus of this review

This review is limited to roadmap scope covered in S31:

- API-first vector database migration ergonomics
- concise SQL-native retrieval syntax
- rerank-ready and query-profile-aware SQL/server workflows

## Shipped in S31

- native JSONL import commands for Qdrant, Weaviate, and Milvus export shapes
- `SEARCH(...)` SQL v2 prototype in CLI SQL mode and server `/v1/sql`
- validation harness covering API-first migration, SQL v2, and rerank-hook compatibility

## Remaining gaps after S31

- no direct network pull connectors from remote Qdrant, Weaviate, or Milvus clusters
- `SEARCH(...)` is a rewrite-based prototype, not a true SQLite virtual table module
- no built-in cross-encoder reranker packaged in-process yet
- no source-specific export assistants for managed vendor backup formats yet

## Target follow-through

- S32+: release-hardening and defect burn-down for SQL v2 semantics
- post-v1: native remote export connectors and richer `SEARCH` syntax
