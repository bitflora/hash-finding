# mixlab — mixer evaluation & search harness

Rust, multi-threaded (rayon). Builds with `target-cpu=native` (see
`.cargo/config.toml`), so the binary is tuned for this machine.

```
cargo build --release
cargo test --release                 # fast checks (bijection, inverses, 16-bit published values, fast path == generic)
cargo test --release -- --ignored    # slow: exact 32-bit published values, sampled-estimator convergence
```

## Commands

### Batch screening of supplied 32-bit candidates

```
cargo build --release --example screen_list
target/release/examples/screen_list INPUT.csv OUTPUT.csv 22 2401
```

The first input column must be `spec`. This example calls the existing SIMD
sampler and writes signed debiased `sampled_ms`, sample count, seed and runtime.
It resumes by skipping completed specs and rejects changes to the sample budget
or seed. It does not estimate uncertainty or establish improvements; use fresh
samples for selection and `mixlab eval --exact` for final verification.

`analysis/new_core_search.py` uses this wrapper for the reproducible 2026-11-24
small-denominator and shared-low-11-bit core search. Run its `generate`, `screen`,
`exact`, `bounded`, `verify`, `history`, and `summary` stages in order from the repository root.

### Independent spectral seed search

`seed-search` explores multiplier populations independently of the local-neighbour
search. It uses the same evaluator, with a fixed sample budget per candidate.
Build with `cargo build --release --bin seed-search` (ordinary `cargo run` still
defaults to `mixlab`). From `harness/`:

```
target/release/seed-search --source spectral --bits 32 --count 64 --samples p20 --seed 1 --verify 4 --out ../results/spectral32.csv
target/release/seed-search --source random   --bits 32 --count 64 --samples p20 --seed 1 --verify 4 --out ../results/random32.csv
target/release/seed-search --source matched  --bits 32 --count 64 --samples p20 --seed 1 --verify 4 --out ../results/matched32.csv
target/release/seed-search --source mixer    --bits 32 --count 64 --samples p20 --seed 1 --verify 4 --out ../results/mixer32.csv
```

- `spectral`: ordered pairs from Steele–Vigna Tables 4–7; provenance and the
  exact source version are in `data/README.md`. The default uses full-width
  constants (two per word width), with three choices for each shift around
  half-width, hence 108 distinct default candidates.
- `random`: uniform odd multipliers across the full word domain.
- `matched`: random odd multipliers with lengths drawn from the spectral pool,
  to distinguish spectral selection from the effect of multiplier length.
- `mixer`: recombinations of constants from the existing two-round Xmx reference
  mixers, with the same shift grid. This is a seed-pool control, not a replay of
  the original reference functions or the local-search algorithm.

`--min-mult-bits 1` includes shorter table constants at their original word
width; it filters the spectral/mixer pools and the matched lengths, but does
not restrict `random`. `--shifts 15,16,17` sets the common grid (up to nine
distinct shifts). Generation is deterministic, without duplicate configurations;
requests larger than a finite pool fail. No constants are truncated to fit.

Use `--generate-only` to export without evaluating. The first column is a mixer
specification, so generated or screened CSVs feed the existing 32-bit search:

```
target/release/mixlab race --bits 32 --start "[16 21f0aaad 15 735a2d97 15]" --candidates ../results/spectral32.csv --seed 2
```

Screening retains **signed** debiased mean-square scores, approximate standard
errors, seeds and cumulative elapsed seconds. `--verify K` selects the K lowest
screening scores and writes `OUT.csv.verified.csv`: exact enumeration at 32 bits,
fresh samples at 64 bits (`--verify-samples p26` by default). Zero is the default
verification count. Output files are not overwritten. Use different output paths
for different runs. At 64 bits a sampled winner is not an exact record or a
statistically established improvement; screening can miss good candidates.

The commands above equalize candidate counts, sample counts and finalist counts,
not elapsed time. Compare recorded runtime too, and repeat over multiple seeds
before interpreting transfer from spectral quality to mixer quality. Finalists
can then seed ordinary local searches. No carry-state surrogate or alternative
linear-layer architecture is implemented by this command.

```
mixlab refs   [--exact32] [--samples p26] [--extra]       # table of literature mixers
mixlab eval   --name lowbias32 --exact [--extra]
mixlab eval   --bits 32 --spec "[16 21f0aaad 15 735a2d97 15]" --exact
mixlab eval   --bits 64 --spec "[27 3c79ac492ba7b653 33 1c69b3f74ac4ae35 27]" --samples p28
mixlab sweep  --bits 8  --exhaustive --out ../results/sweep8.csv
mixlab sweep  --bits 16 --random 100000 --seed 1 --out ../results/sweep16.csv
mixlab search --bits 32 --start "[16 21f0aaad 15 735a2d97 15]" --iters 40 --seed 1 --out ../results/search32.csv
mixlab search --bits 64 --rounds 2 --iters 100 --final-samples 26 --seed 1 --out ../results/search64.csv
```

`p<k>` means 2^k.

## Metrics

- **bias**: hash-prospector's score, `1000·sqrt(mean_jk diff²)`, where
  `diff = (count_jk − n/2)/(n/2)`. Exact over all 2^bits inputs for widths of
  32 bits or fewer. This reproduces the published 32-bit values to 17 digits.
  hash-prospector's *16-bit* figures leave out the ×1000; this tool always
  includes it.
- **sampled bias**: the unbiased mean-square estimate is
  `(n*raw_ms - 1)/(n-1)`, which can legitimately be negative. Rankings and
  baseline CSV columns (`sampled_ms`, `sampled_ms32_eqcost_w...`) retain that
  signed value. Only the displayed bias uses `1000*sqrt(max(0, ms))`.
  Uncertainty is reported on the mean-square scale. Weighted batch residuals
  estimate the covariance of SAC entries; a Gaussian batch approximation
  estimates the variance of the **pooled** point estimate. All requested
  inputs are used, including batch remainders. This is approximate, not an
  always-valid confidence bound; `eval` needs at least 32 inputs.
- `--extra`:
  - **structured avalanche** reports `ms·n` on counter inputs, on RRC-style
    inputs (reversed, rotated and complemented counters) and on
    low-Hamming-weight inputs. A value near 1 means it looks ideal; much larger
    than 1 means structure is leaking through.
  - **BIC** gives the z-scores of the pairwise correlation between output-bit
    flips. An RMS near 1 is ideal; compare the max against the stated
    expected max.

## Analytic surrogate (`surrogate.rs`)

```
mixlab surrogate --bits 32 --spec "[16 21f0aaad 15 735a2d97 15]" --compare
mixlab surrogate --bits 32 --list ../results/published_2round32.csv --baseline p20 --top 0.5 --out F.csv
mixlab surrogate --bits 16 --random 5000 --baseline p12 --top 0.1
```

- This is a carry-chain differential model (Markov assumption), about 0.05 ms
  per 32-bit configuration.
- It is near-exact for one multiply. For 2-round mixers its absolute values
  collapse to about 10⁻¹¹, so use it **only for ranking**.
- Family-screening performance is tentative. Historical sampling baselines
  clipped negative estimates before ranking and must be recomputed.
- `--hybrid A|B1|B --samples p24` runs the ablation estimators (exact
  difference up to the last multiply, modelled after it). They showed that these approximations missed substantial bias on two
  mixers; they do not rule out every Markov-style model.

## Width scaling (`scale.rs`)

```
mixlab scale --list ../results/published_2round32.csv --widths 12,16,20,24 --baseline --out F.csv
mixlab scale --bits 24 --random 1200 --seed 7 --widths 12,16,20 --baseline --out F.csv
```

- Projects each mixer to smaller widths: shifts with `prop` or `offset`,
  multipliers with `low` or `high` (choose with `--maps offset:low,...`).
- Scores each projection exactly and reports its Spearman ρ against the
  full-width exact bias, per group, for the best half and for the best 10%.
- `--baseline` adds equal-cost sampling of the real mixer.
- `--truth` computes missing full-width exact values.
- **Result:** projection is a poor predictor, and sampling at equal cost wins.
  See `../plan.md`.

## Noise control (`racing.rs`)

```
mixlab paired --bits 32 --base "[16 21f0aaad 15 735a2d97 15]" --neighbours 24 --samples p18 --reps 32 --truth --out F.csv
mixlab design --bits 32 --spec "[16 21f0aaad 15 735a2d97 15]" --samples p20 --reps 24 --k 10
python ../analysis/racing_cost.py F.csv 32 1.1458e-8 3
```

- `paired` measures the variance of ms(A) − ms(B) over replicates, with
  independent inputs and with common inputs, against neighbours of a base
  mixer (or `--random K` configurations, or an `--others` list). `--truth`
  adds exact differences (32 bits or fewer).
- `design` compares i.i.d., low-bit-stratified and affine-subspace input
  designs.
- **Result:** sd(mŝ) ≈ √(2/N)/n for good mixers (N = bits²). Common inputs and
  the tested input designs gave little saving. The historical cost model
  predicted about ×11 from adaptive budgets; this is not a calibrated
  sequential-testing guarantee. See
  `../plan.md`.

## Racing search (`race`, recommended)

```
mixlab race --bits 32 --start "[16 21f0aaad 15 735a2d97 15]" --iters 12 --neighbors 96 --seed 11 --out F.csv
mixlab race --bits 32 --start SPEC --candidates LIST.csv          # one race of a fixed list (validation)
mixlab race --bits 64 --start SPEC --verify-samples p32 --cap p31 --out F.csv
```

Each iteration races K fresh neighbours against the incumbent:
1. Every candidate that is still undecided gets its sample count doubled,
   starting at 2^14.
2. A z-test uses the approximate noise law: sd ≈ √((4·ms/n + 2/n²)/N).
   The nominal `--z` (default 3) becomes `sqrt(z² + 2*ln(K*(L+1)))`,
   allocating Gaussian tail probability across K candidates, L possible
   sampling looks and one verification per candidate. This corrects peeking
   within a race under the model; correlated entries, non-Gaussian tails and
   repeated search iterations still prevent a rigorous overall guarantee.
   Candidates worse by this threshold are dropped; better ones are verified.
3. Candidates still undecided at the cap (2^28 by default) are verified in
   order of their estimate, best first.
4. The first verified improvement is accepted. Unfinished candidates remain
   eligible for later iterations rather than being permanently blacklisted.

Verification is exact for 32 bits or fewer. For 64 bits it is a fresh sample
of `--verify-samples` inputs that must itself win at the adjusted threshold, and the new incumbent
is then re-estimated from another fresh sample.

The log's `detail` column shows how many candidates were dropped and at what
sample count, plus the inputs and exact evaluations spent.

**Historical validation (before review fixes; timings need rerunning):**
- Against the record's 24 known-worse neighbours, all were dropped correctly
  using 2^29.7 inputs, in 54 s (exact scoring takes about 12 min).
- Starting from 0.1076, racing the 57 issue #19 constants found a verified
  better one (0.10734) in 187 s (exact scoring takes about 28 min).

## Search (older, fixed successive halving; can drop the true best on noise)

`search` is a local search with successive halving. Each iteration:
1. Generate K neighbours of the current configuration: change one shift by ±1,
   or flip 1–3 bits of one multiplier.
2. Score them on 2^16, then 2^18, then 2^20, then 2^22 random inputs, keeping
   the best quarter after each round. All candidates in a round use the same
   inputs (common random numbers).
3. Give the finalists an exact evaluation (32-bit or smaller) or a 2^26-sample
   evaluation with a fixed seed (64-bit).
4. Accept a finalist only if it beats the current configuration.

For 64-bit results, re-check any winner with a fresh `--seed` and more
samples before claiming anything.

## Performance (Ryzen 5 5600H, 12 threads)

- Exact 32-bit, 2-round: about 29 s per configuration (AVX2 path, `fast32.rs`).
- Exact 16-bit sweep: about 3,800 configurations per second (20,000 random
  2-round configurations in 5.2 s, parallel across configurations).
- 64-bit sampled, 2³⁰ inputs: about 80–120 s per mixer.

The hybrid B estimator computes joint carry correlations in O(bits) per
sample using `Cov(c_k, c_{k+s}) = 2^-s Var(c_k)`, with stack storage for carry
marginals. Previously it replayed the carry chain per output bit and allocated
a vector per sample. In a local release-build comparison (three runs each),
the command below improved from a median 1.07 s to 0.12 s (about 9×), with
identical displayed scores. Timings depend on the machine; floating-point
rounding can differ. Exhaustive 2–8-bit addition validates the joint model.

```
mixlab surrogate --bits 32 --spec "[16 21f0aaad 15 735a2d97 15]" --hybrid B --samples p16
```
