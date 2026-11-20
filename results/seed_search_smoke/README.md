# Seed-search workflow validation (2026-11-19)

Run from `mixing-functions/harness`, release build:

```
target/release/seed-search --source SOURCE --bits 64 --count 8 --samples p14 --seed 1 --verify 1 --verify-samples p16 --out ../results/seed_search_smoke/SOURCE.csv
target/release/seed-search --source spectral --bits 32 --count 4 --samples p14 --seed 1 --verify 1 --out ../results/seed_search_smoke/spectral32.csv
```

The first command was run for `spectral`, `random`, `matched`, and `mixer`.
Each CSV records the screening seed, signed mean-square estimate and approximate
standard error. The companion `.verified.csv` files contain the selected
finalist's exact (32-bit) or independent sampled (64-bit) result.

These small runs validate command execution and verification paths. They do not
estimate comparative search performance. In particular, negative sampled values
are legitimate noise-corrected estimates and do not indicate negative true bias.
Repeated, larger runs are needed to assess spectral seeds as a search strategy.
