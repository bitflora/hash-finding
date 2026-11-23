> **Review correction (2026-11-19):** This is a historical experiment log.
> Exact scores remain valid, but old baseline correlations, uncertainty bars,
> search timings and categorical conclusions are superseded by the corrections
> in `FINDINGS.md` and `results/REVIEW_STATUS.md`. The universal single-bit parity
> claim needs shift conditions; the entire mixer is not a T-function; tested
> surrogate failures do not rule out every Markov model.

# Plan: Searching for better integer mixer constants (`mixing-functions/`)

## Context
New line of research (the folder is currently empty). The topic is the small "finalizer" functions used in hash tables and PRNGs (for example the MurmurHash3 fmix, SplitMix64/Stafford Mix13, lowbias32 and triple32). They are built from a few xorshift and multiply steps, and their quality depends heavily on the constants chosen. Hobbyists have done almost all the work, and improvements keep coming (the most recent is TheIronBorn's 0.1076 to 0.10704 two-round 32-bit constants). The 64-bit space is especially under-explored because exact evaluation is impossible there. The goal is to (1) record the literature, (2) build a fast, reproducible evaluation harness, (3) look for better constants, and especially for *why* some constants are good, and (4) give a plain-language explainer.

## Key sources (to go in `mixing-functions/sources.md`)
- skeeto, *Prospecting for Hash Functions* (2018), https://nullprogram.com/blog/2018/07/31/ and the repo github.com/skeeto/hash-prospector. Defines the bias metric. Baselines: lowbias32 0.1735, triple32 0.0209 (about the random-permutation floor).
- hash-prospector issue #19 (TheIronBorn): best known two-round constants `[16 21f0aaad 15 d35a2d97 15]`, bias 0.1076, later 0.10704. Found by a metaheuristic, then an exact search around the best points.
- hash-prospector issue #28: the lowest-bias function is *not* always best on PractRand/RRC. Bias alone is not enough.
- David Stafford, *Better Bit Mixing* (2011), zimbry.blogspot.com/2011/09/better-bit-mixing-improving-on.html. Mix01–Mix14; Mix13 = SplitMix64's finalizer.
- Pelle Evensen, *Mostly Mangling* blog: rrmxmx (2018), Moremur (2019-12), NASAM (2020-01), and the RRC-64-xx-TF2-0.94 test methodology (counters fed in reversed, rotated and complemented forms, scored with PractRand).
- Jon Kågström (Maiga): mx3, *The construction of bit mixers*, *Tuning bit mixers* (jonkagstrom.com). Uses stack-machine program search for mixer structures.
- Steele & Vigna, *LXM* paper (vigna.di.unimi.it/ftp/papers/LXM.pdf). Compares the Stafford, Lea and degski mixers.
- Webster & Tavares (CRYPTO '85): the Strict Avalanche Criterion (SAC). Bit Independence Criterion (BIC).
- SMHasher / SMHasher3 (rurban, Frank Wojcik): broader hash test suites.

## Layman explainer (to be written to `mixing-functions/README.md`)
**What a mixer is for.** Computers often need to turn a number like 1, 2, 3… into something that *looks* random, for example to spread items evenly across a hash table or to produce random numbers from a counter. A mixer is a tiny recipe that scrambles a number's bits so that similar inputs give wildly different outputs.

**The recipe.** It alternates two cheap steps. *Xorshift* folds the top half of the number onto the bottom half, so information flows downward. *Multiply by a large odd constant* pushes information upward, because each bit affects every bit above it. Alternate them two or three times and every input bit ends up affecting every output bit. Both steps can be undone, so no two inputs ever collide. The only free choices are the shift amounts and the multipliers, and those "magic numbers" are what people search for.

**How quality is judged (avalanche).** Flip one input bit. In an ideal mixer each output bit then flips exactly half the time, like a fair coin, independently of the others. Measure how far each of the 32×32 (or 64×64) "flip input bit i → output bit j changes" probabilities strays from 50%, and combine those deviations into a single *bias* score. Lower is better. A truly random scrambler still scores about 0.02 at 32 bits, just from chance, so that is the floor.

**Why it's hard.** There are about 2³¹ choices for each 32-bit multiplier and about 2⁶³ for each 64-bit one, so the space can't be fully explored. At 32 bits you can test *all* 4 billion inputs exactly in seconds. At 64 bits you can only sample, so scores are noisy, and noise can make a mediocre candidate look like a winner. A good bias score also doesn't guarantee the output passes tougher randomness test suites (PractRand), so you need several yardsticks.

**Why it matters.** These functions run trillions of times a day inside Java, Zig, Rust crates, game engines and databases. A better constant costs nothing to adopt, because the speed is identical and only the quality improves.

## Research directions (most promising first)
1. **Small-width "model organisms."** Each *single configuration* can be evaluated exactly at 8–24 bits. The *whole* two-round space can only be covered at 8 bits (7³·128² ≈ 5.6M configs); at 16 bits it has 15³·2³⁰ ≈ 3.6·10¹² configs, so use large random samples plus exhaustive slices (for example, one multiplier fixed). *(Revised 2026-11-18: the 16-bit space was originally assumed to be enumerable.)* Find out which properties of good multipliers predict low bias: popcount, bit-run structure, spectral-test figure of merit, how the multiplier interacts with the shift amounts, and the sign of the multiplicative inverse. Then check whether those predictors carry over to 32 and 64 bits. This is the most *understudied* angle, since prior work has been black-box search.
2. **Beat the 32-bit two-round record (0.10704).** Use the predictors from (1) to choose where to start, then run a noise-aware local search (cheap sampled estimate, *racing* or successive halving to discard weak candidates, exact 2³² evaluation for finalists).
3. **64-bit two-round and three-round constants.** The space is barely mapped beyond Murmur, Stafford, Moremur and NASAM. Use sampled bias with confidence intervals and successive halving, plus a sparse-input (low-Hamming-weight and counter) avalanche test that catches the failures Evensen's RRC exposes.
4. **Multi-objective view.** Trace the Pareto frontier of bias versus higher-order avalanche (2-bit flips) versus BIC versus the PractRand failure length. Report it as a front, not a single winner, which addresses issue #28.
5. (Stretch) **Alternative cheap operations.** Examples: rotate-xor, `x ^ (x >> a) ^ (x >> b)`, multiply-high folding (mum) and 32-bit sub-lane multiplies. Also try GPU evaluation on the RTX 3050 via wgpu or CUDA.

## Deliverables / layout
- `README.md`: the layman explainer, the current best-known table and a summary of findings.
- `sources.md`: annotated bibliography (above).
- `plan.md`: this research plan and a log of experiments.
- `harness/`: Rust crate (cargo is installed; no C compiler is present). Uses `rayon` across 12 threads.
  - `mixers.rs`: generic parametrised `xmx…` mixers for widths 16/32/64, plus known reference mixers.
  - `bias.rs`: exact bias (full domain, 32-bit and below) and sampled bias with standard error. Uses the same formula as hash-prospector so numbers are comparable.
  - `extra.rs`: BIC, 2-bit-flip avalanche, sparse-input/counter avalanche.
  - `search.rs`: exhaustive (16-bit), random-restart hill climbing / simulated annealing with racing (32/64-bit).
  - CLI: `eval`, `search`, `sweep16`; results written as CSV/JSON to `results/`.
- `analysis/`: Python notebooks or scripts that read `results/` and fit the predictors for direction (1).
- PractRand (download the source and build it) is optional and needs the user's OK. A compiler (MSYS2/MinGW or WSL) would be needed. Deferred until stage 3.

## Execution steps
1. Create the folder, `README.md` (explainer), `sources.md` and `plan.md`.
2. Build the harness. **Validate first** by reproducing the published numbers: lowbias32 ≈ 0.1735, triple32 ≈ 0.0209, TheIronBorn ≈ 0.1076, and the Murmur3 fmix32 value from the README. If these don't match, nothing downstream can be trusted.
3. Run the 64-bit references (fmix64, Mix13, Moremur, rrmxmx, NASAM, mx3) under the sampled metric to establish baselines with error bars.
4. Run the exhaustive 16-bit sweep, then the predictor analysis (direction 1).
5. Run a guided 32-bit search (direction 2), followed by an exact re-evaluation of the top candidates.
6. Run the 64-bit search (direction 3) and the Pareto report (direction 4).
7. Write up findings in the README.

## Verification
- `cargo test`: unit tests check that every mixer is a bijection (16-bit exhaustive, invertibility via explicit inverse functions) and that the bias of known functions matches published values to 4 significant figures.
- The sampled-estimator test checks that the sampled bias of triple32 converges to the exact value as the sample count increases.
- Any "new record" claim needs exact evaluation (32-bit) or ≥2³⁰ samples with a non-overlapping confidence interval (64-bit). Every run must record its seed.

## Experiment log

### 2026-11-18: Harness built and validated
- `harness/` (Rust, `mixlab`). It reproduces the hash-prospector exact
  32-bit bias for lowbias32, both TheIronBorn functions, prospector32 and
  triple32 to 17 significant digits. It also reproduces the 16-bit values
  (hp16 omits the ×1000 factor).
- Exact 32-bit 2-round evaluation takes about 29 s (AVX2 path). The first
  scalar version took 187 s.
- Bug found and fixed: structured-input avalanche double-counted pairs when
  both x and x⊕e_j were in the input set, which inflated the baseline to
  about 1.5. Inputs are now canonicalised to unique pairs, and the baseline
  is about 1.0.

### 2026-11-18: Reference table (`results/refs.txt`, seed 1, 64-bit at 2³⁰ samples)
See the README table. Key points:
- Only fmix64 (0.0183 ± 0.0024) and Moremur (0.0139 ± 0.0041) are
  measurably biased at 2³⁰ samples. The rest are consistent with 0, so
  sampled avalanche bias can't rank good 64-bit mixers.
- BIC exposes perfectly correlated output-bit pairs (|z| = √n) in
  lowbias32, prospector32, fmix32, both 16-bit references and Stafford
  Mix13.

### 2026-11-18: Structural parity weakness of 2-round xmx (constant-independent)
- Derivation: multiply is a T-function; bit s1 of z = m·u depends on
  u_{s1} only linearly. So y_0 = z_0 ⊕ z_{s1} always flips when u_{s1}
  flips, and so does w_0 (an odd multiply preserves bit 0). The final
  xorshift then gives w_0 = XOR of output bits {0, s2, 2·s2, …}.
- Input bits that flip only u_{s1}: x_{s1} (when s1 < s0) and x_{s0+s1}
  (when it is < bits).
- Verified: 100.00% over 20k random inputs for lowbias32 (out 0,16), for
  TheIronBorn 0.10704 (out 0,15,30), and for random constants.
- Implication: 2-round constant searches can't fix this. Searches aimed at
  PRNG-grade quality should target ≥3 rounds or rotation-augmented forms.

### 2026-11-18: Analytic carry-chain surrogate (`harness/src/surrogate.rs`)
**Model.** For each input bit, propagate per-bit flip correlations
c = 1 − 2p:
- Xorshift: piling-up rule.
- Multiply: convert to an additive difference with random signs, then build
  D = m·a from signed shifted copies of m. Get the output XOR difference from
  the exact 2-state carry chain of (z + D) ⊕ z with z taken as uniform.
- Round 1 enumerates the sign patterns of D exactly. Later rounds assume
  independent bits (the Markov assumption).
- All arithmetic is in the correlation domain, to avoid cancellation near
  p = ½.
- Cost: about 0.05 ms per 32-bit configuration and 0.3 ms per 64-bit one.

**Results.**
1. **A single multiply is modelled almost exactly.** Per-entry Pearson
   correlation with the exact matrix is 0.997 for `[16 21f0aaad 15]`, and the
   model reproduces the deterministic entries. There is a regression test.
2. **Absolute bias collapses for 2-round mixers.** The model predicts about
   10⁻¹¹ against an exact 0.107. The independence assumption between rounds
   discards everything that causes the bias. **So the avalanche bias of good
   2-round mixers comes almost entirely from correlations between bits of the
   intermediate difference, and from the dependence between value and
   difference, not from how well single bits diffuse.**
3. **Ranking power (Spearman ρ against exact):**

   | set | n | surrogate | sampling 2²⁰ (≈600× the cost) | sampling 2¹¹ (≈equal cost) |
   |---|---|---|---|---|
   | published 2-round 32-bit, all | 116 | 0.44 | 0.35 | 0.06 |
   | published, best half | 58 | 0.68 | 0.26 | −0.40 |
   | record's single-bit neighbours (issue #19) | 37 | 0.25 | −0.07 | — |
   | 450c2d35 family | 14 | −0.29 | −0.30 | — |
   | README list (diverse) | 62 | 0.15 | 0.27 | — |
   | random 16-bit configs, all / best 10% | 5000 / 500 | 0.78 / 0.31 | 1.00 / 0.99 (2¹²) | — |

   The surrogate separates **families** (different shifts or multiplier
   lineages) much better than sampling at equal cost. It is **weak within a
   family**, and it gives identical scores to constants that differ only in
   the high bits of the last multiplier. At 16 bits, sampling is cheap and
   nearly exact, so the surrogate isn't needed there.
4. **Method error caught and fixed:** an early breakdown used CSV values
   printed with 10 fixed decimals. That created ties, which were broken by
   row order, and the list was sorted by the true answer. This inflated the
   within-family ρ to 0.6–1.0. The CSV now uses `%e`, and the analysis uses
   average ranks for ties. The table above is the corrected one.

**Verdict.** Useful as a cheap, noise-free *coarse screen*: pick shift
triples and multiplier families before spending exact or sampled compute. It
is not a replacement for fine ranking. To improve it, keep second-order
(pairwise) difference correlations between rounds, or treat the low bits of
the difference exactly (T-function window).

### 2026-11-18: Second-order surrogate: ablation of where 2-round bias lives
**Why ablate instead of building a pairwise model directly:** a
pairwise-correlation model isn't closed under xorshift. One bit of
Δ ⊕ (Δ >> s) already needs 4-point correlations. So
`surrogate::hybrid_matrix` keeps the difference *exact* up to the last
multiply, taken from sampled input pairs, and models only the rest:
- **A**: exact XOR difference Δy before the last multiply, then the
  independent-bit model.
- **B1**: exact additive difference D = m·(y′ − y), then the exact carry chain
  with w uniform and independent of D. The final xorshift uses the piling-up
  rule.
- **B**: like B1, with the final xorshift parity taken jointly from the carry
  chain. (B and B1 differ by < 10⁻⁵: carries decorrelate within a few bits.)

**Sanity check:** on 1-round `[16 21f0aaad 15]`, A and B both match the exact
table (per-entry Pearson 0.997).

**Results:** exact 2-round bias recovered, as a fraction of the exact
mean-square, with scores in brackets. A used 2¹⁸ samples per row and B1 used
2²⁴, both debiased.

| variant | record [16 21f0aaad 15 735a2d97 15] = 0.10704 | lowbias32 = 0.17353 |
|---|---|---|
| Markov surrogate | ≈0% (≈10⁻¹¹) | ≈0% |
| A: exact Δy | 0.8% (0.0097) | 2.2% (0.0258) |
| B1: exact D, w ⟂ D | 21% (0.049 ± ~0.006) | 54% (0.128) |

**Conclusions:**
1. Keeping all correlations *across the round boundary* (A) recovers almost
   none of the bias. This tested cross-round approximation failed on the two examined mixers. The bias
   depends on the joint bit structure of the signed sum D (a sum of about n/2
   signed shifted copies of m), which takes ~2^{n/2} sign patterns to
   represent exactly.
2. Even with D exact, assuming the value w is independent of D (B1) loses
   46–79% of the bias. The rest comes from dependence between the actual
   values and the differences, which every Markov-style analysis assumes away.
3. As an estimator, B gives almost no variance reduction: the per-sample
   variance is about 0.21, against 0.25 for plain sampling. So it is no
   shortcut for 64-bit evaluation either.
4. **Overall:** the avalanche bias of good 2-round xmx mixers is a "global"
   property. The tested Markov approximations did not predict it quantitatively;
   broader impossibility claims are not supported by these experiments. For quantitative work at 64 bits, sampling
   (with noise control) or exact evaluation at reduced width are the remaining
   options. The first-order surrogate is still useful as a cheap family-level
   screen.

### 2026-11-19: Width-scaling study (`harness/src/scale.rs`, `mixlab scale`)
**Question.** Can exact evaluation of a *shrunken* copy of a mixer (cheap)
rank candidates for the full-width mixer (expensive, or impossible at 64
bits)?

**Projection** from width b to w:
- Shifts: `prop` s·w/b rounded, or `offset` w/2 + (s − b/2), which keeps
  15/16/17 distinct.
- Multipliers: `low` m mod 2^w, or `high` the leading w bits. Both forced odd.
- All four combinations were tested.

**Baseline:** sampling the real full-width mixer at **equal cost**, meaning the
same number of mixer calls. Exact at width w costs (w+1)·2^w calls; sampling
costs b+1 calls per input.

**Results (Spearman ρ against the exact full-width bias).** Data are in
`results/scale_published32.csv` and `results/scale_random24.csv`.

| set | best projection (w = target − 4) | best projection, other widths | equal-cost sampling |
|---|---|---|---|
| 116 published 32-bit, all | 0.57 (w24 offset:low) | −0.56 … +0.41 at w12–20, sign flips between widths | **0.88** (2^23.6) |
| published, best half | −0.17 (w24) | −0.65 … +0.52 | **0.58** |
| 1200 random 24-bit (shifts 9–15), all | 0.85 (w20 prop:low) | 0.60 (w12), 0.74 (w16) | **1.00** (from w16 cost) |
| random 24-bit, best 10% | 0.38 (w20 offset:low) | 0.30 (w12), 0.32 (w16) | 0.25 / **0.94** / **1.00** (w12/16/20 cost) |

**Conclusions:**
1. **Projection doesn't work as a predictor.** Even a 4-bit gap keeps only
   ρ ≈ 0.35–0.38 among good configurations. For published 32-bit
   constants, the ranking is unstable from one width to the next (ρ goes from
   −0.38 at w20 to +0.57 at w24 on the same set). That instability is the
   signature of an unrelated function, not a coarse version of the same one.
   Mapping the low bits of the multiplier beats mapping its leading bits.
2. **Noise floor isn't the reason.** The projected mixers score 20–150× above
   the 1/2^w random floor. They have strong structural defects of their own,
   which don't track the full-width mixer's defects. Avalanche bias depends on
   the exact constant at the exact width.
3. **Sampling the real mixer wins at equal cost, and also at equal wall time.**
   At 24 bits, 2^16-equivalent sampling (0.9 ms) already beats the
   20-bit projection (4.5 ms).
4. **For 64 bits:** use noisy-but-unbiased sampling with successive halving
   and common random numbers (the existing `search`). Don't use small-width
   proxies. The within-family 32-bit run (record neighbours, about 33 min) was
   skipped: projections of single-bit multiplier changes are often identical
   under `low`/`high`, so they can only tie or do worse.

### 2026-11-19: Noise control for sampled comparisons (`racing.rs`, `mixlab paired` / `design`)
**Question.** If we have to sample, how cheaply can "is candidate B better
than A?" be settled?

**Noise law (verified).** With N = bits² matrix entries and n i.i.d. inputs,
the debiased mean-square estimate has

  Var(mŝ) ≈ (4·ms/n + 2/n²) / N.

For good mixers the 2/n² term (the fluctuation of the noise floor itself)
dominates up to n ≈ 1/(2·ms): about 2^25.6 at 32 bits for the record and
about 2^31 at 64 bits for Moremur. So in practice **sd(mŝ) ≈ √(2/N)/n**. The
noise falls as 1/n, not 1/√n. The measured sd matches this formula within
replicate error at 32 bits (2^18, 2^20, 2^22) and at 64 bits (2^24).

**Variance-reduction tricks tried:**

| trick | variance saving |
|---|---|
| common random numbers (same inputs for A and B), 24 neighbours of the 32-bit record | ×1.09 (geometric mean; range 0.5–2.0) |
| the same, 16 neighbours of Moremur (64-bit, 2^24, 8 reps) | ×1.20 (range 0.4–3.6, noisy) |
| stratified low bits (blocks over all 2^10 low-bit values) | **worse**: biased mean, sd ×1.0–1.5 |
| random affine subspaces of dimension 10 (Gray-code walk) | ×1.0–1.4 (≈ none) |

The flip events of two different mixers on the same input are effectively
independent, even for single-bit neighbours, and so are the flip events within
a structured input block. **No black-box input design beats i.i.d. sampling
here.** The noise-floor fluctuation is a hard limit.

**Sample counts needed.** Two mixers differing by Δms need about
n ≈ z·2/(bits·|Δms|) inputs each to decide at z σ. For the record's 24
neighbours, with exact truths computed (all 24 are worse, confirming a local
optimum), at z = 3:
- Required n ranges from 2^17.2 (s2−1) to 2^29.0 (s0−1, only 1.1% worse in ms).
  3, 15 and 23 of the 24 are decided within 2^20, 2^24 and 2^28 inputs
  respectively.
- **Sequential testing** (stop each comparison as soon as it is decided):
  2^30.2 inputs in total, against 2^33.6 for a fixed budget that resolves the
  hardest case. That is a **×10.9 saving**, and about 85× fewer inputs than
  exact evaluation of all 24 (24·2^32).
- Moremur (ms ≈ 1.9·10⁻¹⁰) needs about 2^29 inputs just to show its *own*
  bias is nonzero at 3σ. Neighbour gaps of 10⁻¹⁰ to 10⁻¹¹ need 2^30 to 2^33 per
  comparison: about 1.5–13 min each on this machine.

**Consequences for the search:**
1. The current successive halving (2^16 → 2^22, keep the best quarter) works
   inside the noise for most good neighbours. At 2^22 only about half of the
   record's neighbours are distinguishable from it. So it can drop the true
   best candidate on noise. It should be replaced by **sequential racing
   against the incumbent**: a z-test after each doubling of n, drop when worse
   at z σ, promote when better, with a cap.
2. At 64 bits, 2-round avalanche bias sits close to the measurement floor.
   A local search is feasible but slow, about hours per accepted step. The
   2-round 64-bit objective is nearly saturated, so for 64 bits the extra
   metrics (structured inputs, BIC) may discriminate more than bias does.

Cost model: `analysis/racing_cost.py`. Data: `results/paired32_record_n18.*`
and `results/paired64_moremur_n24.*`.

### 2026-11-19: Sequential racing search (`racing::race_search`, `mixlab race`)
**Design.** Each iteration races K = 96 fresh neighbours against the
incumbent:
- Undecided candidates have their sample count doubled each round,
  starting at 2^14.
- A z = 3 test against the noise law decides each one: drop it if worse,
  verify it exactly if better.
- Candidates still undecided at the 2^28 cap are verified exactly, best
  estimate first.
- The first exactly verified improvement is accepted.

**Validation:**
- The record's 24 neighbours, all known worse: all dropped correctly in
  2^29.7 inputs and 54 s, against about 12 min exact.
- Starting from 0.1076 and racing the 57 issue #19 constants: it found a
  verified better one (`f35a2d97`, 0.10734) in 187 s, against about 28 min
  exact.

**Runs** (12 iterations each, z = 3, cap 2^28; all reported scores are exact):

| start | result | time | notes |
|---|---|---|---|
| record [16 21f0aaad 15 735a2d97 15] 0.10704 | no improvement | 20 min | 1,152 random 1–3-step neighbours raced; 1,132 dropped by sampling (median at 2^23), 20 exact checks. No improvement found in this sampled neighborhood. |
| lowbias32 [16 7feb352d 15 846ca68b 16] 0.17353 | **[16 93f3352d 15 042cb68b 16] 0.14182** | 31 min | 10 of 12 iterations improved |
| arbitrary [16 2b9e1a3f 15 9d2c5689 16] 1.7233 | [16 3d9e1b2d 14 8d0c9299 16] 0.15338 | 16.5 min | every iteration improved |
| *old fixed-halving `search` from lowbias32* (20 iterations) | [16 7f8f352d 15 dc6ca64b 16] 0.14919 | 27.5 min | stalled from iteration 7 to 15 at 0.1495 |

**Comparison at about 27 min from lowbias32:** racing reached 0.1423 and the
old search 0.1492. The old search's long stall fits the predicted failure:
it drops close competitors on noise at 2^16–2^22. This is one run each, so
the comparison is indicative, not conclusive.

**Assessment.** Racing is now the default way to search: it decides most
neighbours at 2^20–2^24 and spends exact evaluations only on genuinely close
calls. Neither run gets near the record. *(Corrected 2026-11-19.)* 0.14182 beats every
entry in the hash-prospector README list, whose best is
[15 d168aaad 15 af723597 15] at 0.15984. It does not beat the issue #19
families: the record family at 0.10704, 450c2d35/caf649a9 at 0.12243 and
d30332d3/c865c965 at 0.12786. Beating
0.10704 would need either other families or larger neighbourhoods, for
example simultaneous shift and multiplier moves.

### 2026-11-19: Long racing runs from distinct families (`analysis/run_family_races.sh`)
Setup: 25 iterations, 96 neighbours per iteration, stop after 6 iterations
without improvement, z = 3, cap 2^28. The script is resumable. Total wall time
about 2.3 h. All scores are exact; the two headline results were re-checked
with `mixlab eval --exact`.

| family (start) | start bias | result | iterations / time |
|---|---|---|---|
| README best [15 d168aaad 15 af723597 15] | 0.15984 | **[16 7870aaad 15 be523597 15] 0.11527** | 25 / 73 min, **still improving at the iteration cap** |
| #19 [16 450c2d35 15 caf649a9 16] | 0.12243 | [16 450c2d35 15 8af649a9 16] 0.12122 | 8 / 14 min (stopped by patience) |
| #19 [16 d30332d3 15 c865c965 15] | 0.12786 | no improvement | 7 / 13 min (stopped by patience) |
| README [17 9e485565 16 ef1d6b47 16] | 0.16143 | [17 19085565 16 af1d6b47 16] 0.14014 | 12 / 20 min (stopped by patience) |
| our lowbias32 descendant | 0.14182 | [16 81e3352d 15 042cb68b 16] 0.13873 | 10 / 18 min (stopped by patience) |

**Findings:**
1. **[16 7870aaad 15 be523597 15] = 0.11527 is the best 2-round 32-bit mixer
   we know of outside the record's own `21f0aaad/735a2d97` lineage.** That
   lineage holds 0.10704–0.1133. This mixer beats the best other published
   families: 450c at 0.1224 and d303 at 0.1279.
   - Extra metrics: structured avalanche counter 0.90, RRC 1.05, low-weight
     1.05 (all ideal).
   - BIC max |z| = 86.64, at input 17 → outputs (2, 17), corr +0.3384.
     This is *identical* to the record's and to 0.1076's worst entry (same
     sampling seed), which supports the low-bit-core idea below.
     *(Corrected: an earlier note said the record has 256; that value belongs
     to lowbias32 and others.)*
2. **The search converged on familiar low bits.** It started from d168aaad,
   which already ends in `aaad` like the record's 21f0aaad, and the second
   multiplier drifted to end in `…3597`, close to the record's `…2d97`.
   Multiplication is a T-function, but right shifts make this inference
   invalid for the full mixer; fixed low multiplier bits do not fix low outputs. This suggests a shared "good low-bit core" that the high
   bits then tune. Worth testing directly: fix the low 16 bits and search the
   high bits.
3. The 450c and d303 families are tight local optima under 1–3-bit moves, like
   the record. The search makes big gains only from starting points that
   weren't already locally optimised: README entries, which came from
   hash-prospector's own noisy search.

### Next candidates (not started)
- Continue the d168 → 7870aaad run (it was still improving), then a
  low-bit-core experiment: fix the low 16 bits of both multipliers to
  aaad/…97 patterns and race over the high bits.
- 64-bit racing search (verification by a 2^32 fresh sample; hours per run).
- Spectral-test and NAF-weight features versus exact bias over a large
  random 16-bit sweep.
- Automatic structural screen (T-function window analysis) for any
  candidate design.

### 2026-11-19: Continuation of the 7870aaad run (corrected racing code)
- Start: [16 7870aaad 15 be523597 15], 0.11527.
- Settings: 96 neighbours per iteration, nominal z = 3, adjusted for multiple
  looks to about 4.9; cap 2^28; patience 6; seed 124.
- **Result: no improvement.** Over 6 iterations (22 min), 576 random
  1–3-step neighbours were raced. 553 were dropped by sampling, with the
  median drop at 2^24 (one doubling later than the uncorrected runs' 2^23,
  as expected from the stricter threshold). 23 needed exact checks, and all
  were worse. Log: `results/family_races/famd168_cont.log`.
- **Interpretation (exploratory):** the earlier run's last gains were large
  (0.1236 → 0.1187 → 0.1153 in iterations 22–25), but no further 1–3-bit move
  in this sample improves on it. This is *not* proof of a local optimum:
  only a random subset of the neighbourhood was tried, and the
  false-elimination rate is still unmeasured. Larger moves (4–6-bit flips,
  combined shift and multiplier moves) and the racing validation are the
  next tests.

### 2026-11-20: Racing validation against exact truth (`mixlab trace`, `analysis/race_validation.py`)
- New `trace` command. It records every candidate's estimate after each
  doubling and stops a candidate only at |gap| > 6 sd or at the cap. Policies
  can then be scored offline.
  - `--resume B:R` gives byte-identical output to an uninterrupted run
    (tested).
  - `--max-s2` filters generated neighbours.
  - A `.cands.csv` sidecar records each candidate's spec.
- **24 bits** (`trace24_{a,b}`): 32 races × 96 neighbours, looks from 2^10 to
  2^24.
  - The exact climbs stalled after 7–8 steps at bias about 1.15, giving 2
    bases each. The gaps were large, so this is the easy case.
- **32 bits, issue #19** (`trace32_issue19`): incumbents ranked 25%, 50% and
  75% (0.128, 0.135, 0.147) against the other 56, with 4 replicates each.
  - The run was interrupted and resumed with `--resume 0:4`.
  - Two copies of the script ran at once by accident: an orphaned queue
    script. Their output was byte-identical, which I checked: no duplicate
    keys, and 56 candidates in each of the 12 blocks.
- **32 bits, de0f5553** (`trace32_negrec`): 64 fresh neighbours with
  s2 ≤ 15, exact truth (31 min), 4 replicates. Only one neighbour is truly
  better: 5e0f5553, at 0.10936.
- **Results** (see FINDINGS F13):
  - The noise model is calibrated: sd 1.02–1.08.
  - There were zero false drops at the adjusted threshold in every set, and
    one at plain z = 3 (24-bit).
  - The tightest true gaps (0.2–0.9 sd at 2^28) always reach the cap and get
    an exact check.
  - Promoting at z ≤ 2 wastes exact checks and accepts smaller gains.
  - Plain z = 3 saves 1.4–2.8× inputs, but captures about 0.7 of the best
    gain against about 0.9.
  - Decision: keep the current threshold.
- Coordination: these runs were scheduled around the other session's
  negation races, so they did not share the CPU with them. The `target-trace/`
  build directory at the repo root was only needed while that race held
  target/release/mixlab.exe.

### 2026-11-20: Prior art, refs rerun, BIC, core controls, seed search
- Prior art: all 32 hash-prospector issues and the web; no pure-xmx 2-round
  32-bit constant below 0.1153 outside the 21f0aaad lineage (F9). A CRC32C
  2-step function reaches 0.0511, but uses a different operation set.
- `refs_corrected.txt`: the corrected estimator confirms that only fmix64
  and Moremur are measurably biased at 2^30 (F2).
- New `mixlab bic` subcommand; 13 mixers × 5 seeds at 2^20. The (2,17)
  worst pair at ≈ 1/3 is identical across aaad-core mixers at every seed (F15).
- Core controls: 7 cores × 16 shared random high halves, all exact. Every
  good mixer's core is good, with medians of 0.27–0.36 against 3.16 for
  random mixers (F16).
- Seed search: the spectral pool is no better than length-matched random
  multipliers; recombined published constants are best (F17).

### 2026-11-20 (afternoon): BIC as a second objective; races from other cores
- Exact check: the record's (2,17) pair flips together with P = 0.33337
  (corr +0.33337), close to but not exactly 1/3. Within the aaad core the
  worst pair is set by s2 alone: 0.034 / 0.102 / 0.127 / 0.334 at s2 =
  12 / 13 / 14 / 15 (F15).
- New `race --max-bic-corr C` (2^16 BIC filter on neighbours).
  C = 0.05 gives `[16 d610aaad 15 4d90b597 12]` 0.20300, rms z 1.19;
  C = 0.15 with s2 ≤ 14 gives `[16 6870aaad 15 f452f593 14]` 0.13168,
  worst pair 0.08 (F18).
- Core races (best F16 draw per core, s2 moved to 15): d303 0.11992,
  lowbias32 0.12561, 450c 0.12797, 19085565 0.13568. These are new family
  bests for d303, lowbias32 and 19085565 (F19).
- Script: `analysis/queue_bic_cores.sh`; logs `results/bic_frontier/`,
  `results/core_races/`, `results/queue_bic_cores.log`.

### 2026-11-21: TheIronBorn's method (F20)
- Read TheIronBorn's account in issue #19: a memetic algorithm, pattern
  mining, and an exhaustive exact search within 4 steps (details in
  `sources.md`).
- Radius-2 check: all 2,343 neighbours of the record were raced in 22 min,
  and none is better. New second-best mixer: `[16 21f0aaad 15 535a2d97 15]`
  at 0.10731.
- Crossover: 791 children of 31 parents, screened at 2^26; the top 15 were
  scored exactly. One genuine cross, `130332d3 × 919a4597`, scored 0.11558
  and raced to **`[16 d40332d3 15 919a4597 15]` 0.10882**. That is the best
  non-aaad mixer, though its BIC worst correlation is 0.60.
- Also fixed the campaign's 0.25 level: it now starts at s2 = 14, and the
  old no-op results are in `results/campaign/void_c25_s2start15/`.
- **Crossover generation 1 (F21):** 687 genuine children (new core pairs)
  from 40 parents; 20 scored exactly; the 3 best distinct ones raced. The
  lowbias32-core × record-lineage cross raced to **`[16 9c013335 15
  335a2d97 15]` = 0.10207 exact**, 4.6% below the 2022 record.
  - Confirmed by the generic scalar counter (`examples/exact_generic.rs`),
    which also reproduces the old record to 17 digits.
  - Radius-2 optimum: 2,343 neighbours, none better.
  - BIC worst correlation 0.40.

### 2026-11-21/22: Gen-1 extra races and crossover generation 2 (F22)
- All 10 gen-1 races: 7 of 10 finish below 0.115. Best besides F21:
  `[16 6f0f32d3 15 2d66b9a9 15]` 0.10799.
- Gen 2: 51 parents, 1,015 genuine children, 5 races. The best are
  `[16 ec013335 15 cca5d269 15]` 0.10344 and `[16 02fccd35 15 b15a3597 15]`
  0.10447; the latter is the best mixer without the 2d97 m1 core.
- No new record: 0.10207 stands.

## 2026-11-22
- Crossover gen 3 (whole/half/uniform, 3 seeds per finalist): paused at 11/18 races. Half-swap race found `[16 b20332d3 15 d21a3597 15]` = 0.10091 (both exact counters), best bias so far but BIC 0.60 vs F21 0.40 (FINDINGS F23). Resume with `analysis/queue_crossover_gen3.sh`.
- 2026-11-23: gen 3 finished (18/18; half 0.10091, whole 0.10464, uniform 0.11060). 39 hand-picked core pairings (FINDINGS F24): best race 0.10477, no record.
- 2026-11-23: beam search past radius 2 around F23 (FINDINGS F25): `[16 b20332d3 15 549a3597 15]` = 0.10076, verified on both exact counters; best issue-#12 too. BIC still 0.60.
