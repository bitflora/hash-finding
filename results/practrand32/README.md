# PractRand comparison of 32-bit mixers

PractRand 0.96, run November 26, 2026, with `stdin32` and the standard core
battery and foldings. Each mixer saw the same five input streams starting at
zero: the counter, the counter rotated left by 16 bits, the bit-reversed counter,
the complemented counter, and the Weyl counter `i * 0x9e3779b9` modulo `2^32`.
Each stream was evaluated separately. [Candidate specs and exact avalanche scores](candidates.csv).

## Two rounds

All nine two-round mixers had a `FAIL` by 1 MiB, the first checkpoint, on each
of the plain, rotated, reversed, and complemented counters. All nine reached
512 MiB without a `FAIL` on the Weyl counter.

The table counts `FAIL` rows at 512 MiB. The total is an unweighted count of
flagged rows across the five streams. Tests and folded variants are correlated,
so the count has no calibrated statistical meaning.

| Mixer | Exact avalanche bias | Plain | Rotated | Reversed | Complement | Weyl | Total |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Published `lowbias32` | 0.173534 | 8 | 3 | 19 | 8 | 0 | **38** |
| F30, current record | 0.098730 | 8 | 6 | 22 | 9 | 0 | **45** |
| F30 runner-up (`d49a3597`) | 0.099058 | 9 | 7 | 22 | 10 | 0 | 48 |
| F29 | 0.099084 | 9 | 6 | 23 | 11 | 0 | 49 |
| F26 | 0.099668 | 20 | 4 | 23 | 20 | 0 | 67 |
| F21 | 0.102068 | 16 | 8 | 32 | 15 | 0 | 71 |
| F45, alternate family | 0.100178 | 16 | 9 | 31 | 16 | 0 | 72 |
| F9 | 0.115274 | 16 | 9 | 36 | 16 | 0 | 77 |
| Published TheIronBorn record | 0.107043 | 17 | 12 | 37 | 14 | 0 | 80 |

F30 had the fewest failures among the project’s two-round candidates and 35
fewer than TheIronBorn’s record. `lowbias32` still had seven fewer than F30.
F26 scored better than F30 on the rotated counter and had the smaller
full-width `Gap-16` statistics on the reversed counter.

The 512 MiB reversed-counter statistics reported for TheIronBorn, F21, and F26
in [FINDINGS.md](../../FINDINGS.md) agree with the rerun for the rows below that
were recorded there. Compare `R` values within a row.

| Test | TheIronBorn | F21 | F26 | F30 | `lowbias32` |
|---|---:|---:|---:|---:|---:|
| Full-width Gap-16:A | +4955 | +1265 | **+152.4** | +199.9 | +205.8 |
| Full-width Gap-16:B | +3059 | +920.9 | **+98.1** | +150.1 | +118.1 |
| Full-width BRank(12) | +5307 | +3435 | +1743 | +1746 | **+1710** |
| Low1/32 Gap-16:B | +6207 | +2408 | +2137 | +2139 | **+695.6** |

Extending the Weyl streams gave every two-round mixer a full-width `FPF/16`
failure at 1 GiB. F29 and F9 also failed `Gap-16:B` at 1 GiB. By 2 GiB, every
two-round mixer failed `Gap-16:A` or `Gap-16:B`.

## Three rounds

The best exact result in the saved [descent search](../triple32_memetic/descent/ils_record2/exact.csv),
the earlier refinement winner, and published `triple32` all reached 512 MiB
without a `FAIL` on any of the five streams. All three first failed at 1 GiB
on every stream. Every three-round failure at 1 and 2 GiB was a negative,
full-width `FPF/16` result.

| Mixer | Exact avalanche bias | Streams without `FAIL` at 512 MiB | Total `FAIL` rows at 2 GiB |
|---|---:|---:|---:|
| Descent winner (`19924dbb`) | 0.01996190 | 5/5 | 14 |
| Refinement winner (`ad5ad4bb`) | 0.02024135 | 5/5 | 13 |
| Published `triple32` | 0.02088858 | 5/5 | 15 |

The descent winner reduced exact avalanche bias by 4.4% versus published
`triple32`. PractRand found the same first failure length and failure family
for all three, so this experiment gives no clear three-round winner. The
shared negative full-width FPF results are consistent with sampling without
replacement. A bijection on distinct 32-bit inputs cannot repeat a full
output word, whereas independent uniform 32-bit outputs can. PractRand’s
built-in `sfc32` with seed `1` had zero `FAIL` rows through 2 GiB with the same
battery and folding. [Control log](sfc32-control.txt).

## Results and reproduction

- [1–16 MiB](16m-early/scores.csv) for the four early-failing two-round patterns
- [16–512 MiB](512m/scores.csv) for all 60 mixer-pattern pairs
- [1–2 GiB](2g-three-round/scores.csv) for all three-round patterns
- [1–2 GiB](2g-weyl/scores.csv) for the two-round Weyl streams

Each scores directory also contains the original PractRand logs. The first
failure column gives the base-2 byte exponent of the first reported `FAIL`
in that run. An empty field means no `FAIL` was observed. Suspicious results
are retained in the logs and contribute neither to the `FAIL` count nor to
the count of results without anomalies.

Download [PractRand 0.96](https://sourceforge.net/projects/pracrand/files/PractRand_0.96.zip/download)
and extract it to `target/practrand/PractRand`. The archive used here has SHA-256
`e4caf7fda98b2c597bbda3b576753cf5a0f6047aab837c82be370ab798a672e1`.
From a Visual Studio 2022 x64 developer prompt in the repository root, build
and run the main matrix with the following commands.

```bat
msbuild target\practrand\PractRand\VisualC_net\PractRand.sln /t:RNG_test /p:Configuration=Release /p:Platform=x64 /p:PlatformToolset=v143 /m
cl /nologo /O2 /W4 /Fe:target\practrand\practrand_feed.exe /Fo:target\practrand\practrand_feed.obj analysis\practrand_feed.c
python analysis\run_practrand.py target\practrand\practrand_feed.exe target\practrand\PractRand\VisualC_net\x64\Release\RNG_test.exe results\practrand32\candidates.csv target\practrand\comparison --jobs 6
```

Use `--minimum 20 --cap 24` for early checkpoints or `--minimum 30 --cap 31`
for the longer runs, with repeated `--candidate` and `--pattern` flags to
select streams. The runner pipes bytes directly between the programs.
