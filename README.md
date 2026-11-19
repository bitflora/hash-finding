# Mixing functions for hashing and PRNGs

Searching for better constants in integer "finalizers" — the multiply-xorshift
mixers used by hash tables and random number generators — judged primarily by
avalanche bias, with additional statistical yardsticks.

- `sources.md` — annotated bibliography
- `plan.md` — research plan and experiment log
- `harness/` — (to be built) Rust evaluation and search tool
- `results/`, `analysis/` — (to be created) raw results and analysis scripts

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

## Finding so far: every 2-round mixer has a parity that always flips

For any `[s0 m0 s1 m1 s2]` mixer, **whatever the multipliers**:
- Flipping input bit `s1` (and also input bit `s0+s1`, when that is inside the
  word) flips the XOR of output bits `0, s2, 2·s2, …` **every time**.
- Why: multiplication only moves information upward, and the top bit of each
  "window" passes through it linearly. So output word bit 0 of the last multiply
  equals `z_0 ⊕ z_{s1}`, and the last xorshift spreads that bit over output
  bits `0, s2, 2·s2, …`.
- Confirmed empirically (100.00% over 20k trials) for lowbias32, TheIronBorn's
  record and random constants.
- The avalanche bias score can't see this, because it looks at one output bit
  at a time. When `s2 ≥ bits/2` the parity covers only two output bits, so it
  shows up as a pairwise BIC correlation of exactly ±1. That holds for lowbias32,
  Murmur3 fmix32 and SplitMix64/Mix13.
- **Consequence:** searching 2-round constants can lower avalanche bias but can
  never remove this weakness. Applications that need generator-quality output
  need ≥3 rounds or rotations (mx3, NASAM).

## Reference measurements (`results/refs.txt`)

The 32-bit and 16-bit figures are exact and match the published values to 17
digits. The 64-bit figures use 2³⁰ random inputs and are debiased, ± one
standard error. "BIC max|z|" should be about 4.5–5 for an ideal mixer; 256
means a perfectly correlated output-bit pair.

| Width | Function | Rounds | Bias | BIC max\|z\| | Source |
|---|---|---|---|---|---|
| 32 | prospector32 | 2 | 0.34968 | 256 | hash-prospector |
| 32 | Murmur3 fmix32 | 2 | 0.26399 | 256 | MurmurHash3 |
| 32 | lowbias32 | 2 | 0.17353 | 256 | hash-prospector |
| 32 | TheIronBorn `[16 21f0aaad 15 d35a2d97 15]` | 2 | 0.10760 | 87 | hash-prospector #19 |
| 32 | TheIronBorn `[16 21f0aaad 15 735a2d97 15]` (record) | 2 | 0.10704 | 87 | hash-prospector #19 |
| 32 | triple32 | 3 | 0.02089 | 4.0 | hash-prospector |
| 64 | Murmur3 fmix64 | 2 | 0.0183 ± 0.0024 | 247 | MurmurHash3 |
| 64 | Moremur | 2 | 0.0139 ± 0.0041 | 9.2 | Evensen 2019 |
| 64 | NASAM | 2 + rotations | 0.002 ± 0.018 | 4.6 | Evensen 2020 |
| 64 | Stafford Mix13 (SplitMix64) | 2 | 0.005 ± 0.007 | 256 | Stafford 2011 |
| 64 | rrmxmx | 2 + rotations | 0.000 ± 0.009 | 57 | Evensen 2018 |
| 64 | mx3 | 3 | 0.001 ± 0.080 | 4.5 | Maiga |

Even at 2³⁰ samples, only fmix64 and Moremur are measurably biased. The other
64-bit mixers can't be told apart by avalanche bias alone. This is why the
64-bit work needs either analytic surrogates or other yardsticks.
