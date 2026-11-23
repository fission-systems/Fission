import sys
import unittest

import memory_ceiling

# Grows to at most 1 GiB, then waits: enough to cross the test ceilings, and
# bounded so a broken watchdog cannot take the machine with it.
GROW = (
    "import time\nb=[]\n"
    "for _ in range(32):\n b.append(bytearray(32*1024*1024)); time.sleep(0.05)\n"
    "time.sleep(60)\n"
)
CHILD_GROWS = (
    "import subprocess,sys\n"
    f"subprocess.run([sys.executable,'-c',{GROW!r}])\n"
)


class MemoryCeilingTests(unittest.TestCase):
    def test_a_growing_process_is_stopped_at_the_ceiling(self):
        outcome, peak, _ = memory_ceiling.run_under_ceiling(
            [sys.executable, "-c", GROW], ceiling_kib=256 * 1024, timeout=60)
        self.assertEqual(outcome, "ceiling")
        self.assertGreaterEqual(peak, 256 * 1024)

    def test_memory_held_by_a_child_process_counts(self):
        outcome, _, _ = memory_ceiling.run_under_ceiling(
            [sys.executable, "-c", CHILD_GROWS], ceiling_kib=256 * 1024, timeout=60)
        self.assertEqual(outcome, "ceiling")

    def test_a_small_process_finishes(self):
        outcome, _, _ = memory_ceiling.run_under_ceiling(
            [sys.executable, "-c", "x = bytearray(1024)"], ceiling_kib=1024 * 1024, timeout=60)
        self.assertEqual(outcome, "ok")

    def test_a_slow_process_is_a_timeout_not_a_ceiling(self):
        outcome, _, _ = memory_ceiling.run_under_ceiling(
            [sys.executable, "-c", "import time; time.sleep(30)"], ceiling_kib=1024 * 1024, timeout=1)
        self.assertEqual(outcome, "timeout")

    def test_sample_takes_each_projects_largest_binary(self):
        entries = [
            {"project": "a", "binary_path": "a/1", "functions": ["f"] * 5},
            {"project": "a", "binary_path": "a/2", "functions": ["f"] * 9},
            {"project": "b", "binary_path": "b/1", "functions": ["f"] * 7},
            {"project": "c", "binary_path": "c/1", "functions": ["f"] * 1},
        ]
        self.assertEqual([e["binary_path"] for e in memory_ceiling.sample(entries, 2)], ["a/2", "b/1"])


if __name__ == "__main__":
    unittest.main()
