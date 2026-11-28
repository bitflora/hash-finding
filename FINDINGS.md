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

### F2. Reference table (solid; rerun 2026-11-20, `results/refs_corrected.txt`)
- Rerun with the corrected estimator and pooled SE at 2^30 samples, seed 1.
  Signed mean square ms (× 10⁻¹¹) ± SE, with z = ms/SE:

  | mixer | ms ± SE | z |
  |---|---|---|
  | fmix64 | 33.5 ± 2.7 | 12.4 |
  | Moremur | 19.2 ± 2.0 | 9.7 |
  | Stafford Mix13 | 2.3 ± 2.1 | 1.1 |
  | NASAM | 0.6 ± 2.1 | 0.3 |
  | mx3 | 0.04 ± 2.0 | 0.0 |
  | rrmxmx | −1.0 ± 2.2 | −0.5 |

  The conclusion stands: only fmix64 and Moremur are measurably biased.
  The other four are consistent with 0. The old per-row "±" values (for
  example mx3 ± 0.080 on the score scale) were not valid uncertainties; the
  pooled SE is ≈ 2·10⁻¹¹ in ms for every mixer, as the F7 noise law
  predicts. **Sampled avalanche bias cannot rank good 64-bit mixers at
  affordable sample counts.** The exact 16/32-bit rows and all BIC lines are
  byte-identical to the old run.
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
  record's own `21f0aaad` lineage, which covers 0.10704–0.1133.
  *Prior-art search (2026-11-20):* all 32 hash-prospector issues and PRs,
  including every comment through Jul 2026, the current README, and web
  searches for the constants and for newer 2-round 32-bit work. No pure
  xmx 2-round 32-bit constant below 0.1153 outside the `21f0aaad` lineage
  was found; the best other published family is still 450c2d35 (0.12243).
  The only lower 2-step functions use hardware CRC32C (0.0511; see
  `sources.md`), a different operation set. Node.js/V8's 2026 integer hash
  is 24-bit with random multipliers. Not searched: other people's forks,
  non-English sources, and private code. The claim is now well supported,
  but not proven.
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
  exact pair: input 15 → outputs (2, 17). F3 does not explain that pair;
  F37 does (a shift-forced pair at 16/13/15).
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

**Method.** `analysis/symmetry_check.py` (exact, scored by `mixlab exact --list`; originally numpy, which reproduced mixlab's 8- and
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

**Follow-up race (2026-11-19, `results/negation_races/negrec_m0.*`).** A race from
`[16 de0f5553 15 735a2d97 15]` (0.11123) with `--max-s2 16` improved in its first
iteration to **`[16 5e0f5553 15 735a2d97 15]` = 0.10935989** (exact), 2.2% above
the record. It then stalled for 6 iterations (576 neighbours, 33 exact checks;
31 min). Caveat: −`5e0f5553` = `a1f0aaad`, the record's m0 with only its top bit
flipped. So this point is a negation plus one top-bit flip away from the record,
not an independent family.

**Limits:** the 16-bit sample is 1,500 configs at one shift triple, and its best
is 10.07 (TheIronBorn's best 16-bit 2-round is 7.25). At 32 bits there are only two
mixers.

### F13. Racing thresholds validated against exact truth (solid for these sets; few hard cases at 32 bits)
**Method.** `mixlab trace` follows each candidate on the race's doubling
schedule. It records the estimate at every look and stops only at |gap| > 6 sd
or at the cap. `analysis/race_validation.py` then scores any (z_drop,
z_promote) policy offline, with first-improvement acceptance and exact
verification. Data are in `results/race_validation/`:
- **24 bits:** 32 races of 96 neighbours, from bases on two exact hill-climbs.
  Looks run from 2^10 to 2^24.
- **32 bits, issue #19 set:** 12 races. The incumbents sit at the 25/50/75%
  ranks of the 57 constants (0.128, 0.135, 0.147), raced against the other 56.
  Looks run from 2^14 to 2^28.
- **32 bits, de0f5553 (0.11123):** 4 races of 64 fresh s2 ≤ 15 neighbours.
  Exactly one of them is better: 5e0f5553, at 0.10936.

**Results.**
- **The noise model is calibrated.** At the first look, (ms_hat − true)/sd_model
  has sd 1.02 (issue #19), 1.08 (de0f5553) and 1.04–1.08 (24-bit), with mean
  ≈ 0. The share with |z| > 3 is 0–0.6%, so the tails are slightly heavy at
  24 bits.
- **False drops of truly better candidates:** 0 at the adjusted threshold
  (z ≈ 4.75–4.87) in every set. There is 1 at plain z = 3, in one 24-bit
  set (0.7% of that set's better candidates). No race dropped its best
  candidate or missed an available improvement, for s2 < 16 and s2 = 16 alike.
- **The hardest gaps are not decided by sampling.** The tightest truly
  better candidates were only 0.2–0.9 sd apart even at 2^28. Those reach the cap
  and are verified exactly, so near the record the cost is exact checks, and
  racing cannot cause losses there.
- **Lowering the promote bar is harmful.** With z_promote ≤ 2, races spend 2–15
  exact checks instead of about 1–2, and they accept smaller improvements.
  The accepted share of the best available gain falls to 0.26–0.59, against
  0.86–0.95 at the adjusted symmetric threshold. A strict promote bar works
  like a cheap best-improvement rule.
- **Cost.** Plain z = 3 needs 1.4–2.8× fewer inputs than the adjusted
  threshold, but it accepts smaller gains (0.71–0.75 of the best).

**Recommendation.** Keep the current symmetric adjusted threshold.

**Limits.**
- At 32 bits only 4 + 12 races were run, and only a handful of candidates had
  gaps below 1 sd at the cap.
- The 24-bit climbs stalled at bias ≈ 1.15, so their gaps were large.
- None of this bounds search-wide error over many iterations.

### F14. Overnight 2026-11-20: escapes via negation, and the low-bit core (indicative; one run each)
All scores are exact over 2^32 inputs, with shifts [16 _ 15 _ 15] and `--max-s2 16`.
Logs: `results/negation_races/` (`queue.log` has the timeline; scripts are
`analysis/overnight_queue*.sh` and `gen_topbit_cands.py`). The *sweep* step races
66 candidates: every top-3-bit combination of both multipliers, plus the three negations.

**Escape from stalled local optima (F12 follow-up).**

| start | sweep | then race | earlier status |
|---|---|---|---|
| record 0.10704 | none of 66 better (33 exact checks) | — | local optimum |
| 7870aaad 0.11527 | none of 66 better (35 exact) | — | stalled (F9) |
| d303 `[16 d30332d3 15 c865c965 15]` 0.12786 | **−m1 → 0.12510** | **0.12015** `[16 d70332d3 15 379a369b 15]` | stalled in `famd303` |
| d303 second round, from 0.12015 | none of 66 better (31 exact) | — | — |
| negated record `de0f5553` 0.11123 | (race first) 0.10936 → sweep → a1f0aaad 0.10847 → record | — | — |
| both negated 0.11271 | — | 0.11214 `[16 5e0f5553 15 8ca5d269 15]` | — |
| 7870 −m1 0.11677 | — | no improvement | — |

Negation escaped one of two stalled family optima (d303, −6% overall). From
the record's neighbourhood, every negation path led back to the record. With
top-bit flips and negations added to the move set, the record and 7870aaad are
still local optima.

**Low-bit core test (F10, first direct test).**
- *Direct:* 20 mixers with the record's low 16 bits (`..aaad`, `..2d97`) and
  random high halves, against 20 fully random mixers (`core_exact.csv`).
  - Core: min 0.173, median 0.239, max 0.437.
  - Random: min 0.543, median 3.16, max 16.1.
  - All 400 core-vs-random pairs favour the core.
  - The best core start, with random high halves, already equals lowbias32.
  - Caveat: random cores are the only control here. *F16 now tests six
    other good mixers' cores: none is special; all are good.*
- *Races from that best core start* (0.1734, same seed and budget):
  - `--fix-low-bits 16`: 0.12069 `[16 68f0aaad 15 cd852d97 15]` after 15
    iterations (68 min), still improving.
  - Unrestricted: 0.11994 `[16 68f0aaad 15 ed21ad97 15]` in 29 min. It changed
    only one low bit (bit 15 of m1).
  - Both converged on m0 = `68f0aaad`, next to the `21f0aaad`/`7870aaad` family.
  - A sweep from the unrestricted result then improved it to **0.11813**
    `[16 88f0aaad 15 ed21ad97 15]` (one top-bit change in m0). A second sweep
    round found nothing better (31 exact checks).
- *Reading:* the low halves carry most of the quality and make excellent seeds.
  Locking them gave no search advantage in this one comparison. A
  random-high-half start climbed in about 50 minutes (race plus sweep) to
  0.11813, behind only the record lineage and 7870aaad. It is not independent
  of the record, because it shares the record's core.

### F15. BIC at 2^20 over 5 seeds: the worst output pair is set by the low-bit core (solid)
**Method.** New `mixlab bic --samples p20 --seeds 1,2,3,4,5` (its 2^16 seed-1
output reproduces the earlier `--extra` line exactly). 13 mixers.
Data: `results/bic20/bic20_seeds1-5.txt`.

| mixer | exact bias | rms z | worst pair | corr |
|---|---|---|---|---|
| record 21f0aaad/735a2d97 | 0.10704 | 7.80 | (2,17) | ±0.334 |
| 21f0aaad/d35a2d97 (0.1076) | 0.10760 | 7.80 | (2,17) | ±0.334 |
| 7870aaad/be523597 | 0.11527 | 7.63 | (2,17) | ±0.334 |
| 88f0aaad/ed21ad97 | 0.11813 | 7.61 | (2,17) | ±0.334 |
| 5e0f5553/735a2d97 (−m0 family) | 0.10936 | 8.40 | (2,17) | ±0.334 |
| 5e0f5553/8ca5d269 | 0.11214 | 17.39 | (2,17) | −0.62 |
| d30332d3/c865c965 | 0.12786 | 15.12 | (2,17) | +0.50 |
| d70332d3/379a369b | 0.12015 | 11.97 | (2,17) | −0.60 |
| 450c, 81e3352d, 19085565, lowbias32 (s2 = 16) | 0.121–0.174 | 28–37 | (0,16) | −1 (exact) |
| triple32 (3 rounds, control) | 0.02089 | 1.00 | random | ≈0 |

**Results.**
- For the four mixers that share m0's low 16 bits `aaad` (the record, the
  0.1076 constant, 7870aaad, 88f0aaad), the worst entry is **bit-identical
  at every seed** (for example seed 2: max|z| = 342.79, corr −0.3348), even
  though their m1 differs in 5–16 bits. At a common seed the counted events
  must be the same, so the (2, 17) pair statistic depends only on bits these
  mixers share. The magnitude is ≈ 1/3 (0.3335–0.3348 across seeds).
- The negated-m0 point 5e0f5553/735a2d97 has the same ≈ 1/3 magnitude, but
  different per-seed values: the same defect reached through another
  multiplier. Its sibling with a different m1 jumps to 0.62.
- The d303 family has a *stronger* (2, 17) defect (0.50); the negation escape
  d70332d3 trades it for 0.60 while lowering bias. Lower bias does not mean
  lower worst-pair correlation.
- Every s2 = 16 mixer has the exact F11 pair (in 15 or 16 → out 0, 16),
  with rms z 4× that of the s2 = 15 mixers.
- **rms z ranks the s2 = 15 aaad-core mixers slightly differently from bias:**
  88f0aaad (7.61) and 7870aaad (7.63) are below the record (7.80).

**Exact check** (`analysis/exact_pair_corr.py`, all 2^32 inputs, the record,
input 17): P(out 2 flips) = 0.50010, P(out 17 flips) = 0.49995,
P(both) = 0.333372, corr = **+0.3333741**. So the pair flips together with
probability ≈ 1/3 instead of 1/4, which gives corr ≈ 1/3, but neither value
is exactly 1/3.

**Within the aaad core, the worst pair depends on s2 alone** (`results/bic_frontier/s2_grid.txt`,
2^18 samples). For the record, 7870aaad and 88f0aaad multipliers, the worst
entry is identical across all three at each s2:

| s2 | worst pair | corr | exact bias (record / 7870 / 88f0) |
|---|---|---|---|
| 12 | (8, 20) | 0.034 | 0.728 / **0.246** / 0.491 |
| 13 | (6, 19) | 0.102 | 0.282 / 0.259 / 0.312 |
| 14 | (4, 18) | 0.127 | 1.179 / 0.281 / 0.291 |
| 15 | (2, 17) | 0.334 | 0.107 / 0.115 / 0.118 |

`[16 7870aaad 15 be523597 12]` scores 0.2456 with a worst pair of only 0.034.
That is lower bias than fmix32 (0.264, which has an exact −1 pair), and
better BIC than any s2 = 15 mixer, without any search for this trade-off.

**Consequence.** This resolves threat #6: the identical worst entries are
not a seed artifact. The (2, 17) defect at ≈ 1/3 is a property of the
`aaad`-core lineage. Every s2 = 15 two-round mixer tested has a (2, 17)
defect of at least 0.33, so a BIC objective is a real second axis for
search, not a re-scaling of bias.

### F16. Every good mixer's low-16-bit core carries most of its quality (solid for 7 cores)
**Method.** F14 compared only the record's core against random mixers.
Here, 7 source mixers (record, 7870aaad, 450c, d303, 19085565, lowbias32,
fmix32) each keep their shifts and the low 16 bits of both multipliers. The
**same** 16 random high-half pairs are attached to every core (paired
design, 112 exact evaluations). Scripts: `analysis/gen_core_controls.py`,
`run_core_controls.sh`, `core_controls.py`; data:
`results/core_controls/core_controls_exact.csv`.

| core | source bias | min | median | max | beats record core (paired) |
|---|---|---|---|---|---|
| record | 0.10704 | 0.167 | 0.269 | 1.04 | — |
| 7870aaad | 0.11527 | 0.188 | 0.277 | 0.48 | 10/16 |
| 450c | 0.12243 | 0.198 | 0.304 | 1.07 | 5/16 |
| d303 | 0.12786 | 0.191 | 0.299 | 0.78 | 7/16 |
| 19085565 | 0.14014 | 0.157 | 0.360 | 1.18 | 6/16 |
| lowbias32 | 0.17353 | 0.208 | 0.276 | 1.82 | 8/16 |
| fmix32 | 0.26399 | 0.285 | 0.345 | 1.08 | 4/16 |
| *fully random (F14)* | — | 0.543 | 3.16 | 16.1 | — |

**Results.**
- **The record's core is not special.** Every good mixer's core with random
  high halves gives a median of 0.27–0.36, roughly 10× better than fully
  random mixers. lowbias32's core (median 0.276) is as good as the record's
  (0.269), and no core beats the record's core consistently in paired
  comparisons (4–10 wins out of 16).
- The source mixer's own score predicts its core's quality only weakly
  (Spearman 0.54 for medians, 0.61 for minima, n = 7). Even fmix32's core
  (source 0.264) does far better than random.
- No random draw beats its own source, but the best of 16 comes within
  8–56% of it (fmix32 0.285 vs 0.264; 19085565 0.157 vs 0.140; the record
  0.167 vs 0.107). One draw for 19085565 (0.157) is below lowbias32.

**Reading.** "A good low-16-bit core" is a property that published mixers
share in general, not a signature of the record lineage. This supports
seeding searches from any good core with random high halves (F14 did that
once and reached 0.118). The low halves fix the coarse structure; the high
halves do the final tuning that separates 0.107 from 0.17.

### F17. Spectrally good LCG multipliers are not good mixer seeds (indicative; small pool)
**Method.** `seed-search` at 32 bits with four populations: Steele–Vigna
spectral multipliers, `matched` (random odd multipliers with the same
lengths), uniform `random`, and `mixer` (recombined constants from published
mixers). Each run uses 64 candidates, screening at 2^22, and exact checks of
the best 4, over 3 seeds. A second run uses the extended pool
(`--min-mult-bits 1`) with 256 candidates at 2^20, seed 1, spectral against
matched. Script: `analysis/run_seed_search.sh`; data and table:
`results/seed_search32/` (`summary.txt`).

| population | median bias | 10th pct | best exact (per seed) |
|---|---|---|---|
| spectral (default: only **2** 32-bit multipliers) | 9.51 | 1.26 | 0.941, 0.927, 0.927 |
| matched | 3.82 | 1.01 | 0.361, 0.453, 0.484 |
| random | 3.67 | 1.11 | 0.376, 0.782, 0.556 |
| mixer recombinations | 0.555 | 0.304 | 0.210, 0.210, 0.202 |
| spectral, extended (11 multipliers) | 26.5 | 1.90 | 0.575 |
| matched, extended (511 multipliers) | 24.3 | 2.13 | 0.258 |

**Results.**
- At 32 bits the default spectral pool is degenerate: two multipliers, so
  half its configurations use m0 = m1. It is worse than random at every
  quantile.
- With the extended pool, spectral selection is no better than random
  multipliers of the same lengths (median 26.5 vs 24.3; best 0.575 vs 0.258).
- Recombining published mixer constants is the best seed pool by about 7×
  in median, consistent with F16: those constants already carry good cores.
- Curiosity: the best `matched` candidate, `[16 00032a9d 15 bb8e4d95 15]`
  (exact 0.2576), uses an 18-bit first multiplier.

**Reading.** LCG spectral quality does not transfer to finalizer avalanche
here. Seed searches should start from good cores (F16) or recombined
published constants, not from spectral tables. Caveat: the 32-bit spectral
pool is tiny (11 constants), so this is a weak test of the idea; 64 bits
has a larger pool and is untested.

### F18. BIC-constrained search: bias and worst-pair correlation trade off steeply (exact bias; one run each)
**Method.** New `race --max-bic-corr C`: any neighbour whose worst BIC
|corr| at 2^16 inputs exceeds C is skipped before racing. Both runs start
from 7870aaad with a lower final shift (F15 table), with seed 61, 96
neighbours, patience 6 and at most 25 iterations. Logs:
`results/bic_frontier/`; BIC of winners at 2^20, 3 seeds:
`winners_bic20.txt`.

| constraint | start | result (exact) | worst pair corr | rms z | iterations / time |
|---|---|---|---|---|---|
| C = 0.05 (s2 free) | `[16 7870aaad 15 be523597 12]` 0.24559 | **`[16 d610aaad 15 4d90b597 12]` 0.20300** | 0.020–0.024 | **1.19** | 25 / 72 min, last gain at 18 |
| C = 0.15, s2 ≤ 14 | `[16 7870aaad 15 be523597 14]` 0.28086 | **`[16 6870aaad 15 f452f593 14]` 0.13168** | 0.082 | 3.38 | 16 / 38 min (patience) |
| *reference: record (s2 = 15)* | | 0.10704 | 0.334 | 7.80 | |
| *reference: lowbias32 (s2 = 16)* | | 0.17353 | 1 (exact) | 30.3 | |

**Results.**
- At s2 = 14, bias 0.1317 is reachable with a worst pair of 0.08, a quarter
  of the record's 0.33. That bias is lower than lowbias32 and every
  hash-prospector README entry, and its rms z (3.4) is less than half the
  record's (7.8).
- At s2 = 12, the winner is nearly BIC-clean: rms z 1.19, against ≈ 1 for an
  ideal mixer, with every pair below 0.025. It pays for this with bias 0.203.
- The race lowered the worst pair below the s2-only value of F15: at s2 = 14
  it went from 0.127 to 0.082. So multipliers, not only shifts, set the
  defect size once the core changes. (m1's low bits went from `3597` to
  `f593`.)
- The filter cost is small: 330–930 skipped neighbours over a run, at
  ~0.2 s each.

**Reading.** For 2-round 32-bit mixers, bias alone selects s2 = 15 and a
≈ 1/3 worst pair. The constrained points are a better default wherever
output-bit independence matters (hash-table probing with several output
bits, PRNG use). None of this has been checked with PractRand. Each
constraint level is a single run, so the frontier is only sketched.

### F19. Races from other good cores reach 0.120–0.136 (exact bias; one run each)
The d303, lowbias32 and s2 = 14 headline values were re-checked with an
independent `mixlab eval --exact`.
**Method.** The best random-high-half draw per core from F16 is the start,
with s2 = 16 sources moved to s2 = 15 (F11). Settings match F14's core
race: seed 61, 96 neighbours, patience 6, `--max-s2 16`, at most 25
iterations. Logs: `results/core_races/`.

| core | start (exact) | result (exact) | previous best in that family | BIC worst corr |
|---|---|---|---|---|
| d303 | 0.19106 | **`[16 702f32d3 15 e6134975 15]` 0.11992** | 0.12015 (F14 negation escape) | 0.50 |
| lowbias32 | 0.20520 (s2 → 15) | **`[16 7803352d 15 49eb26cb 15]` 0.12561** | 0.13873 (F9, s2 = 16) | 0.41 |
| 450c | 0.20814 (s2 → 15) | `[16 d50c2d35 15 f2a649a9 15]` 0.12797 | 0.12122 (F9, s2 = 16) | 0.82 |
| 19085565 | 0.31292 (s2 → 15) | `[17 8206552d 16 e5ae2947 15]` 0.13568 | 0.14014 (F9, s2 = 16) | 0.33 |
| *record core (F14)* | 0.17340 | 0.11813 (with sweep) | 0.10704 | 0.33 |

**Results.**
- **Starting from random high halves on a good core beats racing from the
  published constants themselves** in 3 of 4 families: d303 (0.11992, a new
  family best), lowbias32 (0.12561 against 0.13873) and 19085565 (0.13568
  against 0.14014). The exception is 450c: at s2 = 15 it reaches 0.12797,
  worse than its s2 = 16 family (0.12122), but without that family's exact
  output pair. The races took 32–110 min; lowbias32 used all 25 iterations, with its last
  gain at iteration 22, so it may not have converged.
- The lowbias32 race result 0.12561 is the second-best non-`aaad` mixer we
  know of, behind only d303's 0.11992, and both beat 450c's published
  0.12243.
- Races keep the core's characteristic defect: the d303 winner has the same
  0.50 (2, 17) pair as d303, and 19085565 keeps 0.33. The lowbias32 winner
  has 0.41, and the 450c core at s2 = 15 a large 0.82. Bias races do not clean BIC.
- No race beat 7870aaad (0.11527) or the record.

**Reading.** F16's claim holds up in search: a good core plus a short
race is a better starting point than the published constants, which sit
in stalled local optima. The best non-record result is still in the
`aaad` lineage (7870aaad), but d303 is now within 4%. *(Superseded by F20:
a crossover child reaches 0.10882.)*

### F20. TheIronBorn's method, applied: the record is a radius-2 optimum, and crossover finds 0.10882 (exact; one run each)
TheIronBorn's own account of the method is summarised in `sources.md`.
Two pieces of it were missing from our search: an *exhaustive* small-radius
check and recombination of good solutions. Script:
`analysis/queue_ironborn_ideas.sh`; generators `gen_radius2_cands.py` and
`gen_crossover.py`; screen `screen_list.py`; outputs `results/ironborn_ideas/`.

**Radius-2 check of the record.**
- Candidates: every neighbour within two moves of TheIronBorn's move set,
  2,343 in total. A move flips one multiplier bit 1–31, or shifts s0/s1/s2
  by ±1 (s2 ≤ 16).
- These were raced against the record in 6 chunks of about 390 (`race
  --candidates`, z = 3 with the multiple-look adjustment) and took 22 min.
- 2,287 were dropped by sampling and 56 were checked exactly. **None beats
  the record.**
- So the record is an exhaustive radius-2 local optimum under this move
  set. That holds up to the racing false-drop rate, which F13 measured as
  zero in 48 races. Shift ±2 neighbours, missed by the original generator, were scored exactly later (all ≥ 0.135; `shift2_check.txt`).
- The closest exact neighbours are all m1 top-nibble variants:

  | m1 | bias |
  |---|---|
  | `535a2d97` | 0.10731 |
  | `f35a2d97` | 0.10735 |
  | `d35a2d97` | 0.10760 |
  | `935a2d97` | 0.10840 |

- **`[16 21f0aaad 15 535a2d97 15]` = 0.10730718959493174** is a new
  second-best mixer. It is two bit flips from the record and not in issue #19.

**Crossover.**
- Parents: 31 mixers with shifts 16/15/15. These are 11 campaign winners
  plus the 20 best published mixers, many of them #19 variants of the
  record.
- Children: m0 from one parent and m1 from another, giving 791 children,
  449 of them unique. Each was screened at 2^26 samples.
- The 15 best unique children were then scored exactly. The 2^26 screen was
  optimistic for its top picks by up to 0.01 bias (winner's curse).
- Most top children are trivial: the record's m0 with another #19 m1, which
  is just a radius-1 or radius-2 neighbour of the record.
- One child is a **genuine cross that beats both parents**: `[16 130332d3 15
  919a4597 15]` = 0.11558 exact.
  - Its m0 comes from the published d303 variant `[16 130332d3 15 c865c965
    15]` (0.12941).
  - Its m1 comes from the campaign winner `M_16_15_694b_4593` (0.12456).
- That child was raced with the usual settings (96 neighbours, patience 6,
  seed 71, `--max-s2 16`; 16 min). It improved 7 times, all small moves on
  m0's top byte plus one m1 bit, and reached:

  **`[16 d40332d3 15 919a4597 15]` = 0.1088178567 (exact).**

- This is the **best mixer outside the record's immediate neighbourhood.**
  - It is 1.7% above the record.
  - It beats the negated-record line (0.11123, F14), 7870aaad (0.11527) and
    d303's 0.11992 (F19).
  - Its low-16-bit core (32d3 / 4597) is not the record's aaad / 2d97.
- Its BIC is poor: the worst pair is input 0 → outputs (2, 17) at corr
  +0.60, rms z 11.1 at 2^20. This is consistent with F15/F19: s2 = 15
  winners carry a large single-pair defect.

**Reading.**
- The exhaustive radius-2 result strengthens the case that the record is
  hard to improve by local moves. Threat #3 now applies only to radius ≥ 3
  and to the other "local optimum" claims.
- Crossover is the first operator in this project to produce a strong
  mixer from a core *not* in the aaad lineage. A cross of two
  mid-0.12 parents, plus a 16-min race, landed at 0.1088. That suggests
  the population of family winners holds recombinable building blocks, and
  that a few generations of cross-then-race could reach the record's level.
- n = 1: one cross was raced, with one seed. The crossover gain could be
  specific to this pair.

### F21. Crossover generation 1 beats the record: `[16 9c013335 15 335a2d97 15]` = 0.10206799761502400 (exact, two independent counters)

**The new best mixer.**

```c
x ^= x >> 16; x *= 0x9c013335;
x ^= x >> 15; x *= 0x335a2d97;
x ^= x >> 15;
```

- Exact avalanche bias over all 2^32 inputs is **0.10206799761502400**.
  That is 4.6% below TheIronBorn's 0.10704308166917044, the best published
  2-round 32-bit xmx.
- It was confirmed by two independent exact counters:
  - the SIMD Harley–Seal counter (`mixlab eval --exact`);
  - the generic scalar counter (`examples/exact_generic.rs`, 87 s). The same
    run reproduces the old record to all 17 digits.
- **Confirmed externally (2026-11-21).** The user ran hash-prospector
  itself (`./prospector -E -e -p xorr:16,mul:9c013335,xorr:15,mul:335a2d97,xorr:15`)
  and it agrees, with the old record as control. Note that `-E` alone is a
  2^18 sampled estimate (≈ 2 for any good mixer); `-e` is the exact mode.
- Prior art: the constants appear nowhere in hash-prospector issue #19, its
  2026 comments, or the F9 web search. No pure xmx below 0.10704 was known.
- **It also wins under issue #12's metric** (camel-cdr's mean |count − n/2| / n
  ×1000; `examples/metric12.rs`). It reproduces TheIronBorn's published values
  to 17 digits (`d35a2d97` 0.03668391400424298, `f35a2d97` 0.03633152391557815).

  | mixer | issue #12 | hash-prospector |
  |---|---|---|
  | old record `735a2d97` | 0.036194 | 0.107043 |
  | **new `9c013335 / 335a2d97`** | **0.035729** | **0.102068** |
  | neighbour `9c013335 / b35a2d97` | 0.035657 | 0.102149 |

  - The margin is smaller under this metric: 1.3%, against 4.6%.
  - The two metrics disagree about the radius-1 neighbour, as they did in
    2022: `b35a2d97` is best under #12.
  - Values are in `results/ironborn_ideas/metric12.txt`.
- BIC is poor, like every s2 = 15 winner (exact over 2^32: corr +0.40002 for input 1 → outputs (2, 17), `exact_pair_corr.py`; old record +0.33337): the worst pair is input 1 →
  outputs (2, 17) at corr +0.40, rms z 10.5 at 2^20. PractRand: see below.

- **PractRand, bit-reversed counter** (`analysis/practrand_feed.c` mode 2,
  `RNG_test stdin32`; run by the user, 2026-11-21).
  - Both mixers fail clearly, at the same length (results compared at 512 MB).
  - The new mixer's failures are mostly 2–4× weaker:

    | test | old record | new |
    |---|---|---|
    | Gap-16:A | R=+4955 | R=+1265 |
    | BRank(12) | R=+5307 | R=+3435 |
    | BDayS2 | R=+30.8 | R=+24.0 |
    | [Low1/32] Gap-16:B | R=+6207 | R=+2408 |
    | [Low1/32] FPF:all | R=+1082 | R=+304.9 |
    | [Low8/32] FPF:all | R=+201.8 | R=+243.7 |
    | tests with no anomalies | 111 | 122 |

  - The new mixer has 122 tests with no anomalies, against 111 for the old.
    The exception is [Low8/32] FPF:all, where it is slightly worse.
  - Limits: one input pattern and one length. R values show the size of a
    failure; they are not a calibrated quality scale. The plain counter,
    complement, Weyl and rotation patterns are untested.
  - Both mixers fail clearly under a bit-reversed counter, so neither is
    suitable where such inputs occur.

**How it was found.** This is one generation of
`analysis/crossover_round.py`, with outputs in `results/ironborn_ideas/gen1/`.

- *Parents:* 40 mixers with shifts 16/15/15, covering 24 low-16-bit core
  pairs: campaign winners, the 20 best published, and the F14/F19/F20 race
  results (`gen1_extra.csv`).
- *Children:* each child takes m0 from one parent and m1 from another. Only
  genuine children are kept, meaning the child's core pair (m0 & 0xffff,
  m1 & 0xffff) belongs to no parent. That leaves 687; 304 were newly
  screened at 2^26.
- *Exact scoring:* the 20 best by the 2^26 screen were scored exactly.
  The best exact child was 0.11753; the eventual winner's start was 0.12157,
  the 8th-best exact score.
- *Races:* the 3 best exact children with distinct core pairs were raced
  (96 neighbours, patience 6, seeds 81–83, `--max-s2 16`).

| start (exact) | parents | race result (exact) |
|---|---|---|
| `[16 21f0aaad 15 8ca5d269 15]` 0.11753 | #19 record m0 × F14 both-negated m1 | `[16 61f0aaad 15 8ca5d269 15]` 0.10959 |
| `[16 7803352d 15 535a2d97 15]` 0.12157 | F19 lowbias32-core race m0 × F20 0.10731 m1 | **`[16 9c013335 15 335a2d97 15]` 0.10207** |
| `[16 d70332d3 15 db5a2d97 15]` 0.12188 | F14 d303 race m0 × #19 m1 | `[16 d00332d3 15 db5a2d97 15]` 0.11450 |

**The winning race's trajectory** (9 improvements, 28 min, stopped by
patience at iteration 21):

| step | mixer | bias |
|---|---|---|
| start | 7803352d / 535a2d97 | 0.12157 |
| | 78033535 | 0.11634 |
| | 58033535 | 0.11289 |
| | 58033535 / d35a2d97 | 0.11222 |
| | 58033335 | 0.11093 |
| | 58033335 / 935a2d97 | 0.10942 |
| | d8033335 | 0.10892 |
| | d8033335 / 335a2d97 | 0.10748 |
| | dc013335 | 0.10367 |
| end | 9c013335 / 335a2d97 | 0.10207 |

- m0 started from the lowbias32-core race winner (`7803352d`, F19), but
  its low byte changed (2d → 35), so the final core is 3335.
- m1 moved around the record's `?35a2d97` top nibble and settled on
  `335a2d97`. That m1 gives 0.10802 with the record's m0 (F20), so the gain
  comes from the new m0 with the record-lineage m1.

**Reading.**
- Crossover between independently optimised families, followed by a race,
  beat a record that had survived an exhaustive radius-2 check (F20). This
  supports TheIronBorn's memetic design. Their own pattern-mining and
  crossover, by their account, were used for the 16-bit records; they don't
  remember which metaheuristic produced the 32-bit one.
- The 2^26 screen picked a start that was only the 8th-best exact child.
  The screen chooses *where to race*, not which child is already best.
- **Radius-2 check of the new mixer:** all 2,343 neighbours were raced
  (`results/ironborn_ideas/r2_new/`, 26 min, 75 exact checks), and none is
  better. So it is a radius-2 local optimum, like the old record. Shift ±2 neighbours, missed by the original generator, were scored exactly later (all ≥ 0.135; `shift2_check.txt`).
- The closest neighbour is `[16 9c013335 15 b35a2d97 15]` = 0.10215, which
  also beats the old record. Next are m0 variants `dc013335` (0.10367),
  `cc013335` (0.10374) and `dd013335` (0.10377).
- n = 1 generation, one seed per race. The README's BIC column is max|z|
  scaled to 2^16 (0.40 × 256 ≈ 102).

### F22. Ten gen-1 races and generation 2: crosses reliably land low; no new record (exact; one seed per race)
Script `analysis/queue_crossover_gen2.sh`; outputs `results/ironborn_ideas/gen1/`
(races 4–10) and `gen2/`.

**Generation 1, all 10 distinct-core races** (the 3 from F21 plus 7 more,
seeds 81–90). The results are 0.10207, 0.10799, 0.10813, 0.10959, 0.10998,
0.11089, 0.11450, 0.11738, 0.12051 and 0.12462.
- Seven of the ten finished below 0.115, the best that any non-crossover
  race reached outside the record's own neighbourhood (F9/F14/F19).
- The F21 winner was the best draw, but not an outlier from a weak
  distribution.
- New this round: `[16 6f0f32d3 15 2d66b9a9 15]` 0.10799 (d303-core race ×
  multi-start winner) and `[16 8bf8d535 15 b35a2d97 15]` 0.10813.

**Generation 2.**
- Parents: 51, covering 34 core pairs, now including every gen-1 race
  winner (`gen2_extra_full.csv`).
- Children: 1,015 genuine, 396 of them newly screened. The 20 best were
  scored exactly and 5 distinct ones raced (seeds 91–95, about 2 h in all).
- Several exact children were already near the record before any race.
  All of them are the F21 winner's m0 `9c013335` with a gen-1 m1:

  | m1 | exact bias |
  |---|---|
  | `cca5d269` | 0.10462 |
  | `2ca5d269` | 0.10654 |
  | `8ca5d269` | 0.10756 |

| start (exact) | race result (exact) | BIC worst corr (2^20) |
|---|---|---|
| `9c013335 / cca5d269` 0.10462 | `[16 ec013335 15 cca5d269 15]` **0.10344** | 0.66 |
| `b1f8cd35 / b65a3597` 0.12193 | `[16 02fccd35 15 b15a3597 15]` **0.10447** | 0.40 |
| `b1fccd35 / 335a2d97` 0.11205 | `[16 31fccd35 15 b35a2d97 15]` 0.11175 | — |
| `8bf8d535 / 2ca5d269` 0.11464 | no improvement | — |
| `d70332d3 / 2a8649a9` 0.12448 | `[16 e70332d3 15 4ea649a9 15]` 0.11651 | — |

**Reading.**
- **No new record:** 0.10207 (F21) still stands.
- **The population is stronger and more varied.** There are now three
  mixers below 0.105 on three different m1 cores: 2d97 (0.10207), d269
  (0.10344) and 3597 (0.10447).
- `[16 02fccd35 15 b15a3597 15]` = 0.10447 is the best mixer without the
  record lineage's 2d97 m1 core.
  - It came from a weak start (0.12193) via 8 improvements.
  - Its BIC (worst corr 0.40) matches the F21 winner's.
- The best gen-2 race started next to the record (0.10462) and improved
  only 1%. Its BIC is much worse: corr 0.66 on input 27 → (2, 17).
- **Gen 2 improved the population but not the top score.** There are two
  readings, and one more generation should tell them apart:
  - the 0.102–0.104 basin is close to this space's floor;
  - or the operators (whole-multiplier swaps, one seed, patience 6) are now
    the limit.
- The §5 variations (finer crossover, other shift templates, several seeds
  per child) target the second reading.

### F23. Crossover generation 3: half-swap children reach 0.10091, a new best bias but with worse BIC (exact, two independent counters)

Generation 3 tries FINDINGS §5 variations 1 (finer crossover) and 6 (several
race seeds per child): `analysis/crossover_gen3.py`, outputs in
`results/ironborn_ideas/gen3/`. Parents: the 51 gen-2 parents plus the 5 gen-2
race winners (56; `gen3_parents.csv`). Three child kinds, kept apart:
*whole* (as gen 1/2; 1,105 genuine children), *half* (each multiplier = high
16 bits of one top-16 parent | low 16 bits of another; 500 sampled) and
*uniform* (per-bit random mix of two top-16 parents; 500 sampled). Each kind:
2^26 screen, exact top 12, race the 2 best distinct-core children with 3 seeds.
All 18 races done (2026-11-23; paused once at 11/18 and resumed).

- Best exact score before racing: half 0.1135, whole 0.1153, uniform 0.1225
  (most uniform children 0.142-0.153: random bit mixing breaks the parents' structure).
- Race results (exact):

  | kind | start (exact) | seed 0 | seed 1 | seed 2 |
  |---|---|---|---|---|
  | whole | `[16 d8433535 15 d35a2d97 15]` (0.11533) | 0.10748 | 0.10848 | 0.11169 |
  | whole | `[16 d00332d3 15 b15a3597 15]` (0.11672) | 0.10490 | 0.10464 | 0.10490 |
  | half | `[16 d40332d3 15 b35a3597 15]` (0.11350) | **0.10091** | 0.10214 | 0.10800 |
  | half | `[16 02fcd535 15 535a3597 15]` (0.11960) | 0.11599 | 0.10674 | 0.11194 |
  | uniform | `[16 1bfcd535 15 b75a3597 15]` (0.12245) | 0.11484 | 0.11194 | 0.11060 |
  | uniform | `[16 abf8ad2d 15 535a2d97 15]` (0.13078) | 0.12479 | 0.12479 | 0.12479 |

- **`[16 b20332d3 15 d21a3597 15]` = 0.10091140751290814**, 1.1% below F21's
  0.10206799761502400. Confirmed by both exact counters (`mixlab eval --exact`
  and `examples/exact_generic.rs`, 91 s; the same run reproduces F21 to all
  17 digits). Not yet run through hash-prospector itself.
- **It is a radius-2 local optimum** (2026-11-23,
  `results/ironborn_ideas/r2_b203/`, `analysis/queue_r2_b203.sh`): all 2,343
  neighbours (1-2 bit flips of m0/m1, shifts ±1 with s2 ≤ 16) raced against it,
  none accepted. 66 came close enough to be scored exactly; the best,
  `[16 f00332d3 15 d21a3597 15]` and `[16 b70332d3 15 d21a3597 15]`, are
  0.10108 and 0.10111 (≈ 1000·√ms from the race's exact verification), so
  the 32d3 neighbourhood also holds two more mixers below F21's 0.10207. Shift ±2 neighbours, missed by the original generator, were scored exactly later (all ≥ 0.135; `shift2_check.txt`).
- **It is not better on every axis.** Issue #12 metric: 0.035762 vs F21's
  0.035729 (slightly worse). BIC worst-pair correlation 0.60 (in 0 -> out 2,17;
  max|z| 154 at 2^16) vs F21's 0.40 (101). The 0.6022 is identical for every
  `32d3` m0 mixer tried, so it is a property of that core, not of the m1.
  So it is the best bias found but F21 remains the best balanced mixer.
- Seeds matter: one child spanned 0.1009-0.1080 across seeds and another
  0.1067-0.1160; a single seed would have missed the new best. Conversely the
  `d00332d3` child's seeds 0 and 2 converged to the same mixer, so its ~0.1047
  is that family's floor, not bad luck.
- By kind (best seed): half 0.10091, whole 0.10464, uniform 0.11060. Half-swap
  crossover is the useful finer variant; uniform bit mixing gives weak starts
  and weak finishes. Different starts converge: half seed 2 and uniform seed 1
  both ended at `[16 18fcd535 15 cb5e3597 15]` (0.11194), and all three seeds
  of the last uniform child ended at the same mixer.

### F24. 39 hand-picked core pairings: no new record; best race 0.10477 (exact; 2 seeds each)

A reviewer-supplied list of 39 m0/m1 core pairings whose cores had only been
tried with their own partners (heavy on m1 `34d7`, `35a7`, `5269` and m0
`2aad`); `results/ironborn_ideas/core_list.csv`, run by `analysis/race_list.py`
(exact-score all, race the best 5 distinct core pairs with 2 seeds each).

- Exact starting scores: 0.1222-0.2120, median 0.1446. Weaker than gen-3
  crossover starts (0.1135 best): by m1 core, `35a7` 0.1222-0.1838,
  `34d7` 0.1257-0.2120, `5269` 0.1302-0.1600; every `2aad` m0 pairing
  0.1349 or worse.
- Races (exact):

  | start (exact) | seed 0 | seed 1 |
  |---|---|---|
  | `[16 02fccd35 15 535a35a7 15]` (0.12217) | 0.11358 | 0.11276 |
  | `[16 02fccd35 15 735a34d7 15]` (0.12573) | **0.10477** | 0.11054 |
  | `[16 9c013335 15 735a34d7 15]` (0.12597) | 0.10556 | 0.11509 |
  | `[16 8bf8d535 15 ed21ad97 15]` (0.13018) | 0.11318 | 0.11547 |
  | `[16 dbfad52d 15 535a35a7 15]` (0.13068) | 0.11203 | 0.11169 |

- Best: `[16 54fccd35 15 535a34d7 15]` = 0.10477, near gen 2's 0.10447 but
  short of F21 (0.10207) and F23 (0.10091). The `34d7` m1 core does pair
  well outside its own family (two finishes under 0.106), but the gain over
  each family's own best is small.
- Seed spread again large (up to 0.0095 between seeds of one child).

### F25. Beam search past radius 2 finds `[16 b20332d3 15 549a3597 15]` = 0.10075753534782024 (exact, two independent counters)

The F23 mixer `[16 b20332d3 15 d21a3597 15]` (0.10091) is a radius-2 optimum,
so I searched further out with a beam instead of an exhaustive radius-3/4
scan (≈50k / ≈800k candidates): `analysis/beam_step.py`,
`analysis/queue_beam_b203.sh`, outputs in `results/ironborn_ideas/beam_b203/`.
Every candidate is raced against the centre with `race --candidates` (early
acceptance), same move set as the radius-2 check.

- Step 1: all unseen radius-1 neighbours of the 66 radius-2 neighbours that
  the radius-2 check had scored exactly: 3,270 candidates (radius ≤ 3), none
  accepted; closest `[16 b20332d3 15 d49a3597 15]` ≈ 0.10102.
- Step 2: unseen radius-1 neighbours of the 66 best exactly-scored step-1
  candidates: 4,063 candidates (radius ≤ 4). Chunk 4 accepted
  **`[16 b20332d3 15 549a3597 15]`** and stopped early (its remaining
  candidates are untested); the other 5 chunks accepted none.
- It differs from the centre by 4 bit flips in m1 (`d21a3597` -> `549a3597`),
  out of reach of the radius-2 check.
- Exact bias **0.10075753534782024** on both counters (`mixlab eval --exact`,
  `examples/exact_generic.rs`, 106 s): 0.15% below F23, 1.3% below F21.
- Issue #12 metric **0.035309**, now the best of all (F21 0.035729, F23 0.035762).
- BIC unchanged from F23: worst corr 0.60 (in 0 -> out 2,17; max|z| 154 at
  2^16), the `32d3` m0 core's signature. F21 (0.40) remains the best balanced.
- Not yet: hash-prospector `-e`; radius-2 or beam check around the new mixer;
  the untested remainder of step-2 chunk 4.
- Cost: ≈ 2.9 h of racing for 7,333 candidates (vs ≈ 10 h for exhaustive
  radius 3, which would not have reached this radius-4 point).

### F26. Local search around F25 breaks 0.1: `[16 e00332d3 15 549a3597 15]` = 0.09966791588921688 (exact, two independent counters)

Radius-2 check + 2-step beam around F25's `[16 b20332d3 15 549a3597 15]`
with the reusable `analysis/queue_local_search.sh` (+ `filter_seen.py`, which
skipped 66 radius-2 neighbours already raced against F23); outputs in
`results/ironborn_ideas/local_549a/`. All candidates raced against F25 with
early acceptance, so each accepting chunk stopped early (rest untested).

| step | candidates | accepted (exact bias) |
|---|---|---|
| radius 2 | 2,277 | `[16 b70332d3 15 549a3597 15]` 0.10073628061716855 |
| beam 1 | 2,167 | **`[16 e00332d3 15 549a3597 15]` 0.09966791588921688** |
| beam 2 | 4,079 | `[16 e00332d3 15 d49a3597 15]` 0.10004414175138568 |

- **0.09966791588921688 is the first 2-round 32-bit xmx below 0.1** that we
  know of: 1.1% below F25, 2.4% below F21, 6.9% below TheIronBorn's 0.10704.
  Same value on `mixlab eval --exact` and `examples/exact_generic.rs` (78 s).
- It is 3 bit flips from F25, all in m0's top byte (`b2` -> `e0`); the core
  pair `32d3/3597` is unchanged. Earlier hints: `e00332d3` with F23's m1
  was already close (ms 1.0413e-8 in the F25 beam step 1).
- Issue #12 metric 0.035701: better than F21 (0.035729), worse than F25
  (0.035309), so the two metrics now prefer different mixers.
- BIC unchanged: worst corr 0.60 (in 0 -> out 2,17; max|z| 154 at 2^16), the
  `32d3` m0 core's signature. F21 (0.40) remains the best balanced.
- Within one family, repeated local search keeps paying: F23 0.10091 ->
  F25 0.10076 -> F26 0.09967, each a radius-2 optimum or beyond it.
- **PractRand, bit-reversed counter** (`analysis/practrand_feed.c` mode 2,
  `RNG_test stdin32 -tlmax 512MB`; run by the user, 2026-11-23). It fails at
  512 MB like F21 and the old record, but its failures are smaller on almost
  every test (R values at 512 MB; F21 rerun 2026-11-23 reproduced F21's
  numbers exactly):

  | test | old record | F21 | F26 |
  |---|---|---|---|
  | Gap-16:A | +4955 | +1265 | **+152.4** |
  | Gap-16:B | | +920.9 | **+98.1** |
  | BRank(12) | +5307 | +3435 | **+1743** |
  | BDayS2 | +30.8 | +24.0 | **+8.4** |
  | [Low8/32] Gap-16:B | | +172.6 | **+46.8** |
  | [Low8/32] FPF:all | +201.8 | +243.7 | **+115.7** |
  | [Low1/32] Gap-16:A | | +1189 | **+1036** |
  | [Low1/32] Gap-16:B | +6207 | +2408 | **+2137** |
  | [Low1/32] FPF:all | +1082 | +304.9 | **+133.2** |
  | [Low1/32] DC6-9x1Bytes-1 | | **+284.6** | +322.3 |
  | tests with no anomalies | 111 | 122 | **126** |

  - Full-width failures shrink most (Gap-16:A about 8x vs F21); the lowest
    output bit barely improves and now dominates. Only [Low1/32] DC6 is worse.
  - So the worse BIC worst pair (0.60 vs 0.40) does not show up as worse
    PractRand results here. Same limits as F21: one input pattern, R is not a
    calibrated scale; modes 0, 3 and 4 are untested.
- Not yet: hash-prospector `-e`. (The local search centred on F26 is F29.)

### F27. Arithmetic core search finds 476 previously unseen pairs; best reaches 0.11718 (exact bias; one campaign)

2026-11-24. `analysis/new_core_search.py`; all artifacts in
`results/new_cores_2026_11_24/`. The current overall record remains F26's
**0.09966791588921688**. This search found useful new starting points, not a
record or a better demonstrated bias/BIC trade-off.

**Design and novelty.** At shifts 16/15/15, generate first-multiplier cores
`a = q^-1 * r mod 2^16`, with q in {3,5,7,9,17} and 32 small signed odd
residuals per q (16 closest to zero, 16 sampled within +/-1023). Pair them
with second cores 2d97, 3597 and 34d7. Separately vary bits 11-15 of a
second core ending in 0x597, including negation, with first cores 3335,
32d3 and cd35. Include random first-core controls with the same second-core
pool. Deduplicate core pairs after independently identifying each 16-bit
core with its modular negative; this is a search restriction motivated by
F12, not an exact symmetry of score.

- Freeze the prior exact dataset: 2,535 specs, 293 sign-folded core pairs.
- Exclude those pairs from the new groups. This leaves **516 previously
  unscored pairs**: 333 arithmetic, 87 shared-low-11-bit, 96 random.
- Add 6 known control pairs. Attach the same three high-half pairs to each:
  9c01/335a, e003/549a, and the seeded random pair ef87/a5c4.
- Total: **1,566 configurations**, screened at 2^22 inputs, seed 2401.
- Independently rescreen the best extension of the best 8 distinct pairs
  per group at 2^26, seed 2402 (62 configurations; the known group has 6).
- Score 17 finalists exactly: the best 2 per group plus the best 8 overall
  by the independent screen. The F21 control reproduces all 17 digits.
- A separate scan of prior saved specs finds that **476** of the pairs
  were absent even from generated candidate lists. Another 40 had appeared
  in generated lists but had no saved exact score. See `prior_seen.csv`.

**Refinement and exact results.** Select the best four new pairs with
distinct sign-folded first cores. Fix both low 16-bit halves and the shifts.
For each pair, run two seeds with two rounds each: 96 high-bit variants,
2^22 screening, fresh 2^26 rescreening of the top three, exact scoring of
one proposal, and acceptance only on exact improvement. Variants include
all single high-bit flips, short arithmetic steps in the high halves, mixed
2-6-bit flips, and random high-half replacements.

| Low-16-bit pair (raw) | Best full spec | Initial exact bias | Best exact bias | Previously generated pair? | BIC worst abs(corr), 3 seeds |
|---|---|---|---|---|---|
| 3335 / 4597 | `[16 e0073335 15 509a4597 15]` | 0.12141145984169195 | **0.11599999499509583** | yes; previously unscored | 0.398-0.401 |
| d2d3 / 34d7 | `[16 9c04d2d3 15 335a34d7 15]` | 0.13308051097832266 | **0.11717554315024437** | no | 0.648-0.649 |
| 3353 / 34d7 | `[16 e0033353 15 549a34d7 15]` | 0.12560516705290276 | 0.12560516705290276 | no | 0.598-0.600 |
| 552d / 2d97 | `[16 dc01552d 15 335a2d97 15]` | 0.13060397347235267 | 0.13059123885031584 | no | 0.334-0.338 |

An initial adaptive-racing pilot on 3335/4597 accepted two moves before
being interrupted to bound refinement cost across all four pairs. Its last
incumbent was re-evaluated exactly and reused by seed zero of the bounded
search. Thus the first row received extra search effort; do not compare
refinement efficiency between rows. The partial pilot and its status are
retained. All final bias values are full-domain mixlab evaluations; BIC uses
2^18 inputs per seed, seeds 1/2/3, and still shows strong pair correlations.
No new PractRand measurements were made.

**What the arithmetic pattern predicts in this campaign.** The q=5 group
has the largest coarse-screen fraction below ms=4e-8 (bias scale 0.2):
36/192 = **18.75%**, versus 10/288 = **3.47%** for matched random first
cores. The shared-low-11-bit group has the lowest new-group median ms
(1.13e-7 versus 7.29e-7 for random). These are descriptive sampled results,
not exact proportions or independent statistical replications: extensions
and input samples are shared.

The best unseen pair comes from `17 * 0xd2d3 = 14 * 65536 + 3`.
The q=5 finalist satisfies `5 * 0x3353 = 65536 + 159`. However, the q=9
winner also satisfies `3 * 0x552d = 65536 - 121`, so its denominator label
does not establish a distinct mathematical family. The selected pairs are
only 2-3 bit changes from prior exact sign-folded pairs, and d2d3 shares
its bottom 13 bits with 32d3. Novel saved pairs do not imply independent
carry structures.

**Reading.** Small-denominator residuals and the shared low-11-bit pattern
are useful biased candidate generators in this run. They have not exposed
a superior independent basin. The coarse screens can miss promising
extensions (the F26 control's best extension was not the one retained), and
the small refinement budget establishes no local-optimum or family-floor
claim. q=5 enriches promising starts; q=17 supplies the best previously
unseen pair after refinement. Neither improves the current record.

**Reproduction.** Build `harness/examples/screen_list.rs` in release mode,
then run the script's `generate screen exact bounded verify history summary`
stages from the repository root. Existing completed rows/rounds are reused.
To regenerate the extra first-row pilot from scratch, run `refine` until
seed 2500 logs its second accepted move, interrupt it, then run `bounded`;
the retained partial pilot otherwise supplies that starting point.
The Rust wrapper delegates all score computation to the existing SIMD
sampler and retains signed debiased ms. The generic/SIMD sampled-count
regression passed, as did checks of oddness, residual congruences, novelty,
resume behavior and changed-seed rejection. JSON/CSV files retain selection
rules, seeds, budgets, exact proposals, rejected moves and final scores.

### F29. Local search around F26 finds `[16 cb0332d3 15 549a3597 15]` = 0.09908399853581717 (exact, two independent counters)

(Numbered F29 because F27–F28 are on the `64bit` branch.) 2026-11-24.
The F25/F26 recipe, centred on F26 `[16 e00332d3 15 549a3597 15]`:
`bash analysis/queue_local_search.sh "[16 e00332d3 15 549a3597 15]"
results/ironborn_ideas/local_e003 3000 SEEN`, with SEEN listing every
candidate CSV from `local_549a/` and `beam_b203/`. Outputs are in
`results/ironborn_ideas/local_e003/`. Every candidate was raced against F26
with early acceptance. Total run time was 3.1 h.

| step | candidates | accepted (exact bias) |
|---|---|---|
| radius 2 | 2,193 (155 already tried were skipped) | none |
| beam 1 | 2,186 | none |
| beam 2 | 4,059 | **`[16 cb0332d3 15 549a3597 15]` 0.09908399853581717** (chunk 4; the rest of that chunk is untested) |

- **F26 is a radius-2 optimum, and nothing in beam 1 beat it.** The
  improvement came at radius 4: 4 bit flips in m0's top byte (`e0` -> `cb`).
  The core pair `32d3/3597` and m1 are unchanged.
- Exact bias **0.09908399853581717** on `mixlab eval --exact` (26 s) and on
  `examples/exact_generic.rs` (168 s). That is 0.59% below F26, 2.9% below
  F21 and 7.4% below TheIronBorn's 0.10704.
- Issue #12 metric 0.035235, the best yet (F25 0.035309, F26 0.035701).
- BIC (2^16 inputs, seed 1) is the same as F26: worst correlation 0.60
  (input 0 -> outputs 2,17), max|z| 154, the `32d3` signature.
- **Also below 0.1:** the F25 radius-2 race (`local_549a/r2_k0`) had
  accepted `[16 f00332d3 15 549a3597 15]`, but F26 did not report it. It
  scores **0.09994230415343848** on both counters, with issue #12 metric
  0.035508.
- m0's top byte keeps moving within this core: `b2`, `b7`, `e0`, `f0` and
  `cb` all score at or below 0.1008 with m1 `549a3597`. An exhaustive exact
  sweep of that byte (256 specs) is the obvious next check.
- Not yet: PractRand; hash-prospector `-e`; a local search centred on F29;
  the untested rest of beam-2 chunk 4.

### F30. Exact sweep of m0's top byte finds `[16 4b0332d3 15 549a3597 15]` = 0.09873011186879989 (exact, two independent counters)

2026-11-24. All 256 values of bits 24-31 of m0, with the rest of F29
fixed, scored exactly: `python analysis/topbyte_sweep.py run "[16 cb0332d3
15 549a3597 15]" m0 results/ironborn_ideas/topbyte_cb03_m0` (`mixlab exact
--list`, 12 threads, 65 min), then `summary`. Outputs are in
`results/ironborn_ideas/topbyte_cb03_m0/`.

| top byte | exact bias | flips from F29 |
|---|---|---|
| **4b** | **0.09873011186879989** | 1 (bit 31) |
| cb (F29) | 0.09908399853581717 | 0 |
| e0 (F26) | 0.09966791588921688 | 4 |
| f0 | 0.09994230415343848 | 5 |
| 0b | 0.10039535220465691 | 2 |
| b7 | 0.10073628061716855 | 5 |
| b2 (F25) | 0.10075753534782024 | 5 |

- **New best, 0.09873011186879989**, 0.36% below F29. Same value on
  `examples/exact_generic.rs` (84 s). It is F29's radius-1 neighbour, which
  no search had tried: F29 was found by a search centred on F26.
- With the rest fixed, this byte is now exhausted: 4b is its exact optimum.
- Issue #12 metric 0.035315: worse than F29 (0.035235) and F25 (0.035309),
  so the two metrics again prefer different mixers.
- BIC (2^16, seed 1) unchanged: worst corr 0.60 (in 0 -> out 2,17),
  max|z| 154.
- **The byte matters but is not bit-separable.** Bias over the 256 values
  ranges 0.0987-0.1213 (median 0.1081; 4 values below 0.1, 51 below 0.105).
  The largest single-bit effect, the difference in mean bias with the bit
  set versus clear, is only 0.0023 (bit 26), and medians by top nibble range
  0.1068-0.1108. The good values are scattered (4b, cb, e0, f0, 0b), so the
  byte acts jointly, not bit by bit. Every top-byte value that earlier local
  searches accepted (b2, b7, e0, f0, cb) is in the top 7.
- Not yet: the same sweep of m1's top byte with m0 = `4b0332d3` (done:
  F31); PractRand; hash-prospector `-e`.

### F31. Exact sweep of m1's top byte: F30 is already optimal there (exact; no new best)

2026-11-24. All 256 values of bits 24-31 of m1, with F30's m0 `4b0332d3`
and the rest fixed: `python analysis/topbyte_sweep.py run "[16 4b0332d3 15
549a3597 15]" m1 results/ironborn_ideas/topbyte_4b03_m1` (64 min).

| top byte | exact bias | flips from F30 |
|---|---|---|
| **54 (F30)** | **0.09873011186879989** | 0 |
| d4 | 0.09905794251693231 | 1 (bit 31) |
| 94 | 0.10012620964659884 | 2 |
| 74 | 0.10049928261110645 | 1 |
| 14 | 0.10051539973246745 | 1 |
| f4 | 0.10070938244159955 | 2 |

- **F30's `54` is the exact optimum of m1's top byte.** F30 is therefore
  optimal in each top byte separately (m0 by F30, m1 here), so alternating
  byte sweeps have converged after one round. Changing both bytes at once
  is not ruled out.
- **Unlike m0's byte, m1's byte has structure.** Its spread is much wider
  (0.0987-0.2303, median 0.1170), and the low bits of the byte dominate:
  mean bias with bit 24 set vs clear is 0.1333 vs 0.1128 (bit 25: 0.1287 vs
  0.1174; bit 26: 0.1175 vs 0.1286), while bits 29-31 move it by under
  0.002. The 6 best values all have low nibble 4 (`x4`), as do 11 of the
  top 20. The m1 high half F20-F26 settled on (`549a`, also `d49a`) fits
  this pattern.
- Not yet: a joint search over both top bytes (done: F32), or bits 16-23;
  PractRand; hash-prospector `-e`.

### F32. Joint sweep of both top bytes: no pair beats F30; the two bytes act almost additively (exact finalists; sampled screen)

2026-11-24/25. All 65,536 (m0 top byte, m1 top byte) pairs around F30,
everything else fixed. `analysis/joint_topbyte_sweep.py`, outputs in
`results/ironborn_ideas/joint_topbyte_4b03/`.

**Calibration first.** The 511 specs F30/F31 scored exactly were screened
with two seeds per budget. The worst sampled rank (of 511) of a true top-10
spec was 271 at 2^24, 160 at 2^26 and 29 at 2^28. The 2^24 screen was
therefore dropped.

**Pipeline.** Screen all pairs at 2^26 (seed 3101, 4.7 h); rescreen the
best 4,096 with fresh samples at 2^28 (seed 3102, 1.7 h); score the best 64
exactly (22 min). Then, because the screen had missed F29, exactly score
the 24 not-yet-scored two-byte pairs with the best additive prediction
(below; 8 min). 88 new exact scores in all.

| m0 byte / m1 byte | exact bias | source |
|---|---|---|
| **4b / 54 (F30)** | **0.09873011186879989** | finalist |
| 4b / d4 | 0.09905794251693231 | finalist (= F31 runner-up) |
| cb / 54 (F29) | 0.09908399853581717 | missed by the screen |
| cb / d4 | 0.09936563208435061 | additive check |
| e0 / 54 (F26) | 0.09966791588921688 | finalist |
| f0 / 54 | 0.09994230415343848 | missed by the screen |
| e0 / d4 | 0.10004414175138568 | finalist |
| f0 / d4 | 0.10004911977533627 | additive check |

- **No new best.** Every exactly scored pair is worse than F30. Of the 6
  known pairs below 0.1, all use m0 byte 4b, cb, e0 or f0 and m1 byte 54 or
  d4, the best single-byte values of F30 and F31.
- **The two bytes act almost additively.** Predict a pair's bias as F30's
  bias plus each byte's change from F30 in its single-byte sweep. For the 69
  two-byte pairs scored exactly, the error is at most 7.7e-4 (median about
  1.5e-4), small next to the byte effects (up to 0.022 for m0, 0.13 for
  m1). Under this model F30 is the joint optimum, which is consistent with
  the alternating sweeps stopping at F30 after one round.
- **Limits.** The sampled screen is noisy: F30 ranked 178 in the screen and
  33 in the rescreen, and F29 and `f0/54` did not reach the 64 finalists. A
  pair that is much better than additive and ranked poorly in both screens
  is not excluded. The residual range matters here. The cheapest two-byte
  move from F30 (cb with d4) is predicted to cost only 6.8e-4, and the
  most negative residual seen was -7.7e-4. So the residuals alone do not
  rule out a better pair. The 24 best-predicted unscored pairs were scored
  exactly for that reason. None beats F30, and the closest ones have small
  residuals (cb/d4: -4.6e-5).
- Not yet: the same treatment of bits 16-23; PractRand; hash-prospector
  `-e`.

### F33. Bit-position importance around F30: m0 steps down at bit 15, m1 at bit 22; the "16-bit core" is half real (exact; one base mixer, one template)

2026-11-25. Is the low-16-bit core (F10, F14, F16) a property of
multiplication, or of the 16/15/15 shifts? Earlier tests only ever split
at 16. `analysis/bit_sensitivity.py`; data and summary in
`results/bit_sensitivity/` (103 exact scores, 35 min).

**Method.** (a) All 62 single-bit flips of F30's multipliers (bits 1-31;
bit 0 would make them even). (b) Keep the low k bits of both multipliers,
k = 8, 12, 16, 20, 24, and draw the rest at random: 8 draws, the same
draws for every k (paired).

**(a) Single flips, bias / F30's bias.**

| bits | m0 median (range) | m1 median (range) |
|---|---|---|
| 1-14 | 1.75 (1.35-2.41) | 1.81 (1.31-4.95), bits 1-21 |
| 15-21 | 1.24 (1.18-1.41) | (included above) |
| 22-31 | 1.09 (1.00-1.28) | 1.11 (1.00-1.17) |

- **m0 has a step between bits 14 and 15.** Every flip of bits 8-14
  (min 1.53) costs more than every flip of bits 15-21 (max 1.41). The step
  sits at s1 = 15 and next to s0 = 16, so it may be tied to the shifts.
- **m1 has no step at 16.** Bits 16-21 cost almost as much as low bits
  (median 1.60, range 1.36-2.51); the step is between 21 and 22 (min 1.31
  vs max 1.17).
- Above bit 22, both multipliers fall off smoothly toward 1.0 at bit 31,
  which fits F30-F32's small, additive top-byte effects.

**(b) Core width, exact bias of the 8 draws.**

| k | min | median | max | better than k−4, paired |
|---|---|---|---|---|
| 8 | 0.243 | 1.433 | 2.989 | — |
| 12 | 0.243 | 0.739 | 1.556 | 5/8 |
| 16 | 0.152 | 0.204 | 2.030 | 7/8 |
| 20 | 0.139 | 0.200 | 0.451 | 4/8 |
| 24 | 0.116 | 0.131 | 0.159 | 8/8 |

- Bits 12-15 bring the big gain (median 0.74 → 0.20); bits 16-19 bring
  none (4 of 8 draws better); bits 20-23 bring a reliable one (8/8).
- So m1's bits 16-21 matter singly in F30's context (a), but kept on a
  random background they do not help (b). Their good values depend on the
  bits above them, while bits 0-15 are good whatever lies above. F30's core
  with random high halves (median 0.204) is better than the record's (F16,
  0.269 over 16 draws).

**Reading.** Multiplication alone predicts a smooth decline with bit
position (bit j of m reaches only product bits ≥ j and input bits ≤ 31−j).
That holds above bit 22. Below it there are steps that a smooth slope does
not explain: m0's at 14/15, and the core-width plateau at 16-20. That
plateau is why a 16-bit cut looked natural. Whether the steps move with the
shifts is not settled by one template; repeating (a) and (b) at a
non-16/15/15 template in Plan A step A4 would settle it
(`bit_sensitivity.py --base SPEC`).

**No new sub-0.10704 mixers.** 9 of the 103 are below 0.10704 (top-bit
flips), all already scored in F30-F32. Rerunning
`collect_explored32.py` also picked up the campaigns since 2026-11-22:
`explored32.csv` now has 3,365 specs, 551 of them below 0.10704 (was
1,498 and 54).

### F34. Plan A, equal-budget races at 9 shift templates: 16/15/15 still wins by ≥0.011 (exact winners; 6 races per template)

2026-11-25. Plan A steps A1-A2 (plan.md), `analysis/shift_templates.py`;
everything is in `results/shift_templates/` (54 races, ~1.5 h on the GPU
backend).

**A1 screen.** 144 templates (s0 13-18, s1 12-17, s2 12-15) × 84 starts
(64 random odd pairs, plus the 20 best known low-16-bit core pairs with
random high halves), sampled bias at 2^22. Ranked by the 10th percentile
over all starts, 16/15/15 came first, then 17/15/15, 15/15/15, 15/13/15,
18/15/15, 16/13/15, 14/15/15, 14/13/15 and 16/14/15
(`templates_ranked.csv`). The ranking is tilted toward the control because
the core starts reuse cores tuned at 16/15/15. On random starts alone the
best p10s were 16/13/15, 17/13/15 and 16/12/15.

**A2 races.** For the top 8 templates plus 16/15/15, the 3 best starts
(best core start, best random start, best start from another core)
× seeds 1, 2. Settings: `--fix-shifts`, 25 iterations, patience 6,
96 neighbours. Every winner was scored exactly.

| template | best | median | worst | best spec | BIC worst \|corr\| (2^20, 5 seeds) |
|---|---|---|---|---|---|
| 16/15/15 | **0.10471** | 0.12493 | 0.13271 | `[16 f13f3335 15 cca5d269 15]` | 0.656 |
| 15/15/15 | 0.11566 | 0.13765 | 0.18517 | `[15 c6f05553 15 7b5a2d97 15]` | 0.335 |
| 14/15/15 | 0.12879 | 0.14021 | 0.15730 | `[14 c60d1553 15 4c572da3 15]` | 1.000 |
| 17/15/15 | 0.12908 | 0.13643 | 0.17282 | `[17 a20cb335 15 4d6e2d97 15]` | 0.402 |
| 18/15/15 | 0.12959 | 0.14992 | 0.20555 | `[18 f9bc2aad 15 2da9d269 15]` | 1.000 |
| 16/14/15 | 0.14175 | 0.16420 | 0.17760 | `[16 0a3c7335 14 4668ed67 15]` | 0.313 |
| 16/13/15 | 0.15132 | 0.15983 | 0.17588 | `[16 404132cb 13 f1a9b69b 15]` | 1.000 |
| 15/13/15 | 0.16513 | 0.18025 | 0.19353 | `[15 3f587293 13 cea34975 15]` | 1.000 |
| 14/13/15 | 0.18015 | 0.19673 | 0.21400 | `[14 6b183335 13 cb9ad26d 15]` | 1.000 |

- **Under equal budget, 16/15/15 is the best template.** Its best race
  beats the next template (15/15/15) by 0.011, and its median is lower than
  every other template's median. Changing only s0 (14, 15, 17, 18) costs
  less than changing s1 (13, 14). Every s1 ≠ 15 template stays above 0.14.
- **Plan A's A3 gate is not met.** A3 needs another template's best within
  ~5% of the control's (≤ 0.110), and 15/15/15's 0.11566 is 10% above.
- **Exact output-bit pairs.** Five of the nine best winners have a worst
  BIC pair with |corr| = 1.000 (|z| = 1024 = √n). These are 14/15/15,
  18/15/15, 16/13/15, 15/13/15 and 14/13/15. In every template the worst
  output pair is (2, 17), which is s2 = 15 apart. 16/15/15, 15/15/15 and
  17/15/15 avoid an exact pair. Whether this is structural for those
  shift combinations (in the spirit of F3/F11) or specific to these
  multipliers is not yet derived.
- **Limits.** Each race has one seed per start, there are 6 races per
  template, and the budget is short. 16/15/15's record (F30, 0.09873) took
  far more effort than this, so the claim is only about equal budget. All
  templates keep s2 = 15, because no s2 ≤ 14 template reached A1's top 9.

**Sub-0.10704 mixers: 2 new**, both 16/15/15 from race T16_15_15_k0_s1:
`[16 f13f3335 15 cca5d269 15]` = 0.1047107021 and
`[16 b13f3335 15 cca5d269 15]` = 0.1066765664 (`below_10704.csv`). Their
core pairing (…3335 / …d269) is already known: the F21/F22 crossover
family has 7 others below 0.10704, best 0.10344. The race reached it from
a random high half (f13f… instead of …013335). Rerunning
`collect_explored32.py` gives `explored32.csv` 4,013 specs (was 3,365),
553 of them below 0.10704 (was 551).

### F35. Other templates: 31 of 144 have a shift-forced exact BIC pair, and new cores do not lift 15/15/15 (exact winners; one campaign)

2026-11-25. Follow-up to F34: were the other templates held back because
every start reused cores tuned at 16/15/15? Three parts, all scored by
mixlab.

**(a) Exact output pairs are often forced by the shifts.**
`analysis/exact_pair_scan.py`, `results/exact_pair_scan/`. For all 144
Plan A templates, 16 random odd multiplier pairs each, `mixlab bic` at 2^16,
seed 1. In **31 templates every draw** has a pair with |corr| = 1.000, and
in 5 more some draws do. The 31 include every F34 template that showed an
exact pair (14/15/15, 18/15/15, 16/13/15, 15/13/15, 14/13/15), so those
are capped by structure, not by their multipliers. The worst output pair is
always (k, k+s2); the flipped input bit is s0+s1+2 or s1+2 (for
example 14/15/15: input 31; 16/13/15: input 15). Clean s2 = 15 templates
include 16/15/15, 15/15/15, 17/15/15, 16/14/15, 15/16/15 and 16/16/15.
Sampled at 2^16 with one seed, so "exact" here means |corr| printed as
1.0000. F37 derives the mechanism and confirms all 144 templates.

**(b) Bit importance at two more templates** (`bit_sensitivity.py --base`,
`results/bit_sensitivity_15_15_15/`, `results/bit_sensitivity_16_14_15/`;
one base mixer each, the F34 winners).
- 15/15/15: m0's step is at bits 14/15 (flip cost 1.50× vs 1.14×), the
  same place as at 16/15/15 (F33). Changing s0 did not move it.
- 16/14/15: the step is at bits 11/12 (1.95× vs 1.22×). With s1 changed,
  it moved.
- Core width at 15/15/15: median 0.99 / 0.45 / 0.23 / 0.20 / 0.15 for
  k = 8 / 12 / 16 / 20 / 24, the same plateau shape as F33.

**(c) Template-native core search at 15/15/15.**
`analysis/native_cores.py`, `results/native_cores/15_15_15/`.
- Stage 1: 20,000 random sign-folded core pairs and 1,571 F27-style
  arithmetic pairs (q ∈ {3,5,7,9,17}), none in `explored32.csv`, plus the
  30 best 16/15/15 core pairs ("transplant"), each with the same 2 random
  high halves; sampled at 2^22 (43,256 specs, 11 min).
- Stage 2: top 300 pairs by stage-1 mean plus all transplants, 8 fresh
  high halves each, new seed. In the top 40 by median: **24 transplants**
  (of 30), 9 arithmetic (of 1,571), 7 random (of 20,000).
- Exact: best extension of the top 40 non-transplant pairs and the top 5
  transplants. Best native 0.17823, best transplant 0.17766.
- Races (`--fix-shifts`, campaign defaults: 25 iterations, patience 6,
  96 neighbours), top 10 native finalists and top 3 transplants, seeds 1, 2:

  | starts | races | best | median | worst |
  |---|---|---|---|---|
  | native cores | 20 | 0.12521 `[15 d7f34cd3 15 e9173523 15]` | 0.14377 | 0.16844 |
  | transplanted 16/15/15 cores | 6 | 0.12783 `[15 a5c2aaad 15 ca3a3597 15]` | 0.13031 | 0.15123 |
  | F34's 15/15/15 races (for reference) | 6 | 0.11566 | 0.13765 | 0.18517 |

**Reading.**
- At 15/15/15 the transplanted cores were not a handicap: they dominate
  the core screen, and their races match the native ones. That fits (b):
  15/15/15 keeps s1 = 15, and the core seems to follow s1.
- Searching 21,600 new cores found nothing that races better than F34's
  start choice did. 15/15/15 still sits ≥ 0.011 above 16/15/15's F34 best
  under a comparable budget, so the A3 gate stays unmet.
- Templates with s1 ≠ 15 are where native cores should matter most, but
  most of them have a forced exact pair (a); 16/14/15, 15/16/15 and
  16/16/15 are the clean exceptions.
- Limits: one template, one stage-1 seed, short races, n = 1 base mixer
  per bit-importance profile.

**No new sub-0.10704 mixers** (best 0.12521). Rerunning
`collect_explored32.py` gives `explored32.csv` 4,513 specs (was 4,013),
still 553 below 0.10704.

### F36. Native cores at 16/14/15: better screens, slightly better races, still ≥ 0.039 behind 16/15/15 (exact winners; one campaign)

2026-11-25. F35(c) repeated at 16/14/15, the clean template where s1 ≠ 15
and F35(b) showed m0's step moving (bits 11/12). Same script, settings and
budget: `python analysis/native_cores.py <stage> --template 16/14/15`,
`results/native_cores/16_14_15/` (21,628 pairs, 43,256 stage-1 specs,
stage 2 on 322 pairs, 45 exact finalists, 26 races; about 1.3 h).

| | 15/15/15 (F35) | 16/14/15 |
|---|---|---|
| transplants in stage-2 top 40 (of 30) | 24 | 13 |
| best exact finalist, native / transplant | 0.17823 / 0.17766 | **0.20476** / 0.22561 |
| native races (20): best / median | 0.12521 / 0.14377 | 0.14373 / 0.16628 |
| transplant races (6): best / median | 0.12783 / 0.13031 | 0.15730 / 0.16525 |
| F34 races at this template: best | 0.11566 | 0.14175 |

- **Where s1 changes, transplanted cores lose their edge in screening.**
  They fill 13 of the top 40 (24 at 15/15/15), and the best native finalist
  beats the best transplant by 0.021. The best native races beat the best
  transplant race by 0.014, but the medians are equal (0.166 vs 0.165), so
  the race advantage rests on two races.
- **The template is still far behind.** The best of 26 races (0.14373,
  `[16 9cbb674b 14 e1972539 15]`) does not beat F34's 0.14175 there, and is
  0.039 above 16/15/15's F34 best (0.10471) under a comparable budget.
- Together with F35: core origin matters a little when s1 moves, but not
  enough to close the gap between templates. The template ranking from F34
  stands.

**No new sub-0.10704 mixers** (best 0.14373). `explored32.csv` now has
4,793 specs (was 4,513), still 553 below 0.10704.

### F37. The shift-forced exact pairs follow a closed-form rule (solid: derived, exhaustive at 8-12 bits, matches all 144 F35 templates)

2026-11-25. Derives F35(a)'s observation, and with it F11's s2 = 16 pair and
the second fmix32 pair F11 left unexplained. Code: `harness/src/forced.rs`
(`forced_pairs`, `exact_pairs_exhaustive`), `mixlab forced`, tests in
`harness/tests/forced_pairs.rs`.

**Mechanism.** Odd multiplication keeps the lowest bit of an XOR
difference: if a and b first differ at bit t, then m·a and m·b agree below
t and differ at t. Flip input bit i of `[s0 m0 s1 m1 s2]` at width n:
1. v = x ^ (x >> s0) first differs at a = i − s0 (if i ≥ s0) or a = i.
2. w = m0·v first differs at a too.
3. z = w ^ (w >> s1) mixes w_k with w_{k+s1}. If a ≥ s1 + j, z is unchanged
   below bit j. Bit j is also unchanged if a > s1 + j, and always flips if
   a = s1 + j.
4. y = m1·z inherits this, so y_j never flips or always flips.
5. The final xorshift gives out_k = y_k for k ≥ n − s2, and
   out_j = y_j ^ y_{j+s2}. So for j ≥ n − 2·s2,
   Δout_j ^ Δout_{j+s2} = Δy_j is constant.

So input bit i forces the output pair (j, j+s2) for every odd multiplier
pair when max(0, n − 2·s2) ≤ j < n − s2 and s1 + j ≤ a(i) < s1 + j + s2.
The correlation is −1 when a = s1 + j and +1 when a > s1 + j. The upper
bound only excludes cases where both output flips are constant; it never
binds at 32 bits with s2 ≥ 8.

**At 32 bits** (s2 from 8 to 16) this reduces to: a template is clean iff

    s0 + s1 ≥ 2·s2   and   s0 − s1 ≤ 32 − 2·s2

checked against `mixlab forced` on all 8,649 such templates. Otherwise the
worst pair is (32 − 2·s2, 32 − s2), triggered by input s1 + 32 − 2·s2 (when
that is < s0) or s0 + s1 + 32 − 2·s2 (when < 32).
- s2 = 15: clean iff s0 + s1 ≥ 30 and s0 ≤ s1 + 2. At s1 = 15 only
  s0 ∈ {15, 16, 17} are clean, which is exactly F34's split: 14/15/15 and
  18/15/15 carry the pair, 15/15/15 and 17/15/15 do not. 16/14/15 sits on
  both boundaries (30 and 2).
- s2 = 16: the pair is (0, 16) with trigger s1 or s0 + s1, which is F3/F11.
  It is clean only when s0 + s1 ≥ 32 and s0 ≤ s1 (for example 16/16/16).
- F11's unexplained second pair, fmix32's multipliers at 16/13/15 with
  input 15 → (2, 17), is this rule: a = 15 = s1 + 2.
- Of the 14,415 32-bit templates with s2 ≤ 15, 3,815 are forced.

**Checks.**
- The rule matches F35(a) on all 144 templates: the 31 templates where
  every draw had an exact pair are exactly the forced ones. The 5
  templates where only some draws had one are multiplier-specific, not
  shift-forced.
- Exhaustively at 8, 10 and 12 bits, for every template with s2 ≤ n/2 and
  12 random multiplier pairs each (1,327 templates): every predicted
  relation holds for every input. Every predicted pair is a real
  correlation in some draw. Every pair exact in all 12 draws is predicted,
  so at these widths no other multiplier-independent exact pair exists.
- At 32 bits, the predicted relations hold on 20,000 random inputs for 4
  multiplier pairs at each of 6 forced templates.

**Limits.**
- With fewer draws (4), the s2 = n/2 templates at small widths show extra
  pairs that hold for many but not all multipliers. Output bits 0 and 1 of
  a product are linear in the input bits, so a pair through out_0 or out_1
  can be exact whenever, say, bit 1 of m1 is 0. These are
  multiplier-dependent and only reachable at s2 = n/2, which F11 already
  excludes.
- The rule covers 2-round xmx only.

**Consequence for search.** The exact-pair check no longer needs sampling.
Any template search should drop forced templates up front with
`mixlab forced` (s0 + s1 ≥ 2·s2 and s0 − s1 ≤ 32 − 2·s2 at 32 bits). The
clean region is a band around s0 ≈ s1, and it widens as s2 drops: at
s2 = 14 it allows s0 − s1 up to 4 with s0 + s1 ≥ 28. Plan A searched only
s2 ≥ 12 and ranked those templates by bias, so this finding does not
change its conclusion. It does show which templates carry the defect
regardless of multipliers.

No mixers were scored, so there are no new sub-0.10704 mixers.

### F38. Random starts only: 16/15/15 still beats 15/15/15, by more (exact winners; 20 races per template)

2026-11-25. F34's control had a head start: two of each template's three
starts reused low-16-bit cores tuned at 16/15/15. This removes it. Both
templates race only from random odd multiplier pairs: each template's 10
best random starts from F34's 2^22 screen, seeds 3 and 4 (new races; F34
used 1 and 2), same settings (`--fix-shifts`, 25 iterations, patience 6,
96 neighbours). `shift_templates.py jobs --templates 16/15/15,15/15/15
--group random --per 10 --seeds 3,4` with
`CAMPAIGN_DIR=results/shift_templates/random_only`; everything is in that
directory.

| template | races | best | Q1 | median | Q3 | worst | best spec |
|---|---|---|---|---|---|---|---|
| 16/15/15 | 20 | **0.11561** | 0.12978 | 0.13983 | 0.14988 | 0.16909 | `[16 280f366d 15 aa364ae7 15]` |
| 15/15/15 | 20 | 0.13663 | 0.15342 | 0.16296 | 0.18239 | 0.20987 | `[15 761b52ad 15 152dcb27 15]` |

- **The gap survives and widens.** The median gap is 0.023 (F34, with
  cores: 0.013); the best-race gap is 0.021 (F34: 0.011). A 16/15/15 race
  beats a 15/15/15 race in 87% of the 400 pairs (Mann-Whitney U = 348), and
  a permutation test on the median gap gives p < 0.001 (20,000 shuffles).
  9 of the 20 16/15/15 races beat the best 15/15/15 race.
- **Tuned cores helped both templates, not just the control.** Without them
  16/15/15's best drops by 0.011 (0.10471 â†’ 0.11561) and 15/15/15's by 0.021
  (0.11566 â†’ 0.13663). This fits F35: the core follows s1, and 15/15/15
  keeps s1 = 15, so 16/15/15 cores transfer there.
- **BIC** (2^20, 5 seeds, best winner): 16/15/15 worst |corr| 0.426,
  15/15/15 0.354; worst pair (2, 17) in both, no exact pair.
- **Consistency check.** The best random start at 16/15/15 is the one F34
  raced with seeds 1, 2 (0.11796, 0.13189); seeds 3, 4 give 0.13302 and
  0.11561.
- **Limits.** Two templates; one screen seed picks the starts; 2 seeds per
  start. Same iteration budget for both templates. Races took 5-25 s
  against F34's 50-175 s because this ran on new, much faster hardware
  (all 40 races in 8 min); wall times before 2026-11-25 evening are not
  comparable with later ones.

F34's ranking stands, and the reason is not the control's head start:
16/15/15 is the better template from random starts too.

**No new sub-0.10704 mixers** (best 0.11561). `explored32.csv` now has
5,349 specs (was 4,793), still 553 below 0.10704.

### F39. The forced-pair rule extends to any number of rounds, and races can now avoid forced templates (solid: exhaustive at 8-14 bits for 1-4 rounds)

2026-11-25. Generalises F37. Code: `forced::forced_pairs_rounds`,
`forced::shifts_clean`, `mixlab forced --shifts S0/S1/S2/S3...`,
`mixlab race --clean-shifts`; tests in `harness/tests/forced_pairs.rs` and
`harness/tests/shift_templates.rs`.

**Rule.** For a k-round xmx with shifts s0, s1, …, sk, follow c, the
lowest bit at which a run with input bit i flipped differs from the
original:
- The first xorshift gives c = i − s0 (if i ≥ s0) or c = i.
- Every odd multiply keeps c (F37).
- A middle xorshift s gives c → c − s if c ≥ s. Bit c − s then mixes the
  changed bit c into an unchanged bit, and every lower bit mixes two
  unchanged bits. If c < s, the bits below c pick up the multiply's
  unpredictable changes above c, and nothing stays forced.
- After the last multiply, the final xorshift s_k forces (j, j + s_k) for
  max(0, n − 2·s_k) ≤ j < n − s_k with j ≤ c < j + s_k. The correlation is
  −1 when j = c and +1 when j < c.

So a template is forced only if
max(s0 − 1, n − 1 − s0) ≥ s1 + … + s_{k−1} + max(0, n − 2·s_k): the first
xorshift must leave a change high enough to survive every middle shift.
With 2 rounds this is F37; with 1 round there are no middle shifts.

**Checks** (the F37 protocol, 12 random multiplier lists per template; 8
for 4 rounds). Every predicted relation holds for every input. Every
predicted pair is a real correlation in some draw. Every pair exact in all
draws is predicted.
- 1 round at 8, 10, 12 and 14 bits, every template with s1 ≤ n/2: 178 of
  230 forced.
- 2 rounds at 8, 10 and 12 bits (F37).
- 3 rounds at 8 and 10 bits: 332 of 5,017 forced.
- 4 rounds at 8 bits: 79 of 9,604 forced.

At these widths, no other multiplier-independent exact pair exists for
1-4 rounds.

**Published mixers.**
- triple32 `[17 ed5ad4bb 11 ac4c1b51 15 31848bab 14]` is clean with a wide
  margin: it would need c ≥ 11 + 15 + 4 = 30 after the first xorshift, and
  can reach at most 16.
- mx3's 64-bit shifts 32/29/32/29 are clean.
- In general, three or more rounds with shifts near n/2 are clean, because
  the middle shifts add up past anything the first xorshift can leave. The
  defect is a 1- and 2-round problem.

**Race guard.** `mixlab race --clean-shifts` (`RaceCfg::clean_shifts`)
skips every neighbour whose shifts are forced, and logs a `start_forced`
line if the start itself is. This replaces `--max-s2` as the defect guard:
`--max-s2 16` only covers F11's s2 ≥ n/2 case, while the 31 forced F35
templates have s2 ≤ 15. In the 8-bit test, free races from `[4 ad 3 97 3]`
reach forced templates, and the same races with the flag never do. The
flag is opt-in, so earlier campaigns reproduce unchanged. Races with
`--fix-shifts` at a clean template (all Plan A work on 16/15/15) cannot
reach a forced template anyway.

No mixers were scored, so there are no new sub-0.10704 mixers.

### F40. The shortest forced parity has weight three for every recorded sub-0.10704 mixer

`mixlab forced --parity --spec SPEC` reports the smallest number of output
bits whose XOR has a fixed response to a single input-bit flip under the
F39 rule. It prints every witness tied at that weight. The score depends on
the shift schedule and holds for any odd multipliers.

If the lowest difference reaching the final xorshift is bit `c`, the first
`c+1` pre-shift bits have fixed differences. The inverse final shift of
distance `s` expresses pre-shift bit `j` as the XOR of output bits
`j, j+s, j+2s, ...`. Within each residue class modulo `s`, XORing adjacent
fixed rows gives singletons. Keep the last fixed row as a suffix. These
singletons and suffixes form a basis on disjoint supports, so the smallest
basis mask is the minimum over every nonempty combination.

| 32-bit mixer | Exact avalanche bias | Shortest forced parity | Witness |
|---|---:|---:|---|
| F30 `[16 4b0332d3 15 549a3597 15]` | 0.09873 | 3 | Input 15 or 31 flips XOR of outputs 0, 15, 30 |
| F21 `[16 9c013335 15 335a2d97 15]` | 0.10207 | 3 | Same as F30 |
| F18 `[16 6870aaad 15 f452f593 14]` | 0.13168 | 3 | Input 15 or 31 flips XOR of outputs 0, 14, 28 |
| `lowbias32` | 0.17353 | 2 | Input 15 or 31 flips XOR of outputs 0, 16 |
| `[16 de0f5553 16 735a2d97 15]` | 0.19136 | None found | Best bias among such two-round entries in `explored32.csv` |
| `triple32` | 0.02089 | None found | Three-round reference |

The 5,349 entries in `results/explored32.csv` cover 37 shift schedules.
Of those entries, 4,786 have weight three, 552 have weight two, and 11
have no witness under this rule. All 553
entries below 0.10704 use 16/15/15 and share F30's witness. The check
therefore gives no new ordering among those leaders. F18's candidate has
the same weight as F30 despite a much lower measured worst pair correlation,
so both measurements are useful. A schedule with no witness can still have
dependencies outside the propagated-prefix rule.

### F41. Two-bit input differences force output parities even in triple32

The F39/F40 proof extends to a fixed XOR input mask `d`. The first
xorshift turns it into `d ^ (d >> s0)`. Let `a` be the index of the lowest
set bit in that mask. Every odd multiplication preserves that index. If
`a` is at least the sum of the middle shifts, the lowest difference just
before the final xorshift is `c = a - sum(middle shifts)`. Pre-shift bits
below `c` stay fixed and bit `c` always flips. The inverse final
xorshift recovers each such bit `j` as the XOR of output bits
`j, j+s, j+2s, ...`, where `s` is the final shift. If `a` is below
the sum, this rule yields no witness.

In both examples below the two input flips cancel the low copy made by
the first xorshift, leaving bit 31 as the only difference. Bit indices
are zero-based from the least significant bit.

| Mixer | Input bits flipped together | Pre-final lowest difference | Fixed output responses |
|---|---|---:|---|
| F30 `[16 4b0332d3 15 549a3597 15]` | 15, 31 | 16 | Bit 0 stays fixed. Bit 1 always flips. |
| `triple32 [17 ed5ad4bb 11 ac4c1b51 15 31848bab 14]` | 14, 31 | 5 | XOR of bits 4 and 18 stays fixed. XOR of bits 5 and 19 always flips. |

These responses hold for every input and every choice of odd multipliers
at the listed schedules. Spot checks matched all four predictions on
100,000 uniform 32-bit inputs per mixer using Python's
`random.Random(39)` and the listed constants.

The rule gives the following results over all 496 weight-two input masks
at width 32. Each mask is counted once, at its shortest forced output
parity under F40's basis argument. When `0 <= c < s`, that weight is
`1 + floor((31 - c) / s)`. When `c >= s`, two fixed inverse rows
spaced by `s` XOR to a singleton.

| Schedule | Weight 1 | Weight 2 | Weight 3 | No witness under the rule |
|---|---:|---:|---:|---:|
| `16/15/15` (F30) | 2 | 13 | 1 | 480 |
| `16/15/14` (F18) | 3 | 10 | 3 | 480 |
| `16/16/15` | 1 | 13 | 2 | 480 |
| `17/11/15/14` (triple32) | 0 | 2 | 4 | 490 |

Report input-mask weight alongside output-parity weight in future
structural screens. Both `16/16/15` and `triple32` have no single-bit
witness under F40, yet both have two-bit witnesses. A candidate experiment
is to put an invertible rotate-XOR layer between the multiplies and measure
forced parities, avalanche bias, and throughput. Applying another invertible
linear layer only at the output preserves the fixed linear response under
a change of output mask. See the rotate-XOR controls `rrmxmx` and `nasam`
in `harness/src/refs.rs`.

### F42. Round traces for MD5, SHA-256, and SHA3-256

An instrumented Python run used 128 random 32-byte messages from
`random.Random(39).randbytes(32)`. Each message was compared with two
variants, separately flipping the low bit of byte 0 and byte 31. All final
digests matched Python's `hashlib`. The table gives mean Hamming distances
of the working state for the byte-0 flip. An update is one of MD5's 64
steps, one SHA-256 round, or one Keccak-f[1600] round.

| Algorithm | State bits | After 1 update | After 2 | After 3 | After 8 |
|---|---:|---:|---:|---:|---:|
| MD5 | 128 | 1.09 | 3.30 | 6.98 | 55.15 |
| SHA-256 | 256 | 3.65 | 25.95 | 56.95 | 127.27 |
| SHA3-256's Keccak permutation | 1,600 | 21.99 | 335.86 | 800.45 | 798.55 |

- MD5 injects the changed message word through addition and rotation
  into one working word. For byte 0, all four working words differed
  after four steps. The byte-31 flip first entered at step 8.
  [MD5 specification](https://www.rfc-editor.org/rfc/rfc1321.html)
- SHA-256 injects the changed message word into `T1`, which updates two
  working words. For byte 0, all eight working words differed after
  four rounds. The byte-31 flip first entered at round 8.
  [SHA-256 processing specification](https://www.rfc-editor.org/rfc/rfc6234.html#section-6.2)
- Keccak absorbs both tested bytes before the first round. The column
  mixing step `θ` spreads a one-bit difference to exactly 11 bits.
  The rotations `ρ` and lane permutation `π` preserve that count.
  The nonlinear step `χ` produced 16–28 changed bits for the byte-0
  flip in round one. The round constant added by `ι) cancels in the XOR
  difference.
  [Keccak specification](https://keccak.team/keccak_specs_summary.html)

Keccak's designers also tested differences that cancel in the column
parities. Two changed bits in the same column leave every column parity
unchanged, so `θ` passes that difference through unchanged. The
rotations and lane permutation can then move those changes into different
columns. This is the design lesson behind F41. Test input differences
chosen to cancel in the early mixing layer, and track whether later
layers disperse them. The table samples ordinary diffusion at two bit
positions and establishes no all-input property for the full digests.
[Keccak reference, §§2.4.3 and 4](https://keccak.team/files/Keccak-reference-3.0.pdf)
· [On Alignment in Keccak](https://keccak.team/files/KeccakAlignment.pdf)

### F43. A 1,005-race sweep from unseen cores at 16/15/15: 6 new sub-0.10704 mixers (best 0.10394), and races drift back to the known basin (exact winners; one race per start)

2026-11-25, on the new hardware (F38). Do good 16/15/15 mixers exist
outside the lineage that produced the current best? `native_cores.py` with
`--template 16/15/15 --dir sweep_16_15_15` (state in
`results/sweep_16_15_15/`):

- **Stage 1:** 200,000 random odd core pairs (low 16 bits of m0, m1) and
  1,598 F27 arithmetic pairs, all absent from `explored32.csv` after sign
  folding, plus the 30 best known pairs as controls; 2 random high halves
  each, sampled at 2^22 (403,256 specs, 39 min, gen seed 7).
- **Stage 2:** top 2,000 pairs by stage-1 mean plus all controls, 8 fresh
  high halves each (16,208 specs, 2 min).
- **Exact:** best extension of the top 1,000 new pairs and the top 5
  controls (1,005 specs, 108 s).
- **Races:** one race from each (`--fix-shifts`, 25 iterations, patience 6,
  96 neighbours, seed 1): 1,005 races, 3.6 h.

**Screen.** Known cores still lead stage 2 (ranks 1-5 and 20 of the top
40). The best new pair, `5353/4add`, is 6th.

**Races.**

| quantile | 1% | 5% | 10% | 25% | 50% | 75% |
|---|---|---|---|---|---|---|
| exact bias of winner | 0.11205 | 0.11897 | 0.12424 | 0.13238 | 0.14236 | 0.15420 |

8 winners are below 0.11, 24 below 0.115. The 5 control races: best
0.10893, median 0.11562.

| bias | winner | m0 / m1 core (folded) | known before? | BIC worst \|corr\| (2^20, 3 seeds) |
|---|---|---|---|---|
| **0.10394** | `[16 e63f2ab3 15 7291c9a9 15]` | 2ab3 / 3657 | m0 core yes (best 0.1117), m1 no | 0.627 |
| 0.10672 | `[16 9340cab5 15 a2b9ad19 15]` | 354b / 52e7 | neither | 0.667 |
| 0.10683 | `[16 1f00d54d 15 22b5cb29 15]` | 2ab3 / 34d7 | both halves, not as a pair | 0.667 |
| 0.10706 | `[16 c73f2acb 15 d672b635 15]` | 2acb / 49cb | m0 yes, m1 no | |
| 0.10863 | `[16 d83cd54b 15 e9c6b4e5 15]` | 2ab5 / 4b1b | neither | |

The 0.10683 winner was reached twice, from two different starts.

- **Races do not stay where they start.** 968 of 1,005 races changed a
  low-16-bit core, so a start's "new core" is not the winner's. Winners
  drift toward m0 cores near the alternating pattern 2aab/2aad (224 of
  1,005 winners within 3 bits, against 154 starts; 45 of the top 100) and
  m1 cores near 34xx-36xx, the region the current best lives in
  (m1 core 3597).
- **The start's exact bias does not predict the race result.** Spearman
  between start bias and winner bias is 0.07 over 1,005 races. Starts
  ranged 0.149-0.35; the best winner came from a start at 0.319. So the
  screen picks plausible starts, but race luck dominates which ones end
  low. Many short races from diverse starts beat a few chosen ones.
- **No new best.** The best winner, 0.10394, would have ranked among the
  F22-F23 crossover children; it is 5% above the current best (0.09873).
  The one winner with both core halves new (354b/52e7, 0.10672) is the
  most distinct new family.
- **Limits.** One seed per start; each "family" is one race; BIC only for
  the top 3 (none has an exact pair, as F37 predicts for 16/15/15).

**Sub-0.10704 mixers: 6 new**: the 3 winners above (`below_10704.csv`) and
3 earlier incumbents of the 0.10394 race (`[16 a63f2ab3 15 7291c9a9 15]`
0.10519, `8e3f2ab3`/`7291c9a9` 0.10601, `8e3f2ab3`/`f291c9a9` 0.10623).
With F44, rerunning `collect_explored32.py` gives `explored32.csv` 18,416
specs (was 5,349; most of the growth is the sweep's race incumbents), 591
of them below 0.10704 (was 553).

### F44. The current best is a radius-3 local optimum (exact near misses; racing for the rest)

2026-11-26. F30's `[16 4b0332d3 15 549a3597 15]` (0.09873) had never had a
radius-2 check. `gen_radius2_cands.py` now takes a radius argument; radius 3
under TheIronBorn's move set (flip bit 1-31 of m0 or m1, shift Â±1, moves
may repeat) gives 52,601 neighbours. Each was raced against the centre
(`mixlab race --candidates`, z = 3, cap 2^28; 12 chunks, 2 min in all;
`results/radius3_4b03/run.sh`).

- **None is better.** 52,292 were dropped as worse by racing; 309 reached
  the cap without being shown worse, and all 309 were then scored exactly
  (`exact.csv`, 34 s): none beats the centre.
- **The closest neighbours are top-bit flips**, which F30-F32 had already
  swept: `[16 4b0332d3 15 d49a3597 15]` 0.09906 (m1 bit 31),
  F29's `cb0332d3` 0.09908, and both together 0.09937. 104 of the 309 are
  below 0.10704.
- So the current best is a local optimum out to radius 3 under this move
  set. This rests on F13's validated racing drop rate for the 52,292
  sampled verdicts; the 309 near misses are exact. Threat #3 now applies
  only to radius â‰¥ 4 or other move sets.

**Sub-0.10704 mixers: 32 new**, all radius-3 neighbours of F30
(0.09906-0.10703).

### F45. Follow-up of the F43 families: the new 354b/52e7 family climbs to `[16 87c0cab5 15 62b9ad19 15]` = 0.10018 (exact; one climb)

2026-11-26. `analysis/family_followup.py`, `results/family_followup/`.
The two most interesting F43 winners, `2ab3/c9a9` (0.10394) and
`cab5/ad19` (folded 354b/52e7, both halves unseen before F43; 0.10672).

**Multi-seed races** (`--fix-shifts`, campaign budget; 40 jobs, 10 min):

| family | sweep | R: restart from F43 start, seeds 2-9 | C: continue from winner, seeds 2-9 | K: winner's low-16 core, 8 exact high halves â†’ race (4 jobs) |
|---|---|---|---|---|
| 2ab3/c9a9 | 0.10394 | best 0.10649, median 0.13874 | all 0.10394 | best 0.11487, median 0.12281 |
| cab5/ad19 | 0.10672 | best 0.10565, median 0.12424 | best 0.10618 (`a340cab5`) | best 0.12134, median 0.12706 |

- F43's 0.10394 was a lucky race: none of 8 restarts from its start got
  below 0.106. Racing from either winner with new seeds barely moves it.
- Neither family's quality sits in its low 16 bits alone (K arm 0.115+),
  which fits F33 (m1 bits 16-21 matter in context).
- The best cab5 restart drifted to a different core,
  `[16 6900d535 15 92b9b519 15]` = 0.10565.

**Radius-3 checks** (`gen_radius2_cands.py ... 3`, every neighbour raced,
accepted and near-miss neighbours scored exactly; 767 exact):
- `[16 e63f2ab3 15 7291c9a9 15]` and `[16 6900d535 15 92b9b519 15]` are
  radius-3 optima.
- `[16 a340cab5 15 a2b9ad19 15]` is not: m0 top-byte moves win, and
  `[16 87c0cab5 15 a2b9ad19 15]` = **0.10056** exact.

**Iterated radius-3 climb** from 0.10056 (`climb.sh`): step 1 moves m1's
top bits to `62b9ad19` = **0.10018**; step 2 finds nothing better (best
neighbour 0.10023). Exact sweeps of each top byte (256 each) and of the
16 Ã— 16 best byte pairs: 0.10018 is optimal in all three.

| | spec | exact bias | BIC worst \|corr\| (2^20) |
|---|---|---|---|
| current best (F30) | `[16 4b0332d3 15 549a3597 15]` | 0.09873 | 0.602 |
| **new family best** | `[16 87c0cab5 15 62b9ad19 15]` | **0.10018** | 0.667 (3 seeds) |

- **A second basin.** 0.10018 is 1.5% above the current best, from a core
  pair (354b/52e7) that no earlier search had touched, and it got there
  in one sweep race plus 3 local-search steps (about 20 min in all). The
  current best's lineage took F20-F30.
- **The same top-byte effect.** As in F29-F30, the last 5% came from
  m0's and m1's top bits, after the low bits had settled.
- **Limits.** One climb; racing verdicts for the ~50k worse neighbours per
  step (F13 drop rate); the joint top-byte check covered only the 16 best
  values of each byte (F32 found the bytes act almost additively). The
  full 65,536-pair joint sweep was started and stopped, because another
  session's CPU campaign was running on all cores (`joint_p24_partial.csv`).

**Sub-0.10704 mixers: 232 new** (188 now in the cab5 family, 47 in
2ab3). `explored32.csv` now has 20,555 specs (was 18,416), 823 below
0.10704 (was 591).

### F46. Parity-free templates do not race better: 16/16/15 is far behind 16/15/15 (exact winners; 20 races)

2026-11-26. Tests whether removing F40's forced parity buys bias.

**Parity scan.** `mixlab forced --bits 32 --shifts S0/S1/S2 --parity` over
all 405 templates with s0, s1 in 12-20 and s2 in 12-16:

| shortest forced parity | templates | meaning |
|---|---:|---|
| 2 | 120 | the F37 forced pairs |
| 3 | 160 | F37-clean, still a forced 3-bit parity (16/15/15, 15/15/15, 17/15/15, 15/16/15, 17/16/15, 16/15/14) |
| none | 125 | no forced parity under the F39 prefix rule |

The 125 are exactly **s0 ≤ s1 and s0 + s1 ≥ 32**, 25 per s2 value, the same
band for every s2 from 12 to 16. This is read off the grid, not derived.
Before this campaign `explored32.csv` held 1-2 specs for each of these
templates (16/16/15: 2, best 0.191).

**Races.** F38's protocol at 16/16/15: the 10 best random starts of F34's
2^22 screen, seeds 3 and 4, `--fix-shifts`, 25 iterations, patience 6, 96
neighbours, run with `--cpu` (about 2 h, alongside another GPU campaign, so
wall times are not comparable). `CAMPAIGN_DIR=results/shift_templates/random_16_16_15`.

| template | races | best | median | worst |
|---|---|---|---|---|
| 16/15/15 (F38) | 20 | 0.11561 | 0.13983 | 0.16909 |
| 15/15/15 (F38) | 20 | 0.13663 | 0.16296 | 0.20987 |
| 16/16/15 | 20 | **0.16261** `[16 a6f95535 16 a1b25947 15]` | 0.20109 | 0.30144 |

- **16/16/15 is worse than both.** Median gap to 16/15/15 is 0.061, and even
  its best race is 0.047 above 16/15/15's best.
- **No family.** The winners share no core; the second best is
  `[16 20239657 16 f4d734a3 15]` (0.16379).
- **BIC** (2^20, 5 seeds, best winner): worst |corr| 0.335 at pairs (0, 17)
  and (1, 17); no exact pair.
- **Reading.** Having no forced parity under F40's rule does not predict
  low bias in 2-round xmx; the record template carries a weight-3 parity
  and wins by a wide margin. The other parity-free templates (16/16/14,
  15/17/15, 16/17/15, ...) were not raced.
- **Limits.** One template raced; random starts only (F35/F36: cores follow
  s1, so native cores might help, and F36 saw gains of about 0.01 at
  16/14/15); one screen seed picks the starts; the parity scan is the
  prefix rule only (F40's caveat).

**No new sub-0.10704 mixers** (best 0.16261). `collect_explored32.py` now
reports 20,570 exact specs.

### F47. The gaps in the atlas are empty: crosses between families fail, and quality collapses within a few bits of the best (exact; 16 crosses, 1,287 probes)

2026-11-26. Prompted by the Mixer Bit Atlas (`viz/explorer.html`) after
`collect_explored32.py` picked up F43-F46 (1,497 → 20,570 specs). Its
"bias against distance from the reference" panel at 16/15/15 shows clumps:
columns at fixed low-16 distances, and three clumps at all-64 distance from
F30. This tests whether the gaps between them hide good mixers.

**What the clumps are** (distance from F30, negation folded on bit 31 as
the atlas does):

| all-64 distance | specs | best | median | sub-0.10704 | what they are |
|---|---:|---|---|---:|---|
| 0-11 | 568 | 0.09873 | 0.110 | 131 | F30's own family (crossover children, radius-3 neighbours) |
| 12-19 | 210 | 0.10830 | 0.14 | 0 | thin |
| 20-43 | 16,873 | 0.09906 | 0.154 | 548 | race winners from unrelated starts; two unrelated 64-bit words differ in ~32 bits, so they pile up there |
| 48-59 | 474 | 0.10004 | 0.108 | 143 | almost all F30's family in mirrored form (core pair cd2d/ca69: 435): `34fccd2d` = −(`4b0332d3` ^ 0x80000000), core `cd2d` = `32d3` ^ `fffe` |

The low-16 columns are the same effect: crossover children carry family
cores unchanged, so their core distance to F30 takes a few fixed values
(0, 11, 15, 30 = mirrored core). The 48-59 clump is a display artifact of
folding on bit 31 alone; the core matrix already merges c and c ^ `fffe`.

**Whole-multiplier crosses between families.** The atlas's core matrix
lists 71 never-tried pairs among the 12 best m0 and 12 best m1 cores; its
top suggestions cross F30 with F45. The 12 suggested children plus 4 more
F30 × F45 crosses (`results/atlas_untried_crosses.txt`, `mixlab exact
--list`, 2 s): **all 16 lie between 0.10924 and 0.17443.** Best
`[16 783f354b 15 549a3597 15]` = 0.10924 (F45's m0 with F30's m1); F30's
m0 with F45's m1 `62b9ad19` = 0.12888. m0 and m1 are co-adapted: a strong
m1 does not carry over to another family's m0. The matrix's "untried
strong pairs" heuristic is a poor guide.

**Distance profile of F30.** `analysis/distance_profile.py` draws random
mutants of `[16 4b0332d3 15 549a3597 15]` with exactly k flipped bits
(never bit 0), in bits 1-31 of both multipliers ("any"), 1-15 ("low") or
16-31 ("high"); 40 per (region, k), duplicates dropped, 1,287 specs, all
exact (`results/distance_profile/best_f30.*`, 145 s on the GPU).

| k | any: best / median | high only: best / median | low only: best / median |
|---:|---|---|---|
| 1 | 0.09908 / 0.139 | 0.09906 / 0.110 | 0.13354 / 0.151 |
| 2 | 0.10797 / 0.156 | 0.10013 / 0.125 | 0.13036 / 0.250 |
| 3 | 0.12412 / 0.234 | 0.10583 / 0.155 | 0.13766 / 0.387 |
| 4 | 0.16638 / 0.292 | 0.11112 / 0.155 | 0.17704 / 0.436 |
| 8 | 0.22697 / 0.666 | 0.12251 / 0.239 | 0.24642 / 1.461 |
| 16 | 0.29416 / 1.466 | 0.15497 / 0.282 | 0.45731 / 2.318 |
| 24 | 0.61564 / 1.881 | 0.15301 / 0.228 | 0.24521 / 0.925 |

(k = 1 has only 30/13/10 distinct mutants.) Sub-0.10704 probes: 13, all at
k ≤ 3, almost all top-bit flips F44 had already found.

- **The gaps hold bad mixers.** By k = 4 no probe is within 0.004 of
  0.10704; at k ≥ 8 the median is that of a random mixer. The 20-43
  clump's median of 0.154 comes from racing, not from that region being
  good. There are about 5 × 10^14 mixers at distance 16; sampling them at
  random is not a search.
- **The cleanest direct test of the low-bit core so far.** One flipped
  core bit (bits 1-15) already costs ≥ 0.035 (best 0.13354). One flipped
  high bit often costs nothing; even 24 high flips keep the median at
  0.23, far better than 24 low flips' 0.93. This fits F10/F16, and F33's
  caveat that some high bits matter in context.
- **Limits.** One centre (F30); 40 draws per cell; folding on bit 31 in
  the distance table.

**Atlas.** `collect_explored32.py` now labels `distance_profile/` and
`atlas_untried_crosses*` as a "Probes" origin, so they can be
highlighted in the scatter, where they fill the gaps well above the race
floor.

**Sub-0.10704 mixers: 1 new**, `[16 4f0332d3 15 749a3597 15]` = 0.10585
(a k = 2 high-bit probe). `explored32.csv` now has 21,806 specs, 824 below
0.10704.

### F48. Races from mid-distance mutants of F30: its basin is about 4 bits wide, and past 12 flips a start is no better than a random one (exact winners; 112 races)

2026-11-26. Follow-up of F47: instead of sampling the gaps at random, race
from them. `analysis/midstart_races.py` draws 8 starts per arm by flipping
k random bits of F30 `[16 4b0332d3 15 549a3597 15]` (never bit 0): in
bits 1-31 of both multipliers ("any", k = 4, 8, 12, 16, 24) or bits 16-31
only ("high", k = 8, 16). Each start is raced with seeds 1 and 2 under
F38's protocol (`--fix-shifts`, 25 iterations, patience 6, 96 neighbours,
`campaign.py`); everything is in `results/midstart_f30/`. Core distances
below count a multiplier and its negation as the same.

| arm | races | start median | best winner | median | worst | sub-0.10704 | on F30's core | exactly F30 |
|---|---:|---|---|---|---|---:|---:|---:|
| any 4 | 16 | 0.370 | **0.09873** | 0.11358 | 0.13545 | 6 | 7 | 6 |
| any 8 | 16 | 1.309 | 0.10306 | 0.11902 | 0.15007 | 3 | 2 | 0 |
| any 12 | 16 | 0.841 | 0.10571 | 0.12427 | 0.18078 | 1 | 0 | 0 |
| any 16 | 16 | 1.179 | 0.11208 | 0.13997 | 0.18546 | 0 | 0 | 0 |
| any 24 | 16 | 1.223 | 0.11133 | 0.14124 | 0.16778 | 0 | 0 | 0 |
| high 8 | 16 | 0.246 | **0.09873** | 0.10752 | 0.12334 | 8 | 8 | 1 |
| high 16 | 16 | 0.353 | 0.10105 | 0.11754 | 0.14651 | 1 | 3 | 0 |
| F38 random starts | 20 | | 0.11561 | 0.13983 | 0.16909 | 0 | | |

- **The basin is narrow.** From 4 random flips, 6 of 16 races climb back
  to F30 exactly; from 8 flips none do, and from 12 none even keep its
  core. Winners end *further* from F30 than their starts (median all-64
  distance 4 → 7, 8 → 12.5, 12 → 16, 16 → 19.5): racing does not pull
  back toward the best, it settles in whatever basin is nearest.
- **Past about 12 flips the head start is gone.** The any-16 and any-24
  medians (0.1400, 0.1412) match F38's random starts (0.1398). The
  any-16 and any-24 starts sit 6-10 and 9-12 core bits from F30.
- **High-bit perturbations are recoverable, core ones are not.** Starts
  with an intact core (high 8) give the best median (0.1075) and 8 sub-
  0.10704 winners, but race back to F30 itself only once in 16. High 16
  keeps 3 of 16 winners on the core. This repeats F47's asymmetry
  under search.
- **Nearby cores, none better.** Winners below 0.11 sit on 7 cores, all
  within a few bits of F30's: 32d3/3597 (15), 4d2d/3597 (5; core bit 15
  flipped, best `[16 4f0fb2d3 15 ad1a3597 15]` 0.10450), 4cd3/3597
  (0.10532), 32d3/34d7 (`[16 e20332d3 15 84ca34d7 15]` 0.10571),
  34cb/3597, 32cb/3593, 2d2d/4a69. No race found a new family below
  0.10.
- **Seeds disagree.** For 55 of 56 starts the two seeds end at different
  winners, so a single race per start says little about the start.
- **Limits.** One centre; 8 starts per arm; F38 is the only random-start
  comparator at this budget. The first 14 races ran on a mixlab build
  without GPU support (4-12 min each; identical counts), the rest on the
  GPU (median 15 s), so wall times are not comparable within the run.
  `any4_i6_s1` was interrupted and resumed (`campaign.py` continues from
  the last incumbent with a fresh neighbour seed).

**Reading.** Random restarts near F30 are only worth it within about 4-8
flips, and then mostly re-find F30. The gaps between atlas clumps are
neither hiding nor leading to good mixers. New families have to come
from new cores (as F43/F45 did), not from perturbing the current best.

**Sub-0.10704 mixers: 25 new** (all from race incumbents near F30's
core; best `[16 8ffb32d3 15 d49a3597 15]` = 0.10093). `explored32.csv`
now has 23,232 specs, 849 below 0.10704. `collect_explored32.py` labels
these races as "Mid-distance races".

### F49. 16-bit: a 6x faster exact kernel; TheIronBorn's `[8 a3d3 7 4b2d 9]` = 7.25294 is the exact optimum of 8/7/9, and nothing on 12 other exhausted triples comes within 4% (exact; 8 races, 12 exhaustive triples)

2026-11-26. **Kernel.** `harness/src/fast16.rs` scores widths 9-16 in 32
u16 lanes with explicit AVX-512BW intrinsics (`vpmullw`), with a portable
fallback. It tabulates y = x ^ (x >> s0) and h(x) once, then does one
Harley-Seal pass per input bit. Counts are bit-identical to `fast32`
(`tests/fast16.rs`, widths 9-16, 1-4 rounds; the fallback was tested with
`-C target-cpu=x86-64-v3`). Throughput for 16-bit 2-round on 24 threads:

| path | configs/s |
|---|---:|
| GPU (one launch per config; what `exact --list` used by default) | ~5,700 |
| `fast32` CPU | ~26,000 |
| `fast16`, array form (LLVM left it scalar) | ~32,000 |
| **`fast16`, AVX-512BW intrinsics** | **~158,000** |

The array form compiled to scalar `imull`, and the emitted assembly has
no vector multiplies for `fast32` either, so fast32's "auto-vectorises"
note does not hold for this build. `exact::exact_xmx` now sends widths
9-16 to `fast16` on the CPU unless `--gpu` is given. Per row, the
kernel's `exact_ms_bounded` stops once the rows done so far exceed a
bound, but no command uses it yet.

**Races.** `analysis/memetic16.py`: 8 races (seed 1), each from 256 random
2-round mixers with shifts 4-12. Every generation exactly scores 3,000
crossover/mutation children plus the full 1-flip neighbourhood of the top
16 and the 2-flip neighbourhood of the best. The population is 64, with at
most 16 per shift triple; each race has patience 40. In total 682,908
distinct configs over 821 shift triples were scored in a few minutes.
Results are in `results/memetic16/run1/` (`exact.csv.gz` holds every score).

| race | best | bias | gen of best |
|---:|---|---:|---:|
| 2, 7 | `[8 a3d3 7 4b2d 9]` | **7.25294** | 27, 23 |
| 4 | `[8 84d3 7 34d3 9]` | 7.55857 | 9 |
| 5 | `[8 8c4b 7 3cd3 9]` | 7.71893 | 6 |
| 0 | `[8 242d 7 2ccb 9]` | 7.72804 | 10 |
| 1 | `[8 53cb 7 1b2d 7]` | 7.74078 | 6 |
| 6 | `[9 f11b 7 d34b 7]` | 7.81745 | 18 |
| 3 | `[9 7b35 7 eccb 8]` | 8.25851 | 5 |

- Two independent races end exactly on TheIronBorn's published mixer
  (hash-prospector #19, 7.25294). No scored config is below it. The best
  of the other shift triples is `[6 5c23 7 52d3 9]` = 7.56831.
- hash-prospector's `hash16_xm2` (8.59051) is beaten within a few
  generations by every race.
- Every race but 3 ends at s1 = 7, and 5 of 8 at 8/7/9.

**Exhaustive check.** `mixlab exhaust --bits 16 --shifts S0/S1/S2
--max-score 8` scores all 2^30 odd multiplier pairs of one triple with
`exact_ms_bounded`, and keeps every mixer whose bias is <= 8. Rows are
taken from the top input bit down. The test is `exhaust2_matches_brute_force`
(10 bits, all pairs). The run on 8/7/9 took 1,301 s; the others took
15-25 min each, 24 threads. `analysis/queue_exhaust16.sh` queues triples.
Every kept mixer is in `results/exhaust16/s<S0>_<S1>_<S2>.csv`. The
runner-up triples are the next 12 by best race score.

| triple | exact optimum (all 2^30 pairs) | bias | race best | kept (<= 8) |
|---|---|---:|---:|---:|
| 8/7/9 | `[8 a3d3 7 4b2d 9]` | **7.25294** | 7.25294 | 239 |
| 8/7/7 | `[8 dc53 7 64d3 7]` | 7.56300 | 7.74078 | 42 |
| 6/7/9 | `[6 5c23 7 52d3 9]` | 7.56831 | 7.56831 | 74 |
| 7/7/8 | `[7 84d3 7 2f2d 8]` | 7.75270 | 8.6458 | 4 |
| 9/7/7 | `[9 f11b 7 d34b 7]` | 7.81745 | 7.81745 | 3 |
| 9/6/8 | `[9 c363 6 3693 8]` | 7.85868 | 7.85868 | 4 |
| 7/7/7 | `[7 5257 7 f525 7]` | 7.93444 | 8.5444 | 4 |
| 7/7/9 | `[7 5c23 7 2b2d 9]` | 7.93566 | 7.93566 | 6 |
| 9/7/9 | `[9 f2cb 7 f2d3 9]` | 7.96778 | 8.0820 | 2 |
| 8/7/8 | `[8 e3db 7 c5a9 8]` | 7.99459 | 8.0288 | 1 |
| 9/7/8 | none <= 8 | | 8.2585 | 0 |
| 8/6/8 | none <= 8 | | 8.2639 | 0 |
| 7/6/8 | none <= 8 | | 8.4434 | 0 |

- **TheIronBorn's `[8 a3d3 7 4b2d 9]` is the exact optimum of 8/7/9.**
  No other triple searched gets below 7.56. The two next best, 8/7/7 and
  6/7/9, are 4.3% worse.
- Races found the exact optimum on 5 of the 6 triples they had searched
  closely (best race score < 8). On weakly sampled triples they stopped
  0.03-0.6 above it.
- The triples are already sparse at 8.0: only 8/7/9 (239), 6/7/9 (74)
  and 8/7/7 (42) have more than a handful of mixers below 8.

**Reading.** TheIronBorn's mixer is the best 2-round 16-bit mixer on
every shift triple searched exhaustively. Over the remaining triples it
is very likely, though not proven, to be the global 2-round optimum:
races reached 821 triples, and none came within 0.3 of it outside
8/7/9. Proving it for all 3,375 triples at ~22 min each would take
about 52 days on this machine. Covering shifts 5-11 (343 triples) would
take about 5 days.

### F50. Three rounds: exact-scored descent breaks 0.020, reaching `[17 88bb3153 13 238e8679 17 7da6b66b 17]` = 0.019134091125396723 in round four (exact, CPU and GPU agree)

An exact three-round score now costs ~0.13 s on the GPU, so
`analysis/triple32_descent.py` searches with exact scores only. A step
flips one of bits 1-31 of a multiplier or moves one shift by one.
Descent moves to the best improving neighbour in the first improving
chunk of 34 shuffled neighbours. Iterated local search (ILS) kicks the
incumbent with 3-6 random multiplier-bit flips, descends, and keeps the
result if it is no worse. Results are in `results/triple32_memetic/descent/`.

| Run | Descents | Below `triple32` | Best end |
| --- | ---: | ---: | --- |
| ILS from the old record (0.0202413) | 150 | 55 | `[17 895245bb 10 a04c13d1 15 cac49ba3 14]` 0.0201755990 |
| ILS from that | 150 | 72 | `[18 111a45bb 10 e04c12d1 16 a2869b67 15]` **0.0199396273** |
| Random restarts on 17/11/15/14 (best of 32 random starts each) | 30 | 10 | `[17 6c32c163 11 2f92edc9 15 ea50d969 14]` 0.0202651 |
| ILS from that restart | 80 | 33 | `[17 a03ac133 11 ad92e95d 15 994cd969 14]` 0.0200770 |
| Shift grid 13..19 x 2 donors (4,802 specs), multiplier descent of the top 20 | 20 | 8 | `[15 b16b5153 13 3bcf8663 17 5506b66b 19]` 0.0205351 |

The runs added 143,690 exact scores; 1,237 distinct configurations
beat published `triple32` (0.0208886). The new record is 1.49% below
the old record and 4.54% below `triple32`. Its path went through
17/10/15/14 and 18/10/15/14 to 18/10/16/15; every improvement moved
both multipliers and at least one shift.

**Reading.**
- The old record was a local optimum only at radius two. ILS kicks of
  3-6 bits kept finding better basins; improvements arrived at
  iterations 140, 142, 20 and 128, so neither ILS run had converged.
- Good basins are not confined to the `triple32` lineage. A restart
  from random multipliers reached 0.02027 in six steps, and its ILS
  reached 0.02008, also below the old record.
- The shift grid only tuned transplanted multipliers; its ranking of
  schedules is weak evidence. Schedules with s1 = 13 (15/13/17/19,
  14/13/16/14, 17/13/13/16) reached ~0.0205 with four to seven steps.
- Not checked yet: BIC and forced two-bit parities (F41) for the round-two record (0.0199396).

**Round three** (`analysis/queue_triple32_descent3.sh`) continued ILS
from the three lineage leaders:

| Start | Iterations | Below `triple32` | Best end |
| --- | ---: | ---: | --- |
| Round-two record 0.0199396 | 150 | 113 | `[18 9d2ac69b 10 604c9af1 15 7b269b67 15]` **0.0195741015** |
| Restart lineage 0.0200770 | 80 | 34 | `[16 803ac233 11 ad96a959 15 c96cd969 14]` 0.0200480 |
| Schedule-grid leader 15/13/17/19, 0.0205351 | 60 | 35 | `[17 88bb3153 13 238e8679 17 4da6b66b 17]` 0.0196619 |

The descent runs (all three rounds) now hold 243,876 distinct exact scores, 3,227 of
them below `triple32`. The record is 1.83% below round two and 6.29%
below `triple32`. Its improvements came at iterations 44 (0.0199087) and
55. The schedule-grid lineage is the surprise: from a transplant it
dropped 4.25% in 60 iterations, through 15/13/17/17 to 17/13/17/17, and
is within 0.45% of the record on an unrelated schedule and multipliers.
So the shift schedule is far from settled, and 17/13/17/17 had the
steepest descent of any lineage.

The record run was accidentally executed twice in parallel with the same
seed. Both copies produced identical descents (all 150 iterations), but
their writes interleaved in `ils_record3/exact.csv`; 26 broken lines were
dropped and duplicates merged. The driver now takes a per-run lock.

**Round four** (`analysis/queue_triple32_descent4.sh`), on the faster
scoring path (`mixlab exact --stream` and the pair-symmetric GPU kernel,
0.052 s per exact score):

| Start | Iterations | Below `triple32` | Best end |
| --- | ---: | ---: | --- |
| Round-three record 0.0195741 | 150 | 107 | `[18 1daac69b 10 404c9af9 15 f122bb27 15]` 0.0194893 |
| 17/13/17/17 lineage 0.0196619 | 150 | 102 | `[17 88bb3153 13 238e8679 17 7da6b66b 17]` **0.0191341** |

The 17/13/17/17 lineage overtook the 18/10/15/14 one. Its new record is
2.25% below round three and 8.40% below `triple32`, and it came at
iteration 120. It differs from that run's start only in bits 28 and 29
of the third multiplier, so it is a two-step neighbour that the one-step
descents had not tried. Round five therefore scores both leaders'
two-step neighbourhoods exhaustively (`triple32_descent.py radius2`)
before more ILS. The descent runs now hold 340,045 distinct exact scores,
5,460 of them below `triple32`.

## 3. Threats to validity (please check)
1. **Single runs.** The racing-versus-old comparison (F8) and the family runs
   (F9) are one seed each. The search improvements are exact values, but the
   claim that racing is *better than* the old search is n = 1.
2. **Racing statistics.**
   - The historical runs used about 10 sequential looks per candidate at z = 3 with no peeking
     correction. Many false "worse" drops of truly better neighbours are
     possible across about 1,000 candidates per run.
   - The noise law assumes the N table entries are roughly independent;
     entries are in fact correlated, as F3 shows. (F13: empirically calibrated
     anyway, sd of standardized error 1.02–1.08, with zero false drops at the
     adjusted threshold over 48 truth-labelled races.)
   - Acceptance is first-improvement, not best-improvement.
3. **Neighbourhood bias.** Only 1–3-bit multiplier flips and ±1 shifts.
   "Local optimum" claims (the record, d303) are relative to this move set
   and to a random subset of about 1,000 moves. *(2026-11-21, F20: the
   record's full radius-2 neighbourhood under single-bit flips and ±1 shifts
   is now checked exhaustively; none is better. 2026-11-26, F44: the current
   best's radius-3 neighbourhood (52,601 configs) is checked; none is better.)*
4. **Novelty of F9**: searched all hash-prospector issues and the web
   (2026-11-20; see F9); forks and private code are not covered.
5. **Bias is not overall quality.** hash-prospector issue #28 notes the
   lowest-bias function is not always best on PractRand or RRC. No
   PractRand/RRC runs have been done; no C compiler is installed.
6. ~~**BIC is sampled at 2^16** with a fixed seed.~~ Resolved by F15:
   5 seeds at 2^20; the identical worst entries are structural.
7. **The F6 projection maps are ad hoc.** A better map (for example, one
   preserving the multiplier's spectral properties) is not excluded.

## 4. Questions for reviewers
- Is the noise law in F7 derived and applied correctly? Are the sequential
  z-tests in `racing.rs` sound enough, or should they use an
  always-valid / alpha-spending boundary?
- Is the novelty claim in F9 correct? Are there published 2-round 32-bit
  constants between 0.1070 and 0.1153 outside the 21f0aaad lineage?
- ~~Does the F3 derivation hold for all shift triples, including s1 ≥ s0?~~
  Answered by F37/F39: the general rule covers every shift list.
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
6. ~~Spectral-test features~~: done as F17 (spectral seeds do not help at
   32 bits). NAF-weight features remain untested.

**Crossover variations to try later (noted 2026-11-21, after F21).** The 7
extra gen-1 races and generation 2 are done (F22).
1. **Finer crossover.**
   - Swap 16-bit halves: one parent's core with another's high half.
   - Uniform bit crossover over the bits where the parents differ.
   - F16 says core swaps usually score worse before racing, but racing
     rescued gen 1's 8th-ranked start.
2. **Other shift templates.**
   - So far every cross used 16/15/15. Cross families on 15/15/15,
     16/14/15 and 17/15/15.
   - Also try crossing across templates: m0 and m1 come from different
     templates, and the shifts from either parent.
3. **Pattern mining** (TheIronBorn's term).
   - Find bit patterns shared by elite mixers, such as the `?35a2d97` m1
     top nibble and m0 low bytes …3335 / …32d3.
   - Seed children that keep a pattern and randomise the rest.
4. **An automated memetic loop.**
   - A population of about 20 that spans many core pairs.
   - Each generation: cross, screen at 2^26, exact top-k, race the best few
     with shorter races (patience 3), and replace the weakest members.
   - Run it as pausable chunks through `campaign.py`'s lock, PAUSE/STOP and
     resume machinery.
5. **Radius-3 check of the new record.** About 45,000 candidates,
   roughly 7–8 h of racing. It is a check rather than a likely improvement.
6. **Several race seeds per child.** The gen-1 winner came from one seed;
   repeat its race with 2–3 seeds to measure how much is luck.

**Status 2026-11-21 (afternoon):** F21: crossover generation 1 found
`[16 9c013335 15 335a2d97 15]` = 0.10207, a new best known 2-round 32-bit
xmx and a radius-2 optimum. Next: generation 2 with the new winners as
parents, more race seeds per child, and a PractRand / issue-#12-metric check
of the new constant before announcing it anywhere.

**Status 2026-11-21:** F20 is done. The record is a radius-2 optimum, and a
crossover child raced to 0.10882. The most promising open item is
*generational crossover*: cross the campaign's family winners (including
0.10882), screen, race the best few genuine crosses, and repeat. The
pausable campaign (F18/F19 follow-up) is at 30 of 94 jobs.

**Status 2026-11-20 (evening):** BIC-constrained search (F18) and the
other-core races (F19) are done. Open next: repeat F18/F19 over more seeds,
map the bias/BIC frontier at more constraint levels, and run a top-bit/negation
sweep (F14) from the new winners.

**Status 2026-11-20:** prior-art search (F9), refs rerun (F2), BIC at 2^20
(F15), core controls (F16) and the seed-search run (F17) are done. Items
2, 3, 5 and the PractRand check (4) remain.

## 6. State of the repository
- Each findings section is committed together with the work it describes;
  `git log` is the authoritative record. Commit subjects cite the F-numbers
  above.
