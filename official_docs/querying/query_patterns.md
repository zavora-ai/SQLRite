# Query Patterns Guide

This guide explains the main retrieval patterns exposed by the SQLRite CLI.

## Retrieval Modes at a Glance

| Pattern | Use when | Main flags | Typical output signal |
|---|---|---|---|
| Text-only | you want lexical retrieval | `--text` | `text` dominates |
| Vector-only | you already have embeddings | `--vector` | `vector` dominates |
| Hybrid | you want vector and text together | `--text`, `--vector`, `--alpha` | `hybrid` reflects both |
| Metadata-filtered | you need exact metadata constraints | `--filter` | result set is narrowed before ranking |
| Document-scoped | you want one document boundary | `--doc-id` | only rows from one document |
| Query-profile tuned | you want latency/recall tradeoffs | `--query-profile` | candidate limit changes |
| RRF fusion | you want rank-based fusion | `--fusion rrf` | rank blend instead of weighted score blend |

## Before You Start

Create a reproducible demo database:

```bash
sqlrite init --db sqlrite_demo.db --seed-demo
```

## 1. Text-Only Retrieval

Use text-only retrieval when your query is naturally lexical and you do not already have an embedding vector.

```bash
sqlrite query --db sqlrite_demo.db --text "keyword signals retrieval" --top-k 3
```

What to expect:

- `text` score carries the ranking
- `vector` stays `0.000`

## 2. Vector-Only Retrieval

Use vector-only retrieval when embeddings are produced elsewhere and you want pure semantic ranking.

```bash
sqlrite query --db sqlrite_demo.db --vector 0.95,0.05,0.0 --top-k 3
```

What to expect:

- `vector` score drives ranking
- `text` stays `0.000`

## 3. Hybrid Retrieval

Use hybrid retrieval when you want vector similarity and lexical ranking to contribute to the final ordering.

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "local memory" \
  --vector 0.95,0.05,0.0 \
  --alpha 0.65 \
  --top-k 3
```

How `alpha` works:

| `alpha` value | Effect |
|---|---|
| closer to `0.0` | more weight on text ranking |
| around `0.5` | more balanced |
| closer to `1.0` | more weight on vector similarity |

## 4. Metadata-Filtered Retrieval

Use metadata filters when you need exact constraints before ranking.

Set up a scratch database so the example stays reproducible:

```bash
sqlrite init --db sqlrite_filter_demo.db --seed-demo
sqlrite ingest \
  --db sqlrite_filter_demo.db \
  --id chunk-meta-1 \
  --doc-id doc-meta-1 \
  --content "Agent memory stays local for demo tenants." \
  --embedding 0.95,0.05,0.0 \
  --metadata '{"tenant":"demo","topic":"memory"}'
```

Now query with filters:

```bash
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

## 5. Document-Scoped Retrieval

Use document scope when you want to search only within one document.

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "local memory" \
  --doc-id doc-a \
  --top-k 3
```

What to expect:

- only chunks from `doc-a` are considered

## 6. Query Profiles

Query profiles control the candidate-search tradeoff.

| Profile | Best for | Typical effect |
|---|---|---|
| `latency` | fast agent calls | smaller candidate set |
| `balanced` | default use | moderate candidate set |
| `recall` | broader search | larger candidate set |

Example:

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "local memory" \
  --query-profile latency \
  --top-k 5
```

What changes:

- the result header shows a different `resolved_candidate_limit`
- lower latency profiles inspect fewer candidates

## 7. Reciprocal-Rank Fusion (RRF)

Use RRF when weighted score fusion is too sensitive to score scale and you want rank-based merging.

```bash
sqlrite query \
  --db sqlrite_demo.db \
  --text "local memory" \
  --vector 0.95,0.05,0.0 \
  --fusion rrf \
  --rrf-k 60 \
  --top-k 5
```

## Choosing the Right Pattern

| You have | Start with |
|---|---|
| just user text | text-only |
| just embeddings | vector-only |
| user text and embeddings | hybrid |
| hard tenant/topic constraints | metadata filters |
| long documents with internal boundaries | doc-scoped retrieval |
| tight latency SLOs | `--query-profile latency` |
| unstable score scales across signals | `--fusion rrf` |

## Next Step

Continue with `official_docs/sql/sql_retrieval_guide.md` if you want the SQL-native retrieval surface instead of the CLI-only flow.
