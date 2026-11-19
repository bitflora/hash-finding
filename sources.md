# Sources

## Core hobbyist work
- **Chris Wellons (skeeto), "Prospecting for Hash Functions" (2018)** —
  https://nullprogram.com/blog/2018/07/31/ . Introduces the automated search
  and the avalanche-bias metric used throughout. Repo:
  https://github.com/skeeto/hash-prospector (lowbias32 ≈ 0.1735, triple32 ≈
  0.0209 ≈ random-permutation floor).
- **hash-prospector issue #19, "New best known functions" (TheIronBorn)** —
  https://github.com/skeeto/hash-prospector/issues/19 . 2-round 32-bit
  `[16 21f0aaad 15 d35a2d97 15]` bias ≈ 0.1076, later ≈ 0.10704. Method:
  metaheuristic on sampled estimate (2²⁵ inputs) constrained near good
  functions, then exhaustive exact search in the neighbourhood.
- **hash-prospector issue #28, "About the avalanche bias"** —
  https://github.com/skeeto/hash-prospector/issues/28 . The lowest-bias
  function is not necessarily best by RRC-PractRand / TFR. Motivates
  multi-objective evaluation.
- **David Stafford, "Better Bit Mixing — Improving on MurmurHash3's 64-bit
  Finalizer" (2011)** —
  https://zimbry.blogspot.com/2011/09/better-bit-mixing-improving-on.html .
  Mix01–Mix14; Mix13 (shifts 30/27/31, 0xbf58476d1ce4e5b9, 0x94d049bb133111eb)
  became SplitMix64's finalizer and Java's.
- **Pelle Evensen, "Mostly Mangling" blog** — http://mostlymangling.blogspot.com/
  - On the mixing functions in SplitMix, Murmur3 and Stafford (2018-07):
    http://mostlymangling.blogspot.com/2018/07/on-mixing-functions-in-fast-splittable.html
    (rrmxmx; introduces RRC testing).
  - Moremur (2019-12):
    https://mostlymangling.blogspot.com/2019/12/stronger-better-morer-moremur-better.html
  - NASAM (2020-01):
    http://mostlymangling.blogspot.com/2020/01/nasam-not-another-strange-acronym-mixer.html
    (passes RRC-64-42-TF2-0.94, fails RRC-64-44 after 13 TB).
  - RRC = feed counters (with gammas/strides) reversed, rotated and
    complemented through the mixer and run PractRand; a much harsher test
    than avalanche alone.
- **Jon Kågström (Maiga)** — mx3 (https://github.com/jonmaiga/mx3),
  "The construction of bit mixers" (https://jonkagstrom.com/bit-mixer-construction/),
  "Tuning bit mixers" (https://jonkagstrom.com/tuning-bit-mixers/index.html).
  Stack-machine program search for mixer *structures*, then constant tuning;
  mxmxmx mixers passing PractRand beyond 2⁴⁴.

## Academic / adoption
- **Webster & Tavares, "On the Design of S-Boxes", CRYPTO '85 (LNCS 218)** —
  Strict Avalanche Criterion; Bit Independence Criterion.
- **Steele & Vigna, "LXM: Better Splittable Pseudorandom Number Generators"** —
  https://vigna.di.unimi.it/ftp/papers/LXM.pdf . Compares Stafford, Lea and
  degski mixers as output functions.
- **Steele, Lea & Flood, "Fast Splittable Pseudorandom Number Generators"
  (OOPSLA 2014)** — SplitMix.
- **Apache Commons RNG MixFunctions / RNG-94** — production use of Stafford
  Mix13 and RotateRotateMultiplyXorMultiplyXor (rrmxmx).
- **Zig std.hash.int** — adopted skeeto's constants after finding the old
  default mixer very biased.
- **SMHasher (rurban) / SMHasher3 (Frank Wojcik)** — broader hash test suites.
- **CAFxX/mix-inst** — https://github.com/CAFxX/mix-inst : hardware
  (RISC-V instruction) finalizer search with a staged validation battery.
