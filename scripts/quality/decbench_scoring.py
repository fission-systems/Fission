"""Scoring for the sample-set gate, built on the `decbench` package.

The gate used to import `ged.py` from `<fission-benchmark>/runner`, a local
re-implementation of DecBench's GED that went away when that repository was
reorganised (2026-10-31). This module calls DecBench itself instead:

* GED               `decbench.metrics.ged.GEDMetric` (isomorphism shortcut, exact
                    vj_ged up to `DECBENCH_GED_MAX_NODES`, default 200, size lower
                    bound above it)
* source CFGs       `decbench.publish.cfg_export.rebuild_cfg`
* decompiled-C prep `decbench.utils.cfg.{sanitize,preprocess}_decompiled_c`

Only the CFG parser stays local. rust-joern is the parser DecBench scores with
since 2026-10-09 (Noelo-Lab/decbench#99), but the `decbench` commits that
`fission-benchmark` pins and vendors still call pyjoern's JVM, so the parse
call is made here, the same way `decbench`'s `_parse_cfgs` makes it:
`parse_source(path, no_ddg=True, strict=True, preprocessed=True)`.
`FISSION_GED_BACKEND=pyjoern` selects the JVM parser, only to reproduce older
numbers.

Differences from the old local `ged.py` that move scores (so a baseline taken
with it is not comparable): GED is exact up to 200 nodes instead of 60, an
approximated GED is at least 1 (the old code could score two equally sized
large graphs 0), and the decompiled translation unit is run through
`preprocess_decompiled_c` as DecBench does.
"""

from __future__ import annotations

import json
import logging
import math
import os
import sys
import tempfile
from pathlib import Path
from typing import Any, Callable

logger = logging.getLogger(__name__)

RUST_JOERN_DIR = Path(os.environ.get("RUST_JOERN_DIR", str(Path.home() / "rust-joern")))


def _library_name() -> str:
    return "librust_joern.dylib" if sys.platform == "darwin" else "librust_joern.so"


def _rust_parse_source(path: Path) -> Any:
    python_dir = str(RUST_JOERN_DIR / "python")
    if python_dir not in sys.path:
        sys.path.insert(0, python_dir)
    os.environ.setdefault(
        "RUST_JOERN_LIBRARY", str(RUST_JOERN_DIR / "target" / "release" / _library_name())
    )
    from rust_joern import parse_source

    return parse_source(path, no_ddg=True, strict=True, preprocessed=True)


def _pyjoern_parse_source(path: Path) -> Any:
    from pyjoern import parse_source

    return parse_source(path)


def parse_source_for_backend() -> Callable[[Path], Any]:
    backend = os.environ.get("FISSION_GED_BACKEND", "rust-joern")
    if backend == "rust-joern":
        return _rust_parse_source
    if backend == "pyjoern":
        return _pyjoern_parse_source
    raise ValueError(f"unknown FISSION_GED_BACKEND {backend!r}")


def _translation_unit(functions: dict[str, str]) -> str:
    from decbench.utils.cfg import preprocess_decompiled_c, sanitize_decompiled_c

    marked = [f"// Function: {name}\n{sanitize_decompiled_c(code)}" for name, code in functions.items()]
    return preprocess_decompiled_c("\n\n".join(marked))


def _parse_batch(functions: dict[str, str], parse_source: Callable[[Path], Any]) -> dict[str, Any]:
    with tempfile.NamedTemporaryFile(mode="w", suffix=".c", delete=False, encoding="utf-8") as f:
        f.write(_translation_unit(functions))
        temp_path = Path(f.name)
    try:
        parsed = parse_source(temp_path)
        cfgs: dict[str, Any] = {}
        for key, func in (parsed or {}).items():
            cfg = func.cfg if hasattr(func, "cfg") else None
            if cfg is not None:
                cfgs[func.name if hasattr(func, "name") else str(key)] = cfg
        return cfgs
    finally:
        temp_path.unlink(missing_ok=True)


def extract_decompiled_cfgs(functions: dict[str, str]) -> dict[str, Any]:
    """CFGs for every function Joern accepts; a batch that fails is bisected.

    One unparseable function must cost that function, not its whole batch, so a
    failed or short batch is split and the halves retried.
    """
    if not functions:
        return {}
    parse_source = parse_source_for_backend()

    def isolate(items: list[tuple[str, str]]) -> dict[str, Any]:
        try:
            parsed = _parse_batch(dict(items), parse_source)
        except Exception as exc:
            if len(items) == 1:
                logger.warning("CFG extraction failed for %s: %s", items[0][0], exc)
                return {}
            logger.warning("CFG extraction from %d functions failed; bisecting: %s", len(items), exc)
            return _bisect(items)
        missing = [item for item in items if item[0] not in parsed]
        if not missing:
            return parsed
        if len(items) == 1:
            logger.warning("CFG extraction omitted %s", items[0][0])
            return parsed
        if len(missing) == len(items):
            return _bisect(items)
        return {**parsed, **isolate(missing)}

    def _bisect(items: list[tuple[str, str]]) -> dict[str, Any]:
        middle = len(items) // 2
        return {**isolate(items[:middle]), **isolate(items[middle:])}

    return isolate(list(functions.items()))


def load_published_source_cfgs(source_cfg_path: str) -> dict[str, Any]:
    from decbench.publish.cfg_export import rebuild_cfg

    try:
        payload = json.loads(Path(source_cfg_path).read_text(encoding="utf-8"))
    except (OSError, ValueError) as exc:
        logger.warning("Published source CFG %s failed: %s", source_cfg_path, exc)
        return {}
    return {
        str(name): rebuild_cfg(raw)
        for name, raw in (payload.get("functions") or {}).items()
        if isinstance(raw, dict)
    }


def compute_ged(source_cfg: Any, decompiled_cfg: Any) -> dict[str, Any]:
    """`{"ged": float, ...}`, or `{"error": ...}` (no `ged` key) when DecBench declines.

    DecBench returns a non-finite value for a missing or degenerate source CFG so
    the recording layer drops it from GED's denominator; that is an error here.
    """
    from decbench.metrics.ged import GEDMetric

    value = GEDMetric().compute_for_function(None, source_cfg, decompiled_cfg)
    meta = dict(value.metadata or {})
    if value.value is None or not math.isfinite(value.value):
        return {**meta, "error": meta.get("error", "non-finite GED")}
    return {**meta, "ged": float(value.value)}


def _probe_parser() -> None:
    """Fail now, loudly, if the CFG parser cannot parse one trivial function.

    A parser that fails inside `extract_decompiled_cfgs` only logs and omits the
    function, which would end a run with every function unscored and no error.
    """
    probe = {"probe_fn": "int probe_fn(int a)\n{\n    if (a) {\n        return 1;\n    }\n    return 0;\n}\n"}
    try:
        cfgs = _parse_batch(probe, parse_source_for_backend())
    except Exception as exc:
        raise RuntimeError(f"the CFG parser failed on a trivial function: {exc}") from exc
    if "probe_fn" not in cfgs:
        raise RuntimeError("the CFG parser returned no CFG for a trivial function")


def load(repo: Path) -> dict[str, Any]:
    """Scoring entry points for the gate; makes the `decbench` package importable."""
    sys.path.insert(0, str(repo / "vendor" / "decbench"))
    if os.environ.get("FISSION_BENCHMARK_NO_CACHE") == "1":
        os.environ.setdefault("DECBENCH_NO_CACHE", "1")
    from decbench.decompilers.raw.fission_raw import _variables
    from decbench.metrics.type_match import TypeMatchMetric, extract_ground_truth_types

    _probe_parser()
    return {
        "compute_ged": compute_ged,
        "extract_decompiled_cfgs": extract_decompiled_cfgs,
        "load_published_source_cfgs": load_published_source_cfgs,
        "variables": _variables,
        "TypeMatchMetric": TypeMatchMetric,
        "extract_ground_truth_types": extract_ground_truth_types,
    }
