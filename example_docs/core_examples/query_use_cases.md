# Example: `query_use_cases`

Source file:

- `examples/query_use_cases.rs`

## Purpose

This is the most complete retrieval-pattern example in the repository.

Use it when you want one file that shows:

- text-only retrieval
- vector-only retrieval
- hybrid retrieval
- metadata filters
- document scoping
- reciprocal-rank fusion
- candidate-limit tuning

## Run It

```bash
cargo run --example query_use_cases
```

## Sections Printed by the Example

| Section | Meaning |
|---|---|
| `1) Text-only query` | lexical retrieval |
| `2) Vector-only query` | semantic retrieval with a supplied embedding |
| `3) Hybrid query with alpha tuning` | weighted combination of text and vector signals |
| `4) Metadata-filtered query` | exact metadata constraints |
| `5) Doc-scoped query` | retrieval restricted to one document |
| `6) RRF fusion query` | rank-based fusion |
| `7) Candidate-limit tuning` | precision and latency tradeoff |

## Observed Output

```text
== 1) Text-only query ==
- c1 | doc=doc-rag | tenant=acme | topic=retrieval | hybrid=1.000 | vector=0.000 | text=1.000
- c2 | doc=doc-security | tenant=acme | topic=security | hybrid=0.000 | vector=0.000 | text=0.000
- c3 | doc=doc-ingest | tenant=acme | topic=ingestion | hybrid=0.000 | vector=0.000 | text=0.000

== 2) Vector-only query ==
- c1 | doc=doc-rag | tenant=acme | topic=retrieval | hybrid=0.999 | vector=0.999 | text=0.000
- c5 | doc=doc-rag | tenant=acme | topic=retrieval | hybrid=0.993 | vector=0.993 | text=0.000
- c3 | doc=doc-ingest | tenant=acme | topic=ingestion | hybrid=0.964 | vector=0.964 | text=0.000
```

The full example continues through hybrid, filtered, scoped, RRF, and candidate-limit cases.

## What to Notice

- the same seeded corpus can support several retrieval modes
- metadata filters change the candidate set before ranking
- RRF produces smaller fused values because it is rank-based rather than score-based
- candidate limits are a performance control, not just a result-count control

## Good Follow-Up Changes

- replace the seeded corpus with your own domain-specific content
- tune `alpha` to match your workload
- compare weighted fusion against RRF using the same queries
