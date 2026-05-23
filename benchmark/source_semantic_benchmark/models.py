from dataclasses import dataclass
from pathlib import Path
from typing import Any

@dataclass(frozen=True)
class BenchmarkEntry:
    id: str
    binary_path: Path
    source_path: Path
    language: str
    tags: list[str]
    weight: float = 1.0
    behavior_cases: dict[str, list[dict[str, Any]]] | None = None


@dataclass(frozen=True)
class SourceFunction:
    name: str
    signature: str
    body: str
    return_kind: str
    param_kinds: list[str]
    param_names: list[str]
    line: int
    is_static: bool = False


@dataclass(frozen=True)
class FissionFunction:
    address: str
    name: str
