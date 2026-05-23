import datetime
import json
from pathlib import Path
from typing import Any

from benchmark.source_semantic_benchmark.config import (
    ROOT_DIR,
    SANITIZE_ID_RE,
    TRAILING_DECORATION_RE,
    NORMALIZE_PREFIX_RE,
    NON_ALNUM_RE,
)


try:
    import orjson
except ImportError:
    orjson = None


def rel(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(ROOT_DIR))
    except ValueError:
        return str(path)


def sanitize_id(text: str) -> str:
    text = SANITIZE_ID_RE.sub("-", text.strip())
    text = text.strip("-._")
    return text or "entry"


def utc_now() -> datetime.datetime:
    return datetime.datetime.now(datetime.UTC)


def utc_timestamp_slug(now: datetime.datetime) -> str:
    return now.strftime("%Y%m%dT%H%M%SZ")


def utc_isoformat(now: datetime.datetime) -> str:
    return now.replace(microsecond=0).isoformat().replace("+00:00", "Z")


def load_json(path: Path) -> dict[str, Any]:
    data = path.read_bytes()
    if orjson is not None:
        return orjson.loads(data)
    return json.loads(data.decode("utf-8"))


def dump_json_pretty(value: Any) -> str:
    if orjson is not None:
        return orjson.dumps(value, option=orjson.OPT_INDENT_2 | orjson.OPT_SORT_KEYS).decode("utf-8") + "\n"
    return json.dumps(value, indent=2, sort_keys=True) + "\n"


def dump_json_line(value: Any) -> str:
    if orjson is not None:
        return orjson.dumps(value, option=orjson.OPT_SORT_KEYS).decode("utf-8") + "\n"
    return json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n"


def load_json_list_or_dict(path: Path) -> Any:
    data = path.read_bytes()
    if orjson is not None:
        return orjson.loads(data)
    return json.loads(data.decode("utf-8"))


def resolve_path(path: str | Path, root_dir: Path = ROOT_DIR) -> Path:
    p = Path(path)
    return p if p.is_absolute() else root_dir / p


def percent(value: float) -> float:
    return round(value * 100.0, 3)


def numeric_distribution(values: list[float]) -> dict[str, Any]:
    if not values:
        return {
            "count": 0,
            "min": 0.0,
            "max": 0.0,
            "avg": 0.0,
            "p50": 0.0,
            "p90": 0.0,
            "p95": 0.0,
        }
    sorted_values = sorted(values)

    def percentile(rank: float) -> float:
        if len(sorted_values) == 1:
            return sorted_values[0]
        index = (len(sorted_values) - 1) * rank
        lower = int(index)
        upper = min(lower + 1, len(sorted_values) - 1)
        fraction = index - lower
        return sorted_values[lower] + (sorted_values[upper] - sorted_values[lower]) * fraction

    return {
        "count": len(sorted_values),
        "min": round(sorted_values[0], 6),
        "max": round(sorted_values[-1], 6),
        "avg": round(sum(sorted_values) / len(sorted_values), 6),
        "p50": round(percentile(0.50), 6),
        "p90": round(percentile(0.90), 6),
        "p95": round(percentile(0.95), 6),
    }



def canonical_address(value: str | int) -> str:
    if isinstance(value, int):
        return f"0x{value:x}"
    val_str = str(value).strip().lower()
    if val_str.startswith("0x"):
        try:
            return f"0x{int(val_str, 16):x}"
        except ValueError:
            return val_str
    try:
        return f"0x{int(val_str):x}"
    except ValueError:
        return val_str


def normalize_name(name: str) -> str:
    name = TRAILING_DECORATION_RE.sub("", name.strip().lower())
    name = NORMALIZE_PREFIX_RE.sub("", name)
    return NON_ALNUM_RE.sub("", name)


