# Mixing functions: findings for review (2026-11-19)

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
(debiased by −1/n) at 64 bits.

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

### F2. Reference table (solid; `results/refs.txt`)
- At 2^30 samples, only fmix64 (0.0183 ± 0.0024) and Moremur
  (0.0139 ± 0.0041) are measurably biased among the 64-bit references. Mix13,
  mx3, rrmxmx and NASAM are consistent with 0. **Sampled avalanche bias
  cannot rank good 64-bit mixers at affordable sample counts.**
- Pairwise output-flip correlation (BIC, 2^16 samples) shows a perfectly
  correlated pair (max |z| = 256 = √n) in fmix32, prospector32, lowbias32,
  both 16-bit references and Stafford Mix13. TheIronBorn's constants have
  max |z| = 86.6. triple32, mx3, NASAM and Moremur are clean (max |z| ≤ 9.2).

### F3. A structural parity weakness of every 2-round xmx mixer (solid)
Flipping input bit s1 always flips the XOR of output bits {0, s2, 2·s2, …},
whatever the constants. If s1 < s0, flipping input bit s0 + s1 does the same.

**Why:** multiply is a T-function; an odd multiply preserves bit 0; xorshift
is linear.

**Verified** at 100.00% over 20k random inputs for lowbias32, TheIronBorn
and random constants.

**Consequence:** no choice of constants removes it. Anyone who wants
PRNG-grade quality needs at least 3 rounds or rotations. This is a
qualitative argument for why 2-round bias records only matter for hashing.

### F4. A first-order analytic surrogate works only as a family screen (solid)
**Model.** A carry-chain differential model (`src/surrogate.rs`) with a
Markov/independence assumption between rounds, costing about 0.05 ms per
config.

**Results.**
- It is near-exact for one multiply: per-entry Pearson 0.997.
- For 2 rounds its absolute value collapses to about 10⁻¹¹, against an exact
  0.107.
- Rank correlation (Spearman ρ) against the exact bias:
  - all 116 published 32-bit configs: 0.44
  - best half: 0.68 (sampling at about 600× the cost: 0.35 / 0.26)
  - within one family: 0.25 and −0.29, i.e. useless

**Method error caught and fixed:** printing values with 10 fixed decimals
created ties, which were broken by row order, and that order was sorted by
the true answer. This inflated the within-family ρ. The analysis now uses
`%e` output and averaged ranks for ties.

### F5. 2-round bias can't be predicted by Markov-style models, even second-order ones (solid, negative)
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
  every differential/trail model assumes away.
- The hybrid also gives no variance reduction (per-sample variance 0.21
  against 0.25), so it is not a faster estimator either.

### F6. Small-width proxies don't predict full-width ranking (solid, negative)
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

### F7. A noise law for sampled bias, and which variance-reduction tricks fail (solid)
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

### F8. Sequential racing search: validated, and beats the old search in one comparison (solid / indicative)
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

**Why that would follow.** Multiplication is a T-function: output bits below
k depend only on the multiplier bits below k. So low output-bit behaviour is
fixed by the multipliers' low bits (and the shifts), and the high bits only
tune the rest.

**Untested:** whether fixing a known-good low-bit core and searching only the
high bits beats the record, and whether the core explains most of the gap
between families.

## 3. Threats to validity (please check)
1. **Single runs.** The racing-versus-old comparison (F8) and the family runs
   (F9) are one seed each. The search improvements are exact values, but the
   claim that racing is *better than* the old search is n = 1.
2. **Racing statistics.**
   - About 10 sequential looks per candidate at z = 3 with no peeking
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
1. **Test the low-bit-core hypothesis (F10).**
   - Fix the low 16 bits of both multipliers to the record's / 7870aaad's
     values and race over the high bits plus the shifts.
   - Re-run BIC with several seeds at 2^20.
   - If the core holds, it turns a 2^62 search into roughly 2^30 and is the
     most plausible route past 0.10704.
2. **Continue the 7870aaad run.** It was still improving at iteration 25.
   Then try larger moves: combined shift and multiplier moves, and
   4–6-bit flips.
3. **Harden the racing statistics.**
   - Add an alpha-spending or always-valid boundary.
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
