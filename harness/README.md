# mixlab — mixer evaluation & search harness

Rust, multi-threaded (rayon). Builds with `target-cpu=native` (see
`.cargo/config.toml`), so the binary is tuned for this machine.

```
cargo build --release
cargo test --release                 # fast checks (bijection, inverses, 16-bit published values, fast path == generic)
cargo test --release -- --ignored    # slow: exact 32-bit published values, sampled-estimator convergence
```

## Commands

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
- **sampled bias** (64-bit, or when `--exact` is not given): over n random
  inputs (xoshiro256**). Sampling noise adds 1/n to the mean square, so the
  tool reports the *debiased* `1000·sqrt(ms − 1/n)` with a conservative
  standard error from 16 independent batches.
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
- It ranks families of constants better than sampling at equal cost, but it is
  weak within a family. See the experiment log in `../plan.md`.
- `--hybrid A|B1|B --samples p24` runs the ablation estimators (exact
  difference up to the last multiply, modelled after it). They were used to show
  that 2-round bias can't be captured by Markov-style models.

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
  input designs give no real saving. Sequential stopping saves about ×11. See
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
2. A z-test uses the verified noise law: sd ≈ √((4·ms/n + 2/n²)/N).
   - A candidate worse by more than z·sd (default z = 3) is dropped.
   - A candidate better by more than z·sd is verified.
3. Candidates still undecided at the cap (2^28 by default) are verified in
   order of their estimate, best first.
4. The first verified improvement is accepted.

Verification is exact for 32 bits or fewer. For 64 bits it is a fresh sample
of `--verify-samples` inputs that must itself win at z, and the new incumbent
is then re-estimated from another fresh sample.

The log's `detail` column shows how many candidates were dropped and at what
sample count, plus the inputs and exact evaluations spent.

**Validation:**
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
