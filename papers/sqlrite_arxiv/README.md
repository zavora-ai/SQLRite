# SQLRite arXiv Paper Draft

This directory contains the current arXiv-oriented manuscript for SQLRite.

## Primary files

| File | Purpose |
|---|---|
| `main.tex` | primary LaTeX manuscript |
| `references.bib` | bibliography |
| `build.sh` | local build wrapper |
| `embedded_competitive_snapshot.json` | current embedded benchmark snapshot used in the paper |
| `embedded_competitive_snapshot.md` | human-readable summary of the embedded benchmark snapshot |
| `public_dataset_results.json` | refreshed BEIR/SciFact benchmark results |
| `public_dataset_results.md` | human-readable summary of the public benchmark |
| `run_public_dataset_eval.py` | reproducible public benchmark harness |
| `submission_notes.md` | submission guidance |

## Current paper framing

The current manuscript is a systems-and-practice paper centered on SQLRite's embedded-first use case.

It makes three claims only:

- SQLRite is designed first for embedded local-first retrieval.
- SQLRite is currently strong on the repository's deterministic filtered embedded benchmark.
- SQLRite's hybrid retrieval path shows a meaningful quality win on the refreshed SciFact benchmark.

It does **not** claim universal dominance across all vector databases, deployment models, or workloads.

## Evidence used by the current draft

### Embedded benchmark snapshot

Source files:

- `embedded_competitive_snapshot.json`
- `embedded_competitive_snapshot.md`

This snapshot is the product-facing filtered cosine workload currently cited by the main repository docs.

### Public benchmark

Source files:

- `public_dataset_results.json`
- `public_dataset_results.md`
- `run_public_dataset_eval.py`

This benchmark uses BEIR/SciFact with deterministic hashed embeddings so it can be rerun locally.

## Build

```bash
cd /Users/jameskaranja/Developer/projects/SQLRight/papers/sqlrite_arxiv
./build.sh
```

`build.sh` prefers `tectonic`, then `latexmk`, then `pdflatex` + `bibtex`.

## Refresh the public benchmark

```bash
cd /Users/jameskaranja/Developer/projects/SQLRight
source papers/sqlrite_arxiv/.venv/bin/activate
papers/sqlrite_arxiv/run_public_dataset_eval.py \
  --max-queries 100 \
  --embedding-dim 128 \
  --top-k 10 \
  --warmup 10
```

## Notes

- `run_competitive_eval.py` remains in the paper folder as an auxiliary localhost harness, but the current manuscript is primarily grounded in `embedded_competitive_snapshot.*` and the refreshed public benchmark.
- If the paper is revised again, the most valuable next step is adding one more public dataset and one more hardware class.
