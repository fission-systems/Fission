import hashlib
import time
from pathlib import Path
from typing import Any

from benchmark.source_semantic_benchmark.config import DEBUG_DECOMP_EVIDENCE_CONTRACT
from benchmark.source_semantic_benchmark.utils import (
    canonical_address,
    dump_json_pretty,
    load_json_list_or_dict,
)


def file_cache_fingerprint(path: Path) -> str:
    try:
        resolved = path.resolve()
        stat = resolved.stat()
    except OSError:
        return f"{path}:missing"
    return f"{resolved}:size={stat.st_size}:mtime_ns={stat.st_mtime_ns}"


def decomp_cache_key(
    binary_path: Path,
    address: str,
    fission_bin: Path,
    include_debug_decomp: bool,
) -> str:
    return "|".join(
        [
            "source-semantic-decomp-v1",
            f"binary={file_cache_fingerprint(binary_path)}",
            f"fission_bin={file_cache_fingerprint(fission_bin)}",
            f"addr={canonical_address(address)}",
            f"debug={int(include_debug_decomp)}",
            f"debug_contract={DEBUG_DECOMP_EVIDENCE_CONTRACT if include_debug_decomp else 'none'}",
        ]
    )


def list_cache_key(binary_path: Path, fission_bin: Path) -> str:
    return "|".join(
        [
            "source-semantic-list-v1",
            f"binary={file_cache_fingerprint(binary_path)}",
            f"fission_bin={file_cache_fingerprint(fission_bin)}",
        ]
    )


def behavior_cache_key(code: str, clang: str, timeout_sec: int) -> str:
    return "|".join(
        [
            "source-semantic-behavior-v2",
            f"clang={file_cache_fingerprint(Path(clang))}",
            f"timeout_sec={timeout_sec}",
            f"code_sha256={hashlib.sha256(code.encode('utf-8')).hexdigest()}",
        ]
    )


def load_decomp_cache(path: Path | None) -> dict[str, dict[str, Any]]:
    if path is None or not path.exists():
        return {}
    try:
        raw = load_json_list_or_dict(path)
    except Exception:
        return {}
    if not isinstance(raw, dict):
        return {}
    entries = raw.get("entries", raw)
    if not isinstance(entries, dict):
        return {}
    return {str(key): value for key, value in entries.items() if isinstance(value, dict)}


def save_decomp_cache(path: Path | None, cache: dict[str, dict[str, Any]]) -> None:
    if path is None:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "format": "source-semantic-decomp-cache-v1",
        "updated_at_unix": round(time.time(), 6),
        "entry_count": len(cache),
        "entries": cache,
    }
    tmp_path = path.with_suffix(path.suffix + ".tmp")
    tmp_path.write_text(dump_json_pretty(payload), encoding="utf-8")
    tmp_path.replace(path)


def load_list_cache(path: Path | None) -> dict[str, dict[str, Any]]:
    return load_decomp_cache(path)


def save_list_cache(path: Path | None, cache: dict[str, dict[str, Any]]) -> None:
    if path is None:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "format": "source-semantic-list-cache-v1",
        "updated_at_unix": round(time.time(), 6),
        "entry_count": len(cache),
        "entries": cache,
    }
    tmp_path = path.with_suffix(path.suffix + ".tmp")
    tmp_path.write_text(dump_json_pretty(payload), encoding="utf-8")
    tmp_path.replace(path)


def load_behavior_cache(path: Path | None) -> dict[str, dict[str, Any]]:
    return load_decomp_cache(path)


def save_behavior_cache(path: Path | None, cache: dict[str, dict[str, Any]]) -> None:
    if path is None:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    payload = {
        "format": "source-semantic-behavior-cache-v1",
        "updated_at_unix": round(time.time(), 6),
        "entry_count": len(cache),
        "entries": cache,
    }
    tmp_path = path.with_suffix(path.suffix + ".tmp")
    tmp_path.write_text(dump_json_pretty(payload), encoding="utf-8")
    tmp_path.replace(path)
