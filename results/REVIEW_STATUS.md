# Review corrections, 2026-11-19

Raw historical results are retained as provenance; do not interpret them as
measurements of the corrected implementation.

## What remains valid

Exact full-domain avalanche scores, the scalar/SIMD agreement checks, and
sampled BIC counts are unaffected. The 0.11527422572175930 headline score was
independently reproduced during review.

## What changed

- Counts use floating-point n/2 for odd sample counts. Sampled mean square is
  now exactly unbiased: `(n*raw_ms - 1)/(n-1)`, rather than `raw_ms - 1/n`.
- Surrogate/scaling baselines rank signed mean-square estimates. Old CSVs
  store clipped square-root scores, so their negative observations cannot
  be recovered. New column names include `sampled_ms` to distinguish units.
- Sampled evaluation reports signed mean square and an approximate SE for
  the pooled estimator. Weighted batch residuals estimate the covariance
  between entries. Under a Gaussian approximation, with per-input covariance
  Sigma and mean vector mu, the variance is
  `(4*mu' Sigma mu/n + 2*tr(Sigma^2)/n^2)/N^2`. Finite-batch Wishart
  corrections estimate the trace term; estimated negative variance components
  are clamped to zero. This may be conservative near zero and is not a
  confidence sequence. All n requested inputs are used.
- Racing releases unfinished candidates from the seen set. Its threshold
  now allocates Gaussian tail probability across candidates and possible
  looks within each race, including sampled verification. This remains a
  model-based heuristic: it does not correct a miscalibrated covariance model
  or give a guarantee across an entire adaptive search.
- Non-power-of-two race caps are respected. Invalid sample budgets are
  rejected. The paired report no longer equates a variance ratio with a
  sample-count ratio in the noise-dominated 1/n regime.

## Historical results requiring reruns

- `refs.txt`: point scores use the old small finite-n approximation; the
  displayed score-scale uncertainties do not estimate pooled uncertainty.
  Do not reuse the old significance conclusions without a rerun.
- `surrogate_*.csv`, `neighbours_record32_surrogate.csv`, and `scale_*.csv`:
  clipped baseline rankings; scaling may also use odd sample counts.
- `race*.csv` and `family_races/`: exact accepted scores remain valid, but
  search efficiency and stagnation conclusions predate bookkeeping and
  multiple-look fixes. Existing traces cannot recover abandoned candidates.
- `paired*.csv`: historical subtraction-only correction. The finite-n
  difference is tiny for their sample sizes. The old cost model is an oracle
  budget estimate, not a measured sequential error guarantee.

## Corrected research claims

The parity result needs the shift conditions in the main README. A general
input difference can be multi-bit. The full mixer is not a T-function:
right shifts import high-bit dependencies. Fixed low multiplier bits are an
experimental search restriction, not a guarantee about output bits. Two
ablations do not establish impossibility of all Markov-style surrogates.

## Reproduction

Run from `mixing-functions/harness`:

```
cargo test --release
cargo test --release sampled_estimator_converges_to_exact -- --ignored
cargo run --release -- surrogate --bits 32 --list ../results/published_2round32.csv --baseline p20 --top 0.5 --seed 1 --out ../results/review_surrogate_p20.csv
cargo run --release -- surrogate --bits 32 --list ../results/published_2round32.csv --baseline p11 --top 0.5 --seed 1 --out ../results/review_surrogate_p11.csv
```

The corrected baseline files are new artifacts; original measurements remain
unchanged. Long 64-bit reference tables and multi-seed search benchmarks are
follow-up experiments, not silently regenerated results.

## Corrected published-config baseline rerun

Seed 1; 116 published configurations, with the best half selected by exact
score. These are signed-ms Spearman correlations from the commands above:

| Predictor | All 116 | Best 58 |
|---|---:|---:|
| Surrogate | 0.4403 | 0.6814 |
| Sampling, 2^20 | 0.3574 | 0.2413 |
| Sampling, 2^11 | 0.0641 | -0.2966 |

The broad conclusion on this dataset survives, but this is still one seed
and does not demonstrate within-family predictive power. The width-scaling,
random-16 and neighbor baselines have not been regenerated.
