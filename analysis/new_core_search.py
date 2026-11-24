"""Search new 32-bit low-16-bit cores; all scoring stays in mixlab.

Usage: python analysis/new_core_search.py generate|screen|exact|bounded|verify|summary
Results are resumable under results/new_cores_2026_11_24/.
The optional refine action reproduces the larger adaptive-race pilot budget.
"""
import collections
import csv
import importlib.util
import json
import random
import re
import statistics
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'results' / 'new_cores_2026_11_24'
BIN = ROOT / 'harness' / 'target' / 'release' / 'mixlab.exe'
SCREEN = BIN.parent / 'examples' / 'screen_list.exe'
SEED = 20261124


def read(path):
    return list(csv.DictReader(path.open(newline=''))) if path.exists() else []


def write(path, fields, rows):
    with path.open('w', newline='') as f:
        w = csv.DictWriter(f, fieldnames=fields)
        w.writeheader()
        w.writerows(rows)


def core(spec):
    p = spec.strip('[]').split()
    return tuple(min(v, -v % 65536) for v in (int(p[1], 16) & 65535, int(p[3], 16) & 65535))


def run(args):
    return subprocess.run([str(BIN), *map(str, args)], check=True, capture_output=True,
                          text=True, encoding='utf-8').stdout


def generate():
    OUT.mkdir(exist_ok=True)
    if (OUT / 'candidates.csv').exists():
        print('Existing frozen candidates retained.', flush=True)
        return
    loader = importlib.util.spec_from_file_location('collect', ROOT / 'analysis' / 'collect_explored32.py')
    c = importlib.util.module_from_spec(loader)
    loader.loader.exec_module(c)
    for p in sorted(c.RES.rglob('*')):
        if OUT in p.parents:
            continue
        if p.suffix == '.csv' and not p.name.startswith('explored32'):
            c.read_csv(p)
        elif p.suffix == '.txt':
            c.read_txt(p)
    known = {core(s) for s in c.rows}
    rng = random.Random(SEED)
    pairs, used = [], set()

    def put(group, a, b, **extra):
        key = core(f'[16 {a:08x} 15 {b:08x} 15]')
        if key in used or (group != 'known' and key in known):
            return False
        used.add(key)
        pairs.append(dict(group=group, a=a, b=b, **extra))
        return True

    partners = [0x2d97, 0x3597, 0x34d7]
    for q in (3, 5, 7, 9, 17):
        residuals = [sign * r for r in range(1, 16, 2) for sign in (1, -1)]
        residuals += rng.sample([r for r in range(-1023, 1024, 2) if r not in residuals], 16)
        for r in residuals:
            a = pow(q, -1, 65536) * r % 65536
            for b in partners:
                put(f'rational_q{q}', a, b, q=q, residual=r)
    for a in (0x3335, 0x32d3, 0xcd35):
        for upper in range(32):
            b = (upper << 11) | 0x597
            for sign in (1, -1):
                put('shared_low11', a, sign * b % 65536)
    while sum(p['group'] == 'random' for p in pairs) < 96:
        a = rng.randrange(1, 65536, 2)
        for b in partners:
            put('random', a, b)
    for a, b in ((0x3335, 0x2d97), (0x32d3, 0x3597), (0xcd35, 0x3597),
                 (0xcd35, 0x34d7), (0xaaad, 0x2d97), (0x3535, 0x34d7)):
        put('known', a, b)
    # Two elite high-half pairs plus one common random pair, paired across every core.
    extensions = [(0x9c01, 0x335a), (0xe003, 0x549a), (rng.randrange(65536), rng.randrange(65536))]
    rows = []
    for p in pairs:
        for draw, (ha, hb) in enumerate(extensions):
            a, b = ha << 16 | p['a'], hb << 16 | p['b']
            rows.append(dict(spec=f'[16 {a:08x} 15 {b:08x} 15]', group=p['group'],
                             core0=f"{p['a']:04x}", core1=f"{p['b']:04x}", draw=draw,
                             q=p.get('q', ''), residual=p.get('residual', '')))
    write(OUT / 'candidates.csv', ['spec', 'group', 'core0', 'core1', 'draw', 'q', 'residual'], rows)
    (OUT / 'manifest.json').write_text(json.dumps(dict(seed=SEED, shifts=[16, 15, 15],
        known_exact_specs=len(c.rows), known_canonical_core_pairs=len(known),
        known_pairs_snapshot=sorted(known), extensions=extensions,
        groups=dict(collections.Counter(p['group'] for p in pairs)),
        coarse_samples=2**22, coarse_seed=2401, fine_samples=2**26, fine_seed=2402,
        selection='Best eight distinct canonical core pairs per group; best coarse extension per pair. '
                  'Exact best two per group plus best eight overall by independent fine screen.',
        novelty='Core pair and all independent modular sign variants absent from prior exact dataset; '
                'does not assert absence from sampled or generated candidates.'), indent=2))
    print(f'{len(pairs)} core pairs, {len(rows)} configurations: {collections.Counter(p["group"] for p in pairs)}', flush=True)


def screen():
    subprocess.run([str(SCREEN), str(OUT / 'candidates.csv'), str(OUT / 'screen_p22.csv'), '22', '2401'], check=True)
    meta = {r['spec']: r for r in read(OUT / 'candidates.csv')}
    coarse = read(OUT / 'screen_p22.csv')
    chosen = []
    for group in sorted({r['group'] for r in meta.values()}):
        seen = set()
        for r in sorted(coarse, key=lambda r: float(r['sampled_ms'])):
            s = r['spec']
            if meta[s]['group'] != group or core(s) in seen:
                continue
            seen.add(core(s))
            chosen.append(meta[s])
            if len(seen) == 8:
                break
    fields = list(next(iter(meta.values())))
    write(OUT / 'fine_candidates.csv', fields, chosen)
    print(f'Independent p26 rescreen: {len(chosen)} configurations', flush=True)
    subprocess.run([str(SCREEN), str(OUT / 'fine_candidates.csv'), str(OUT / 'screen_p26.csv'), '26', '2402'], check=True)


def exact():
    meta = {r['spec']: r for r in read(OUT / 'candidates.csv')}
    fine = sorted(read(OUT / 'screen_p26.csv'), key=lambda r: float(r['sampled_ms']))
    if not fine:
        raise RuntimeError('Fine screening must finish first')
    chosen = {r['spec'] for r in fine[:8]}
    for group in sorted({meta[r['spec']]['group'] for r in fine}):
        chosen.update(r['spec'] for r in [r for r in fine if meta[r['spec']]['group'] == group][:2])
    done = {r['spec'] for r in read(OUT / 'exact.csv')}
    path = OUT / 'exact.csv'
    with path.open('a', newline='') as f:
        w = csv.DictWriter(f, fieldnames=['spec', 'bias', 'group', 'canonical_core0', 'canonical_core1'])
        if not path.stat().st_size:
            w.writeheader()
        for s in sorted(chosen):
            if s in done:
                continue
            result = run(['eval', '--bits', 32, '--spec', s, '--exact'])
            bias = float(re.search(r'bias=(\S+)', result)[1])
            a, b = core(s)
            w.writerow(dict(spec=s, bias=repr(bias), group=meta[s]['group'], canonical_core0=f'{a:04x}', canonical_core1=f'{b:04x}'))
            f.flush()
            print(f'exact {meta[s]["group"]}: {s} = {bias:.17f}', flush=True)


def refinement_starts():
    ranked = sorted((r for r in read(OUT / 'exact.csv') if r['group'] != 'known'), key=lambda r: float(r['bias']))
    picked, seen = [], set()
    for r in ranked:
        first = core(r['spec'])[0]
        if first in seen:
            continue
        seen.add(first)
        picked.append(r)
        if len(picked) == 4:
            break
    return picked


def refine():
    picked = refinement_starts()
    write(OUT / 'refinement_starts.csv', list(picked[0]), picked)
    (OUT / 'refinement_budget.json').write_text(json.dumps(dict(
        selection='Four best new pairs with distinct canonical first-multiplier cores',
        seeds_per_start=2, iterations=8, neighbors=48, patience=3, cap=2**28,
        fixed_low_bits=16, max_s2_exclusive=16), indent=2))
    for k, row in enumerate(picked):
        for rep in range(2):
            tag = f'refine_{k}_s{rep}'
            marker = OUT / f'{tag}.done.txt'
            if marker.exists():
                continue
            print(f'{tag}: {row["spec"]} ({row["group"]})', flush=True)
            args = ['race', '--bits', 32, '--start', row['spec'], '--iters', 8, '--neighbors', 48,
                    '--patience', 3, '--max-s2', 16, '--fix-low-bits', 16, '--cap', 'p28',
                    '--seed', 2500 + 10*k + rep, '--out', OUT / f'{tag}.csv']
            # Live stderr provides progress; stdout is small and saved after completion.
            with (OUT / f'{tag}.log').open('w') as log:
                res = subprocess.run([str(BIN), *map(str, args)], check=True, text=True,
                                     encoding='utf-8', stdout=subprocess.PIPE, stderr=log)
            marker.write_text(res.stdout)
            print(res.stdout.strip(), flush=True)


def bounded():
    """Two seeds x two rounds per core, one exact proposal per round.

    The interrupted adaptive pilot is retained separately. The bounded search
    fixes shifts and both low halves, with 96 variants per round, p22 screen,
    fresh p26 top three, and exact verification of the best fine-screen proposal.
    """
    picked = refinement_starts()
    write(OUT / 'refinement_starts.csv', list(picked[0]), picked)
    (OUT / 'bounded_budget.json').write_text(json.dumps(dict(
        seeds_per_core=2, rounds_per_seed=2, variants_per_round=96,
        coarse_samples=2**22, fine_samples=2**26, fine_finalists=3,
        exact_proposals_per_round=1, fixed_shifts=[16, 15, 15], fixed_low_bits=16,
        pilot='Interrupted adaptive seed 2500; exact accepted improvements retained '
              'and re-evaluated before reuse as seed zero of the first core.'), indent=2))

    def exact_cached(s, tag):
        path = OUT / f'{tag}.exact.txt'
        if path.exists():
            raw = path.read_text()
        else:
            raw = run(['eval', '--bits', 32, '--spec', s, '--exact'])
            path.write_text(raw)
        assert raw.startswith(s)
        return float(re.search(r'bias=(\S+)', raw)[1])

    results = []
    for k, start in enumerate(picked):
        for rep in range(2):
            tag = f'bounded_{k}_s{rep}'
            finished = OUT / f'{tag}.done.json'
            if finished.exists():
                results.append(json.loads(finished.read_text()))
                continue
            current, bias = start['spec'], float(start['bias'])
            if k == 0 and rep == 0:
                pilot = OUT / 'refine_0_s0.csv'
                if pilot.exists():
                    with pilot.open() as f:
                        history = list(csv.DictReader(line for line in f if not line.startswith('#')))
                    accepted = [r for r in history if r['event'] in ('start', 'improved')]
                    if accepted:
                        last = accepted[-1]['config']
                        assert core(last) == core(current)
                        current, bias = last, exact_cached(last, 'pilot_final')
            print(f'{tag}: starting {current} = {bias:.10f}', flush=True)
            for turn in range(2):
                t = f'{tag}_r{turn}'
                state = OUT / f'{t}.done.json'
                if state.exists():
                    saved = json.loads(state.read_text())
                    assert saved['start_spec'] == current
                    current, bias = saved['end_spec'], saved['end_bias']
                    continue
                rng = random.Random(SEED + 10000*k + 100*rep + turn)
                p = current.strip('[]').split()
                a, b = int(p[1], 16), int(p[3], 16)
                proposals = set()

                def put(x, y):
                    s = f'[16 {x:08x} 15 {y:08x} 15]'
                    assert core(s) == core(current)
                    if s != current:
                        proposals.add(s)

                for j in range(16, 32):
                    put(a ^ (1 << j), b)
                    put(a, b ^ (1 << j))
                for delta in (-3, -2, -1, 1, 2, 3):
                    put((a + (delta << 16)) % 2**32, b)
                    put(a, (b + (delta << 16)) % 2**32)
                while len(proposals) < 96:
                    x, y = a, b
                    if rng.random() < .2:
                        if rng.randrange(2):
                            x = rng.randrange(65536) << 16 | (a & 65535)
                        else:
                            y = rng.randrange(65536) << 16 | (b & 65535)
                    else:
                        for j in rng.sample(range(32), rng.randint(2, 6)):
                            if j < 16:
                                x ^= 1 << (j + 16)
                            else:
                                y ^= 1 << j
                    put(x, y)
                candidates = OUT / f'{t}.csv'
                write(candidates, ['spec'], [dict(spec=s) for s in sorted(proposals)])
                coarse = OUT / f'{t}.p22.csv'
                seed = 3000 + 100*k + 10*rep + turn
                subprocess.run([str(SCREEN), str(candidates), str(coarse), '22', str(seed)], check=True)
                fine_list = OUT / f'{t}.fine.csv'
                write(fine_list, ['spec'], [dict(spec=r['spec']) for r in sorted(read(coarse), key=lambda r: float(r['sampled_ms']))[:3]])
                fine = OUT / f'{t}.p26.csv'
                subprocess.run([str(SCREEN), str(fine_list), str(fine), '26', str(seed + 1000)], check=True)
                proposed = min(read(fine), key=lambda r: float(r['sampled_ms']))['spec']
                exact_bias = exact_cached(proposed, t)
                saved = dict(start_spec=current, start_bias=bias, proposed_spec=proposed,
                             proposed_exact_bias=exact_bias, accepted=exact_bias < bias,
                             end_spec=proposed if exact_bias < bias else current,
                             end_bias=min(exact_bias, bias))
                state.write_text(json.dumps(saved, indent=2))
                current, bias = saved['end_spec'], saved['end_bias']
                print(f'{t}: proposed {exact_bias:.10f}; best {current} = {bias:.10f}', flush=True)
            aa, bb = core(current)
            result = dict(spec=current, exact_bias=bias, group=start['group'], seed=rep,
                          start_spec=start['spec'], start_bias=float(start['bias']),
                          canonical_core0=f'{aa:04x}', canonical_core1=f'{bb:04x}')
            finished.write_text(json.dumps(result, indent=2))
            results.append(result)
            write(OUT / 'bounded_results.csv', list(results[0]), results)
    write(OUT / 'bounded_results.csv', list(results[0]), results)
    unique = {r['spec']: {k: r[k] for k in ('spec', 'exact_bias', 'group', 'canonical_core0', 'canonical_core1')} for r in results}
    write(OUT / 'finalists_exact.csv', list(next(iter(unique.values()))), list(unique.values()))


def history():
    """Distinguish absent exact scores from absent prior generated candidates."""
    pat = re.compile(r'\[\d+ [0-9a-f]{8} \d+ [0-9a-f]{8} \d+\]')
    seen = set()
    for p in (ROOT / 'results').rglob('*'):
        if OUT in p.parents or p.suffix not in ('.csv', '.txt', '.log'):
            continue
        seen.update(core(m[0]) for m in pat.finditer(p.read_bytes().decode(errors='replace')))
    manifest = json.loads((OUT / 'manifest.json').read_text())
    seen.update(tuple(k) for k in manifest['known_pairs_snapshot'])
    rows = {}
    for r in read(OUT / 'candidates.csv'):
        a, b = key = core(r['spec'])
        rows[key] = dict(canonical_core0=f'{a:04x}', canonical_core1=f'{b:04x}', group=r['group'],
                         seen_in_prior_generated_or_scored_specs=key in seen)
    write(OUT / 'prior_seen.csv', list(next(iter(rows.values()))), list(rows.values()))
    print('Pairs absent even from prior generated candidates:', sum(k not in seen for k in rows), flush=True)


def summary():
    meta = {r['spec']: r for r in read(OUT / 'candidates.csv')}
    groups = []
    for group in sorted({r['group'] for r in meta.values()}):
        vals = sorted(float(r['sampled_ms']) for r in read(OUT / 'screen_p22.csv') if meta[r['spec']]['group'] == group)
        if vals:
            row = dict(group=group, configurations=len(vals), median_sampled_ms=statistics.median(vals),
                       fraction_sampled_ms_below_4e_8=sum(v < 4e-8 for v in vals)/len(vals))
            groups.append(row)
            print(row)
    if groups:
        write(OUT / 'screen_group_summary.csv', list(groups[0]), groups)
    print('Exact starts:')
    for r in sorted(read(OUT / 'exact.csv'), key=lambda r: float(r['bias'])):
        print(r)
    print('Refined:')
    for p in sorted(OUT.glob('refine_*.done.txt')):
        print(p.name, p.read_text().strip())
    finalists = sorted(read(OUT / 'finalists_exact.csv'), key=lambda r: float(r['exact_bias']))
    historical = read(OUT / 'prior_seen.csv')
    (OUT / 'summary.json').write_text(json.dumps(dict(screen_groups=groups,
        previously_unscored_core_pairs=sum(r['group'] != 'known' for r in historical),
        absent_from_prior_generated_candidates=sum(r['seen_in_prior_generated_or_scored_specs'] == 'False' for r in historical),
        exact_starts=sorted(read(OUT / 'exact.csv'), key=lambda r: float(r['bias'])),
        refinement_starts=read(OUT / 'refinement_starts.csv'), final_exact=finalists), indent=2))


def verify():
    starts = refinement_starts()
    specs = {}
    if (OUT / 'bounded_results.csv').exists():
        specs = {r['spec']: r['group'] for r in read(OUT / 'bounded_results.csv')}
    else:
        for p in sorted(OUT.glob('refine_*.done.txt')):
            m = re.search(r'best (\[[^\]]+\])\s+score (\S+)', p.read_text())
            if not m:
                raise RuntimeError(f'Missing completed result: {p}')
            k = int(p.stem.split('_')[1])
            assert core(m[1]) == core(starts[k]['spec']), 'Refinement changed the core'
            specs[m[1]] = starts[k]['group']
    done = {r['spec'] for r in read(OUT / 'finalists_exact.csv')}
    path = OUT / 'finalists_exact.csv'
    with path.open('a', newline='') as f:
        w = csv.DictWriter(f, fieldnames=['spec', 'exact_bias', 'group', 'canonical_core0', 'canonical_core1'])
        if not path.stat().st_size:
            w.writeheader()
        for s, group in specs.items():
            if s in done:
                continue
            raw = run(['eval', '--bits', 32, '--spec', s, '--exact'])
            (OUT / ('verify_' + s.split()[1] + 'x' + s.split()[3] + '.txt')).write_text(raw)
            bias = float(re.search(r'bias=(\S+)', raw)[1])
            a, b = core(s)
            w.writerow(dict(spec=s, exact_bias=repr(bias), group=group, canonical_core0=f'{a:04x}', canonical_core1=f'{b:04x}'))
            f.flush()
            print(f'final exact: {s} = {bias:.17f}', flush=True)
    best = {}
    for r in sorted(read(path), key=lambda r: float(r['exact_bias'])):
        best.setdefault(core(r['spec']), r)
    for r in best.values():
        s = r['spec']
        target = OUT / ('bic_' + s.split()[1] + 'x' + s.split()[3] + '.txt')
        if not target.exists() or not target.stat().st_size:
            target.write_text(run(['bic', '--bits', 32, '--spec', s, '--samples', 'p18', '--seeds', '1,2,3']), encoding='utf-8')
        print(target.read_text(encoding='utf-8').strip(), flush=True)


if __name__ == '__main__':
    sys.stdout.reconfigure(encoding='utf-8')
    actions = dict(generate=generate, screen=screen, exact=exact, refine=refine, bounded=bounded,
                   verify=verify, history=history, summary=summary)
    for action in sys.argv[1:]:
        actions[action]()
