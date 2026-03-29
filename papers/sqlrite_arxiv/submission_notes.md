# Submission Notes

## Current framing

The manuscript is now strongest as:

- an arXiv systems paper
- a software and engineering paper
- a reproducibility-oriented report on an embedded-first retrieval engine

## What the paper now argues

The current argument is narrower and stronger than the earlier draft.

It does **not** argue that SQLRite is universally the fastest retrieval engine.
It argues that:

- SQLRite is an embedded-first retrieval system built around SQL-native access.
- SQLRite is currently competitive on its target filtered embedded workload.
- SQLRite's hybrid retrieval path can produce a better quality-throughput tradeoff on a judged public dataset than the pgvector baseline used here.

## Evidence the paper depends on

Primary:

- `embedded_competitive_snapshot.json`
- `public_dataset_results.json`

Supporting:

- `embedded_competitive_snapshot.md`
- `public_dataset_results.md`
- `run_public_dataset_eval.py`

## Claim discipline

Keep these boundaries explicit in the final submission:

- benchmark leadership is workload-specific
- embedded mode is SQLRite's strongest deployment path
- compact HTTP is a lower-overhead served path, not a replacement for the embedded claim
- public evaluation is still limited to SciFact in the current revision
- deterministic local embeddings improve reproducibility but do not replace a broader production-model evaluation

## What would strengthen the next revision

1. Add at least one more BEIR dataset.
2. Repeat the public benchmark on a second machine class.
3. Add a tighter apples-to-apples service benchmark that compares compact HTTP against equivalent low-overhead served paths.
4. Add one chart showing the embedded-versus-served SQLRite gap directly.
