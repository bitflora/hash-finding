"""Collect every exactly-scored 2-round 32-bit xmx mixer found under results/.

Writes
  results/explored32.csv        one row per distinct spec (exact bias only)
  results/explored32_edges.csv  race lineage: incumbent -> accepted successor
  viz/explored32.json           compact copy for viz/explorer.html (with --json)

What counts as exact
  * CSV columns named exact_bias / exact / true_ms, or bias in files that are
    written from `mixlab eval --exact` (draws, *exact*, parents, ...).
  * seed-search *.verified.csv rows with stage=exact.
  * Race logs (iter,event,config,score) whose RaceCfg has bits 32 and
    verify_samples None: there the start score and each accepted score are
    exact (racing.rs). Their 10-digit values are kept only if no more precise
    value exists.
  * Race tables (*.race.txt, negation_races/*.txt): the incumbent ms and the
    'verified' column.
  * Text lines '[spec]  exact  bias=...'.
Zero / placeholder scores and any sampled-only score are skipped.

Usage: python analysis/collect_explored32.py [--json]
"""
import csv
import json
import math
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RES = ROOT / "results"
SPEC = re.compile(r"\[(\d+) ([0-9a-f]{8}) (\d+) ([0-9a-f]{8}) (\d+)\]")
CFG32_EXACT = re.compile(r"bits: 32\b.*verify_samples: None")

# 'bias' columns in these files come from sampled estimates, not exact evals.
SAMPLED_BIAS_HINTS = ("screen",)

ORIGINS = [
    ("Published", lambda s: s.startswith("published_2round32")),
    ("Random high halves", lambda s: s.startswith("core_controls/") or s.endswith(".draws.csv")),
    ("Crossover", lambda s: s.startswith("ironborn_ideas/")),
    ("Campaign races", lambda s: s.startswith("campaign/")),
    ("Negation races", lambda s: s.startswith("negation_races/")),
    ("Seed search", lambda s: s.startswith("seed_search")),
    ("Probes", lambda s: s.startswith("distance_profile/") or s.startswith("atlas_untried_crosses")),
    ("Mid-distance races", lambda s: s.startswith("midstart_f30/")),
    ("Other races", lambda s: True),
]

rows = {}   # spec ->{"bias": float, "digits": int, "sources": set}
edges = []  # (from, to, source)
conflicts = []


def ms_to_score(ms):
    return 1000.0 * math.sqrt(max(ms, 0.0))


def norm(spec):
    m = SPEC.search(spec)
    return f"[{m[1]} {m[2]} {m[3]} {m[4]} {m[5]}]" if m else None


def add(spec, bias, digits, source):
    spec = norm(spec)
    if spec is None or not (bias > 0) or math.isnan(bias):
        return None
    r = rows.get(spec)
    if r is None:
        rows[spec] = {"bias": bias, "digits": digits, "sources": {source}}
    else:
        tol = 10 ** -(min(digits, r["digits"]) - 1)
        if abs(r["bias"] - bias) > max(tol, 1e-9) * max(1.0, bias):
            conflicts.append((spec, r["bias"], bias, source))
        if digits > r["digits"]:
            r["bias"], r["digits"] = bias, digits
        r["sources"].add(source)
    return spec


def sig_digits(text):
    t = text.strip().lower().split("e")[0].lstrip("-+0.")
    return len(t.replace(".", ""))


def src_name(path):
    return path.relative_to(RES).as_posix()


def read_race_log(path, lines, cfg):
    if not CFG32_EXACT.search(cfg):
        return
    src = src_name(path)
    prev = None
    for row in csv.DictReader(lines):
        if row["event"] not in ("start", "improved"):
            continue
        s = add(row["config"], float(row["score"]), 10, src)
        if s and prev and s != prev:
            edges.append((prev, s, src))
        prev = s or prev


def read_table(path, lines):
    rdr = csv.DictReader(lines)
    cols = rdr.fieldnames or []
    spec_col = next((c for c in ("spec", "config") if c in cols), None)
    if spec_col is None:
        return
    name = path.name.lower()
    if "exact_bias" in cols:
        score, conv = "exact_bias", None
    elif "exact" in cols:
        score, conv = "exact", None
    elif "true_ms" in cols:
        score, conv = "true_ms", ms_to_score
    elif "bias" in cols and not any(h in name for h in SAMPLED_BIAS_HINTS):
        score, conv = "bias", None
    elif "ms" in cols and "stage" in cols:
        score, conv = "ms", ms_to_score
    else:
        return
    src = src_name(path)
    for row in rdr:
        if score == "ms" and row.get("stage") != "exact":
            continue
        try:
            v = float(row[score])
        except (TypeError, ValueError):
            continue
        digits = 7 if conv else sig_digits(row[score])
        add(row[spec_col], conv(v) if conv else v, digits, src)


def read_csv(path):
    text = path.read_bytes().decode(errors="replace").replace(chr(13), "").splitlines()
    cfg = " ".join(l for l in text if l.startswith("#"))
    lines = [l for l in text if not l.startswith("#")]
    if not lines:
        return
    if lines[0].startswith("iter,event,config,score"):
        read_race_log(path, lines, cfg)
    else:
        read_table(path, lines)


EXACT_LINE = re.compile(r"^\s*(\[[^\]]+\])\s+exact\s+bias=([0-9.eE+-]+)")
INCUMBENT = re.compile(r"^incumbent (\[[^\]]+\]) ms=([0-9.eE+-]+)")
RACE_ROW = re.compile(r"^(\[[^\]]+\])\s+\w+\s+\S+\s+\S+\s+([0-9.eE+-]+)\s*$")


def read_txt(path):
    text = path.read_bytes().decode(errors="replace").replace(chr(13), "").splitlines()
    src = src_name(path)
    exact_race = any(CFG32_EXACT.search(l) for l in text[:3])
    for l in text:
        m = EXACT_LINE.match(l)
        if m:
            add(m[1], float(m[2]), sig_digits(m[2]), src)
            continue
        if not exact_race:
            continue
        m = INCUMBENT.match(l) or RACE_ROW.match(l)
        if m and m[2] != "-":
            add(m[1], ms_to_score(float(m[2])), 5, src)


def screened_core_pairs():
    seen = set()
    for p in RES.rglob("*"):
        if p.suffix in (".csv", ".txt", ".log") and not p.name.startswith("explored32"):
            for m in SPEC.finditer(p.read_bytes().decode(errors="replace")):
                seen.add((m[1], m[2], m[3], m[4], m[5]))
    out = {}
    for s0, m0, s1, m1, s2 in seen:
        if f"[{s0} {m0} {s1} {m1} {s2}]" in rows:
            continue
        out.setdefault(f"{s0},{s1},{s2}", set()).add((int(m0, 16) & 0xFFFF, int(m1, 16) & 0xFFFF))
    return {t: sorted(v) for t, v in out.items()}


def main():
    for p in sorted(RES.rglob("*")):
        if p.suffix == ".csv" and not p.name.startswith("explored32"):
            read_csv(p)
        elif p.suffix == ".txt":
            read_txt(p)

    out = sorted(rows.items(), key=lambda kv: kv[1]["bias"])
    with open(RES / "explored32.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["spec", "s0", "m0", "s1", "m1", "s2", "bias", "n_sources", "sources"])
        for spec, r in out:
            s0, m0, s1, m1, s2 = SPEC.search(spec).groups()
            w.writerow([spec, s0, m0, s1, m1, s2, repr(r["bias"]), len(r["sources"]),
                        ";".join(sorted(r["sources"]))])
    seen = set()
    uniq = [e for e in edges if (e[0], e[1]) not in seen and not seen.add((e[0], e[1]))]
    with open(RES / "explored32_edges.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["from", "to", "source"])
        w.writerows(uniq)

    print(f"{len(out)} exact specs, {len(uniq)} lineage edges, {len(conflicts)} conflicts")
    for c in conflicts[:10]:
        print("  conflict:", c)

    if "--json" in sys.argv:
        # Compact form for the explorer page: one array per mixer.
        idx = {spec: i for i, (spec, _) in enumerate(out)}

        def origin(sources):
            # One origin per mixer, by priority, for colouring.
            for i, (_, test) in enumerate(ORIGINS):
                if any(test(s) for s in sources):
                    return i
            return len(ORIGINS) - 1

        data = {
            "origins": [name for name, _ in ORIGINS],
            "mixers": [
                [int(m[1]), m[2], int(m[3]), m[4], int(m[5]), round(r["bias"], 12),
                 origin(r["sources"]), len(r["sources"])]
                for spec, r in out for m in [SPEC.search(spec)]
            ],
            "edges": [[idx[a], idx[b]] for a, b, _ in uniq],
            # core pairs (raw low 16 bits of m0, m1) seen only with sampled
            # scores, per shift template: screened but never scored exactly
            "screened": screened_core_pairs(),
        }
        (ROOT / "viz").mkdir(exist_ok=True)
        (ROOT / "viz" / "explored32.json").write_text(json.dumps(data, separators=(",", ":")))
        print("wrote viz/explored32.json")


if __name__ == "__main__":
    main()
