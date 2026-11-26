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
