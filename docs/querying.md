# Querying

This guide covers the main CLI retrieval patterns.

## Retrieval Modes

| Pattern | Use when | Main flags |
|---|---|---|
| text-only | lexical retrieval is enough | `--text` |
| vector-only | you already have embeddings | `--vector` |
| hybrid | you want lexical and semantic ranking together | `--text`, `--vector`, `--alpha` |
| metadata-filtered | you need hard tenant or topic boundaries | `--filter` |
| document-scoped | you want one document boundary | `--doc-id` |
| latency or recall tuned | you want predictable tradeoffs | `--query-profile` |
| RRF | you want rank-based fusion | `--fusion rrf` |

Create a demo database first:

```bash
sqlrite init --db sqlrite_demo.db --seed-demo
```

## Text-only

```bash
sqlrite query --db sqlrite_demo.db --text "keyword signals retrieval" --top-k 3
```

## Vector-only

```bash
sqlrite query --db sqlrite_demo.db --vector 0.95,0.05,0.0 --top-k 3
```

## Hybrid

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "local memory" \
  --vector 0.95,0.05,0.0 \
  --alpha 0.65 \
  --top-k 3
```

## Metadata-filtered

```bash
sqlrite init --db sqlrite_filter_demo.db --seed-demo
sqlrite ingest \
  --db sqlrite_filter_demo.db \
  --id chunk-meta-1 \
  --doc-id doc-meta-1 \
  --content "Agent memory stays local for demo tenants." \
  --embedding 0.95,0.05,0.0 \
  --metadata '{"tenant":"demo","topic":"memory"}'

sqlrite query \
  --db sqlrite_filter_demo.db \
  --text "agent memory" \
  --filter tenant=demo \
  --filter topic=memory \
  --top-k 5
```

Expected result:

```text
query_profile=balanced resolved_candidate_limit=500
results=1
1. chunk-meta-1 | doc=doc-meta-1 | hybrid=1.000 | vector=0.000 | text=1.000
   Agent memory stays local for demo tenants.
```

## Document scope

```bash
sqlrite query --db sqlrite_demo.db --text "local memory" --doc-id doc-a --top-k 3
```

## Query profiles

| Profile | Best for | Typical effect |
|---|---|---|
| `latency` | fast interactive calls | smaller candidate set |
| `balanced` | default use | moderate candidate set |
| `recall` | broader search | larger candidate set |

```bash
sqlrite query --db sqlrite_demo.db --text "local memory" --query-profile latency --top-k 5
```

## RRF fusion

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "local memory" \
  --vector 0.95,0.05,0.0 \
  --fusion rrf \
  --rrf-k 60 \
  --top-k 5
```

## How to choose

| You have | Start with |
|---|---|
| only user text | text-only |
| only embeddings | vector-only |
| both text and embeddings | hybrid |
| hard tenant/topic constraints | metadata-filtered |
| tight latency SLOs | `--query-profile latency` |
