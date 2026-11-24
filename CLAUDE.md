# CLAUDE.md

## Current best hash

The top of `README.md` records the current best hash and its exact bias. Whenever a new best is found, update it there (and add the finding to `FINDINGS.md`).

## Rust harness vs. Python scripts

Anything that *computes* a score lives in the harness (`harness/`, the `mixlab` binary), and Python only *uses* scores.

- The harness owns the bias metric, its exact and sampled evaluation, and any transformation that a score is taken of, such as the inverse mixer (`mixlab exact --inverse`). Never reimplement a scoring function in Python or numpy, even a "quick check" copy. A second copy can silently drift from mixlab, and every result rests on that metric.
- Python in `analysis/` does the glue around the harness: choosing which configs to score, building variant specs, running `mixlab` (`mixlab exact --list` scores a batch of specs exactly), parsing logs and CSVs, and statistics over the scores (Spearman, quantiles, multiplier features such as popcount or NAF).
- If a script needs a score that mixlab can't produce yet, add a subcommand or flag to mixlab (with a test in `harness/tests/`) instead of computing it in Python.
