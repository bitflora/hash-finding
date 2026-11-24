"""Regression checks: python -m unittest discover -s analysis -p test_*.py.
Set PRACTRAND_FEED_EXE to a compiled practrand_feed executable to check its bytes.
"""
import importlib.util
import os
from pathlib import Path
import struct
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("campaign", ROOT / "analysis/campaign.py")
campaign = importlib.util.module_from_spec(spec)
spec.loader.exec_module(campaign)


class ReliabilityTests(unittest.TestCase):
    def test_radius_two_matches_two_successive_moves(self):
        # Enumerate the graph independently, applying one move at each step.
        def neighbours(c):
            for i in (1, 3):
                for bit in range(1, 32):
                    v = list(c); v[i] ^= 1 << bit
                    yield tuple(v)
            for i in (0, 2, 4):
                for delta in (-1, 1):
                    v = list(c); v[i] += delta
                    if all(1 <= v[j] <= 31 for j in (0, 2, 4)) and v[4] <= 16:
                        yield tuple(v)

        for center in [(16, 0xb20332d3, 15, 0xd21a3597, 15),
                       (1, 3, 31, 5, 16)]:
            with self.subTest(center=center), tempfile.TemporaryDirectory() as td:
                first = set(neighbours(center))
                expected = first | {v for c in first for v in neighbours(c)}
                expected.discard(center)
                s0, m0, s1, m1, s2 = center
                spec = f"[{s0} {m0:08x} {s1} {m1:08x} {s2}]"
                prefix = Path(td) / "radius"
                subprocess.run([sys.executable, str(ROOT / "analysis/gen_radius2_cands.py"),
                                spec, str(prefix), "3"], check=True, capture_output=True)
                actual = []
                for k in range(3):
                    for row in Path(f"{prefix}_k{k}.csv").read_text().splitlines()[1:]:
                        a, b, c, d, e = row.strip("[]").split()
                        actual.append((int(a), int(b, 16), int(c), int(d, 16), int(e)))
                self.assertEqual(len(actual), len(set(actual)))
                self.assertEqual(set(actual), expected)

    def test_resume_restores_latest_archived_incumbent(self):
        original = "[16 21f0aaad 15 735a2d97 15]"
        improved = "[16 9c013335 15 335a2d97 15]"
        for current in (None, "", "startup output before initial scoring"):
            with self.subTest(current=current), tempfile.TemporaryDirectory() as td:
                directory = Path(td)
                (directory / "demo.part2.log").write_text(
                    f"[ 100s] 1,improved,{original},0.10704,\n")
                (directory / "demo.part10.log").write_text(
                    f"[ 200s] 2,improved,{improved},0.10207,\n")
                if current is not None:
                    (directory / "demo.log").write_text(current)
                with patch.object(campaign, "D", directory), patch.object(
                        campaign, "run_proc", return_value=f"best {improved} score 0.10207") as run:
                    best, bias, parts = campaign.race_job(
                        {"id": "demo", "seed": "1", "args": ""}, original)
                args = run.call_args.args[0]
                self.assertEqual(args[args.index("--start") + 1], improved)
                self.assertEqual(args[args.index("--iters") + 1], str(max(campaign.ITERS - 3, 1)))
                self.assertEqual(best, improved)
                self.assertEqual(parts[:2], ["demo.part2.log", "demo.part10.log"])
                if current is not None:
                    self.assertEqual(parts[-1], "demo.part11.log")

    def test_resume_reuses_completed_result(self):
        best = "[16 9c013335 15 335a2d97 15]"
        with tempfile.TemporaryDirectory() as td:
            directory = Path(td)
            (directory / "demo.log").write_text(f"best {best}  score 0.10207\n")
            with patch.object(campaign, "D", directory), patch.object(campaign, "run_proc") as run:
                result = campaign.race_job({"id": "demo", "seed": "1", "args": ""}, "unused")
            run.assert_not_called()
            self.assertEqual(result, (best, 0.10207, []))

    @unittest.skipUnless(os.environ.get("PRACTRAND_FEED_EXE"), "set PRACTRAND_FEED_EXE to check compiled feeder")
    def test_practrand_stream_matches_mixer_bytes(self):
        def mix(x):
            x = ((x ^ (x >> 16)) * 0x9c013335) & 0xffffffff
            x = ((x ^ (x >> 15)) * 0x335a2d97) & 0xffffffff
            return x ^ (x >> 15)
        expected = b"".join(struct.pack("=I", mix(i)) for i in range(1 << 14))
        self.assertIn(b"\x0a", expected)
        proc = subprocess.Popen([os.environ["PRACTRAND_FEED_EXE"], "9c013335", "335a2d97", "0"],
                                stdout=subprocess.PIPE)
        try:
            actual = proc.stdout.read(len(expected))
            self.assertEqual(actual, expected)
        finally:
            proc.terminate()
            proc.wait(timeout=10)
            proc.stdout.close()


if __name__ == "__main__":
    unittest.main()
