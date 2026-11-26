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
