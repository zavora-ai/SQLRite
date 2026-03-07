# Example: `ingestion_worker`

Source file:

- `examples/ingestion_worker.rs`

## Purpose

This example shows the resumable ingestion path rather than direct one-row inserts.

Use it when you need:

- checkpointed ingestion
- deterministic embeddings for local development
- chunking of longer text
- a follow-up retrieval check after ingest

## Run It

```bash
cargo run --example ingestion_worker
```

## What the Example Does

| Step | Description |
|---|---|
| open database | creates an in-memory SQLRite database |
| create provider | uses the deterministic embedding provider |
| configure checkpoint | stores progress in a temp checkpoint file |
| build ingestion request | configures chunking, metadata, and batch size |
| ingest | loads and chunks the sample content |
| query | runs a text search to confirm ingest success |

## Observed Output

```text
== ingestion_worker report ==
ingested chunks: total=2, processed=2
search results: 2
```

## What to Notice

- the worker reports both total and processed chunks
- the follow-up query proves the new chunks are immediately searchable
- the deterministic provider makes this example stable across machines

## Good Follow-Up Changes

- point the request at a real file or document stream
- change the chunking strategy
- persist the database path and checkpoint path for longer-running imports
