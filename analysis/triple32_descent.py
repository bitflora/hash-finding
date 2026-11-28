"""Exact-scored descent searches for three-round, 32-bit mixers.

Every move is judged by mixlab's exact bias over all 2^32 inputs; no sampling.
Modes:
  restarts   random odd multipliers on a fixed schedule, each descended
  ils        iterated local search: kick the incumbent with K random bit flips,
             descend, keep the result if it is no worse
  radius2    score every configuration one or two steps away, then descend
             the best few
  schedules  score shift schedules with transplanted multipliers, then descend
             the best ones

A step flips one of bits 1-31 of a multiplier or moves one shift by one.
Descent scores shuffled neighbours in chunks and moves to the best improving
neighbour of the first chunk that has one; it stops when all neighbours are
no better. All exact scores go to RUN/exact.csv (resumable) and every
descent to RUN/descents.csv.
"""

import argparse
import csv
import random
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from triple32_memetic import DATA, MULTIPLIERS, SHIFTS, fmt, neighbors, parse, replace  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "harness" / "target-triple32" / "release" / "mixlab"
RECORD = "[17 88bb3153 13 238e8679 17 7da6b66b 17]"


class Scorer:
    def __init__(self, run):
        self.run = run
        self.path = run / "exact.csv"
        self.cache = {}
        for path in DATA.rglob("*.csv"):
            if path == self.path:
                continue
            with path.open() as stream:
                reader = csv.DictReader(stream)
                if not reader.fieldnames or "spec" not in reader.fieldnames or "bias" not in reader.fieldnames:
                    continue
                if path.name in ("schedules.csv", "schedules_polished.csv") or "screen" in path.name:
                    continue
                for row in reader:
                    try:
                        self.cache[parse(row["spec"])] = float(row["bias"])
                    except (ValueError, KeyError, TypeError):
                        pass
        self.prior = len(self.cache)
        if self.path.exists():
            with self.path.open() as stream:
                for row in csv.DictReader(stream):
                    self.cache[parse(row["spec"])] = float(row["bias"])
        self.new = len(self.cache) - self.prior
        # One long-running exact worker keeps CUDA initialised between batches.
        self.worker = subprocess.Popen(
            [str(BINARY), "exact", "--gpu", "--bits", "32", "--stream"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1,
        )
        if self.worker.stdout.readline().strip() != "spec,bias,max_abs":
            raise RuntimeError("mixlab exact --stream did not start")

    def close(self):
        self.worker.stdin.close()
        self.worker.wait()

    def __call__(self, candidates):
        todo = [c for c in dict.fromkeys(candidates) if c not in self.cache]
        if todo:
            rows = []
            # Small slices so neither pipe fills while the other side waits.
            for i in range(0, len(todo), 64):
                part = todo[i:i + 64]
                self.worker.stdin.write("".join(fmt(c) + "\n" for c in part))
                self.worker.stdin.flush()
                for _ in part:
                    line = self.worker.stdout.readline()
                    if not line:
                        raise RuntimeError("mixlab exact --stream worker stopped")
                    spec, bias, max_abs = line.strip().rsplit(",", 2)
                    rows.append({"spec": spec, "bias": bias, "max_abs": max_abs})
            fresh = not self.path.exists()
            with self.path.open("a", newline="") as stream:
                w = csv.writer(stream)
                if fresh:
                    w.writerow(("spec", "bias", "max_abs"))
                for row in rows:
                    c = parse(row["spec"])
                    self.cache[c] = float(row["bias"])
                    w.writerow((fmt(c), row["bias"], row["max_abs"]))
            self.new += len(rows)
            if any(c not in self.cache for c in todo):
                raise RuntimeError("exact output differs from requested candidates")
        return {c: self.cache[c] for c in candidates}


def descend(score, start, rng, chunk=34, mults_only=False):
    current = start
    best = score([start])[start]
    steps = 0
    while True:
        hood = [n for n in neighbors(current)
                if not (mults_only and n[0::2] != current[0::2])]
        rng.shuffle(hood)
        moved = False
        for i in range(0, len(hood), chunk):
            scores = score(hood[i:i + chunk])
            cand = min(scores, key=scores.get)
            if scores[cand] < best:
                current, best, moved = cand, scores[cand], True
                steps += 1
                break
        if not moved:
            return current, best, steps


class Log:
    def __init__(self, run):
        self.path = run / "descents.csv"
        fresh = not self.path.exists()
        self.stream = self.path.open("a", newline="")
        self.w = csv.writer(self.stream)
        if fresh:
            self.w.writerow(("mode", "label", "start", "start_bias", "end", "end_bias", "steps"))

    def __call__(self, mode, label, start, sb, end, eb, steps):
        self.w.writerow((mode, label, fmt(start), sb, fmt(end), eb, steps))
        self.stream.flush()
        print(f"{mode} {label}: {fmt(start)} {sb:.6f} -> {fmt(end)} {eb:.8f} ({steps} steps)", flush=True)


def done_labels(log, mode):
    with log.path.open() as stream:
        return {r["label"] for r in csv.DictReader(stream) if r["mode"] == mode}


def restarts(args, score, log):
    shifts = [int(s) for s in args.schedule.split("/")]
    done = done_labels(log, "restarts")
    for k in range(args.count):
        label = f"{args.schedule}#{args.seed}.{k}"
        rng = random.Random(f"{args.seed}:{k}")
        # Best of a small random pool as the start.
        pool = [tuple(v for pair in zip(shifts, [rng.getrandbits(32) | 1 for _ in range(3)] + [None])
                      for v in pair if v is not None) for _ in range(args.pool)]
        if label in done:
            continue
        s = score(pool)
        start = min(s, key=s.get)
        end, eb, steps = descend(score, start, rng, mults_only=args.fix_shifts)
        log("restarts", label, start, s[start], end, eb, steps)


def ils(args, score, log):
    # Resumable: the path depends only on the seed and on exact scores, so a
    # rerun replays logged iterations from the cache without logging them again.
    done = done_labels(log, "ils")
    rng = random.Random(args.seed)
    inc = parse(args.start)
    inc_b = score([inc])[inc]
    for it in range(args.iters):
        k = rng.choice(range(args.kick_min, args.kick_max + 1))
        kicked = inc
        for _ in range(k):
            i = rng.choice(MULTIPLIERS)
            kicked = replace(kicked, i, kicked[i] ^ (1 << rng.randrange(1, 32)))
        kb = score([kicked])[kicked]
        end, eb, steps = descend(score, kicked, rng, mults_only=args.fix_shifts)
        label = f"{args.seed}.{it}.k{k}"
        if label not in done:
            log("ils", label, kicked, kb, end, eb, steps)
        if eb <= inc_b:
            inc, inc_b = end, eb
            print(f"  incumbent {fmt(inc)} {inc_b:.10f}", flush=True)


def radius2(args, score, log):
    """Score every configuration one or two steps from START exactly, then
    descend from the best few."""
    center = parse(args.start)
    one = set(neighbors(center))
    two = {n2 for n in one for n2 in neighbors(n)} - one - {center}
    hood = [center] + sorted(one) + sorted(two)
    print(f"radius two: {len(one)} one-step, {len(two)} two-step", flush=True)
    s = score(hood)
    with (score.run / "neighborhood.csv").open("w", newline="") as stream:
        w = csv.writer(stream)
        w.writerow(("spec", "distance", "bias"))
        for c in sorted(hood, key=s.get):
            w.writerow((fmt(c), 0 if c == center else 1 if c in one else 2, s[c]))
    rng = random.Random(args.seed)
    done = done_labels(log, "radius2")
    for i, c in enumerate(sorted(hood, key=s.get)[:args.top]):
        label = f"{fmt(center)}#{i}"
        if label in done:
            continue
        end, eb, steps = descend(score, c, rng)
        log("radius2", label, c, s[c], end, eb, steps)


def schedules(args, score, log):
    lo, hi = (int(x) for x in args.range.split(".."))
    donors = [parse(s) for s in args.donors]
    grid = []
    for a in range(lo, hi + 1):
        for b in range(lo, hi + 1):
            for c in range(lo, hi + 1):
                for d in range(lo, hi + 1):
                    for m in donors:
                        grid.append((a, m[1], b, m[3], c, m[5], d))
    print(f"schedule grid: {len(grid)} specs", flush=True)
    s = score(grid)
    best = {}
    for cand, b in s.items():
        key = cand[0::2]
        if key not in best or b < s[best[key]]:
            best[key] = cand
    ranked = sorted(best.values(), key=s.get)
    with (score.run / "schedule_grid.csv").open("w", newline="") as stream:
        w = csv.writer(stream)
        w.writerow(("schedule", "spec", "bias"))
        for c in ranked:
            w.writerow(("/".join(map(str, c[0::2])), fmt(c), s[c]))
    done = done_labels(log, "schedules")
    rng = random.Random(args.seed)
    for c in ranked[:args.top]:
        label = "/".join(map(str, c[0::2]))
        if label in done:
            continue
        end, eb, steps = descend(score, c, rng, mults_only=True)
        log("schedules", label, c, s[c], end, eb, steps)


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("run", type=Path)
    sub = p.add_subparsers(dest="mode", required=True)
    r = sub.add_parser("restarts")
    r.add_argument("--schedule", default="17/11/15/14")
    r.add_argument("--count", type=int, default=20)
    r.add_argument("--pool", type=int, default=32)
    r.add_argument("--seed", type=int, default=1)
    r.add_argument("--fix-shifts", action="store_true")
    i = sub.add_parser("ils")
    i.add_argument("--start", default=RECORD)
    i.add_argument("--iters", type=int, default=50)
    i.add_argument("--kick-min", type=int, default=3)
    i.add_argument("--kick-max", type=int, default=6)
    i.add_argument("--seed", type=int, default=1)
    i.add_argument("--fix-shifts", action="store_true")
    q = sub.add_parser("radius2")
    q.add_argument("--start", default=RECORD)
    q.add_argument("--top", type=int, default=5)
    q.add_argument("--seed", type=int, default=1)
    s = sub.add_parser("schedules")
    s.add_argument("--range", default="13..19")
    s.add_argument("--donors", nargs="+", default=[RECORD])
    s.add_argument("--top", type=int, default=20)
    s.add_argument("--seed", type=int, default=1)
    args = p.parse_args()
    args.run.mkdir(parents=True, exist_ok=True)
    # One writer per run directory: two concurrent copies interleave exact.csv.
    lock = args.run / "LOCK"
    try:
        lock.open("x").close()
    except FileExistsError:
        sys.exit(f"{lock} exists: another copy is running here (delete it if not)")
    try:
        run(args)
    finally:
        lock.unlink()


def run(args):
    score = Scorer(args.run)
    print(f"cache: {score.prior} prior exact scores, {score.new} from this run", flush=True)
    log = Log(args.run)
    try:
        {"restarts": restarts, "ils": ils, "radius2": radius2, "schedules": schedules}[args.mode](args, score, log)
    finally:
        score.close()
    print(f"done; {score.new} exact scores in this run", flush=True)


if __name__ == "__main__":
    main()
