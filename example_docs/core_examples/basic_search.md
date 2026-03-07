# Example: `basic_search`

Source file:

- `examples/basic_search.rs`

## Purpose

This is the smallest embedded Rust example in the repository.

Use it when you want to understand the minimum code needed to:

- open SQLRite in memory
- seed a few chunks
- run a hybrid search
- print ranked results

## Run It

```bash
cargo run --example basic_search
```

## What the Example Does

| Step | Description |
|---|---|
| open database | creates an in-memory SQLRite database |
| seed chunks | inserts three demo chunks with metadata |
| build request | creates a hybrid search request for `local-first sqlite` |
| run search | executes ranked retrieval |
| print rows | prints the top results in a compact format |

## Observed Output

```text
== basic_search results ==
c3 | doc=doc-sqlite | score=0.997
c2 | doc=doc-rag | score=0.576
```

## What to Notice

- `c3` wins because its content is closest to `local-first sqlite`
- the example uses `SearchRequest::hybrid(...)`, which is the most direct embedded API for mixed retrieval
- everything runs in memory, so there is no file cleanup or persistent state to manage

## Good Follow-Up Changes

- replace the seeded chunks with your own content
- switch to `SearchRequest::text(...)` or `SearchRequest::vector(...)`
- persist to a file-backed database when you want repeatable state
