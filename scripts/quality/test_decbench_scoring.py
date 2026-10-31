"""Tests for decbench_scoring.

Run with the python of an environment that has `decbench` installed:

    ~/fission-benchmark/.venv-decbench/bin/python -m unittest scripts/quality/test_decbench_scoring.py

The tests that need the CFG parser skip when rust-joern is not built.
"""

from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(REPO / "vendor" / "decbench"))

_spec = importlib.util.spec_from_file_location(
    "decbench_scoring", Path(__file__).resolve().parent / "decbench_scoring.py"
)
scoring = importlib.util.module_from_spec(_spec)
sys.modules["decbench_scoring"] = scoring
_spec.loader.exec_module(scoring)

try:
    import decbench  # noqa: F401

    HAVE_DECBENCH = True
except ImportError:
    HAVE_DECBENCH = False


def _parser_available() -> bool:
    try:
        scoring._probe_parser()
    except Exception:
        return False
    return True


def _published(nodes: int, edges: list[list[int]], degenerate: bool = False) -> dict:
    return {
        "functions": {
            "f": {
                "nodes": list(range(nodes)),
                "edges": edges,
                "entry": [0],
                "exit": [nodes - 1],
                "degenerate": degenerate,
            }
        }
    }


@unittest.skipUnless(HAVE_DECBENCH, "decbench is not installed")
class PublishedSourceCfgs(unittest.TestCase):
    def _load(self, payload: dict) -> dict:
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
            json.dump(payload, f)
        try:
            return scoring.load_published_source_cfgs(f.name)
        finally:
            Path(f.name).unlink()

    def test_a_graph_is_rebuilt_with_its_nodes_and_edges(self) -> None:
        cfgs = self._load(_published(3, [[0, 1], [1, 2]]))
        self.assertEqual(cfgs["f"].number_of_nodes(), 3)
        self.assertEqual(cfgs["f"].number_of_edges(), 2)

    def test_an_unreadable_file_is_an_empty_result_not_an_exception(self) -> None:
        self.assertEqual(scoring.load_published_source_cfgs("/nonexistent/cfg.json"), {})

    def test_identical_graphs_have_distance_zero(self) -> None:
        graph = self._load(_published(3, [[0, 1], [1, 2]]))["f"]
        other = self._load(_published(3, [[0, 1], [1, 2]]))["f"]
        self.assertEqual(scoring.compute_ged(graph, other)["ged"], 0.0)

    def test_a_missing_node_costs_distance(self) -> None:
        source = self._load(_published(3, [[0, 1], [1, 2]]))["f"]
        shorter = self._load(_published(2, [[0, 1]]))["f"]
        self.assertGreater(scoring.compute_ged(source, shorter)["ged"], 0.0)

    def test_a_degenerate_source_is_an_error_not_a_perfect_score(self) -> None:
        source = self._load(_published(1, [], degenerate=True))["f"]
        decompiled = self._load(_published(2, [[0, 1]]))["f"]
        result = scoring.compute_ged(source, decompiled)
        self.assertNotIn("ged", result)
        self.assertIn("error", result)


class Isolation(unittest.TestCase):
    """A function that cannot be parsed must cost only itself."""

    def setUp(self) -> None:
        self._parse_batch = scoring._parse_batch
        self._backend = scoring.parse_source_for_backend

        def fake_parse(functions: dict[str, str], _parse_source) -> dict:
            if any("BAD" in code for code in functions.values()):
                raise ValueError("cannot parse")
            return {name: f"cfg:{name}" for name in functions}

        scoring._parse_batch = fake_parse
        scoring.parse_source_for_backend = lambda: None

    def tearDown(self) -> None:
        scoring._parse_batch = self._parse_batch
        scoring.parse_source_for_backend = self._backend

    def test_one_bad_function_does_not_take_the_batch_down(self) -> None:
        functions = {f"fn{i}": "int f(){}" for i in range(7)}
        functions["fn3"] = "BAD"
        cfgs = scoring.extract_decompiled_cfgs(functions)
        self.assertEqual(set(cfgs), set(functions) - {"fn3"})

    def test_an_empty_batch_is_empty(self) -> None:
        self.assertEqual(scoring.extract_decompiled_cfgs({}), {})


@unittest.skipUnless(HAVE_DECBENCH and _parser_available(), "decbench or rust-joern missing")
class WithTheRealParser(unittest.TestCase):
    def test_a_branch_gives_two_paths(self) -> None:
        code = "int f(int a)\n{\n    if (a) {\n        return 1;\n    }\n    return 0;\n}\n"
        cfgs = scoring.extract_decompiled_cfgs({"f": code})
        self.assertIn("f", cfgs)
        self.assertGreaterEqual(cfgs["f"].number_of_nodes(), 3)

    def test_a_broken_neighbour_does_not_hide_a_good_function(self) -> None:
        good = "int g(int a)\n{\n    return a + 1;\n}\n"
        cfgs = scoring.extract_decompiled_cfgs({"g": good, "h": "int h( {{{ not c"})
        self.assertIn("g", cfgs)


if __name__ == "__main__":
    unittest.main()
