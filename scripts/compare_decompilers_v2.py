#!/usr/bin/env python3
import runpy
from pathlib import Path

script = Path(__file__).resolve().parent / "compare" / "compare_decompilers_v2.py"
runpy.run_path(str(script), run_name="__main__")
