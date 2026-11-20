# Search campaign (started 2026-11-20)

A pausable job queue for two long experiments. Driver: `analysis/campaign.py`.

**frontier** (32 jobs): BIC-constrained races (`race --max-bic-corr C`) to map
how low bias can go at a given worst output-pair correlation (FINDINGS F18).
The levels (C, starting s2) are (0.05, 12), (0.11, 13), (0.15, 14) and
(0.25, 15). Each level is run for 4 families (7870aaad, record, d303 winner,
lowbias32 winner) with seeds 1 and 2. All jobs use `--max-s2 16`.

**multi** (62 jobs): multi-start, to test whether the record is near the
ceiling. There is one job per distinct published low-16-bit core: 54 cores
after dropping cores within Hamming distance 4 of an already-raced or kept
core, ordered by their best published bias. There are also 8 random-core
controls. Each job scores 8 random high halves exactly, then races the best
draw (seed 61, 96 neighbours, patience 6, at most 25 iterations, s2 ≤ 15).

The two campaigns alternate in `jobs.csv`, so every chunk advances both.
Jobs take about 30–110 min each, so the full queue is roughly 60–90 h.

## Controls (run from `mixing-functions/`)

```
python analysis/campaign.py run                 # run until done, paused or stopped
python analysis/campaign.py run --hours 8       # chunk: start no new job after 8 h
python analysis/campaign.py run --jobs 5        # chunk: at most 5 jobs
python analysis/campaign.py pause               # graceful: exit after the current job
python analysis/campaign.py stop                # immediate: kill the current job (~5 s)
python analysis/campaign.py status              # progress, runner state, time left
python analysis/campaign.py summary             # result tables, with BIC of each winner
```

- Only one runner can be active. A heartbeat lock refuses a second runner,
  and a lock older than 60 s counts as stale, for example after a crash or
  reboot.
- `run` clears old PAUSE/STOP flags when it starts.
- **Resume after `stop`:** the stopped race's log becomes `<id>.partK.log`,
  and the race restarts from its last incumbent with the remaining iteration
  budget and a fresh neighbour seed. Its result is still an exactly verified
  score, but `<id>.done.json` lists the parts, because the trajectory
  differs from an uninterrupted run. For a core job, the exact draw scores
  are kept per draw, so a stop during screening loses at most one draw.
- A finished job has `<id>.done.json`. Delete it to re-run that job.
