"""Symmetry check for 2-round xmxmx mixers: does bias survive m -> -m, or inversion?

Builds the variant configs (negated / top-bit-flipped / randomly replaced
multipliers) and scores them exactly with `mixlab exact --list` (the inverse
mixer f^-1 with `--inverse`); this script only does the statistics.
bias = 1000*sqrt(mean_{j,k} ((count_jk - n/2)/(n/2))^2), exact over all 2^w inputs.

Usage: python analysis/symmetry_check.py 8 [N]          # all 5.6M 8-bit configs, or a random N
       python analysis/symmetry_check.py 16 N [SEED]    # N random multiplier pairs at [8 _ 7 _ 9]
Needs harness/target/release/mixlab (cargo build --release).
"""
import itertools, os, subprocess, sys, tempfile, time
from pathlib import Path
import numpy as np

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "harness" / "target" / "release" / ("mixlab.exe" if os.name == "nt" else "mixlab")


def spec(p):
    return f"[{p[0]} {p[1]:x} {p[2]} {p[3]:x} {p[4]}]"


def mixlab_exact(params, w, inverse=False):
    """Exact bias per row of params (C, 5) via `mixlab exact --list`."""
    with tempfile.TemporaryDirectory() as d:
        lst, out = Path(d) / "specs.txt", Path(d) / "scores.csv"
        lst.write_text("\n".join(spec(p) for p in params.tolist()) + "\n")
        cmd = [str(BIN), "exact", "--bits", str(w), "--list", str(lst), "--out", str(out)]
        subprocess.run(cmd + (["--inverse"] if inverse else []), check=True)
        with open(out) as f:
            next(f)
            return np.array([float(line.split(",")[1]) for line in f])


def rank(a):
    # average ranks for ties
    order = np.argsort(a, kind="mergesort")
    r = np.empty(len(a))
    r[order] = np.arange(len(a))
    sa = a[order]
    # tie handling
    _, idx, cnt = np.unique(sa, return_index=True, return_counts=True)
    for i, c in zip(idx, cnt):
        if c > 1:
            r[order[i:i + c]] = i + (c - 1) / 2
    return r


def spearman(a, b):
    ra, rb = rank(a), rank(b)
    return np.corrcoef(ra, rb)[0, 1]


def variants(params, w, batch):
    """Variant parameter sets keyed by name ("inverse" scores f^-1 of params).
    Random replacements are drawn per batch of `batch` configs from
    default_rng(7), so recorded runs (batch 20000 at 8 bits, 64 at 16) reproduce."""
    N = 1 << w
    V = {}
    for key, cols in [("neg_m0", [1]), ("neg_m1", [3]), ("neg_both", [1, 3])]:
        Q = params.copy()
        for c in cols:
            Q[:, c] = (N - Q[:, c]) % N
        V[key] = Q
    for key, col in [("topflip_m0", 1), ("topflip_m1", 3)]:
        Q = params.copy(); Q[:, col] ^= N >> 1
        V[key] = Q
    rng = np.random.default_rng(7)
    r0, rb = params.copy(), params.copy()
    for a in range(0, len(params), batch):
        k = len(params[a:a + batch])
        r0[a:a + batch, 1] = rb[a:a + batch, 1] = rng.integers(0, N // 2, k) * 2 + 1
        rb[a:a + batch, 3] = rng.integers(0, N // 2, k) * 2 + 1
    V["rand_m0"], V["rand_both"] = r0, rb
    return V


def evaluate(params, w, batch):
    """Return bias for f, its variants, and f^{-1} (one mixlab call each way)."""
    V = variants(params, w, batch)
    keys = ["f"] + list(V)
    fwd = mixlab_exact(np.concatenate([params] + list(V.values())), w)
    out = dict(zip(keys, np.split(fwd, len(keys))))
    out["inverse"] = mixlab_exact(params, w, inverse=True)
    order = ["f", "neg_m0", "neg_m1", "neg_both", "inverse", "topflip_m0", "topflip_m1", "rand_m0", "rand_both"]
    return {k: out[k] for k in order}


def within_shift_spearman(params, f, g):
    key = params[:, 0] * 10000 + params[:, 2] * 100 + params[:, 4]
    rs, ws = [], []
    for k in np.unique(key):
        sel = key == k
        if sel.sum() >= 15:
            rs.append(spearman(f[sel], g[sel])); ws.append(sel.sum())
    return np.average(rs, weights=ws)


def report(out, label, params):
    f = out["f"]
    n = len(f)
    print(f"\n== {label}: {n} configs")
    top = f <= np.quantile(f, 0.01)
    for k in [k for k in out if k != "f"]:
        g = out[k]
        rel = np.abs(g - f) / f
        exact = np.isclose(g, f, rtol=1e-12, atol=0).mean()
        topg = g <= np.quantile(g, 0.01)
        print(f"{k:9} spearman={spearman(f, g):.4f}  exact-equal={exact:.4f}  "
              f"median|rel diff|={np.median(rel):.3f}  top1%-overlap={(top & topg).sum() / top.sum():.3f}  "
              f"top1% median|rel|={np.median(rel[top]):.3f}  within-shift spearman={within_shift_spearman(params, f, g):.4f}")


if __name__ == "__main__":
    w = int(sys.argv[1])
    t = time.time()
    if w == 8:
        sh = range(1, 8)
        ms = range(1, 256, 2)
        params = np.array([(a, m0, b, m1, c) for a, b, c in itertools.product(sh, sh, sh)
                           for m0 in ms for m1 in ms], dtype=np.int64)
        if len(sys.argv) > 2:
            rng = np.random.default_rng(1)
            params = params[rng.choice(len(params), int(sys.argv[2]), replace=False)]
        out = evaluate(params, 8, 20000)
    else:
        rng = np.random.default_rng(int(sys.argv[3]) if len(sys.argv) > 3 else 1)
        C = int(sys.argv[2])
        sh = np.tile([8, 7, 9], (C, 1))
        m = rng.integers(0, 1 << 15, size=(C, 2)) * 2 + 1
        params = np.column_stack([sh[:, 0], m[:, 0], sh[:, 1], m[:, 1], sh[:, 2]])
        out = evaluate(params, 16, 64)
    report(out, f"{w}-bit", params)
    np.savez(f"sym{w}.npz", params=params, **out)
    print(f"[{time.time() - t:.0f}s]")
