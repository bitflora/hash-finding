"""Symmetry check for 2-round xmxmx mixers: does bias survive m -> -m, or inversion?

bias = 1000*sqrt(mean_{j,k} ((count_jk - n/2)/(n/2))^2), exact over all 2^w inputs
(matches mixlab / hash-prospector).
"""
import sys, itertools, time
import numpy as np


def mix(x, s0, m0, s1, m1, s2, mask):
    # x: uint64 array (configs, inputs); params: uint64 column vectors (configs, 1)
    x = x ^ (x >> s0)
    x = (x * m0) & mask
    x = x ^ (x >> s1)
    x = (x * m1) & mask
    x = x ^ (x >> s2)
    return x


def bias_of_tables(T, w):
    """T: (C, 2^w) permutation tables -> exact bias per row."""
    n = 1 << w
    xs = np.arange(n, dtype=np.int64)
    ms = np.zeros(T.shape[0])
    nb = (w + 7) // 8
    for j in range(w):
        d = (T ^ T[:, xs ^ (1 << j)]).astype("<u8")
        bits = np.unpackbits(d.view(np.uint8).reshape(T.shape[0], n, 8)[:, :, :nb],
                             axis=2, bitorder="little")
        cnt = bits.sum(axis=1, dtype=np.int64)  # (C, 8*nb)
        diff = (cnt[:, :w] - n / 2) / (n / 2)
        ms += (diff * diff).sum(axis=1)
    return 1000 * np.sqrt(ms / (w * w))


def tables(params, w):
    mask = np.uint64((1 << w) - 1)
    p = params.astype(np.uint64)
    x = np.arange(1 << w, dtype=np.uint64)[None, :]
    col = lambda i: p[:, i:i + 1]
    return mix(x, col(0), col(1), col(2), col(3), col(4), mask).astype(np.int64)


def inverse_tables(T):
    inv = np.empty_like(T)
    rows = np.arange(T.shape[0])[:, None]
    inv[rows, T] = np.arange(T.shape[1])[None, :]
    return inv


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


def evaluate(params, w, batch):
    """Return bias for f, f with -m0, -m1, -both, and f^{-1}."""
    N = 1 << w
    keys = ["f", "neg_m0", "neg_m1", "neg_both", "inverse", "topflip_m0", "topflip_m1", "rand_m0", "rand_both"]
    out = {k: np.empty(len(params)) for k in keys}
    rng = np.random.default_rng(7)
    for a in range(0, len(params), batch):
        P = params[a:a + batch].copy()
        T = tables(P, w)
        out["f"][a:a + batch] = bias_of_tables(T, w)
        out["inverse"][a:a + batch] = bias_of_tables(inverse_tables(T), w)
        for key, cols in [("neg_m0", [1]), ("neg_m1", [3]), ("neg_both", [1, 3])]:
            Q = P.copy()
            for c in cols:
                Q[:, c] = (N - Q[:, c]) % N
            out[key][a:a + batch] = bias_of_tables(tables(Q, w), w)
        for key, col in [("topflip_m0", 1), ("topflip_m1", 3)]:
            Q = P.copy(); Q[:, col] ^= N >> 1
            out[key][a:a + batch] = bias_of_tables(tables(Q, w), w)
        Q = P.copy(); Q[:, 1] = rng.integers(0, N // 2, len(Q)) * 2 + 1
        out["rand_m0"][a:a + batch] = bias_of_tables(tables(Q, w), w)
        Q[:, 3] = rng.integers(0, N // 2, len(Q)) * 2 + 1
        out["rand_both"][a:a + batch] = bias_of_tables(tables(Q, w), w)
    return out


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
        # validate against mixlab
        chk = evaluate(np.array([[2, 3, 3, 5, 4]]), 8, 1)["f"][0]
        assert abs(chk - 178.75095607261798) < 1e-9, chk
        sh = range(1, 8)
        ms = range(1, 256, 2)
        params = np.array([(a, m0, b, m1, c) for a, b, c in itertools.product(sh, sh, sh)
                           for m0 in ms for m1 in ms], dtype=np.int64)
        if len(sys.argv) > 2:
            rng = np.random.default_rng(1)
            params = params[rng.choice(len(params), int(sys.argv[2]), replace=False)]
        out = evaluate(params, 8, 20000)
    else:
        chk = evaluate(np.array([[8, 0xa3d3, 7, 0x4b2d, 9]]), 16, 1)["f"][0]
        assert abs(chk - 7.25293839370535753) < 1e-9, chk
        rng = np.random.default_rng(int(sys.argv[3]) if len(sys.argv) > 3 else 1)
        C = int(sys.argv[2])
        sh = np.tile([8, 7, 9], (C, 1))
        m = rng.integers(0, 1 << 15, size=(C, 2)) * 2 + 1
        params = np.column_stack([sh[:, 0], m[:, 0], sh[:, 1], m[:, 1], sh[:, 2]])
        # add some known-good 16-bit points
        out = evaluate(params, 16, 64)
    report(out, f"{w}-bit", params)
    np.savez(f"sym{w}.npz", params=params, **out)
    print(f"[{time.time() - t:.0f}s]")
