# Mixing functions: findings for review (2026-11-19)

> **Review correction (2026-11-19):** Exact scores remain valid. Historical
> sampled comparisons, uncertainty estimates, and search timings below predate
> evaluator/search fixes; see [results/REVIEW_STATUS.md](results/REVIEW_STATUS.md).
> F3 and F10 have been corrected, and F5 narrowed.

**Audience:** agents or people reviewing this line of research. This is a
self-contained summary; the full chronological log is in [plan.md](plan.md)
(section "Experiment log") and the harness is documented in
[harness/README.md](harness/README.md).

**Topic:** constants for multiply-xorshift integer mixers ("finalizers"), in
hash-prospector notation `[s0 m0 s1 m1 s2]`:

    x ^= x >> s0; x *= m0; x ^= x >> s1; x *= m1; x ^= x >> s2

**Metric:** hash-prospector avalanche bias, 1000·sqrt(mean_jk diff²), where
diff = (count_jk − n/2)/(n/2) over the bits×bits table of "flip input bit j →
output bit k flips". It is exact over all 2^32 inputs at 32 bits and sampled
(unbiased mean square `(n*raw_ms - 1)/(n-1)` in current code) at 64 bits.

Each finding below is labelled with a confidence:
- **Solid**: reproduced or exact.
- **Indicative**: real data, but a small sample or a single run.
- **Hypothesis**: an untested idea.

---

## 1. How to reproduce

```
cd harness
cargo build --release
cargo test --release          # 8 fast tests; `-- --ignored` adds the slow 32-bit published-value checks
./target/release/mixlab eval --bits 32 --spec "[16 7870aaad 15 be523597 15]" --exact --extra   # ~29 s
```

- **Machine:** Ryzen 5 5600H (12 threads), Windows, Rust with
  `target-cpu=native` (AVX2).
- **Seeds:** every run records its seed in the result files.

## 2. Findings

### F1. The harness reproduces published values exactly (solid)
- 32-bit exact bias matches hash-prospector to 17 significant digits:
  - lowbias32 0.17353355999581582
  - TheIronBorn 0.10760229515479501 and 0.10704308166917044
  - prospector32 0.34968228323361017
  - triple32 0.02088857891973891
- The 16-bit values match too, after allowing for hash-prospector omitting the
  ×1000 factor at 16 bits.
- Tests: `harness/tests/validate.rs`. Exact 32-bit 2-round evaluation takes
  about 29 s on the AVX2 path (`src/fast32.rs`). It is bit-identical to the
  generic path, and a test checks this.

### F2. Historical reference table (uncertainty needs rerun; `results/refs.txt`)
- At 2^30 samples, only fmix64 (0.0183 ± 0.0024) and Moremur
  (0.0139 ± 0.0041) are measurably biased among the 64-bit references. Mix13,
  mx3, rrmxmx and NASAM are consistent with 0. **Sampled avalanche bias
  cannot rank good 64-bit mixers at affordable sample counts.**
- Pairwise output-flip correlation (BIC, 2^16 samples) shows a perfectly
  correlated pair (max |z| = 256 = √n) in fmix32, prospector32, lowbias32,
  both 16-bit references and Stafford Mix13. TheIronBorn's constants have
  max |z| = 86.6. triple32, mx3, NASAM and Moremur are clean (max |z| ≤ 9.2).

### F3. Structural parity relation with conditional single-bit triggers (solid)
The XOR of output bits `{0, s2, 2*s2, ...}` always flips when input bit
`s1` flips **if `s1 < s0`**, or when input bit `s0+s1` flips if that position
is inside the word. For arbitrary shifts, applying the inverse first xorshift
to `1 << s1` gives a possibly multi-bit input difference that always flips
this parity. Odd multiplication is triangular and preserves bit 0.

The original unconditional claim is false: `[2 03 3 05 4]` at 8 bits has
192/256 counterexamples for input bit 3. Exhaustive small-width tests cover
the corrected relation. This limitation is invisible to SAC when the parity
involves multiple output bits; it is not a blanket verdict on PRNG suitability.

### F4. A first-order analytic surrogate shows family-screening potential (indicative)
**Model.** A carry-chain differential model (`src/surrogate.rs`) with a
Markov/independence assumption between rounds, costing about 0.05 ms per
config.

**Results.**
- It is near-exact for one multiply: per-entry Pearson 0.997.
- For 2 rounds its absolute value collapses to about 10⁻¹¹, against an exact
  0.107.
- Rank correlation (Spearman ρ) against the exact bias:
  - all 116 published 32-bit configs: 0.4403
  - best half: 0.6814 (corrected signed-ms sampling at 2^20: 0.3574 / 0.2413)
  - within one family: 0.25 and −0.29, i.e. useless

**Method error caught and fixed:** printing values with 10 fixed decimals
created ties, which were broken by row order, and that order was sorted by
the true answer. This inflated the within-family ρ. The analysis now uses
`%e` output and averaged ranks for ties.

### F5. Tested Markov approximations miss substantial bias (limited negative result)
**Method.** An ablation (`hybrid_matrix`) keeps the difference *exact* up to
the last multiply and models only the rest. Share of the exact mean-square
recovered:

| variant | record 0.10704 | lowbias32 0.17353 |
|---|---|---|
| Markov surrogate | ≈0% | ≈0% |
| exact XOR difference before the last multiply | 0.8% | 2.2% |
| exact additive difference D at the last multiply | 21% | 54% |

**Interpretation.**
- The bias lives in the joint bit structure of D (a signed sum of about n/2
  shifted copies of m).
- It also lives in the dependence between values and differences, which
  the tested approximations assume away. Two ablated mixers do not rule out
  all Markov-style or higher-order models.
- The hybrid also gives no variance reduction (per-sample variance 0.21
  against 0.25), so it is not a faster estimator either.

### F6. Tested small-width proxies rank poorly (historical baselines need rerun)
**Method.** Each mixer was projected to 12/16/20/24 bits (shifts
proportionally or by offset from half-width; multipliers by their low or
high bits) and scored exactly at that width (`src/scale.rs`).

**Rank correlation (Spearman ρ) with the full-width exact score:**

| set | best projection | equal-cost sampling of the real mixer |
|---|---|---|
| 116 published 32-bit, all | 0.57 (w24); ρ flips sign between widths | 0.88 |
| published, best half | −0.17 (w24) | 0.58 |
| 1200 random 24-bit, best 10% | 0.38 (w20) | 0.94 (w16 cost) – 1.00 (w20 cost) |

**Why it fails.** Projected mixers sit 20–150× above their own noise floor.
They have strong defects of their own that don't track the full-width
mixer's defects.

### F7. Approximate sampled-bias noise law and tested variance-reduction methods
**The law.** For N = bits² entries and n i.i.d. inputs:

  Var(mŝ_debiased) ≈ (4·ms/n + 2/n²)/N.

For good mixers the 2/n² term (fluctuation of the noise floor) dominates up
to n ≈ 1/(2·ms): about 2^25.6 for the 32-bit record and about 2^31 for
Moremur. **So sd ≈ √(2/N)/n, which falls as 1/n.** This was verified at 32
bits (2^18–2^22) and at 64 bits (2^24).

**Tricks tried:**

| trick | variance saving |
|---|---|
| common random numbers for A vs B (32-bit, 24 neighbour pairs) | ×1.09 |
| the same at 64 bits (Moremur, 16 pairs, 8 reps, noisy) | ×1.20 |
| low-bit-stratified input blocks | worse (biased, sd ×1.0–1.5) |
| random affine-subspace input blocks (dimension 10) | ≈none |

**Sequential stopping is what works.** Deciding each comparison at z = 3
with as few samples as needed, the inputs needed to settle the record's 24
neighbours range from 2^17.2 to 2^29.0. Sequential stopping costs 2^30.2 in
total, against 2^33.6 for a fixed budget: ×10.9 (`analysis/racing_cost.py`).
Moremur needs about 2^29 samples just to show its own bias is nonzero at 3σ.

### F8. Historical racing validation and one search comparison (indicative)
**Method** (`src/racing.rs`, `mixlab race`). K = 96 neighbours per iteration
(1–3 bit flips in a multiplier, or a shift ±1):
- Each undecided candidate's sample count doubles from 2^14.
- A z-test against the noise law drops or promotes candidates.
- Candidates still undecided at the 2^28 cap are verified exactly.
- The first exact-verified improvement is accepted.

**Validation:**
- All 24 known-worse neighbours of the record were dropped correctly
  (2^29.7 inputs, 54 s, against about 12 min exact).
- Starting from 0.1076, it found the known better constant 0.10734 among 57
  candidates (187 s, against about 28 min exact).

**Comparison (indicative, one run each):** from lowbias32 after about 27 min,
racing reached 0.1423 and the old fixed successive-halving search reached
0.1492. The old search stalled for 8 iterations, which is consistent with it
dropping close competitors on noise.

### F9. New 2-round 32-bit constants (solid as exact values; novelty indicative)
All values are exact over 2^32 inputs. The first two were re-checked with an
independent `mixlab eval --exact`.

| mixer | exact bias | origin |
|---|---|---|
| **[16 7870aaad 15 be523597 15]** | **0.11527422572175930** | racing from the README best [15 d168aaad 15 af723597 15] 0.15984; 25 iterations, 73 min, still improving at the cap |
| [16 450c2d35 15 8af649a9 16] | 0.12121808405781900 | from #19's [16 450c2d35 15 caf649a9 16] 0.12243 |
| [17 19085565 16 af1d6b47 16] | 0.1401417246 | from README [17 9e485565 16 ef1d6b47 16] 0.16143 |
| [16 81e3352d 15 042cb68b 16] | 0.1387283538 | from lowbias32 0.17353, in two runs |
| [16 3d9e1b2d 14 8d0c9299 16] | 0.1533779615 | from an arbitrary start, 1.72 |

**Context:**
- The record [16 21f0aaad 15 735a2d97 15] = 0.10704 was **not beaten**.
  1,152 random 1–3-step neighbours were raced and all were worse.
- The #19 family d30332d3/c865c965 (0.12786) also did not improve.
- **Novelty claim:** 0.11527 is the best value we know of outside the
  record's own `21f0aaad` lineage, which covers 0.10704–0.1133. This rests
  only on the hash-prospector README list and issue #19. Other forks, issues
  and blog posts were **not** searched exhaustively.
- Extra metrics for 0.11527: structured-input avalanche counter 0.90, RRC
  1.05, low-weight 1.05 (all ideal ≈ 1).

### F10. Hypothesis: good 2-round mixers share a "low-bit core" (hypothesis, with suggestive evidence)
**Observation 1.** The 0.11527 mixer reached from d168aaad keeps the low 16
bits `aaad` of the record's m0. Its m1 converged to `…3597`, close to the
record's `…2d97`.

**Observation 2.** Its BIC worst entry is *identical* to the record's to four
digits: input 17 → outputs (2, 17), corr +0.3384, |z| = 86.64. TheIronBorn's
0.1076 constant shows the same entry.

*Update (2026-11-19, F11):* the worst pair's **location** is set by the shifts
alone. It is (2, 17) = (k, k+s2) for every multiplier pair tested at s2 = 15,
and (4, 18) at s2 = 14. Its **magnitude** matches exactly, across 3 BIC seeds,
for pairs whose multipliers share low bits: the record and 7870aaad (m0 shares
16 low bits, m1 shares 11), and lowbias32 and 81e3352d/042cb68b (16 and 12).
Unrelated multipliers give different values (0.33–0.82). So this entry is
consistent with being fixed by the low multiplier bits. It shows that shared
low bits fix a BIC defect, not that they predict bias.

**Correction to the proposed mechanism.** Multiplication alone is a
T-function; the complete mixer is not, because right shifts move high bits
downward. Keeping the low 16 bits of both multipliers does not fix low output
bits: at input 1, changing the record's m0 from `21f0aaad` to `21f1aaad`
changes output bit 0. Shared low bits remain an empirical hypothesis, not a
consequence of triangular multiplication.

**Untested:** whether fixing a known-good low-bit core and searching only the
high bits beats the record, and whether the core explains most of the gap
between families.

### F11. A final shift s2 ≥ n/2 always leaves an exactly correlated output pair (solid)
**Method.** `results/symmetry_bic/bic_s2_swap.txt`: 7 published multiplier pairs
(record, 7870aaad, lowbias32, 450c2d35, 81e3352d, 19085565, fmix32). Each has its
final shift set to 14, 15, 16 and 17. BIC at 2^16 samples, seeds 1–3.

**Results.**
- **s2 = 16 or 17:** all 7 × 2 × 3 runs show |z| = 256 (corr −1). The trigger is
  input bit s1 and the outputs are {0, s2}, exactly the two-bit parity set F3
  predicts. Avalanche bias cannot detect this.
- **s2 = 15:** max |z| is 86–211, except fmix32's multipliers, which give a second
  exact pair: input 15 → outputs (2, 17). F3 does not explain that pair.
- **s2 = 14:** max |z| is 33–89 for all 7. For the record, s2 = 13 gives 25.6 and
  s2 = 12 gives 7.8 (ideal ≈ 4.6).
- **The bias metric pulls the wrong way.** lowbias32 scores 0.1735 at s2 = 16 but
  0.2516 at s2 = 15 (exact). The record goes from 0.10704 to 1.179 at s2 = 14, and
  7870aaad from 0.11527 to 0.2809.

**Consequence for search:** treat s2 < n/2 as a hard constraint, or add a BIC
term. Bias-only searches drift toward s2 = 16. In F9, 450c2d35 (0.12122) and
81e3352d (0.13873) both use s2 = 16 and carry the exact pair. The record and
7870aaad (s2 = 15) do not.

### F12. Negating a multiplier is a near-symmetry of bias (solid at 8/16 bits; indicative at 32)
**Why.** x·(−m) = ~(x·m) + 1. A complement passes through a xorshift as a
constant XOR, which avalanche cannot see, so only the "+1" carry differs. That is
exact for the last multiply up to that carry. For m0, the complement then meets
another multiply, so the symmetry is weaker.

**Method.** `analysis/symmetry_check.py` (numpy, exact; it reproduces mixlab's 8- and
16-bit values). Controls: flipping the top bit (small change) and a random
replacement multiplier. "Within" is the Spearman correlation inside one shift triple.

| variant | 8-bit (300k configs), within ρ | 16-bit (1500 at [8 _ 7 _ 9]) ρ | 16-bit top-1% overlap |
|---|---|---|---|
| −m1 | 0.70 | **0.996** | 87% |
| −m0 | 0.74 | 0.983 | 53% |
| −m0, −m1 | 0.55 | 0.980 | 73% |
| inverse mixer f⁻¹ | 0.54 | 0.25 | 0% |
| top bit of m1 flipped (control) | 0.99 | 0.998 | 93% |
| random m0 (control) | 0.41 | 0.58 | 20% |
| random m0, m1 (control) | 0.00 | 0.01 | 0% |

**32-bit exact** (`results/symmetry_bic/neg32_exact.txt`):

| mixer | original | −m1 | −m0 | both |
|---|---|---|---|---|
| record 21f0aaad/735a2d97 | 0.10704 | 0.11753 | **0.11123** | 0.11271 |
| 7870aaad/be523597 | 0.11527 | 0.11677 | 0.13454 | 0.13653 |

**Consequences.**
- Negation is a long jump in Hamming distance that keeps most of the quality. It
  is a cheap new move type, and it opens unsearched neighbourhoods. For example,
  `[16 de0f5553 15 735a2d97 15]` = 0.11123 sits next to the record but is far from
  it in bit-flip distance, so neither TheIronBorn's search nor ours has covered it.
  It is not an independent discovery.
- For screening, m and −m are near duplicates, so you need only sample one.
- The inverse mixer's bias is nearly unrelated to the forward bias, and good
  mixers' inverses are not good. It is a separate objective, not a symmetry.
- Top-bit flips change bias by a median of 0.3–0.6%, which confirms that high
  multiplier bits only fine-tune.

**Limits:** the 16-bit sample is 1,500 configs at one shift triple, and its best
is 10.07 (TheIronBorn's best 16-bit 2-round is 7.25). At 32 bits there are only two
mixers.

## 3. Threats to validity (please check)
1. **Single runs.** The racing-versus-old comparison (F8) and the family runs
   (F9) are one seed each. The search improvements are exact values, but the
   claim that racing is *better than* the old search is n = 1.
2. **Racing statistics.**
   - The historical runs used about 10 sequential looks per candidate at z = 3 with no peeking
     correction. Many false "worse" drops of truly better neighbours are
     possible across about 1,000 candidates per run.
   - The noise law assumes the N table entries are roughly independent;
     entries are in fact correlated, as F3 shows.
   - Acceptance is first-improvement, not best-improvement.
3. **Neighbourhood bias.** Only 1–3-bit multiplier flips and ±1 shifts.
   "Local optimum" claims (the record, d303) are relative to this move set
   and to a random subset of about 1,000 moves.
4. **Novelty of F9** relies on two sources (see F9).
5. **Bias is not overall quality.** hash-prospector issue #28 notes the
   lowest-bias function is not always best on PractRand or RRC. No
   PractRand/RRC runs have been done; no C compiler is installed.
6. **BIC is sampled at 2^16** with a fixed seed. Identical worst entries
   across mixers (F10) could partly reflect shared seeds. It needs a re-run
   with other seeds and more samples.
7. **The F6 projection maps are ad hoc.** A better map (for example, one
   preserving the multiplier's spectral properties) is not excluded.

## 4. Questions for reviewers
- Is the noise law in F7 derived and applied correctly? Are the sequential
  z-tests in `racing.rs` sound enough, or should they use an
  always-valid / alpha-spending boundary?
- Is the novelty claim in F9 correct? Are there published 2-round 32-bit
  constants between 0.1070 and 0.1153 outside the 21f0aaad lineage?
- Does the F3 derivation hold for all shift triples, including s1 ≥ s0?
- Is F5's conclusion (no Markov-style model can predict 2-round bias)
  overstated, given that only two mixers were ablated?

## 5. Desired next steps (in priority order)

**Revised priority:** validate false-elimination rates with exact truths and
multiple seeds, rerun affected statistical comparisons, then compare
unrestricted, fixed-low-bit and larger-move searches at equal wall time.
The ideas below are experimental options, not established improvements.

1. **Test the low-bit-core hypothesis (F10).**
   - Fix the low 16 bits of both multipliers to the record's / 7870aaad's
     values and race over the high bits plus the shifts.
   - Re-run BIC with several seeds at 2^20.
   - If the core holds, fixing both low 16-bit portions leaves 2^32 multiplier
     combinations at fixed shifts. This restriction may also exclude better mixers.
2. **Continue the 7870aaad run.** It was still improving at iteration 25.
   Then try larger moves: combined shift and multiplier moves, and
   4–6-bit flips.
3. **Harden the racing statistics.**
   - Validate the new within-race Gaussian multiple-look adjustment; it is
     approximate and does not give a search-wide error guarantee.
   - Switch to best-improvement within a race.
   - Repeat the racing-versus-old comparison over about 5 seeds to turn F8's
     comparison into a solid claim.
4. **Independent quality check of the new constants.** Build PractRand (this
   needs a C toolchain, and the user's approval to download and install it),
   then run RRC-style tests on 7870aaad against the record and lowbias32.
5. **64-bit racing search** with fresh-sample verification at 2^32.
   - It costs hours per run, and 2-round bias is near the measurement floor
     (F2, F7), so first decide whether bias is still the right objective at
     64 bits.
   - The alternative is to target 3-round mixers on structured-input and BIC
     metrics.
6. Spectral-test and NAF-weight features of multipliers against exact bias.
   This is cheap, but expected to work only as a coarse screen, like the
   surrogate.

## 6. State of the repository
- Commit `8bdeb0d` contains everything up to and including F8's validation
  and first runs.
- **Not yet committed:**
  - the `--patience` option
  - `analysis/run_family_races.sh`
  - `results/family_races/`
  - the F9/F10 updates to `plan.md`
  - this file
