# Best Hashes

## Current best two-round 32-bit hash
`[16 4b0332d3 15 549a3597 15]`

`x ^= x >> 16; x *= 0x4b0332d3; x ^= x >> 15; x *= 0x549a3597; x ^= x >> 15`.

Avalanche bias: **0.09873011186879989**


## Current best three-round 32-bit hash
`[17 88bb3153 13 238e8679 17 7da6b66b 17]`

Avalanche bias: **0.019134091125396723**

(FINDINGS F50; [campaign results](results/triple32_memetic/README.md))


## Current best 16-bit hash

TheIronBorn's record for 8/7/9 is undefeated and will forever be undefeated. This project searched that space exhaustively.

`[8 a3d3 7 4b2d 9]`

`x ^= x>>8; x *= a3d3; x ^= x>>7; x *= 4b2d; x ^= x>>9`

Avalance bias: **7.25294**

If there is a better mixer for 16 bits, it is outside of these triples, which were also searched exhaustively:
- 8/7/9
- 8/7/7
- 6/7/9
- 7/7/8
- 9/7/7
- 9/6/8
- 7/7/7
- 7/7/9
- 9/7/9
- 8/7/8
- 9/7/8
- 8/6/8
- 7/6/8


# Intro

This is a scratchpad for finding better integer hash functions, currently
targeting lowbias32(). At the time of this writing, well over 100 new functions
improving on the previous state of the art have been found.

See https://github.com/skeeto/hash-prospector/issues/19#issuecomment-7090221712
for discussion on the progress.


# Mixing functions for hashing and PRNGs

Searching for better constants in integer "finalizers" — the multiply-xorshift
mixers used by hash tables and random number generators — judged primarily by
avalanche bias, with additional statistical yardsticks.

- `FINDINGS.md` — numbered findings (F1, F2, …) with evidence; the main writeup
- `sources.md` — annotated bibliography
- `plan.md` — research plan and experiment log
- `harness/` — Rust evaluation and search tool (`mixlab`); see `harness/README.md`
- `results/`, `analysis/` — raw results and analysis scripts
  (`results/REVIEW_STATUS.md` tracks the status of saved experiments)
- [PractRand comparison](results/practrand32/README.md) — structured-counter tests
  of the leading 32-bit mixers and published references
- `viz/` — `explorer.html`, an interactive browser for the explored 32-bit
  mixers (`explored32.json`)

## Three-round 32-bit research

The [triple32 memetic search](results/triple32_memetic/README.md) has its own
seed list, exact leaderboard, and reproduction commands.

## Building and running

Needs a Rust toolchain. The release build uses `target-cpu=native`
(`harness/.cargo/config.toml`), so build on the machine you run it on.

```sh
cd harness
cargo build --release
cargo test --release
# exact 32-bit avalanche bias of a two-round mixer
target/release/mixlab eval --bits 32 --spec "[16 b20332d3 15 549a3597 15]" --exact
```

The shell scripts in `analysis/` run under `bash` and change to the directory
they need, so they can be started from anywhere. The Python scripts need
Python 3. `viz/explorer.html` loads its data with `fetch`, so serve `viz/`
over HTTP (for example `python -m http.server`) rather than opening the file
directly.

## Plain-language primer

### What a mixer is for
Computers constantly need to turn a predictable number (1, 2, 3, … or a
memory address, or a user ID) into something that *looks* random — to spread
items evenly across a hash table, or to make random numbers from a simple
counter. A **mixer** is a tiny recipe that scrambles a number's bits so that
similar inputs give wildly different outputs.

### The recipe
A typical mixer alternates two cheap steps:

- **Xorshift** — `x ^= x >> s`: copy the number shifted right by `s` places
  onto itself. This moves information from high bits *down* into low bits.
- **Multiply by a large odd constant** — `x *= C`: in a multiplication, each
  bit influences every bit *above* it, so information flows *up*.

Do "shift, multiply, shift, multiply, shift" and every input bit ends up
influencing every output bit. Both steps are reversible (odd multipliers have
inverses; xorshift can be undone), so the whole mixer is a **permutation** —
no two inputs ever collide. The only free choices are the shift amounts and the
multipliers. Those "magic numbers" are what this research hunts for. Example
(lowbias32, from hash-prospector):

```c
x ^= x >> 16; x *= 0x7feb352d;
x ^= x >> 15; x *= 0x846ca68b;
x ^= x >> 16;
```

### How quality is judged: avalanche
Flip one bit of the input. In an ideal mixer, every output bit then flips
exactly half the time, like a fair coin. (This is the *Strict Avalanche
Criterion*, Webster & Tavares 1985.) Build a 32×32 table: entry (i, j) is how
often flipping input bit *i* flips output bit *j*. Measure how far each entry
strays from 50%, combine those into one **bias** score — lower is better.

Even a perfectly random scrambler scores about **0.02** on 32 bits purely from
chance, so that is the floor. Measured results: MurmurHash3's finalizer ≈ 0.264,
lowbias32 ≈ 0.174, the best known 2-round ≈ 0.107, and the 3-round `triple32` ≈
0.021, which is essentially at the floor.

### Why it's hard
- **Huge search space.** About 2³¹ choices per 32-bit multiplier, 2⁶³ per
  64-bit one, times the shift choices. You can't try everything.
- **Noise at 64 bits.** At 32 bits you can test *all* 4 billion inputs exactly
  in seconds. At 64 bits you can only sample, so scores are noisy, and noise can
  make a mediocre candidate look like a winner unless you're statistically
  careful.
- **One number isn't enough.** A good bias score doesn't guarantee the output
  passes tough randomness suites (PractRand), especially on structured inputs
  like counters. Multiple yardsticks are needed.

### Why it matters
These functions run trillions of times a day inside Java (SplitMix), Zig,
Rust crates, game engines and databases. A better constant costs nothing to
adopt — same speed, better quality — which is why improvements spread fast.

### What's new about this project's angle
Prior work is almost entirely black-box search. This project uses two other
approaches:
1. Study *small* mixers, where any constant can be scored exactly, to learn
   **what makes a multiplier good**. Then use that to guide searches at 32 and
   64 bits, where the 64-bit space in particular is barely mapped.
2. Borrow tools from cryptanalysis (bit-dependency structure, carry-chain
   differentials, linear correlations) to *predict* quality instead of only
   measuring it.

## Structural parity relation in 2-round mixers

Let the shifts be `s0, s1, s2`. The XOR of output bits
`{0, s2, 2*s2, ...}` recovers bit 0 immediately before the last xorshift.
For any odd multipliers, this parity always flips under:

- input bit `s1`, **provided `s1 < s0`**;
- input bit `s0+s1`, whenever that bit is inside the word;
- more generally, the input difference obtained by applying the inverse of
  the first xorshift to `1 << s1`. This may involve multiple input bits.

An odd multiply is triangular in the input bits and preserves bit 0.
Flipping only intermediate bit `s1` flips bit `s1` of the first product,
without changing lower bits. The middle xorshift therefore flips bit 0,
which the second multiply preserves.

The earlier unconditional single-bit claim was false: for the 8-bit mixer
`[2 03 3 05 4]`, flipping input bit 3 fails to flip the parity of output bits
0 and 4 on 192 of 256 inputs. Exhaustive small-width regression tests now
cover both the corrected conditions and this counterexample.

SAC cannot detect all such parity relations. When the output parity contains
two bits and a single input flip triggers it, BIC can expose it. This is a
structural limitation, not a universal proof that two-round generators are
unsuitable or that three rounds are sufficient. Stream quality needs separate
testing.

## Reference measurements (`results/refs_corrected.txt`)

The 32-bit and 16-bit figures are exact and match the published values to 17
digits. The 64-bit figures use 2³⁰ random inputs (seed 1) and were rerun on
2026-11-20 with the corrected pooled estimator. Their column is the signed
mean square ms ± approximate SE, in units of 10⁻¹¹; the score-scale bias
cannot carry a symmetric error bar near zero. "BIC max|z|" should be about 4.5–5 for an ideal mixer; 256
means a perfectly correlated output-bit pair.

| Width | Function | Rounds | Bias (64-bit: ms ×10⁻¹¹) | BIC max\|z\| | Source |
|---|---|---|---|---|---|
| 32 | prospector32 | 2 | 0.34968 | 256 | hash-prospector |
| 32 | Murmur3 fmix32 | 2 | 0.26399 | 256 | MurmurHash3 |
| 32 | lowbias32 | 2 | 0.17353 | 256 | hash-prospector |
| 32 | TheIronBorn `[16 21f0aaad 15 d35a2d97 15]` | 2 | 0.10760 | 87 | hash-prospector #19 |
| 32 | TheIronBorn `[16 21f0aaad 15 735a2d97 15]` (previous record) | 2 | 0.10704 | 87 | hash-prospector #19 |
| 32 | **this project `[16 9c013335 15 335a2d97 15]`** (best balanced, F21) | 2 | 0.10207 | 102 | FINDINGS F21 |
| 32 | this project `[16 b20332d3 15 d21a3597 15]` (F23) | 2 | 0.10091 | 154 | FINDINGS F23 |
| 32 | this project `[16 b20332d3 15 549a3597 15]` (F25; best issue-#12) | 2 | 0.10076 | 154 | FINDINGS F25 |
| 32 | this project `[16 e00332d3 15 549a3597 15]` (F26) | 2 | 0.09967 | 154 | FINDINGS F26 |
| 32 | **this project `[16 cb0332d3 15 549a3597 15]`** (best bias, worse BIC; F29) | 2 | 0.09908 | 154 | FINDINGS F29 |
| 32 | triple32 | 3 | 0.02089 | 4.0 | hash-prospector |
| 64 | Murmur3 fmix64 | 2 | 33.5 ± 2.7 (bias 0.0183) | 247 | MurmurHash3 |
| 64 | Moremur | 2 | 19.2 ± 2.0 (bias 0.0139) | 9.2 | Evensen 2019 |
| 64 | NASAM | 2 + rotations | 0.6 ± 2.1 | 4.6 | Evensen 2020 |
| 64 | Stafford Mix13 (SplitMix64) | 2 | 2.3 ± 2.1 | 256 | Stafford 2011 |
| 64 | rrmxmx | 2 + rotations | −1.0 ± 2.2 | 57 | Evensen 2018 |
| 64 | mx3 | 3 | 0.04 ± 2.0 | 4.5 | Maiga |

Only fmix64 (z = 12.4) and Moremur (z = 9.7) are measurably biased; the
other four are consistent with zero, so sampled bias cannot rank them.
See `results/REVIEW_STATUS.md` for the status of saved experiments.
