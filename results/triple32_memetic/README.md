# Three-round 32-bit memetic search

This campaign tunes the four right shifts and three odd multipliers in the
`triple32` construction. Its baseline is `[17 ed5ad4bb 11 ac4c1b51 15 31848bab 14]`
at exact bias **0.020888578919738908**. The [41 starting configurations](seeds.csv)
come from the [three-round section of Hash Prospector](https://github.com/skeeto/hash-prospector#three-round-functions).

## Result

`[17 ad5ad4bb 11 ac4c1b51 15 b9848bab 14]` scored **0.02024134502581794**,
a **3.10%** reduction in exact RMS avalanche bias. The CPU and GPU counters
agreed on its score over all `2^32` inputs.

| Configuration | Exact RMS bias | Largest absolute normalized bit-pair bias |
| --- | ---: | ---: |
| Published `triple32` | 0.020888578919738908 | 0.00007813237607479095 |
| Best from sampled selection | 0.02075258879550577 | 0.00007813237607479095 |
| Best after exact refinement | 0.02024134502581794 | 0.00007202662527561188 |

The campaign screened 2,037 distinct configurations and scored 1,705 new
configurations exhaustively, plus the baseline. **76 new configurations** beat
the baseline. The [final leaderboard](exact_refinement/leaderboard.csv) includes
both phases. The winner comes from `triple32` by changing the first multiplier
from `0xed5ad4bb` to `0xad5ad4bb` and the third from `0x31848bab` to
`0xb9848bab`.

## Method

1. The [sampled run](run_01) generated 192 crossover children per generation by
   splicing low multiplier bits between parents, sometimes inheriting a shift
   or adding a mutation. It then explored single-bit multiplier flips and
   one-step shift changes around both the current parents and promising
   crossover children. Screening used shared input samples within each stage
   and fresh seeds across stages. The top eligible finalists from each
   generation received exhaustive scores.
2. The [exact refinement](exact_refinement) started with the prior exact
   measurements and the 41 published seeds. It exhaustively scored the
   single-step neighborhoods of the three best configurations, plus candidates
   built by flipping bits 28–31 or exchanging upper 16-bit multiplier halves.
   Those high-bit moves preserve the low 16 bits of all three multipliers.

The harness computes Hash Prospector's RMS single-input-bit avalanche bias over
all 1,024 input/output bit pairs. Exact scoring enumerates all `2^32` inputs.
The sampled tables record signed, debiased mean-square estimates.

## Reproduction

From the repository root, with the CUDA Toolkit's NVRTC library on the path:

```sh
cd harness
cargo build --release --locked --target-dir target-triple32 --bin mixlab --example triple32_screen
cd ..
python analysis/triple32_memetic.py results/triple32_memetic/repro_sampled --generations 3
python analysis/triple32_memetic.py results/triple32_memetic/repro_exact --refine-from results/triple32_memetic/repro_sampled --generations 3
```

The commands use new run directories. Both Python invocations accept `--cpu`
for the CPU counter.

## Upper-bit follow-up race

The follow-up starts from the exact refinement leader. It crosses all 32
settings of the upper five bits of the first multiplier with all 32 settings
of the third. For the 16 best pairs, it then sweeps the upper five bits of the
middle multiplier. The final generation checks single-bit multiplier flips
and one-step shift moves around the eight best measured configurations.
Each stage reuses existing exact scores and exhaustively scores fresh specs.

The [follow-up leaderboard](cap_race_01/leaderboard.csv) adds 1,984 exact
scores, of which 188 beat the published `triple32`. The strongest new spec,
`[17 ad5ad4bb 11 ac4c1b51 15 89848bab 14]`, scored **0.020249484873719303**.
The campaign leader remains at **0.02024134502581794**.

```sh
python analysis/triple32_cap_race.py results/triple32_memetic/repro_caps --from-run results/triple32_memetic/repro_exact
```

## Two-step neighborhood

A step flips one of bits 1–31 of an odd multiplier or moves one of the four
shift counts by one. The check starts at
`[17 ad5ad4bb 11 ac4c1b51 15 b9848bab 14]` and covers all distinct
configurations reachable in one or two steps. It reuses prior exact scores
and scores each remaining configuration over all `2^32` inputs.

The [exact neighborhood](extended_radius_two/neighborhood.csv) contains
101 one-step and 5,054 two-step neighbors. The run added 4,808 exact scores
and reused 347. The starting configuration was the unique lowest-bias
configuration in the neighborhood. The closest challenger was two steps
away, `[17 ad5ad4bb 11 ac4c1b51 15 89848bab 14]`, at
**0.020249484873719303**.

## Generations seeded from the 76 winners

Each of four generations generates mutations and crossovers from all 76
original winners. Crossovers draw their second parent from the current
population. Each generation also mutates around the leading candidates,
rescreens the shortlist with fresh inputs, and uses exact scores to select
the next population.

The [seeded leaderboard](extended_seeded/leaderboard.csv) combines the 76
parents and the 128 new exact scores. The best new spec was
`[17 2d5ad4bb 11 ac4c1b51 15 a5848bab 14]` at **0.02087557933450556**,
slightly below published `triple32`.

## Other published shift schedules

The schedule race covers the 26 distinct alternatives in the original
41-seed list. It holds each schedule fixed while testing mutations and
crossovers around its published multipliers and around the prior record's
multipliers transplanted into that schedule. Each transplant and up to six
sample-selected finalists per schedule receive an exact score.

The second pass crosses the upper two bits of the first and third multipliers
around each schedule's best candidate and separately sweeps the same two bits
of its middle multiplier. The two passes added 594 exact scores. New
candidates improved on the published seed for 25 schedules. Including prior
exact results, all 26 schedules have an improvement over their published
baseline. The [schedule leaderboard](extended_shifts/schedules_polished.csv)
is led by `[16 64767aad 12 daa18229 16 09e53beb 16]` at
**0.021088757206867165**.

```sh
python analysis/triple32_extended.py radius_two
python analysis/triple32_extended.py seeded
python analysis/triple32_extended.py shifts
python analysis/triple32_extended.py shift_polish
```

The commands use the original campaign and upper-bit follow-up leaderboards
above as inputs.

## Correlated output flips

The [BIC screen](bit_independence/bic.csv) used `2^18` samples per mixer for
all 269 candidates below published `triple32`, all 26 alternate schedule
leaders, and `triple32` itself. Follow-up used five seeds of `2^20` samples
for the top eight winners, all alternate leaders, and the controls.

| Mixer | Worst absolute single-bit pair correlation across five seeds |
| --- | ---: |
| Three-round record | 0.0037–0.0041 |
| Published `triple32` | 0.0040–0.0046 |
| Best alternate schedule, `16/12/16/16` | 0.0044–0.0062 |
| Two-round record | 0.6020–0.6033 |
| TheIronBorn's two-round reference | 0.3335–0.3348 |
| `lowbias32` | 1.0000 |

For the three-round record, the worst single-bit pairs changed across
seeds and RMS z was 0.994–1.001. Eight alternate schedule leaders had
an exactly confirmed single-bit pair with absolute correlation above 0.01.
The strongest checked pair was **+0.03497750063882869** at input bit 31
and output bits 1 and 17 for
`[15 fc54c453 13 28213789 15 669f96eb 16]`.

Flipping record input bits **14 and 31 together** gives exact correlation
**+1** for output bits **4 and 18** and **−1** for bits **5 and 19**. This
is the same shift-forced relation as in published `triple32`. The best
alternate schedule also has exact pairs. Flipping bits **15 and 31
together** gives **+1** for outputs **0 and 16** and **−1** for **3 and 19**.
The [joint counts](bit_independence/exact_pairs.csv) cover all `2^31`
unordered input pairs for each checked XOR mask.

The [F41 prefix scan](bit_independence/two_bit_parities.csv) found fixed
output parities for at least one two-bit mask in 22 of the 27 schedules.
Nineteen schedules have a weight-two witness under that rule.

```sh
cd harness
cargo build --release --locked --target-dir target-triple32 --bin mixlab --example flip_pair
cd ..
python analysis/triple32_bic.py
python analysis/triple32_bic.py confirm-alternates
python analysis/triple32_bic.py exact-pairs
```

## Beam searches

The record beam used width 12 on `17/11/15/14`. The alternate search
used a separate width-4 beam for each of the 26 published alternatives.
Both ran three generations starting from the best exact scores available
for their schedules. Each generation kept the best half of each beam by
exact bias and reserved the other half for the strongest newly scored
offspring. Candidate generation used multiplier flips, high-bit exchanges,
and two-multiplier mutations. Direct exact scoring of some high-bit
mutations supplemented the two sampled screening stages.

The [record beam](beam_record/beam.csv) added 672 exact scores. Thirteen
beat published `triple32`. The strongest new candidate,
`[17 1d5ad4bb 11 ac4c1b51 15 2c848bab 14]`, scored
**0.02050919663017419**. The record remained at **0.02024134502581794**.

The [alternate beams](beam_alternate/leaders.csv) added 1,248 exact scores
and improved 17 of the 26 schedule leaders. The best was
`[16 a16f7153 12 3bcd8663 15 3d06b66b 16]` at
**0.02089775797941365**, a 0.91% reduction from the previous best
alternate. CPU and GPU exact scores agreed.

Among the schedules with no two-bit witness under the F41 prefix rule,
the best was `[15 a6cd1933 14 ebda1d59 16 7a37445d 16]` at
**0.02158100600695843**. Its worst sampled single-bit pair correlation was
0.0042 across five seeds. The best alternate's was 0.0087.
The alternate's recurring pair, input bit 8 to outputs 2 and 18, had
exact correlation **+0.006765909199493909**.
[Beam BIC measurements](beam_bic/bic.csv) · [Exact joint counts](beam_bic/exact_pairs.csv)

```sh
python analysis/triple32_beams.py record
python analysis/triple32_beams.py alternate
python analysis/triple32_beams.py bic
```
