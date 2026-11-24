"""Pausable, resumable search campaign (2026-11-20).

Two campaigns share one job queue, interleaved so every chunk advances both:
  frontier  BIC-constrained races (`race --max-bic-corr C`) at several
            (C, s2) levels from four families, two seeds each (FINDINGS F18).
  multi     multi-start: for each distinct published low-16-bit core (and
            8 random-core controls), exactly score 8 random high halves,
            then race the best draw (FINDINGS F16/F19).

Usage (from mixing-functions/):
  python analysis/campaign.py init              # write results/campaign/jobs.csv (once)
  python analysis/campaign.py run [--hours H] [--jobs N]
  python analysis/campaign.py pause             # finish the current job, then exit
  python analysis/campaign.py stop              # kill the current job now; it resumes later
  python analysis/campaign.py status            # progress, runner state
  python analysis/campaign.py summary           # results tables (adds BIC for finished jobs)

State lives in results/campaign/: jobs.csv, one <id>.* file set per job, and
<id>.done.json when a job is finished. Deleting PAUSE/STOP is done by `run`.
A stopped race restarts from its last incumbent (logged as <id>.partK.log);
the result is still an exactly verified score, but the trajectory differs
from an uninterrupted run, so the done record lists its parts.
"""
import csv, json, os, random, re, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
D = Path(os.environ.get("CAMPAIGN_DIR", ROOT / "results" / "campaign"))
BIN = ROOT / "harness" / "target" / "release" / ("mixlab.exe" if os.name == "nt" else "mixlab")
JOBS = D / "jobs.csv"
LOCK, PAUSE, STOP = D / "runner.lock", D / "PAUSE", D / "STOP"
ITERS = int(os.environ.get("CAMPAIGN_ITERS", 25))
PATIENCE = 6
NEIGH = int(os.environ.get("CAMPAIGN_NEIGH", 96))
DRAWS = int(os.environ.get("CAMPAIGN_DRAWS", 8))
EXTRA = os.environ.get("CAMPAIGN_EXTRA", "").split()  # test hook, e.g. "--cap p20"
LINE = re.compile(r"\]\s*(\d+),(start|improved|iter|patience|exhausted),(\[[^\]]*\]),([0-9.]+)")


def log(msg):
    line = f"[{time.strftime('%F %T')}] {msg}"
    print(line, flush=True)
    with open(D / "campaign.log", "a") as f:
        f.write(line + "\n")


# ---------------------------------------------------------------- job list
FAMILIES = {  # current best (s0, m0, s1, m1) per family
    "7870": (16, "7870aaad", 15, "be523597"),
    "record": (16, "21f0aaad", 15, "735a2d97"),
    "d303w": (16, "702f32d3", 15, "e6134975"),
    "lb32w": (16, "7803352d", 15, "49eb26cb"),
}
# (C, starting s2). Every start must already satisfy C: a race keeps an over-cap
# start as its incumbent, and when the cap filters out its neighbours it never
# moves (s2=15 starts have worst |corr| 0.34-0.50, so the 0.25 level starts at 14).
LEVELS = [(0.05, 12), (0.11, 13), (0.15, 14), (0.25, 14)]
DONE_CORES = {  # cores already raced (F14, F16, F19)
    (0xaaad, 0x2d97), (0xaaad, 0x3597), (0x32d3, 0xc965), (0x2d35, 0x49a9),
    (0x352d, 0xa68b), (0x5565, 0x6b47), (0xca6b, 0xae35),
}


def init():
    D.mkdir(parents=True, exist_ok=True)
    if JOBS.exists():
        sys.exit(f"{JOBS} exists; delete it to regenerate")
    frontier = []
    for seed in (1, 2):
        for c, s2 in LEVELS:
            for fam, (s0, m0, s1, m1) in FAMILIES.items():
                frontier.append(dict(id=f"F_c{int(c*100):02d}_{fam}_s{seed}", campaign="frontier", kind="race",
                                     start=f"[{s0} {m0} {s1} {m1} {s2}]", seed=seed,
                                     args=f"--max-bic-corr {c} --max-s2 16"))
    pub = list(csv.DictReader(open(ROOT / "results" / "published_2round32.csv", newline="")))
    cores = {}
    for r in pub:
        s0, m0, s1, m1, s2 = r["spec"].strip("[]").split()
        key = (int(s0), int(s1), int(m0, 16) & 0xFFFF, int(m1, 16) & 0xFFFF)
        b = float(r["exact_bias"])
        if key not in cores or b < cores[key][0]:
            cores[key] = (b, min(int(s2), 15))
    rng = random.Random(20261120)
    multi = []
    hd = lambda a, b: bin(a[0] ^ b[0]).count("1") + bin(a[1] ^ b[1]).count("1")
    kept = []  # one core per cluster: skip cores within Hamming distance 4 of a raced or kept core
    for (s0, s1, l0, l1), (b, s2) in sorted(cores.items(), key=lambda kv: kv[1][0]):
        if any(hd((l0, l1), d) <= 4 for d in list(DONE_CORES) + kept):
            continue
        kept.append((l0, l1))
        multi.append(dict(id=f"M_{s0}_{s1}_{l0:04x}_{l1:04x}", campaign="multi", kind="core",
                          start=f"{s0} {l0:04x} {s1} {l1:04x} {s2}", seed=61, args="--max-s2 16"))
    for i in range(8):  # random-core controls, interleaved through the list
        l0, l1 = rng.getrandbits(16) | 1, rng.getrandbits(16) | 1
        multi.insert(min(len(multi), 4 + i * 8), dict(id=f"M_rand{i}_{l0:04x}_{l1:04x}", campaign="multi", kind="core",
                     start=f"16 {l0:04x} 15 {l1:04x} 15", seed=61, args="--max-s2 16"))
    jobs = []
    while frontier or multi:  # interleave 1:1
        if frontier: jobs.append(frontier.pop(0))
        if multi: jobs.append(multi.pop(0))
    with open(JOBS, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["id", "campaign", "kind", "start", "seed", "args"])
        w.writeheader(); w.writerows(jobs)
    print(f"wrote {len(jobs)} jobs to {JOBS}")


def load_jobs():
    return list(csv.DictReader(open(JOBS, newline="")))


def done_rec(j):
    p = D / f"{j['id']}.done.json"
    return json.loads(p.read_text()) if p.exists() else None


# ---------------------------------------------------------------- running
class Stopped(Exception):
    pass


def heartbeat():
    LOCK.write_text(json.dumps({"pid": os.getpid(), "t": time.time()}))


def run_proc(args, out_path):
    """Run mixlab; poll for STOP. Returns stdout text (also written to out_path)."""
    with open(out_path, "w") as out:
        p = subprocess.Popen([str(BIN)] + args, stdout=out, stderr=subprocess.STDOUT, cwd=ROOT / "harness")
        while p.poll() is None:
            heartbeat()
            if STOP.exists():
                p.terminate(); p.wait()
                raise Stopped()
            time.sleep(5)
    return Path(out_path).read_text()


def exact(spec):
    out = subprocess.run([str(BIN), "eval", "--bits", "32", "--spec", spec, "--exact"],
                         capture_output=True, text=True, cwd=ROOT / "harness").stdout
    return float(re.search(r"bias=([0-9.]+)", out).group(1))


def screen_core(j):
    """Exact-score DRAWS random high halves for a core; resumable per draw."""
    s0, l0, s1, l1, s2 = j["start"].split()
    rng = random.Random(f"{j['id']}-highs")
    draws = D / f"{j['id']}.draws.csv"
    have = {}
    if draws.exists():
        for r in csv.DictReader(open(draws, newline="")):
            have[r["spec"]] = float(r["bias"])
    for _ in range(DRAWS):
        m0 = rng.getrandbits(16) << 16 | int(l0, 16)
        m1 = rng.getrandbits(16) << 16 | int(l1, 16)
        spec = f"[{s0} {m0:08x} {s1} {m1:08x} {s2}]"
        if spec in have:
            continue
        if STOP.exists():
            raise Stopped()
        heartbeat()
        have[spec] = exact(spec)
        new = not draws.exists()
        with open(draws, "a", newline="") as f:
            if new: f.write("spec,bias\n")
            f.write(f"{spec},{have[spec]!r}\n")
    return min(have.items(), key=lambda kv: kv[1])


def race_job(j, start):
    """Race with resume: a log without a 'best' line is a stopped part."""
    log_p = D / f"{j['id']}.log"
    part_number = lambda p: int(p.stem.rsplit(".part", 1)[1])
    parts = sorted(D.glob(f"{j['id']}.part*.log"), key=part_number)
    used_iters = 0
    for p in parts:
        hits = list(LINE.finditer(p.read_text()))
        if hits:
            start = hits[-1].group(3)
            used_iters += max(int(m.group(1)) for m in hits)
    if log_p.exists():
        text = log_p.read_text()
        # A crash after the process finished must not discard its result.
        best = re.search(r"^best (\[[^\]]*\])\s+score ([0-9.]+)", text, re.M)
        if best:
            return best.group(1), float(best.group(2)), [p.name for p in parts]
        k = max((part_number(p) for p in parts), default=0) + 1
        log_p.rename(D / f"{j['id']}.part{k}.log")
        hits = list(LINE.finditer(text))
        if hits:
            start = hits[-1].group(3)  # last incumbent
            used_iters += int(hits[-1].group(1))
        parts.append(D / f"{j['id']}.part{k}.log")
    iters = max(ITERS - used_iters, 1)
    seed = int(j["seed"]) + 1000 * len(parts)  # fresh neighbour stream after a resume
    args = ["race", "--bits", "32", "--start", start, "--iters", str(iters), "--patience", str(PATIENCE),
            "--neighbors", str(NEIGH), "--seed", str(seed), "--out", str(D / f"{j['id']}.csv")] + j["args"].split() + EXTRA
    text = run_proc(args, log_p)
    m = re.search(r"^best (\[[^\]]*\])\s+score ([0-9.]+)", text, re.M)
    return m.group(1), float(m.group(2)), [p.name for p in parts]


def run(hours, max_jobs):
    D.mkdir(parents=True, exist_ok=True)
    if LOCK.exists():
        st = json.loads(LOCK.read_text())
        if time.time() - st["t"] < 60:
            sys.exit(f"another runner is active (pid {st['pid']}); not starting")
    for f in (PAUSE, STOP):
        f.unlink(missing_ok=True)
    heartbeat()
    t0, n = time.time(), 0
    log(f"runner start (pid {os.getpid()}, hours={hours}, jobs={max_jobs})")
    try:
        for j in load_jobs():
            if done_rec(j):
                continue
            if PAUSE.exists():
                log("paused (PAUSE file)"); return
            if hours is not None and time.time() - t0 > hours * 3600:
                log(f"chunk limit of {hours} h reached"); return
            if max_jobs is not None and n >= max_jobs:
                log(f"chunk limit of {max_jobs} jobs reached"); return
            log(f"start {j['id']} ({j['campaign']}, {j['start']} {j['args']})")
            t = time.time()
            rec = {"id": j["id"], "campaign": j["campaign"], "args": j["args"]}
            start = j["start"]
            if j["kind"] == "core":
                start, b = screen_core(j)
                rec["core_best_draw"], rec["core_best_draw_bias"] = start, b
            rec["start"] = start
            rec["best"], rec["bias"], rec["parts"] = race_job(j, start)
            # the race logs its start score (exact); the first part has the original start
            first = D / (rec["parts"][0] if rec["parts"] else f"{j['id']}.log")
            m0 = LINE.search(first.read_text())
            rec["start_bias"] = float(m0.group(4)) if m0 else exact(start)
            rec["seconds"] = round(time.time() - t)
            (D / f"{j['id']}.done.json").write_text(json.dumps(rec, indent=1))
            n += 1
            log(f"done {j['id']}: {rec['best']} {rec['bias']:.10f} ({rec['seconds']} s)")
        log("all jobs done")
    except Stopped:
        log("stopped (STOP file); the current job resumes on the next run")
    finally:
        LOCK.unlink(missing_ok=True)


# ---------------------------------------------------------------- reporting
def status():
    jobs = load_jobs()
    done = [j for j in jobs if done_rec(j)]
    for c in ("frontier", "multi"):
        a = [j for j in jobs if j["campaign"] == c]
        print(f"{c:<9} {sum(1 for j in a if done_rec(j)):3d}/{len(a)} done")
    if LOCK.exists():
        st = json.loads(LOCK.read_text())
        age = time.time() - st["t"]
        print(f"runner pid {st['pid']} {'active' if age < 60 else 'STALE'} (heartbeat {age:.0f} s ago)")
    else:
        print("runner: not running")
    for f in (PAUSE, STOP):
        if f.exists(): print(f"{f.name} flag set")
    secs = [done_rec(j)["seconds"] for j in done]
    if secs:
        left = len(jobs) - len(done)
        print(f"mean job {sum(secs)/len(secs)/60:.0f} min; ~{left*sum(secs)/len(secs)/3600:.1f} h left")


def bic(spec):
    out = subprocess.run([str(BIN), "bic", "--bits", "32", "--spec", spec, "--samples", "p20", "--seeds", "1"],
                         capture_output=True, text=True, cwd=ROOT / "harness").stdout
    m = re.search(r"rms z=([0-9.]+).*max\|z\|=([0-9.]+).*corr ([+-][0-9.]+)", out)
    return float(m.group(1)), abs(float(m.group(3)))


def cap(job_id):
    return int(re.match(r"F_c(\d+)_", job_id).group(1)) / 100


def summary():
    recs = [done_rec(j) for j in load_jobs()]
    recs = [r for r in recs if r]
    for r in recs:
        if "bic_rms_z" not in r:
            r["bic_rms_z"], r["bic_worst_corr"] = bic(r["best"])
            (D / f"{r['id']}.done.json").write_text(json.dumps(r, indent=1))
    for c in ("frontier", "multi"):
        rs = sorted((r for r in recs if r["campaign"] == c), key=lambda r: r["bias"])
        print(f"\n== {c} ({len(rs)} done)")
        print(f"{'id':<28} {'start':>8} {'best spec':<30} {'bias':>9} {'worst':>6} {'rmsz':>5} {'min':>4}")
        for r in rs:
            print(f"{r['id']:<28} {r['start_bias']:8.4f} {r['best']:<30} {r['bias']:9.5f} "
                  f"{r['bic_worst_corr']:6.3f} {r['bic_rms_z']:5.2f} {r['seconds']/60:4.0f}"
                  + ("  (resumed)" if r["parts"] else "")
                  + ("  OVER CAP" if c == "frontier" and r["bic_worst_corr"] > cap(r["id"]) else ""))


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "status"
    opt = lambda k: sys.argv[sys.argv.index(k) + 1] if k in sys.argv else None
    if cmd == "init": init()
    elif cmd == "run": run(float(opt("--hours")) if opt("--hours") else None, int(opt("--jobs")) if opt("--jobs") else None)
    elif cmd == "pause": D.mkdir(parents=True, exist_ok=True); PAUSE.touch(); print("PAUSE set: the runner exits after the current job")
    elif cmd == "stop": D.mkdir(parents=True, exist_ok=True); STOP.touch(); print("STOP set: the runner kills the current job within ~5 s")
    elif cmd == "status": status()
    elif cmd == "summary": summary()
    else: sys.exit(__doc__)
