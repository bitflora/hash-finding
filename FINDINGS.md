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
- Not yet: a joint search over both top bytes, or bits 16-23; PractRand;
  hash-prospector `-e`.

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
   is now checked exhaustively; none is better.)*
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
