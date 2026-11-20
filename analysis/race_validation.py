"""Score racing threshold policies offline against exact truth.

Input: CSVs from `mixlab trace` (columns base,rep,cand,true_ms,inc_ms,n,ms_hat).
Each candidate's trajectory was sampled until |ms_hat - inc| > zmax*sd or the
cap, so any policy with thresholds <= zmax decides exactly as a real race with
the same seeds would.

Usage: python race_validation.py TRACE.csv [TRACE2.csv ...]

Reports
  1. noise-model calibration: standardized error (ms_hat - true)/sd_model
  2. per policy (z_drop, z_promote): false drops of truly better candidates,
     races whose improvement was missed entirely, exact checks and inputs
     under first-improvement acceptance.
"""
import csv, math, sys
from collections import defaultdict


def sd_ms(ms, n, bits):
    return math.sqrt((4 * max(ms, 0.0) / n + 2 / n ** 2) / bits ** 2)


def load(path):
    meta = {}
    races = defaultdict(lambda: defaultdict(list))  # (base,rep) -> cand -> [(n, ms_hat)]
    truth = {}
    inc = {}
    with open(path) as f:
        first = f.readline()
        for tok in first[1:].split():
            if '=' in tok:
                k, v = tok.split('=')
                meta[k] = float(v)
        rows = csv.DictReader(l for l in f if not l.startswith('#'))
        for r in rows:
            key = (int(r['base']), int(r['rep']))
            c = int(r['cand'])
            races[key][c].append((int(r['n']), float(r['ms_hat'])))
            truth[(key[0], c)] = float(r['true_ms'])
            inc[key[0]] = float(r['inc_ms'])
    return meta, races, truth, inc


def adjusted_z(z, k, n0, cap):
    looks, n = 1, n0
    while n < cap:
        n = min(2 * n, cap)
        looks += 1
    return math.sqrt(z * z + 2 * math.log(k * (looks + 1)))


def decide(traj, inc, bits, zd, zp, cap):
    """-> (outcome, n_decided, ms_hat_at_decision); outcome in drop/promote/cap."""
    for n, mh in traj:
        sd = sd_ms(mh, n, bits)
        d = mh - inc
        if d > zd * sd:
            return 'drop', n, mh
        if d < -zp * sd:
            return 'promote', n, mh
    n, mh = traj[-1]
    assert n >= cap, f"trajectory truncated at n={n} before deciding (zmax too small?)"
    return 'cap', n, mh


def simulate(cands, truth_of, inc, bits, zd, zp, cap):
    """First-improvement race. Returns dict of metrics."""
    dec = {c: decide(t, inc, bits, zd, zp, cap) for c, t in cands.items()}
    better = {c for c in cands if truth_of(c) < inc}
    false_drop = {c for c in better if dec[c][0] == 'drop'}
    # promotions are verified at the look they happen, best estimate first;
    # cap fallbacks after the last look, best estimate first.
    events = sorted(
        ((n if o == 'promote' else float('inf'), mh, c) for c, (o, n, mh) in dec.items() if o != 'drop'))
    exact = 0
    acc_n = None
    accepted = None
    for n, mh, c in events:
        exact += 1
        if truth_of(c) < inc:
            accepted, acc_n = c, (n if n != float('inf') else cap)
            break
    stop = acc_n if acc_n is not None else float('inf')
    inputs = sum(min(dec[c][1], stop) for c in cands)
    gain = (inc - truth_of(accepted)) if accepted is not None else 0.0
    best_gain = max((inc - truth_of(c) for c in better), default=0.0)
    return dict(k=len(cands), better=len(better), false_drop=len(false_drop),
                missed=int(bool(better) and accepted is None),
                exact=exact, inputs=inputs, gain=gain, best_gain=best_gain,
                best_dropped=int(bool(better) and min(better, key=truth_of) in false_drop),
                exact_all=sum(o != 'drop' for o, _, _ in dec.values()),
                wasted_all=sum(o != 'drop' and c not in better for c, (o, _, _) in dec.items()))


def calibration(races, truth, inc, bits, label):
    # first look: every candidate, no selection
    zs_first = []
    by_n = defaultdict(list)
    for (b, r), cands in races.items():
        for c, traj in cands.items():
            t = truth[(b, c)]
            n, mh = traj[0]
            zs_first.append((mh - t) / sd_ms(t, n, bits))
            for n, mh in traj:
                # little stopping selection when the true gap is small vs sd
                if abs(t - inc[b]) < sd_ms(t, n, bits):
                    by_n[n].append((mh - t) / sd_ms(t, n, bits))

    def summ(z):
        m = sum(z) / len(z)
        s = math.sqrt(sum((x - m) ** 2 for x in z) / (len(z) - 1))
        return f"N={len(z):5d} mean={m:+.3f} sd={s:.3f} P(|z|>3)={sum(abs(x) > 3 for x in z) / len(z):.4f} max|z|={max(abs(x) for x in z):.2f}"

    print(f"\n== calibration {label}: z = (ms_hat - true)/sd_model")
    print(f"  first look       {summ(zs_first)}")
    for n in sorted(by_n):
        if len(by_n[n]) >= 30:
            print(f"  n=2^{math.log2(n):4.1f}  {summ(by_n[n])}")


def main():
    for path in sys.argv[1:]:
        meta, races, truth, inc = load(path)
        bits, n0, cap = int(meta['bits']), int(meta['n0']), int(meta['cap'])
        zmax = meta['zmax']
        k = max(len(c) for c in races.values())
        za = adjusted_z(3.0, k, n0, cap)
        print(f"\n######## {path}: bits={bits} K={k} races={len(races)} adjusted z(3)={za:.2f} zmax={zmax}")
        calibration(races, truth, inc, bits, path)
        nb = sorted({b for b, _ in races})
        for b in nb:
            better = sum(truth[(b, c)] < inc[b] for c in races[(b, 0)])
            print(f"  base {b}: bias={1000 * math.sqrt(inc[b]):.5f} truly better={better}/{len(races[(b, 0)])}")
        policies = [(3, 3), (za, za), (za, 3), (za, 2), (za, 1), (za, 0), (4, 2), (3, 2), (3, 1)]
        policies = [(d, p) for d, p in policies if d <= zmax and p <= zmax]
        print(f"\n  {'z_drop':>6} {'z_prom':>6} | {'better':>6} {'falseDrop':>9} {'bestDrop':>8} {'missed':>6} | "
              f"{'exact/race':>10} {'log2 inputs':>11} {'gain/best':>9} | {'exact_all':>9} {'wasted':>6}")
        for zd, zp in policies:
            agg = defaultdict(float)
            races_with_better = 0
            for (b, r), cands in races.items():
                m = simulate(cands, lambda c: truth[(b, c)], inc[b], bits, zd, zp, cap)
                for key, v in m.items():
                    agg[key] += v
                races_with_better += m['better'] > 0
            R = len(races)
            print(f"  {zd:6.2f} {zp:6.2f} | {agg['better'] / R:6.1f} {agg['false_drop'] / max(agg['better'], 1):9.3f} "
                  f"{agg['best_dropped'] / max(races_with_better, 1):8.3f} {agg['missed'] / max(races_with_better, 1):6.3f} | "
                  f"{agg['exact'] / R:10.2f} {math.log2(agg['inputs'] / R):11.2f} "
                  f"{agg['gain'] / max(agg['best_gain'], 1e-300):9.3f} | {agg['exact_all'] / R:9.2f} {agg['wasted_all'] / R:6.2f}")
        # per-base false drops, split by final shift (F11: s2 >= bits/2 leaves an
        # exactly correlated output pair, so searches should exclude it)
        s2_of = {}
        try:
            with open(path + '.cands.csv') as f:
                for r in csv.DictReader(f):
                    s2_of[(int(r['base']), int(r['cand']))] = int(r['spec'].strip('"[]').split()[-1])
        except FileNotFoundError:
            pass
        print(f"\n  per base: false drops of truly better candidates (count over all reps), "
              f"s2<{bits // 2} | s2>={bits // 2}; tightest gap = smallest |true - inc| among better")
        for zd, zp in [(3, 3), (za, za), (za, 3)]:
            for b in nb:
                fd = defaultdict(int)
                nbtr = defaultdict(int)
                gaps = []
                for (bb, r), cands in races.items():
                    if bb != b:
                        continue
                    for c, traj in cands.items():
                        t = truth[(b, c)]
                        if t >= inc[b]:
                            continue
                        hi = s2_of.get((b, c), 0) >= bits // 2
                        nbtr[hi] += 1
                        gaps.append(inc[b] - t)
                        if decide(traj, inc[b], bits, zd, zp, cap)[0] == 'drop':
                            fd[hi] += 1
                g = min(gaps) if gaps else float('nan')
                print(f"    z=({zd:.2f},{zp:.2f}) base {b}: {fd[False]}/{nbtr[False]} | {fd[True]}/{nbtr[True]}"
                      f"   tightest gap={g:.2e} = {g / sd_ms(inc[b], cap, bits):.2f} sd at cap")
        print("  falseDrop = fraction of truly better candidates dropped; bestDrop/missed = fraction of races "
              "(with any better candidate) that dropped the best one / accepted nothing; gain/best = accepted "
              "improvement relative to the best available; exact_all/wasted = exact checks if every non-dropped "
              "candidate were verified / of those, truly worse.")


if __name__ == '__main__':
    main()
