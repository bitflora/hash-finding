"""Run the 32-bit mixer comparison and save one PractRand log per stream."""

import argparse
import csv
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import re
import subprocess


PATTERNS = {
    "counter": ["0"],
    "rotate16": ["1", "16"],
    "reverse": ["2"],
    "complement": ["3"],
    "weyl": ["4"],
}
LENGTH = re.compile(r"^length=[^\n]*\(2\^(\d+) bytes\)", re.M)
FAILURE = re.compile(r"^.*\bFAIL[ !]*$", re.M)
NORMAL = re.compile(r"no anomalies in (\d+) test result|"
                    r"(\d+) test result\(s\) without anomalies")


def run(feed, rng_test, output, cap, minimum, candidate, pattern):
    log = output / f"{candidate['name']}-{pattern}.txt"
    command = [str(feed), "--spec", candidate["spec"], *PATTERNS[pattern]]
    tester = [str(rng_test), "stdin32", "-tlmin", str(minimum), "-tlmax",
              str(cap), "-tlmaxonly", "-multithreaded"]
    with log.open("w", encoding="utf-8") as file, subprocess.Popen(
        command, stdout=subprocess.PIPE
    ) as feeder:
        try:
            subprocess.run(tester, stdin=feeder.stdout, stdout=file,
                           stderr=subprocess.STDOUT, check=True)
        finally:
            feeder.stdout.close()
            feeder.terminate()
    text = log.read_text(encoding="utf-8")
    lengths = list(LENGTH.finditer(text))
    if not lengths or int(lengths[-1][1]) != cap:
        raise RuntimeError(f"{log} did not reach 2^{cap} bytes")
    first_fail = None
    for index, length in enumerate(lengths):
        end = lengths[index + 1].start() if index + 1 < len(lengths) else len(text)
        failures = FAILURE.findall(text[length.end():end])
        if failures and first_fail is None:
            first_fail = int(length[1])
    normal = NORMAL.search(text[lengths[-1].end():])
    print(f"{log.stem}: first FAIL exponent {first_fail}, {len(failures)} FAIL at cap",
          flush=True)
    return {**candidate, "pattern": pattern, "first_fail_log2_bytes": first_fail,
            "cap_log2_bytes": cap, "fail_at_cap": len(failures),
            "no_anomalies_at_cap": int(normal[1] or normal[2]) if normal else 0}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("feed", type=Path)
    parser.add_argument("rng_test", type=Path)
    parser.add_argument("candidates", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--pattern", choices=PATTERNS, action="append")
    parser.add_argument("--candidate", action="append")
    parser.add_argument("--cap", type=int, choices=range(24, 34), default=29)
    parser.add_argument("--minimum", type=int, choices=range(20, 34), default=24)
    parser.add_argument("--jobs", type=int, default=2)
    args = parser.parse_args()
    if args.minimum > args.cap or args.jobs < 1:
        parser.error("require minimum <= cap and at least one job")
    with args.candidates.open(newline="", encoding="utf-8") as file:
        candidates = list(csv.DictReader(file))
    if args.candidate:
        unknown = set(args.candidate) - {candidate["name"] for candidate in candidates}
        if unknown:
            parser.error(f"unknown candidates: {', '.join(sorted(unknown))}")
        candidates = [candidate for candidate in candidates if candidate["name"] in args.candidate]
    args.output.mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        tasks = [pool.submit(run, args.feed.resolve(), args.rng_test.resolve(),
                             args.output, args.cap, args.minimum, candidate, pattern)
                 for pattern in args.pattern or PATTERNS for candidate in candidates]
        rows = [task.result() for task in tasks]
    with (args.output / "scores.csv").open("w", newline="", encoding="utf-8") as file:
        writer = csv.DictWriter(file, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)


if __name__ == "__main__":
    main()
