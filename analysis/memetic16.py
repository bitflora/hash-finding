"""Memetic races for 2-round 16-bit xorshift-multiply mixers, scored exactly.

At 16 bits `mixlab exact --bits 16` (fast16 kernel) scores ~150k configs/s, so
every candidate is scored exactly and no sampling or racing statistics are
needed. Each race keeps a population, and each generation it scores:
  - crossover children (one multiplier's low bits from the other parent, cut
    at 4/6/8/10/12 bits; shifts inherited from either parent),
  - random mutations (1-3 multiplier bit flips, or a shift +-1),
  - the full 1-flip neighbourhood (all multiplier bits, all shifts +-1) of the
    top `--local` members, and the full 2-flip neighbourhood of the best.
A race stops after `--generations` or `--patience` generations without a new
best. Every scored spec is written to <run>/exact.csv (spec,bias; gzip it before committing), the best
per generation to <run>/progress.csv, the final ranking to leaderboard.csv.

Run from any cwd:
  python analysis/memetic16.py results/memetic16/<run> --races 8 --seed 1
"""

import argparse
import csv
import os
import random
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BITS = 16
MASK = (1 << BITS) - 1
BIN = ROOT / "harness/target/release" / ("mixlab.exe" if os.name == "nt" else "mixlab")
# Candidate = (s0, m0, s1, m1, s2)
MULTS = (1, 3)
SHIFTS = (0, 2, 4)


def fmt(c):
    return f"[{c[0]} {c[1]:04x} {c[2]} {c[3]:04x} {c[4]}]"


def parse(spec):
    f = spec.strip("[]").split()
    return (int(f[0]), int(f[1], 16), int(f[2]), int(f[3], 16), int(f[4]))


def rep(c, i, v):
    return c[:i] + (v,) + c[i + 1:]


def valid(c):
    return all(1 <= c[i] < BITS for i in SHIFTS) and all(c[i] & 1 for i in MULTS)


def neighbours1(c):
    for i in MULTS:
        for b in range(1, BITS):
            yield rep(c, i, c[i] ^ (1 << b))
    for i in SHIFTS:
        for d in (-1, 1):
            y = rep(c, i, c[i] + d)
            if valid(y):
                yield y


def neighbours2(c):
    flips = [(i, b) for i in MULTS for b in range(1, BITS)]
    for a in range(len(flips)):
        for b in range(a + 1, len(flips)):
            (i, p), (j, q) = flips[a], flips[b]
            y = rep(c, i, c[i] ^ (1 << p))
            yield rep(y, j, y[j] ^ (1 << q))


def random_cand(rng, lo, hi):
    return (rng.randint(lo, hi), rng.randrange(1, 1 << BITS, 2), rng.randint(lo, hi),
            rng.randrange(1, 1 << BITS, 2), rng.randint(lo, hi))


def mutate(c, rng):
    if rng.random() < 0.15:
        i = rng.choice(SHIFTS)
        y = rep(c, i, c[i] + rng.choice((-1, 1)))
        return y if valid(y) else c
    for _ in range(rng.choice((1, 1, 2, 3))):
        i = rng.choice(MULTS)
        c = rep(c, i, c[i] ^ (1 << rng.randrange(1, BITS)))
    return c


def crossover(a, b, rng):
    i = rng.choice(MULTS)
    low = (1 << rng.choice((4, 6, 8, 10, 12))) - 1
    child = rep(a, i, (a[i] & ~low & MASK) | (b[i] & low))
    if rng.random() < 0.3:
        j = rng.choice(SHIFTS)
        child = rep(child, j, b[j])
    return child


class Scorer:
    """Exact scores via `mixlab exact --bits 16 --list -`, cached, all logged."""

    def __init__(self, out, backend):
        self.cache = {}
        self.backend = backend
        self.stream = (out / "exact.csv").open("w", newline="", buffering=1)
        self.log = csv.writer(self.stream)
        self.log.writerow(("spec", "bias"))

    def score(self, cands):
        todo = sorted({c for c in cands if c not in self.cache})
        if todo:
            r = subprocess.run([BIN, "exact", self.backend, "--bits", str(BITS), "--list", "-"],
                               input="".join(fmt(c) + "\n" for c in todo), text=True,
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=True)
            rows = list(csv.DictReader(r.stdout.splitlines()))
            if len(rows) != len(todo):
                raise RuntimeError("mixlab returned a different number of rows")
            for c, row in zip(todo, rows):
                if parse(row["spec"]) != c:
                    raise RuntimeError(f"row mismatch: {row['spec']} vs {fmt(c)}")
                self.cache[c] = float(row["bias"])
                self.log.writerow((fmt(c), row["bias"]))
        return {c: self.cache[c] for c in cands}


def race(scorer, rng, args, race_id, progress, start=None):
    pop = {random_cand(rng, args.shift_lo, args.shift_hi) for _ in range(args.pop * 4)}
    if start:
        pop.add(start)
    scores = scorer.score(pop)
    pop = sorted(pop, key=scores.get)[:args.pop]
    best, best_gen = scores[pop[0]], 0
    for gen in range(1, args.generations + 1):
        cand = set()
        while len(cand) < args.children:
            a, b = rng.sample(pop, 2)
            c = crossover(a, b, rng) if rng.random() < 0.6 else mutate(a, rng)
            if rng.random() < 0.3:
                c = mutate(c, rng)
            cand.add(c)
        for p in pop[:args.local]:
            cand.update(neighbours1(p))
        cand.update(neighbours2(pop[0]))
        cand.update(pop)
        s = scorer.score(cand)
        # keep diversity: at most `--per-shift` members per shift triple
        ranked = sorted(cand, key=s.get)
        new, per = [], {}
        for c in ranked:
            k = (c[0], c[2], c[4])
            if per.get(k, 0) < args.per_shift:
                per[k] = per.get(k, 0) + 1
                new.append(c)
            if len(new) == args.pop:
                break
        pop = new
        if s[pop[0]] < best:
            best, best_gen = s[pop[0]], gen
        progress.writerow((race_id, gen, fmt(pop[0]), f"{s[pop[0]]:.17g}", len(scorer.cache)))
        if gen - best_gen >= args.patience:
            break
    print(f"race {race_id}: best {fmt(pop[0])} {best:.6f} at gen {best_gen} (stopped gen {gen})", flush=True)
    return pop[0], best


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("out", type=Path)
    ap.add_argument("--races", type=int, default=8)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--pop", type=int, default=64)
    ap.add_argument("--children", type=int, default=3000)
    ap.add_argument("--local", type=int, default=16)
    ap.add_argument("--per-shift", type=int, default=16)
    ap.add_argument("--generations", type=int, default=300)
    ap.add_argument("--patience", type=int, default=40)
    ap.add_argument("--shift-lo", type=int, default=4)
    ap.add_argument("--shift-hi", type=int, default=12)
    ap.add_argument("--start", help="seed every race's population with this spec")
    ap.add_argument("--gpu", action="store_true", help="score on the GPU (slower at 16 bits)")
    args = ap.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    scorer = Scorer(args.out, "--gpu" if args.gpu else "--cpu")
    start = parse(args.start) if args.start else None
    results = []
    with (args.out / "progress.csv").open("w", newline="", buffering=1) as f:
        progress = csv.writer(f)
        progress.writerow(("race", "gen", "best_spec", "best_bias", "scored_total"))
        for r in range(args.races):
            rng = random.Random(args.seed * 1000 + r)
            results.append(race(scorer, rng, args, r, progress, start))
    with (args.out / "leaderboard.csv").open("w", newline="") as f:
        w = csv.writer(f)
        w.writerow(("spec", "bias"))
        for c in sorted(scorer.cache, key=scorer.cache.get)[:500]:
            w.writerow((fmt(c), f"{scorer.cache[c]:.17g}"))
    print(f"scored {len(scorer.cache)} distinct configs; "
          f"race bests: {sorted(round(b, 4) for _, b in results)}")


if __name__ == "__main__":
    main()
