import unittest

import output_diff


def row(fn, sha, status="ok", ged=None, tm=None):
    r = {"binary": "b", "function": fn, "status": status, "code_sha256": sha}
    if ged is not None:
        r["ged"] = ged
    if tm is not None:
        r["type_match"] = tm
    return r


class OutputDiffTests(unittest.TestCase):
    def test_counts_text_changes_even_when_scores_are_flat(self):
        base = [row("f", "1", ged=0, tm=1.0), row("g", "2", ged=3, tm=0.5)]
        head = [row("f", "1", ged=0, tm=1.0), row("g", "X", ged=3, tm=0.5)]
        report = output_diff.compare(base, head)
        self.assertEqual(report["changed"], 1)
        self.assertEqual(report["listed"][0]["function"], "g")
        # Same distance on the changed row: the score alone would show nothing.
        self.assertEqual(report["changed_rows"]["base"]["ged_total"], report["changed_rows"]["head"]["ged_total"])

    def test_a_status_flip_is_a_change(self):
        base = [row("f", None, status="no_output")]
        head = [row("f", None, status="ok")]
        report = output_diff.compare(base, head)
        self.assertEqual((report["changed"], report["status_flips"]), (1, 1))

    def test_totals_put_distance_next_to_the_perfect_count(self):
        t = output_diff.totals([row("f", "1", ged=0), row("g", "2", ged=5), row("h", "3", status="timeout")])
        self.assertEqual((t["rows"], t["ok"], t["ged_perfect"], t["ged_total"]), (3, 2, 1, 5))

    def test_different_row_sets_are_an_error(self):
        with self.assertRaises(SystemExit):
            output_diff.compare([row("f", "1")], [row("g", "1")])


if __name__ == "__main__":
    unittest.main()
