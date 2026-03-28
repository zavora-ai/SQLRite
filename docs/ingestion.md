# Ingestion and Reindexing

SQLRite supports direct CLI ingest, embedded ingest APIs, checkpointed ingestion workers, and reindexing for embedding-model changes.

## Direct CLI ingest

```bash
sqlrite ingest \
  --db sqlrite_docs.db \
  --id chunk-1 \
  --doc-id doc-1 \
  --content "Local agent memory should be fast and predictable." \
  --embedding 0.95,0.05,0.0 \
  --metadata '{"tenant":"demo","topic":"memory"}'
```

Use this for small tests, samples, and direct fixtures.

## Checkpointed ingestion worker

Example: `/Users/jameskaranja/Developer/projects/SQLRight/examples/ingestion_worker.rs`

Run it:

```bash
cargo run --example ingestion_worker
```

What it demonstrates:

- chunking strategies
- restart-safe checkpoints
- embedding provider integration
- follow-up retrieval after ingest

## Reindexing

Use reindexing when your embedding model changes or you want a different vector storage profile.

```bash
sqlrite-reindex \
  --db sqlrite_demo.db \
  --provider deterministic \
  --model det-v1 \
  --dims 64
```

External providers exist too, but they require real credentials or a reachable embedding service.

## When to use which ingest path

| Need | Best path |
|---|---|
| one chunk or a quick fixture | `sqlrite ingest` |
| embedded Rust app | `db.ingest_chunks(...)` |
| restart-safe batch ingest | `IngestionWorker` |
| refresh embeddings | `sqlrite-reindex` |
