#!/usr/bin/env python3
from __future__ import annotations

import argparse
import datetime
import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import tempfile
import threading
import time
from collections import Counter
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import orjson
except ImportError:  # pragma: no cover - optional fast path
    orjson = None


ROOT_DIR = Path(__file__).resolve().parents[2]
DEFAULT_MANIFEST = Path(__file__).resolve().parent / "manifests" / "source_owned_all.json"
DEFAULT_FISSION_BIN = ROOT_DIR / "target" / "release" / "fission_cli"
DEFAULT_ARTIFACT_ROOT = ROOT_DIR / "benchmark" / "artifacts" / "source_semantic_benchmark"
DEFAULT_DECOMP_CACHE_FILE = DEFAULT_ARTIFACT_ROOT / ".cache" / "decomp_cache.json"
DEFAULT_LIST_CACHE_FILE = DEFAULT_ARTIFACT_ROOT / ".cache" / "list_cache.json"
DEFAULT_BEHAVIOR_CACHE_FILE = DEFAULT_ARTIFACT_ROOT / ".cache" / "behavior_cache.json"
DEFAULT_HISTORY_FILE = DEFAULT_ARTIFACT_ROOT / "source_semantic_history.jsonl"
DEFAULT_LATEST_INDEX_FILE = DEFAULT_ARTIFACT_ROOT / "source_semantic_latest_by_manifest.json"
DEBUG_DECOMP_EVIDENCE_CONTRACT = "template_source_counts_v1"
DEFAULT_JOBS = max(1, (os.cpu_count() or 2) // 2)
CANDIDATE_TIMEOUT_MIN_SEC = 3
CANDIDATE_TIMEOUT_ORACLE_MULTIPLIER = 10

SANITIZE_ID_RE = re.compile(r"[^A-Za-z0-9_.-]+")
BLOCK_COMMENT_RE = re.compile(r"/\*.*?\*/", re.S)
LINE_COMMENT_RE = re.compile(r"//.*")
WORD_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
WORD_BOUNDARY_RE = re.compile(r"\b[A-Za-z_][A-Za-z0-9_]*\b")
CTYPE_RE = re.compile(r"[A-Za-z_][A-Za-z0-9_\s*]*")
CONST_RE = re.compile(r"\b(?:0x[0-9A-Fa-f]+|\d+)\b")
CALL_RE = re.compile(r"\b([A-Za-z_][A-Za-z0-9_:]*)\s*\(")
INDIRECT_CAST_CALL_RE = re.compile(r"\)\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)\s*\(")
DEREF_INDIRECT_CALL_RE = re.compile(r"\(\s*\*\s*([A-Za-z_][A-Za-z0-9_]*)\s*\)\s*\(")
ARRAY_SUFFIX_RE = re.compile(r"\[[^\]]*\]")
RETURN_ARROW_RE = re.compile(r"->\s*([^{]+)$")
ACCESS_LABEL_RE = re.compile(r"\b(public|private|protected)\s*:\s*")
C_LIKE_ACCESS_PREFIX_RE = re.compile(r"^\s*(public|private|protected)\s*:\s*")
C_LIKE_FUNCTION_RE = re.compile(
    r"([~A-Za-z_][A-Za-z0-9_:~]*)\s*\(([^;{}()]*)\)\s*(?:const)?\s*(?:noexcept)?\s*$"
)
C_LIKE_TYPE_DECL_RE = re.compile(r"\b(class|struct|namespace|enum)\s+$")
TRAILING_DECORATION_RE = re.compile(r"\s+\[[^\]]+\]\s*$")
NORMALIZE_PREFIX_RE = re.compile(r"^(sub_|fun_|_+)")
NON_ALNUM_RE = re.compile(r"[^a-z0-9]+")
FISSION_LIST_LINE_RE = re.compile(r"(0x[0-9A-Fa-f]+)\s+\d+\s+(.+?)\s*$")
GO_FUNCTION_RE = re.compile(
    r"(?m)^\s*func\s+(?:\([^)]*\)\s*)?([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)\s*([^{\n]*)\{"
)
RUST_FUNCTION_RE = re.compile(
    r"(?m)^\s*(?:pub\s+)?(?:unsafe\s+)?(?:extern\s+\"[^\"]+\"\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)\s*(?:->\s*([^{\n]+))?\{"
)

SOURCE_EXTENSIONS = {
    ".c": "c",
    ".cc": "cpp",
    ".cpp": "cpp",
    ".cxx": "cpp",
    ".go": "go",
    ".rs": "rust",
}

KNOWN_ARCH_TAGS = {
    "aarch64",
    "arm",
    "arm8",
    "arm8m",
    "ebpf",
    "loongarch64",
    "mips",
    "mips32",
    "mips32le",
    "ppc",
    "ppc64",
    "riscv",
    "riscv64",
    "sparc",
    "x86",
    "x86-64",
    "x86_64",
}

CONTROL_WORDS = {
    "if",
    "else",
    "for",
    "while",
    "do",
    "switch",
    "case",
    "default",
    "return",
    "break",
    "continue",
    "match",
}

CALL_EXCLUDE = CONTROL_WORDS | {
    "sizeof",
    "printf",
    "println",
    "format",
    "vec",
    "make",
    "len",
    "std",
}

INTEGRAL_WORDS = {
    "int",
    "i8",
    "i16",
    "i32",
    "i64",
    "u32",
    "u64",
    "usize",
    "isize",
    "int8_t",
    "int16_t",
    "int32_t",
    "int64_t",
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
    "uint",
    "uchar",
    "ushort",
    "ulonglong",
    "longlong",
    "unsigned",
    "signed",
    "long",
    "short",
    "char",
    "bool",
}

UNSIGNED_INTEGRAL_WORDS = {
    "u8",
    "u16",
    "u32",
    "u64",
    "uint8_t",
    "uint16_t",
    "uint32_t",
    "uint64_t",
    "uint",
    "uchar",
    "ushort",
    "ulonglong",
    "usize",
    "unsigned",
}


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
            "source-semantic-behavior-v1",
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


def discover_source_entries(manifest: dict[str, Any]) -> list[BenchmarkEntry]:
    entries: list[BenchmarkEntry] = []

    for raw in manifest.get("entries", []):
        source_path = resolve_path(raw["source_path"])
        binary_path = resolve_path(raw["binary_path"])
        language = raw.get("language") or SOURCE_EXTENSIONS.get(source_path.suffix, "")
        entries.append(
            BenchmarkEntry(
                id=raw.get("id") or sanitize_id(rel(binary_path)),
                binary_path=binary_path,
                source_path=source_path,
                language=language,
                tags=list(raw.get("tags") or []),
                weight=float(raw.get("weight", 1.0) or 1.0),
                behavior_cases=raw.get("behavior_cases"),
            )
        )

    for spec in manifest.get("discovery", []):
        root = resolve_path(spec.get("root", "benchmark/binary"))
        languages = set(spec.get("languages") or SOURCE_EXTENSIONS.values())
        require_binary = bool(spec.get("require_binary", True))
        tags = list(spec.get("tags") or [])
        for source_path in sorted(root.rglob("*")):
            if not source_path.is_file():
                continue
            language = SOURCE_EXTENSIONS.get(source_path.suffix)
            if not language or language not in languages:
                continue
            if f"{os.sep}source{os.sep}" not in str(source_path):
                continue
            binary_paths = matching_binary_paths(source_path)
            if require_binary and not binary_paths:
                continue
            if not binary_paths:
                binary_paths = [Path("")]
            for binary_path in binary_paths:
                entry_id = sanitize_id(f"{rel(binary_path)}::{rel(source_path)}")
                entries.append(
                    BenchmarkEntry(
                        id=entry_id,
                        binary_path=binary_path,
                        source_path=source_path,
                        language=language,
                        tags=tags + [language],
                        behavior_cases=spec.get("behavior_cases"),
                    )
                )

    dedup: dict[tuple[str, str], BenchmarkEntry] = {}
    for entry in entries:
        key = (str(entry.binary_path), str(entry.source_path))
        dedup.setdefault(key, entry)
    return list(dedup.values())


def filter_entries(
    entries: list[BenchmarkEntry],
    entry_ids: list[str] | None,
    required_tags: list[str] | None,
) -> list[BenchmarkEntry]:
    if entry_ids:
        wanted_ids = set(entry_ids)
        entries = [entry for entry in entries if entry.id in wanted_ids]
    if required_tags:
        wanted_tags = set(required_tags)
        entries = [
            entry
            for entry in entries
            if wanted_tags.issubset(set(entry.tags))
        ]
    return entries


def matching_binary_paths(source_path: Path) -> list[Path]:
    parts = list(source_path.parts)
    try:
        source_idx = parts.index("source")
    except ValueError:
        return []
    if source_idx + 1 >= len(parts):
        return []
    prefix = Path(*parts[:source_idx])
    language_dir = parts[source_idx + 1]
    stem = source_path.stem
    binary_root = prefix / "binary" / language_dir
    if not binary_root.exists():
        return []
    matches: list[Path] = []
    for candidate in sorted(binary_root.rglob("*")):
        if not candidate.is_file():
            continue
        if "_ghidra" in str(candidate):
            continue
        if candidate.name == stem or candidate.stem == stem:
            matches.append(candidate)
    return matches


def infer_entry_arch(entry: BenchmarkEntry) -> str:
    for tag in entry.tags:
        normalized = str(tag).strip().lower()
        if normalized in KNOWN_ARCH_TAGS:
            return "x86-64" if normalized == "x86_64" else normalized
    parts = [part.lower() for part in entry.binary_path.parts]
    try:
        binary_idx = parts.index("binary")
    except ValueError:
        binary_idx = -1
    if binary_idx >= 0 and binary_idx + 1 < len(parts):
        candidate = parts[binary_idx + 1]
        return "x86-64" if candidate == "x86_64" else candidate
    return "unknown"


def source_param_shape(param_kinds: list[str]) -> str:
    if not param_kinds:
        return "arity_0"
    counts = Counter(param_kinds)
    if len(counts) == 1:
        return f"{param_kinds[0]}_arity_{len(param_kinds)}"
    return "+".join(f"{kind}{counts[kind]}" for kind in sorted(counts))


def strip_comments(text: str) -> str:
    text = BLOCK_COMMENT_RE.sub(lambda m: "\n" * m.group(0).count("\n"), text)
    return LINE_COMMENT_RE.sub("", text)


def find_matching_brace(text: str, open_idx: int) -> int | None:
    depth = 0
    i = open_idx
    in_string: str | None = None
    escaped = False
    while i < len(text):
        ch = text[i]
        if in_string:
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == in_string:
                in_string = None
        else:
            if ch in ("'", '"'):
                in_string = ch
            elif ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    return i
        i += 1
    return None


def find_matching_paren(text: str, open_idx: int) -> int | None:
    depth = 0
    i = open_idx
    in_string: str | None = None
    escaped = False
    while i < len(text):
        ch = text[i]
        if in_string:
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == in_string:
                in_string = None
        else:
            if ch in ("'", '"'):
                in_string = ch
            elif ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
                if depth == 0:
                    return i
        i += 1
    return None


def split_params(params: str) -> list[str]:
    params = params.strip()
    if not params or params == "void":
        return []
    result: list[str] = []
    depth = 0
    start = 0
    for i, ch in enumerate(params):
        if ch in "(<[":
            depth += 1
        elif ch in ")>]":
            depth = max(0, depth - 1)
        elif ch == "," and depth == 0:
            result.append(params[start:i].strip())
            start = i + 1
    tail = params[start:].strip()
    if tail:
        result.append(tail)
    return result


def classify_param(param: str, language: str, pointer_aliases: set[str] | None = None) -> str:
    lowered = param.lower()
    words = set(WORD_RE.findall(lowered))
    if language in {"c", "cpp"} and pointer_aliases and words.intersection(pointer_aliases):
        return "aggregate_or_pointer"
    if language in {"c", "cpp"} and any(token in lowered for token in ["*", "[]", "["]):
        if words.intersection(INTEGRAL_WORDS):
            return "int_ptr"
        return "aggregate_or_pointer"
    if any(token in lowered for token in ["*", "&", "[]", "slice", "vec", "vector", "["]):
        return "aggregate_or_pointer"
    if language == "go" and "int" in words:
        return "int"
    if language == "rust" and words.intersection({"u32", "usize", "u64"}):
        return "uint"
    if language == "rust" and words.intersection({"i32", "isize", "i64"}):
        return "int"
    if language in {"c", "cpp"} and words.intersection(UNSIGNED_INTEGRAL_WORDS):
        return "uint"
    if words.intersection(INTEGRAL_WORDS):
        return "int"
    if not words:
        return "unknown"
    return "unsupported"


def param_name(param: str, index: int) -> str:
    cleaned = ARRAY_SUFFIX_RE.sub("", param)
    words = WORD_RE.findall(cleaned)
    type_words = {
        "const",
        "volatile",
        "restrict",
        "signed",
        "unsigned",
        "short",
        "long",
        "int",
        "char",
        "void",
        "bool",
        "static",
    }
    for word in reversed(words):
        if word.lower() not in type_words:
            return word
    return f"param_{index + 1}"


def param_names(params: str) -> list[str]:
    return [param_name(param, index) for index, param in enumerate(split_params(params))]


def classify_return(signature: str, name: str, params: str, language: str) -> str:
    sig = " ".join(signature.strip().split())
    if language == "go":
        after = sig.split(")", 1)[-1].strip()
        return "int" if after == "int" else ("void" if not after else "unsupported")
    if language == "rust":
        m = RETURN_ARROW_RE.search(sig)
        if not m:
            return "void"
        ret = m.group(1).strip().lower()
        return "int" if ret in {"i32", "u32", "usize", "isize", "i64", "u64"} else "unsupported"
    before_name = sig.rsplit(name, 1)[0].strip()
    before_name = ACCESS_LABEL_RE.sub("", before_name)
    if not before_name or before_name.endswith("~"):
        return "void"
    words = set(WORD_RE.findall(before_name.lower()))
    if "void" in words:
        return "void"
    if words.intersection(INTEGRAL_WORDS):
        return "int"
    return "unsupported"


def extract_source_functions(path: Path, language: str) -> list[SourceFunction]:
    text = path.read_text(encoding="utf-8", errors="replace")
    clean = strip_comments(text)
    if language == "go":
        return extract_go_functions(clean)
    if language == "rust":
        return extract_rust_functions(clean)
    return extract_c_like_functions(clean, language)


def extract_go_functions(text: str) -> list[SourceFunction]:
    funcs: list[SourceFunction] = []
    for match in GO_FUNCTION_RE.finditer(text):
        end = find_matching_brace(text, match.end() - 1)
        if end is None:
            continue
        name = match.group(1)
        params = match.group(2)
        signature = text[match.start() : match.end() - 1].strip()
        params_split = split_params(params)
        funcs.append(
            SourceFunction(
                name=name,
                signature=signature,
                body=text[match.end() : end],
                return_kind=classify_return(signature, name, params, "go"),
                param_kinds=[classify_param(p, "go") for p in params_split],
                param_names=param_names(params),
                line=text.count("\n", 0, match.start()) + 1,
                is_static=False,
            )
        )
    return funcs


def extract_rust_functions(text: str) -> list[SourceFunction]:
    funcs: list[SourceFunction] = []
    for match in RUST_FUNCTION_RE.finditer(text):
        end = find_matching_brace(text, match.end() - 1)
        if end is None:
            continue
        name = match.group(1)
        params = match.group(2)
        signature = text[match.start() : match.end() - 1].strip()
        params_split = split_params(params)
        funcs.append(
            SourceFunction(
                name=name,
                signature=signature,
                body=text[match.end() : end],
                return_kind=classify_return(signature, name, params, "rust"),
                param_kinds=[classify_param(p, "rust") for p in params_split],
                param_names=param_names(params),
                line=text.count("\n", 0, match.start()) + 1,
                is_static=False,
            )
        )
    return funcs


def extract_c_like_functions(text: str, language: str) -> list[SourceFunction]:
    funcs: list[SourceFunction] = []
    pointer_aliases = c_like_pointer_typedef_aliases(text) if language in {"c", "cpp"} else set()
    for open_idx, ch in enumerate(text):
        if ch != "{":
            continue
        prefix = text[:open_idx].rstrip()
        start = max(prefix.rfind(";"), prefix.rfind("}"), prefix.rfind("{")) + 1
        signature = prefix[start:].strip()
        signature = C_LIKE_ACCESS_PREFIX_RE.sub("", signature)
        if not signature or "=" in signature:
            continue
        m = C_LIKE_FUNCTION_RE.search(signature)
        if not m:
            continue
        name = m.group(1).split("::")[-1]
        if name in {"if", "for", "while", "switch", "catch", "return"}:
            continue
        if C_LIKE_TYPE_DECL_RE.search(signature):
            continue
        end = find_matching_brace(text, open_idx)
        if end is None:
            continue
        params = m.group(2)
        params_split = split_params(params)
        funcs.append(
            SourceFunction(
                name=name,
                signature=signature,
                body=text[open_idx + 1 : end],
                return_kind=classify_return(signature, name, params, language),
                param_kinds=[classify_param(p, language, pointer_aliases) for p in params_split],
                param_names=param_names(params),
                line=text.count("\n", 0, start) + 1,
                is_static=bool(re.search(r"\bstatic\b", signature)),
            )
        )
    return funcs


def c_like_pointer_typedef_aliases(text: str) -> set[str]:
    aliases: set[str] = set()
    for match in re.finditer(r"\btypedef\b[^;]*\(\s*\*\s*([A-Za-z_]\w*)\s*\)\s*\([^;]*\)\s*;", text):
        aliases.add(match.group(1).lower())
    for match in re.finditer(r"\btypedef\b[^;()]*\*\s*([A-Za-z_]\w*)\s*;", text):
        aliases.add(match.group(1).lower())
    return aliases


def normalize_name(name: str) -> str:
    name = TRAILING_DECORATION_RE.sub("", name.strip().lower())
    name = NORMALIZE_PREFIX_RE.sub("", name)
    return NON_ALNUM_RE.sub("", name)


def run_fission_list(binary_path: Path, fission_bin: Path, timeout_sec: int) -> tuple[list[FissionFunction], str | None]:
    cmd = [str(fission_bin), "list", str(binary_path)]
    try:
        res = subprocess.run(
            cmd,
            cwd=ROOT_DIR,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
            check=True,
        )
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as exc:
        detail = getattr(exc, "stderr", "") or getattr(exc, "stdout", "") or str(exc)
        return [], detail.strip() or "list_failed"

    funcs: list[FissionFunction] = []
    for line in res.stdout.splitlines():
        m = FISSION_LIST_LINE_RE.search(line)
        if not m:
            continue
        name = TRAILING_DECORATION_RE.sub("", m.group(2).strip()).strip()
        funcs.append(FissionFunction(address=canonical_address(m.group(1)), name=name))
    return funcs, None


def run_fission_list_cached(
    binary_path: Path,
    fission_bin: Path,
    timeout_sec: int,
    cache: dict[str, dict[str, Any]],
    cache_stats: Counter[str],
) -> tuple[list[FissionFunction], str | None]:
    key = list_cache_key(binary_path, fission_bin)
    cached = cache.get(key)
    if cached is not None:
        cache_stats["hit"] += 1
        funcs = [
            FissionFunction(address=str(raw.get("address")), name=str(raw.get("name")))
            for raw in cached.get("functions", [])
            if isinstance(raw, dict) and raw.get("address") and raw.get("name")
        ]
        error = cached.get("error")
        return funcs, str(error) if error else None

    cache_stats["miss"] += 1
    funcs, error = run_fission_list(binary_path, fission_bin, timeout_sec)
    cache[key] = {
        "functions": [{"address": func.address, "name": func.name} for func in funcs],
        "error": error,
    }
    cache_stats["stored"] += 1
    return funcs, error


def canonical_address(value: str | int) -> str:
    if isinstance(value, int):
        return f"0x{value:x}"
    text = str(value).strip()
    if not text:
        return "0x0"
    return f"0x{int(text, 16):x}"


def match_function(source: SourceFunction, funcs: list[FissionFunction]) -> tuple[str, FissionFunction | None, list[str]]:
    literal_exact = [f for f in funcs if f.name == source.name]
    if len(literal_exact) == 1:
        return "matched", literal_exact[0], []
    if len(literal_exact) > 1:
        return "ambiguous", None, [f"{f.address}:{f.name}" for f in literal_exact[:8]]

    src_norm = normalize_name(source.name)
    exact = [f for f in funcs if normalize_name(f.name) == src_norm]
    if len(exact) == 1:
        return "matched", exact[0], []
    if len(exact) > 1:
        return "ambiguous", None, [f"{f.address}:{f.name}" for f in exact[:8]]

    suffix = [
        f
        for f in funcs
        if normalize_name(f.name).endswith(src_norm) and src_norm and not normalize_name(f.name).startswith("sub")
    ]
    if len(suffix) == 1:
        return "matched", suffix[0], []
    if len(suffix) > 1:
        return "ambiguous", None, [f"{f.address}:{f.name}" for f in suffix[:8]]
    return "unmapped", None, []


def select_source_functions(
    source_functions: list[SourceFunction],
    fission_funcs: list[FissionFunction],
    limit: int | None,
    fission_error: str | None = None,
) -> list[SourceFunction]:
    if limit is None:
        return source_functions
    if limit <= 0:
        return []
    if fission_error or not fission_funcs:
        return source_functions[:limit]

    matched: list[SourceFunction] = []
    fallback: list[SourceFunction] = []
    for func in source_functions:
        status, matched_func, _ = match_function(func, fission_funcs)
        if status == "matched" and matched_func is not None:
            matched.append(func)
        else:
            fallback.append(func)
    return (matched + fallback)[:limit]


def source_call_counts(body: str) -> Counter[str]:
    return Counter(call_names_for_fingerprint(body))


def is_function_definition_call_match(text: str, match: re.Match[str]) -> bool:
    open_idx = text.rfind("(", match.start(), match.end())
    if open_idx < 0:
        return False
    close_idx = find_matching_paren(text, open_idx)
    if close_idx is None:
        return False
    next_idx = close_idx + 1
    while next_idx < len(text) and text[next_idx].isspace():
        next_idx += 1
    if next_idx >= len(text) or text[next_idx] != "{":
        return False
    statement_start = max(
        text.rfind(";", 0, match.start()),
        text.rfind("{", 0, match.start()),
        text.rfind("}", 0, match.start()),
    ) + 1
    prefix = text[statement_start:match.start()].strip()
    return bool(prefix)


def call_names_for_fingerprint(code: str) -> list[str]:
    stripped = strip_comments(code)
    calls: list[str] = []
    for match in CALL_RE.finditer(stripped):
        lowered = match.group(1).split("::")[-1].lower()
        if lowered in CALL_EXCLUDE:
            continue
        if stripped[match.end() :].lstrip().startswith("*"):
            continue
        if is_function_definition_call_match(stripped, match):
            continue
        calls.append(normalize_name(lowered))
    return calls


def function_pointer_param_names(func: SourceFunction | None) -> set[str]:
    if func is None:
        return set()
    return {
        normalize_name(name)
        for name, kind in zip(func.param_names, func.param_kinds, strict=False)
        if kind == "aggregate_or_pointer"
    }


def indirect_cast_call_names_for_fingerprint(code: str) -> list[str]:
    stripped = strip_comments(code)
    calls = [
        normalize_name(match.group(1))
        for match in INDIRECT_CAST_CALL_RE.finditer(stripped)
    ]
    calls.extend(
        normalize_name(match.group(1))
        for match in DEREF_INDIRECT_CALL_RE.finditer(stripped)
    )
    return calls


def add_call_fingerprint(counter: Counter[str], code: str, func: SourceFunction | None) -> None:
    pointer_params = function_pointer_param_names(func)
    for call in call_names_for_fingerprint(code):
        if call in pointer_params:
            counter["call:indirect_param"] += 1
        else:
            counter[f"call:{call}"] += 1
    for call in indirect_cast_call_names_for_fingerprint(code):
        if call in pointer_params or call.startswith("param"):
            counter["call:indirect_param"] += 1
        else:
            counter["call:indirect"] += 1


def matched_source_names(
    source_functions: list[SourceFunction],
    fission_funcs: list[FissionFunction],
) -> set[str]:
    matched: set[str] = set()
    for func in source_functions:
        status, matched_func, _ = match_function(func, fission_funcs)
        if status == "matched" and matched_func is not None:
            matched.add(normalize_name(func.name))
    return matched


def reachable_source_calls(
    seed_names: set[str],
    functions_by_name: dict[str, SourceFunction],
    max_depth: int = 2,
) -> dict[str, set[str]]:
    callers_by_callee: dict[str, set[str]] = {}
    frontier: list[tuple[str, str, int]] = [
        (seed_name, seed_name, 0)
        for seed_name in sorted(seed_names)
        if seed_name in functions_by_name
    ]
    visited: set[tuple[str, str]] = set()
    while frontier:
        root_name, current_name, depth = frontier.pop(0)
        key = (root_name, current_name)
        if key in visited or depth >= max_depth:
            continue
        visited.add(key)
        current = functions_by_name.get(current_name)
        root = functions_by_name.get(root_name)
        if current is None or root is None:
            continue
        for callee_name in source_call_counts(current.body):
            if not callee_name or callee_name == current_name:
                continue
            callers_by_callee.setdefault(callee_name, set()).add(root.name)
            if callee_name in functions_by_name:
                frontier.append((root_name, callee_name, depth + 1))
    return callers_by_callee


def filter_inlined_static_source_functions(
    source_functions: list[SourceFunction],
    all_source_functions: list[SourceFunction],
    fission_funcs: list[FissionFunction],
    explicit_function_filter: bool,
    fission_error: str | None = None,
) -> tuple[list[SourceFunction], list[dict[str, Any]]]:
    if explicit_function_filter or fission_error or not fission_funcs:
        return source_functions, []
    functions_by_name = {
        normalize_name(func.name): func
        for func in all_source_functions
    }
    matched_names = matched_source_names(all_source_functions, fission_funcs)
    reachable_callers = reachable_source_calls(matched_names, functions_by_name)
    kept: list[SourceFunction] = []
    suppressed: list[dict[str, Any]] = []
    for func in source_functions:
        func_name = normalize_name(func.name)
        status, matched_func, _ = match_function(func, fission_funcs)
        is_matched = status == "matched" and matched_func is not None
        callers = sorted(reachable_callers.get(func_name) or [])
        if func.is_static and not is_matched and callers:
            suppressed.append(
                {
                    "function_name": func.name,
                    "source_line": func.line,
                    "source_signature": func.signature,
                    "reason": "static_source_function_reachable_from_matched_source_but_absent_from_binary_symbols",
                    "matched_callers": callers,
                }
            )
            continue
        kept.append(func)
    return kept, suppressed


def filter_source_functions(
    source_functions: list[SourceFunction],
    function_names: list[str] | None,
) -> list[SourceFunction]:
    if not function_names:
        return source_functions
    wanted = set(function_names)
    return [func for func in source_functions if func.name in wanted]


def parse_json_loose(text: str) -> Any:
    text = text.strip()
    if not text:
        raise json.JSONDecodeError("empty", text, 0)
    starts = [idx for idx in (text.find("["), text.find("{")) if idx >= 0]
    if starts:
        text = text[min(starts) :]
    return json.loads(text)


def run_fission_decomp(
    binary_path: Path,
    address: str,
    fission_bin: Path,
    timeout_sec: int,
    include_debug_decomp: bool = False,
    debug_decomp_bundle_path: Path | None = None,
) -> dict[str, Any]:
    cmd = [
        str(fission_bin),
        "decomp",
        str(binary_path),
        "--addr",
        address,
        "--json",
        "--no-header",
        "--no-warnings",
        "--timeout-ms",
        str(max(1000, timeout_sec * 1000)),
    ]
    if include_debug_decomp:
        cmd.append("--debug-decomp")
    if debug_decomp_bundle_path is not None:
        debug_decomp_bundle_path.parent.mkdir(parents=True, exist_ok=True)
        cmd.extend(["--debug-decomp-bundle", str(debug_decomp_bundle_path)])
    start = time.perf_counter()
    try:
        res = subprocess.run(
            cmd,
            cwd=ROOT_DIR,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
            check=True,
        )
    except subprocess.TimeoutExpired:
        return {"success": False, "failure_kind": "timeout", "wall_sec": round(time.perf_counter() - start, 6)}
    except subprocess.CalledProcessError as exc:
        detail = (exc.stderr or exc.stdout or str(exc)).strip()
        return {
            "success": False,
            "failure_kind": "command_failed",
            "failure_detail": detail[-4000:],
            "wall_sec": round(time.perf_counter() - start, 6),
        }

    try:
        payload = parse_json_loose(res.stdout)
    except json.JSONDecodeError as exc:
        return {
            "success": False,
            "failure_kind": "invalid_json",
            "failure_detail": str(exc),
            "wall_sec": round(time.perf_counter() - start, 6),
        }

    if isinstance(payload, list):
        func = payload[0] if payload else {}
    else:
        func = (payload.get("functions") or [{}])[0] if isinstance(payload, dict) else {}
    if func.get("error"):
        return {
            "success": False,
            "failure_kind": "decompile_error",
            "failure_detail": func.get("error"),
            "wall_sec": round(time.perf_counter() - start, 6),
            "engine_used": func.get("engine_used"),
            "debug_decomp": debug_decomp_summary(func.get("debug_decomp")),
        }
    code = func.get("code") or ""
    if not code.strip():
        return {"success": False, "failure_kind": "empty_output", "wall_sec": round(time.perf_counter() - start, 6)}
    return {
        "success": True,
        "code": code,
        "wall_sec": round(time.perf_counter() - start, 6),
        "engine_used": func.get("engine_used"),
        "fell_back": bool(func.get("fell_back", False)),
        "fallback_reason": func.get("fallback_reason"),
        "preview_build_stats": func.get("preview_build_stats"),
        "debug_decomp": debug_decomp_summary(func.get("debug_decomp")),
        "debug_decomp_bundle_path": rel(debug_decomp_bundle_path)
        if debug_decomp_bundle_path is not None
        else None,
    }


def debug_decomp_summary(debug_decomp: Any) -> dict[str, Any] | None:
    if not isinstance(debug_decomp, dict):
        return None
    quality = debug_decomp.get("quality_evidence") if isinstance(debug_decomp.get("quality_evidence"), dict) else {}
    pipeline = (
        debug_decomp.get("rust_sleigh_pipeline")
        if isinstance(debug_decomp.get("rust_sleigh_pipeline"), dict)
        else {}
    )
    pcode_blocks = pipeline.get("raw_pcode_blocks") if isinstance(pipeline.get("raw_pcode_blocks"), list) else []
    sampled_pcode_blocks = pcode_blocks[:64]
    return {
        "stage_status": debug_decomp.get("stage_status"),
        "stage_metrics": debug_decomp.get("stage_metrics"),
        "owner_buckets": debug_decomp.get("owner_buckets") or [],
        "rust_sleigh_pipeline": {
            key: pipeline.get(key)
            for key in [
                "entry_address",
                "max_bytes",
                "instruction_limit",
                "decode_attempt_count",
                "decode_stop_reason",
                "template_source_counts",
                "raw_pcode_block_count",
                "raw_pcode_op_count",
                "raw_pcode_edge_count",
                "raw_pcode_terminal_opcode_counts",
                "raw_pcode_block_evidence_truncated",
                "strict_indirect_retry_attempted",
                "nir_fallback_kind",
                "nir_fallback_kind_refined",
                "nir_fallback_reason_summary",
                "pipeline_stage_status",
            ]
            if key in pipeline
        } | (
            {
                "raw_pcode_blocks_sampled_count": len(sampled_pcode_blocks),
                "raw_pcode_blocks": sampled_pcode_blocks,
            }
            if sampled_pcode_blocks
            else {}
        ),
        "quality_evidence": {
            key: quality.get(key)
            for key in [
                "validated_pcode_op_count",
                "invalid_pcode_shape_count",
                "replacement_plan_rejected_missing_merge_count",
                "replacement_plan_rejected_alias_unsafe_count",
                "forced_linear_structuring_count",
                "structuring_irreducible_scc_count",
                "region_emit_ready_failed_count",
                "call_target_unresolved_sub_fallback_count",
                "call_prototype_signature_missing_count",
                "typed_fact_conflict_count",
            ]
            if key in quality
        },
    }


def run_fission_decomp_cached(
    binary_path: Path,
    address: str,
    fission_bin: Path,
    timeout_sec: int,
    include_debug_decomp: bool,
    debug_decomp_bundle_path: Path | None,
    cache: dict[str, dict[str, Any]],
    cache_lock: threading.Lock,
    cache_stats: Counter[str],
) -> dict[str, Any]:
    key = decomp_cache_key(binary_path, address, fission_bin, include_debug_decomp)
    with cache_lock:
        cached = cache.get(key)
        if cached is not None:
            cache_stats["hit"] += 1
    if cached is not None:
        cached_result = dict(cached)
        cached_result["decomp_cache_status"] = "hit"
        if (
            include_debug_decomp
            and debug_decomp_bundle_path is not None
            and not debug_decomp_bundle_path.exists()
        ):
            cached_result = run_fission_decomp(
                binary_path,
                address,
                fission_bin,
                timeout_sec,
                include_debug_decomp=include_debug_decomp,
                debug_decomp_bundle_path=debug_decomp_bundle_path,
            )
            cached_result["decomp_cache_status"] = "refreshed_debug_bundle"
            with cache_lock:
                cache[key] = cached_result
                cache_stats["stored"] += 1
        elif debug_decomp_bundle_path is not None:
            cached_result["debug_decomp_bundle_path"] = rel(debug_decomp_bundle_path)
        return cached_result
    with cache_lock:
        cache_stats["miss"] += 1
    decomp = run_fission_decomp(
        binary_path,
        address,
        fission_bin,
        timeout_sec,
        include_debug_decomp=include_debug_decomp,
        debug_decomp_bundle_path=debug_decomp_bundle_path,
    )
    decomp["decomp_cache_status"] = "miss"
    with cache_lock:
        cache.setdefault(key, decomp)
        cache_stats["stored"] += 1
    return dict(decomp)


def decomp_result_from_function_payload(
    func: dict[str, Any],
    wall_sec: float,
    debug_bundle: dict[str, Any] | None,
    debug_decomp_bundle_path: Path | None,
) -> dict[str, Any]:
    if func.get("error"):
        return {
            "success": False,
            "failure_kind": "decompile_error",
            "failure_detail": func.get("error"),
            "wall_sec": round(float(func.get("decomp_sec", wall_sec) or wall_sec), 6),
            "engine_used": func.get("engine_used"),
            "debug_decomp": debug_decomp_summary(debug_bundle or func.get("debug_decomp")),
        }
    code = func.get("code") or ""
    if not code.strip():
        return {
            "success": False,
            "failure_kind": "empty_output",
            "wall_sec": round(float(func.get("decomp_sec", wall_sec) or wall_sec), 6),
        }
    return {
        "success": True,
        "code": code,
        "wall_sec": round(float(func.get("decomp_sec", wall_sec) or wall_sec), 6),
        "engine_used": func.get("engine_used"),
        "fell_back": bool(func.get("fell_back", False)),
        "fallback_reason": func.get("fallback_reason"),
        "preview_build_stats": func.get("preview_build_stats"),
        "debug_decomp": debug_decomp_summary(debug_bundle or func.get("debug_decomp")),
        "debug_decomp_bundle_path": rel(debug_decomp_bundle_path)
        if debug_decomp_bundle_path is not None
        else None,
    }


def write_single_debug_bundle(path: Path, bundle: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        dump_json_pretty({"schema_version": 1, "functions": [bundle]}),
        encoding="utf-8",
    )


def run_fission_decomp_batch(
    binary_path: Path,
    address_paths: list[tuple[str, Path | None]],
    fission_bin: Path,
    timeout_sec: int,
    include_debug_decomp: bool,
    output_dir: Path,
    entry_id: str,
) -> dict[str, dict[str, Any]]:
    if not address_paths:
        return {}
    batch_dir = output_dir / "batch_decomp"
    batch_dir.mkdir(parents=True, exist_ok=True)
    slug = sanitize_id(entry_id)
    address_file = batch_dir / f"{slug}-addresses.txt"
    address_file.write_text(
        "".join(f"{address}\n" for address, _path in address_paths),
        encoding="utf-8",
    )
    debug_bundle_path = batch_dir / f"{slug}-debug-decomp.json"
    cmd = [
        str(fission_bin),
        "decomp",
        str(binary_path),
        "--addresses-file",
        str(address_file),
        "--benchmark",
        "--no-header",
        "--no-warnings",
        "--timeout-ms",
        str(max(1000, timeout_sec * 1000)),
    ]
    if include_debug_decomp:
        cmd.append("--debug-decomp")
        cmd.extend(["--debug-decomp-bundle", str(debug_bundle_path)])
    start = time.perf_counter()
    try:
        res = subprocess.run(
            cmd,
            cwd=ROOT_DIR,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=max(timeout_sec, timeout_sec * len(address_paths)),
            check=True,
        )
    except (subprocess.TimeoutExpired, subprocess.CalledProcessError):
        return {}

    wall_sec = round(time.perf_counter() - start, 6)
    try:
        payload = parse_json_loose(res.stdout)
    except json.JSONDecodeError:
        return {}
    functions = payload.get("functions") if isinstance(payload, dict) else None
    if not isinstance(functions, list):
        return {}

    debug_by_address: dict[str, dict[str, Any]] = {}
    if include_debug_decomp and debug_bundle_path.exists():
        try:
            debug_payload = json.loads(debug_bundle_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            debug_payload = {}
        debug_functions = debug_payload.get("functions", []) if isinstance(debug_payload, dict) else []
        for bundle in debug_functions:
            if not isinstance(bundle, dict):
                continue
            function = bundle.get("function") if isinstance(bundle.get("function"), dict) else {}
            address = function.get("resolved_address") or function.get("requested_address")
            if isinstance(address, str):
                debug_by_address[canonical_address(address)] = bundle

    requested_paths = {
        canonical_address(address): path
        for address, path in address_paths
    }
    results: dict[str, dict[str, Any]] = {}
    for func in functions:
        if not isinstance(func, dict):
            continue
        address = func.get("address")
        if not isinstance(address, str):
            continue
        key = canonical_address(address)
        debug_bundle = debug_by_address.get(key)
        requested_path = requested_paths.get(key)
        if debug_bundle is not None and requested_path is not None:
            write_single_debug_bundle(requested_path, debug_bundle)
        results[key] = decomp_result_from_function_payload(
            func,
            wall_sec,
            debug_bundle,
            requested_path,
        )
    return results


def prewarm_decomp_cache_for_entry(
    entry: BenchmarkEntry,
    source_functions: list[SourceFunction],
    fission_funcs: list[FissionFunction],
    fission_error: str | None,
    fission_bin: Path,
    timeout_sec: int,
    include_debug_decomp: bool,
    output_dir: Path,
    cache: dict[str, dict[str, Any]],
    cache_lock: threading.Lock,
    cache_stats: Counter[str],
) -> None:
    if fission_error or len(source_functions) <= 1:
        return
    address_paths: list[tuple[str, Path | None]] = []
    address_to_cache_key: dict[str, str] = {}
    for func in source_functions:
        _status, matched, _candidates = match_function(func, fission_funcs)
        if matched is None:
            continue
        key = decomp_cache_key(entry.binary_path, matched.address, fission_bin, include_debug_decomp)
        bundle_path = (
            debug_bundle_path_for_parts(output_dir, entry.id, func.name, matched.address)
            if include_debug_decomp
            else None
        )
        with cache_lock:
            cached = cache.get(key)
        if cached is not None and (
            not include_debug_decomp
            or bundle_path is None
            or bundle_path.exists()
        ):
            continue
        canonical = canonical_address(matched.address)
        address_to_cache_key[canonical] = key
        address_paths.append((matched.address, bundle_path))
    if len(address_paths) <= 1:
        return
    batch_results = run_fission_decomp_batch(
        entry.binary_path,
        address_paths,
        fission_bin,
        timeout_sec,
        include_debug_decomp,
        output_dir,
        entry.id,
    )
    if not batch_results:
        return
    with cache_lock:
        for address, result in batch_results.items():
            key = address_to_cache_key.get(address)
            if key is None:
                continue
            cache[key] = result
            cache_stats["miss"] += 1
            cache_stats["stored"] += 1


def shell_command(parts: list[Any]) -> str:
    return " ".join(shlex.quote(str(part)) for part in parts)


def debug_bundle_path_for_parts(
    output_dir: Path,
    entry_id: str | None,
    function_name: str | None,
    address: str | None,
) -> Path:
    entry = sanitize_id(str(entry_id or "entry"))
    function = sanitize_id(str(function_name or "function"))
    address = sanitize_id(str(address or "no-address"))
    return output_dir / "debug_decomp" / entry / f"{function}-{address}.json"


def debug_bundle_path_for_row(output_dir: Path, row: dict[str, Any]) -> Path:
    return debug_bundle_path_for_parts(
        output_dir,
        row.get("entry_id"),
        row.get("function_name"),
        row.get("address"),
    )


def debug_triage_path_for_row(output_dir: Path, row: dict[str, Any], kind: str, suffix: str) -> Path:
    stem = "-".join(
        [
            sanitize_id(str(row.get("entry_id") or "entry")),
            sanitize_id(str(row.get("function_name") or "function")),
            sanitize_id(str(row.get("address") or "unknown")),
        ]
    )
    return output_dir / "debug_triage" / kind / f"{stem}.{suffix}"


def behavior_artifact_dir_for_row(
    output_dir: Path,
    entry: BenchmarkEntry,
    func: SourceFunction,
    address: str | None,
) -> Path:
    entry_id = sanitize_id(entry.id)
    function = sanitize_id(func.name)
    address_id = sanitize_id(address or "no-address")
    return output_dir / "behavior" / entry_id / f"{function}-{address_id}"


def write_behavior_artifacts(
    artifact_dir: Path,
    oracle_code: str,
    candidate_code: str,
    oracle: dict[str, Any] | None,
    candidate: dict[str, Any] | None,
) -> None:
    artifact_dir.mkdir(parents=True, exist_ok=True)
    (artifact_dir / "oracle.c").write_text(oracle_code, encoding="utf-8")
    (artifact_dir / "candidate.c").write_text(candidate_code, encoding="utf-8")
    (artifact_dir / "result.json").write_text(
        dump_json_pretty(
            {
                "oracle": oracle,
                "candidate": candidate,
            }
        ),
        encoding="utf-8",
    )


def decomp_debug_command_for_row(row: dict[str, Any], fission_bin: Path, output_dir: Path) -> dict[str, Any] | None:
    address = row.get("address")
    binary_path = row.get("binary_path")
    if not address or not binary_path:
        return None
    bundle_path = debug_bundle_path_for_row(output_dir, row)
    cmd = [
        fission_bin,
        "decomp",
        resolve_path(str(binary_path)),
        "--addr",
        str(address),
        "--json",
        "--no-header",
        "--no-warnings",
        "--debug-decomp",
        "--debug-decomp-bundle",
        bundle_path,
    ]
    return {
        "debug_decomp_bundle_path": rel(bundle_path),
        "debug_decomp_command": shell_command(cmd),
        "disasm_function_command": shell_command(
            [
                fission_bin,
                "disasm",
                resolve_path(str(binary_path)),
                "--addr",
                str(address),
                "--function",
                "--json",
            ]
        ),
        "xrefs_function_command": shell_command(
            [
                fission_bin,
                "xrefs",
                resolve_path(str(binary_path)),
                "--function",
                str(address),
                "--json",
            ]
        ),
        "preview_candidate_command": None,
        "preview_candidate_note": "inventory preview-candidates is deprecated with native_decomp removal; use debug-decomp and function-facts",
        "function_facts_command": shell_command(
            [
                fission_bin,
                "inventory",
                "function-facts",
                resolve_path(str(binary_path)),
                "--addr",
                str(address),
                "--output-jsonl",
                output_dir / "function_facts" / f"{sanitize_id(str(row.get('entry_id') or 'entry'))}-{sanitize_id(str(row.get('function_name') or 'function'))}-{sanitize_id(str(address))}.jsonl",
                "--summary-json",
                output_dir / "function_facts" / f"{sanitize_id(str(row.get('entry_id') or 'entry'))}-{sanitize_id(str(row.get('function_name') or 'function'))}-{sanitize_id(str(address))}.json",
            ]
        ),
    }


def attach_debug_repro_commands(rows: list[dict[str, Any]], fission_bin: Path, output_dir: Path) -> None:
    for row in rows:
        command = decomp_debug_command_for_row(row, fission_bin, output_dir)
        if command is not None:
            row.update(command)


def top_debug_commands(rows: list[dict[str, Any]], limit: int = 12) -> list[dict[str, Any]]:
    candidates = [
        row
        for row in rows
        if row.get("debug_decomp_command") and float(row.get("semantic_score", 0.0) or 0.0) < 1.0
    ]
    candidates.sort(key=lambda row: (float(row.get("semantic_score", 0.0) or 0.0), row.get("function_name") or ""))
    return [
        {
            "entry_id": row.get("entry_id"),
            "function_name": row.get("function_name"),
            "address": row.get("address"),
            "semantic_score_percent": row.get("semantic_score_percent"),
            "behavior_status": row.get("behavior", {}).get("status"),
            "behavior_artifact_dir": row.get("behavior", {}).get("artifact_dir"),
            "debug_decomp_bundle_path": row.get("debug_decomp_bundle_path"),
            "debug_decomp_command": row.get("debug_decomp_command"),
            "disasm_function_command": row.get("disasm_function_command"),
            "xrefs_function_command": row.get("xrefs_function_command"),
            "preview_candidate_command": row.get("preview_candidate_command"),
            "preview_candidate_note": row.get("preview_candidate_note"),
            "function_facts_command": row.get("function_facts_command"),
        }
        for row in candidates[:limit]
    ]


def run_command_capture(cmd: list[Any], timeout_sec: int) -> dict[str, Any]:
    started = time.perf_counter()
    try:
        res = subprocess.run(
            [str(part) for part in cmd],
            cwd=ROOT_DIR,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
            check=False,
        )
        return {
            "command": shell_command(cmd),
            "returncode": res.returncode,
            "timed_out": False,
            "wall_sec": round(time.perf_counter() - started, 6),
            "stdout": res.stdout,
            "stderr": res.stderr,
        }
    except subprocess.TimeoutExpired as exc:
        return {
            "command": shell_command(cmd),
            "returncode": None,
            "timed_out": True,
            "wall_sec": round(time.perf_counter() - started, 6),
            "stdout": exc.stdout or "",
            "stderr": exc.stderr or "",
        }


def materialize_debug_triage_for_rows(
    selected: list[dict[str, Any]],
    fission_bin: Path,
    output_dir: Path,
    timeout_sec: int,
) -> list[dict[str, Any]]:
    triage_rows: list[dict[str, Any]] = []
    for row in selected:
        binary_path = resolve_path(str(row["binary_path"]))
        address = str(row["address"])
        decomp_bundle_path = debug_bundle_path_for_row(output_dir, row)
        decomp_capture_path = debug_triage_path_for_row(output_dir, row, "debug_decomp", "command.json")
        disasm_capture_path = debug_triage_path_for_row(output_dir, row, "disasm", "command.json")
        xrefs_capture_path = debug_triage_path_for_row(output_dir, row, "xrefs", "command.json")
        facts_jsonl_path = debug_triage_path_for_row(output_dir, row, "function_facts", "jsonl")
        facts_summary_path = debug_triage_path_for_row(output_dir, row, "function_facts", "summary.json")
        facts_capture_path = debug_triage_path_for_row(output_dir, row, "function_facts", "command.json")
        decomp_bundle_path.parent.mkdir(parents=True, exist_ok=True)
        decomp_capture_path.parent.mkdir(parents=True, exist_ok=True)
        disasm_capture_path.parent.mkdir(parents=True, exist_ok=True)
        xrefs_capture_path.parent.mkdir(parents=True, exist_ok=True)
        facts_jsonl_path.parent.mkdir(parents=True, exist_ok=True)

        decomp_capture = run_command_capture(
            [
                fission_bin,
                "decomp",
                binary_path,
                "--addr",
                address,
                "--json",
                "--no-header",
                "--no-warnings",
                "--debug-decomp",
                "--debug-decomp-bundle",
                decomp_bundle_path,
            ],
            timeout_sec,
        )
        decomp_capture_path.write_text(dump_json_pretty(decomp_capture), encoding="utf-8")

        disasm_capture = run_command_capture(
            [
                fission_bin,
                "disasm",
                binary_path,
                "--addr",
                address,
                "--function",
                "--json",
            ],
            timeout_sec,
        )
        disasm_capture_path.write_text(dump_json_pretty(disasm_capture), encoding="utf-8")

        xrefs_capture = run_command_capture(
            [
                fission_bin,
                "xrefs",
                binary_path,
                "--function",
                address,
                "--json",
            ],
            timeout_sec,
        )
        xrefs_capture_path.write_text(dump_json_pretty(xrefs_capture), encoding="utf-8")

        facts = run_command_capture(
            [
                fission_bin,
                "inventory",
                "function-facts",
                binary_path,
                "--addr",
                address,
                "--output-jsonl",
                facts_jsonl_path,
                "--summary-json",
                facts_summary_path,
            ],
            timeout_sec,
        )
        facts_capture_path.write_text(dump_json_pretty(facts), encoding="utf-8")

        triage = {
            "entry_id": row.get("entry_id"),
            "function_name": row.get("function_name"),
            "address": address,
            "semantic_score_percent": row.get("semantic_score_percent"),
            "behavior_status": row.get("behavior", {}).get("status"),
            "baseline_regression": row.get("baseline_regression"),
            "preview_candidate_note": row.get("preview_candidate_note"),
            "debug_decomp_capture_path": rel(decomp_capture_path),
            "debug_decomp_bundle_path": rel(decomp_bundle_path),
            "debug_decomp_returncode": decomp_capture.get("returncode"),
            "disasm_capture_path": rel(disasm_capture_path),
            "disasm_returncode": disasm_capture.get("returncode"),
            "xrefs_capture_path": rel(xrefs_capture_path),
            "xrefs_returncode": xrefs_capture.get("returncode"),
            "function_facts_capture_path": rel(facts_capture_path),
            "function_facts_jsonl_path": rel(facts_jsonl_path),
            "function_facts_summary_path": rel(facts_summary_path),
            "function_facts_returncode": facts.get("returncode"),
        }
        row["debug_decomp_bundle_path"] = rel(decomp_bundle_path)
        row["debug_triage"] = triage
        triage_rows.append(triage)
    return triage_rows


def materialize_debug_triage(
    rows: list[dict[str, Any]],
    fission_bin: Path,
    output_dir: Path,
    timeout_sec: int,
    limit: int,
) -> list[dict[str, Any]]:
    selected = [
        row
        for row in rows
        if row.get("address") and float(row.get("semantic_score", 0.0) or 0.0) < 1.0
    ]
    selected.sort(key=lambda row: (float(row.get("semantic_score", 0.0) or 0.0), row.get("function_name") or ""))
    return materialize_debug_triage_for_rows(selected[: max(0, limit)], fission_bin, output_dir, timeout_sec)


def materialize_regression_debug_triage(
    rows: list[dict[str, Any]],
    comparison: dict[str, Any],
    fission_bin: Path,
    output_dir: Path,
    timeout_sec: int,
    limit: int,
) -> list[dict[str, Any]]:
    rows_by_key = {row_key(row): row for row in rows if row.get("address")}
    selected: list[dict[str, Any]] = []
    seen: set[str] = set()
    for delta in comparison.get("top_regressions") or []:
        key = str(delta.get("key") or "")
        if not key or key in seen:
            continue
        row = rows_by_key.get(key)
        if row is None:
            continue
        row["baseline_regression"] = {
            "baseline_score_percent": delta.get("baseline_score_percent"),
            "current_score_percent": delta.get("current_score_percent"),
            "delta_percent": delta.get("delta_percent"),
            "baseline_behavior": delta.get("baseline_behavior"),
            "current_behavior": delta.get("current_behavior"),
        }
        selected.append(row)
        seen.add(key)
        if len(selected) >= max(0, limit):
            break
    return materialize_debug_triage_for_rows(selected, fission_bin, output_dir, timeout_sec)


def code_fingerprint(code: str, func: SourceFunction | None = None) -> Counter[str]:
    stripped = strip_comments(code)
    counter: Counter[str] = Counter()
    signature_func = func
    if signature_func is None:
        rendered_functions = extract_c_like_functions(stripped, "c")
        if rendered_functions:
            signature_func = rendered_functions[0]
    for word in WORD_BOUNDARY_RE.findall(stripped):
        lowered = word.lower()
        if lowered in CONTROL_WORDS:
            counter[f"ctrl:{lowered}"] += 1
    for op in ["<<", ">>", "==", "!=", "<=", ">=", "&&", "||", "->", "+", "-", "*", "/", "%", "&", "|", "^", "<", ">", "="]:
        counter[f"op:{op}"] += stripped.count(op)
    for const in CONST_RE.findall(stripped):
        counter[f"const:{const.lower()}"] += 1
    add_call_fingerprint(counter, stripped, signature_func)
    counter["mem:index"] += stripped.count("[")
    counter["mem:deref_or_ptr"] += stripped.count("*")
    counter["mem:field"] += stripped.count("->") + stripped.count(".")
    if func is not None:
        add_signature_fingerprint(counter, func.return_kind, func.param_kinds)
    elif signature_func is not None:
        add_signature_fingerprint(counter, signature_func.return_kind, signature_func.param_kinds)
    return +counter


def add_signature_fingerprint(counter: Counter[str], return_kind: str, param_kinds: list[str]) -> None:
    counter[f"sig:return:{return_kind}"] += 1
    counter[f"sig:param_count:{len(param_kinds)}"] += 1
    for kind in param_kinds:
        counter[f"sig:param:{kind}"] += 1


def add_rendered_signature_fingerprint(counter: Counter[str], code: str) -> None:
    functions = extract_c_like_functions(code, "c")
    if not functions:
        return
    rendered = functions[0]
    add_signature_fingerprint(counter, rendered.return_kind, rendered.param_kinds)


def rendered_signature_kinds(code: str) -> tuple[str, list[str]] | None:
    functions = extract_c_like_functions(strip_comments(code), "c")
    if not functions:
        return None
    rendered = functions[0]
    return rendered.return_kind, rendered.param_kinds


def inline_expanded_source_fingerprint(
    func: SourceFunction,
    functions_by_name: dict[str, SourceFunction],
    max_depth: int = 2,
) -> Counter[str]:
    return expand_source_body_fingerprint(
        func.body,
        functions_by_name,
        include_signature=func,
        max_depth=max_depth,
        visiting=(normalize_name(func.name),),
    )


def expand_source_body_fingerprint(
    body: str,
    functions_by_name: dict[str, SourceFunction],
    include_signature: SourceFunction | None,
    max_depth: int,
    visiting: tuple[str, ...],
) -> Counter[str]:
    counter = code_fingerprint(body, include_signature)
    if max_depth <= 0:
        return counter
    calls = source_call_counts(body)
    for callee_name, count in calls.items():
        if count <= 0 or callee_name in visiting:
            continue
        callee = functions_by_name.get(callee_name)
        if callee is None:
            continue
        call_key = f"call:{callee_name}"
        counter[call_key] -= min(counter.get(call_key, 0), count)
        callee_fp = expand_source_body_fingerprint(
            callee.body,
            functions_by_name,
            include_signature=None,
            max_depth=max_depth - 1,
            visiting=(*visiting, callee_name),
        )
        for key, value in callee_fp.items():
            counter[key] += value * count
    return +counter


def multiset_jaccard(left: Counter[str], right: Counter[str]) -> float:
    keys = set(left) | set(right)
    if not keys:
        return 1.0
    inter = sum(min(left[k], right[k]) for k in keys)
    union = sum(max(left[k], right[k]) for k in keys)
    return round(inter / union, 6) if union else 1.0


def multiset_gap_details(left: Counter[str], right: Counter[str], top_limit: int = 12) -> dict[str, Any]:
    keys = set(left) | set(right)
    intersection = sum(min(left[key], right[key]) for key in keys)
    union = sum(max(left[key], right[key]) for key in keys)
    missing = Counter({key: left[key] - right[key] for key in keys if left[key] > right[key]})
    extra = Counter({key: right[key] - left[key] for key in keys if right[key] > left[key]})
    missing_total = sum(missing.values())
    extra_total = sum(extra.values())
    return {
        "source_feature_total": sum(left.values()),
        "decomp_feature_total": sum(right.values()),
        "intersection_feature_total": intersection,
        "union_feature_total": union,
        "missing_feature_total": missing_total,
        "extra_feature_total": extra_total,
        "missing_feature_rate": round(missing_total / sum(left.values()), 6) if left else 0.0,
        "extra_feature_rate": round(extra_total / sum(right.values()), 6) if right else 0.0,
        "top_missing_features": [
            {"feature": feature, "count": count}
            for feature, count in missing.most_common(top_limit)
        ],
        "top_extra_features": [
            {"feature": feature, "count": count}
            for feature, count in extra.most_common(top_limit)
        ],
    }


STATIC_SIMILARITY_COMPONENTS: dict[str, tuple[str, ...]] = {
    "control_flow": ("ctrl:",),
    "operator": ("op:",),
    "call": ("call:",),
    "constant": ("const:",),
    "memory": ("mem:",),
    "signature": ("sig:",),
}


def fingerprint_subset(fp: Counter[str], prefixes: tuple[str, ...]) -> Counter[str]:
    return Counter({key: value for key, value in fp.items() if key.startswith(prefixes)})


def static_similarity_components(source_fp: Counter[str], decomp_fp: Counter[str]) -> dict[str, float]:
    return {
        name: multiset_jaccard(fingerprint_subset(source_fp, prefixes), fingerprint_subset(decomp_fp, prefixes))
        for name, prefixes in STATIC_SIMILARITY_COMPONENTS.items()
    }


def static_similarity_gap_components(source_fp: Counter[str], decomp_fp: Counter[str]) -> dict[str, dict[str, Any]]:
    return {
        name: multiset_gap_details(
            fingerprint_subset(source_fp, prefixes),
            fingerprint_subset(decomp_fp, prefixes),
            top_limit=6,
        )
        for name, prefixes in STATIC_SIMILARITY_COMPONENTS.items()
    }


def gap_feature_count(items: Any, feature: str) -> float:
    if not isinstance(items, list):
        return 0.0
    total = 0.0
    for item in items:
        if (
            isinstance(item, dict)
            and item.get("feature") == feature
            and isinstance(item.get("count"), int | float)
        ):
            total += float(item["count"])
    return total


def signedness_only_signature_gap(details: dict[str, Any]) -> dict[str, float]:
    missing = details.get("top_missing_features")
    extra = details.get("top_extra_features")
    source_int_param_decomp_uint = min(
        gap_feature_count(missing, "sig:param:int"),
        gap_feature_count(extra, "sig:param:uint"),
    )
    source_uint_param_decomp_int = min(
        gap_feature_count(missing, "sig:param:uint"),
        gap_feature_count(extra, "sig:param:int"),
    )
    source_int_return_decomp_uint = min(
        gap_feature_count(missing, "sig:return:int"),
        gap_feature_count(extra, "sig:return:uint"),
    )
    source_uint_return_decomp_int = min(
        gap_feature_count(missing, "sig:return:uint"),
        gap_feature_count(extra, "sig:return:int"),
    )
    return {
        "param_pair_count": source_int_param_decomp_uint + source_uint_param_decomp_int,
        "return_pair_count": source_int_return_decomp_uint + source_uint_return_decomp_int,
        "source_int_param_decomp_uint_count": source_int_param_decomp_uint,
        "source_uint_param_decomp_int_count": source_uint_param_decomp_int,
        "source_int_return_decomp_uint_count": source_int_return_decomp_uint,
        "source_uint_return_decomp_int_count": source_uint_return_decomp_int,
    }


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


def complexity_bucket(value: float) -> str:
    if value <= 5:
        return "tiny"
    if value <= 15:
        return "small"
    if value <= 40:
        return "medium"
    return "large"


def feature_gap_bucket(value: float) -> str:
    if value <= 0:
        return "none"
    if value <= 5:
        return "small"
    if value <= 20:
        return "medium"
    return "large"


def cost_bucket(seconds: float) -> str:
    if seconds <= 0.1:
        return "fast"
    if seconds <= 1.0:
        return "normal"
    if seconds <= 5.0:
        return "slow"
    return "very_slow"


def behavior_failure_owner(status: str) -> str:
    if status.startswith("oracle_"):
        return "oracle"
    if status.startswith("candidate_"):
        return "candidate"
    if status in {"decomp_failed", "unsupported_signature", "host_execution_unavailable"}:
        return status
    if status == "mismatch":
        return "semantic_mismatch"
    if status == "pass":
        return "pass"
    return "unknown"


def behavior_detail_signature(detail: Any) -> str:
    if not isinstance(detail, str) or not detail.strip():
        return "none"
    lines = [line.strip() for line in detail.splitlines() if line.strip()]
    if not lines:
        return "none"
    def stable_detail_line(line: str) -> str:
        line = re.sub(r".*/(candidate|oracle)\.c:", r"\1.c:", line)
        line = re.sub(r"/(?:private/)?(?:tmp|var)/[^\s:]+", "<tmp>", line)
        return re.sub(r"\s+", " ", line)[-240:]

    for line in lines:
        if "error:" in line or "undefined reference" in line or "undeclared" in line:
            return stable_detail_line(line)
    return stable_detail_line(lines[-1])


def furthest_ok_stage(stage_status: Any) -> str:
    if not isinstance(stage_status, dict):
        return "missing"
    furthest = "none"
    for stage in STAGE_FAILURE_ORDER:
        status = stage_status.get(stage)
        if status is None:
            continue
        if status != "ok":
            break
        furthest = stage
    return furthest


NIR_DEBT_METRIC_RE = re.compile(
    r"(rejected|failed|fallback|irreducible|invalid|missing|conflict|forced|unsupported|timeout|error)"
)


def numeric_items(payload: Any) -> list[tuple[str, float]]:
    if not isinstance(payload, dict):
        return []
    return [
        (str(key), float(value))
        for key, value in payload.items()
        if isinstance(value, int | float) and not isinstance(value, bool)
    ]


def is_debt_metric_name(name: str) -> bool:
    return bool(NIR_DEBT_METRIC_RE.search(name))


def add_numeric_debug_pipeline_values(values: dict[str, list[float]], pipeline: Any) -> None:
    if not isinstance(pipeline, dict):
        return
    for key in [
        "decode_attempt_count",
        "raw_pcode_block_count",
        "raw_pcode_op_count",
        "raw_pcode_edge_count",
        "instruction_limit",
        "max_bytes",
    ]:
        value = pipeline.get(key)
        if isinstance(value, int | float) and not isinstance(value, bool):
            values.setdefault(key, []).append(float(value))


ROADMAP_PRIORITY_ORDER = [
    "p1_sleigh_lift_correctness",
    "p2_type_data_abstraction",
    "p3_structuring_hard_cases",
    "p4_fid_name_recovery",
    "p5_architecture_breadth",
]


def add_priority_bucket_row(
    buckets: dict[str, dict[str, Any]],
    priority: str,
    row: dict[str, Any],
    behavior_status: str,
    first_stage: str,
    score: float,
) -> None:
    static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
    bucket = buckets.setdefault(
        priority,
        {
            "row_count": 0,
            "score_sum": 0.0,
            "lost_score_sum": 0.0,
            "missing_feature_total": 0.0,
            "extra_feature_total": 0.0,
            "behavior_status_counts": Counter(),
            "stage_first_failure_counts": Counter(),
            "top_rows": [],
        },
    )
    bucket["row_count"] += 1
    bucket["score_sum"] += score
    bucket["lost_score_sum"] += max(0.0, 1.0 - score)
    bucket["missing_feature_total"] += float(static_gaps.get("missing_feature_total", 0.0) or 0.0)
    bucket["extra_feature_total"] += float(static_gaps.get("extra_feature_total", 0.0) or 0.0)
    bucket["behavior_status_counts"][behavior_status] += 1
    bucket["stage_first_failure_counts"][first_stage] += 1
    bucket["top_rows"].append(triage_row_summary(row))


def metric_bucket_export(metrics: dict[str, Any], total: int, top_limit: int = 12) -> dict[str, Any]:
    row_count = int(metrics.get("row_count", 0) or 0)
    top_rows = sorted(
        metrics.get("top_rows") or [],
        key=lambda row: (
            float(row.get("semantic_score_percent") or 0.0),
            str(row.get("function_name") or ""),
        ),
    )[:top_limit]
    return {
        "row_count": row_count,
        "row_rate_total_denominator": round(row_count / total, 6) if total else 0.0,
        "avg_semantic_score": round(float(metrics.get("score_sum", 0.0) or 0.0) / row_count, 6)
        if row_count
        else 0.0,
        "avg_semantic_score_percent": percent(
            round(float(metrics.get("score_sum", 0.0) or 0.0) / row_count, 6)
        ) if row_count else 0.0,
        "lost_score_sum": round(float(metrics.get("lost_score_sum", 0.0) or 0.0), 6),
        "missing_feature_total": round(float(metrics.get("missing_feature_total", 0.0) or 0.0), 6),
        "extra_feature_total": round(float(metrics.get("extra_feature_total", 0.0) or 0.0), 6),
        "behavior_status_counts": dict(sorted(metrics.get("behavior_status_counts", Counter()).items())),
        "stage_first_failure_counts": dict(sorted(metrics.get("stage_first_failure_counts", Counter()).items())),
        "top_rows": top_rows,
    }


def default_behavior_cases(param_count: int) -> list[tuple[int, ...]]:
    if param_count == 0:
        return [()]
    if param_count == 1:
        return [(0,), (1,), (2,), (5,), (10,), (-1,)]
    if param_count == 2:
        return [(0, 0), (1, 2), (5, 10), (-3, 7), (42, -1)]
    if param_count == 3:
        return [(1, 2, 3), (0, 5, -1), (7, 3, 2)]
    return []


def default_behavior_cases_for_kinds(param_kinds: list[str]) -> list[tuple[int, ...]]:
    unsigned_positions = {idx for idx, kind in enumerate(param_kinds) if kind == "uint"}
    cases = default_behavior_cases(len(param_kinds))
    if not unsigned_positions:
        return cases
    return [
        case
        for case in cases
        if all(case[idx] >= 0 for idx in unsigned_positions)
    ]


def explicit_behavior_cases(entry: BenchmarkEntry, func: SourceFunction) -> list[dict[str, Any]] | None:
    if not entry.behavior_cases:
        return None
    cases = entry.behavior_cases.get(func.name)
    if cases is None:
        return None
    return cases


def behavior_supported(
    entry: BenchmarkEntry, func: SourceFunction, explicit_cases: list[dict[str, Any]] | None
) -> tuple[bool, str | None]:
    language = entry.language
    if language != "c":
        return False, "dynamic harness currently supports C source functions only"
    if func.name == "main":
        return False, "main is not called as a unit function"
    if explicit_cases is not None:
        if func.return_kind not in {"int", "void"}:
            return False, f"unsupported return kind: {func.return_kind}"
        unsupported = [
            kind
            for kind in func.param_kinds
            if kind not in {"int", "uint", "int_ptr", "aggregate_or_pointer"}
        ]
        if unsupported:
            return False, f"unsupported parameter kinds: {func.param_kinds}"
        valid, reason = validate_explicit_behavior_cases(func, explicit_cases)
        if not valid:
            return False, reason
        return True, None
    if func.return_kind != "int":
        return False, f"unsupported return kind: {func.return_kind}"
    if any(kind not in {"int", "uint"} for kind in func.param_kinds):
        return False, f"unsupported parameter kinds: {func.param_kinds}"
    if not default_behavior_cases_for_kinds(func.param_kinds):
        return False, "unsupported arity"
    return True, None


def validate_explicit_behavior_cases(
    func: SourceFunction, cases: list[dict[str, Any]]
) -> tuple[bool, str | None]:
    if not cases:
        return False, "empty explicit behavior case list"
    for case_index, case in enumerate(cases):
        args = case.get("args")
        if not isinstance(args, list):
            return False, f"case {case_index} missing args list"
        if len(args) != len(func.param_kinds):
            return False, f"case {case_index} arity mismatch"
        for arg_index, (arg, kind) in enumerate(zip(args, func.param_kinds)):
            if kind in {"int", "uint"} and not isinstance(arg, int):
                return False, f"case {case_index} arg {arg_index} must be int"
            if kind == "int_ptr":
                if not isinstance(arg, dict) or "int_array" not in arg:
                    return False, f"case {case_index} arg {arg_index} must be int_array"
                values = arg["int_array"]
                if not isinstance(values, list) or not all(isinstance(v, int) for v in values):
                    return False, f"case {case_index} arg {arg_index} has invalid int_array"
            if kind == "aggregate_or_pointer":
                if not isinstance(arg, str) or not WORD_BOUNDARY_RE.fullmatch(arg):
                    return False, f"case {case_index} arg {arg_index} must be a symbol name"
        support_code = case.get("candidate_support_code")
        if support_code is not None:
            if not isinstance(support_code, str):
                return False, f"case {case_index} candidate_support_code must be a string"
            if "#include" in support_code or re.search(r"\bmain\s*\(", support_code):
                return False, f"case {case_index} candidate_support_code has unsupported contents"
        globals_to_observe = case.get("globals", [])
        if globals_to_observe is None:
            globals_to_observe = []
        if not isinstance(globals_to_observe, list):
            return False, f"case {case_index} globals must be a list"
        for global_index, global_spec in enumerate(globals_to_observe):
            if not isinstance(global_spec, dict):
                return False, f"case {case_index} global {global_index} must be an object"
            name = global_spec.get("name")
            if not isinstance(name, str) or not WORD_BOUNDARY_RE.fullmatch(name):
                return False, f"case {case_index} global {global_index} has invalid name"
            ctype = global_spec.get("ctype", "unsigned int")
            if not isinstance(ctype, str) or not CTYPE_RE.fullmatch(ctype.strip()):
                return False, f"case {case_index} global {global_index} has invalid ctype"
            reset = global_spec.get("reset", 0)
            if not isinstance(reset, int):
                return False, f"case {case_index} global {global_index} reset must be int"
    return True, None


def behavior_cases_for(
    entry: BenchmarkEntry, func: SourceFunction
) -> list[tuple[int, ...]] | list[dict[str, Any]]:
    explicit_cases = explicit_behavior_cases(entry, func)
    if explicit_cases is not None:
        return explicit_cases
    return default_behavior_cases_for_kinds(func.param_kinds)


def source_harness(
    source_path: Path, func: SourceFunction, cases: list[tuple[int, ...]] | list[dict[str, Any]]
) -> str:
    calls = "\n".join(render_case_call(func, case, index) for index, case in enumerate(cases))
    return f"""
#include <stdio.h>
#define main source_original_main
#include "{source_path}"
#undef main
int main(void) {{
{calls}
    return 0;
}}
"""


def candidate_harness(
    candidate_code: str, func: SourceFunction, cases: list[tuple[int, ...]] | list[dict[str, Any]]
) -> str:
    calls = "\n".join(render_case_call(func, case, index) for index, case in enumerate(cases))
    support_code = "\n".join(candidate_support_code_blocks(cases))
    observed_globals = collect_observed_globals(cases)
    globals_decl = "\n".join(render_candidate_global_decl(spec) for spec in observed_globals)
    candidate_code = remove_duplicate_candidate_global_decls(candidate_code, observed_globals)
    return f"""
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
typedef unsigned char byte;
typedef unsigned char uchar;
typedef signed char schar;
typedef unsigned char undefined1;
typedef unsigned short ushort;
typedef unsigned short word;
typedef unsigned short undefined2;
typedef unsigned int uint;
typedef unsigned long ulong;
typedef unsigned int dword;
typedef unsigned int undefined4;
typedef unsigned long long ulonglong;
typedef unsigned long long qword;
typedef unsigned long long undefined8;
typedef long long longlong;
typedef unsigned char uint8;
typedef unsigned short uint16;
typedef unsigned int uint32;
typedef unsigned long long uint64;
typedef signed char int8;
typedef short int16;
typedef int int32;
typedef long long int64;
static inline bool __fission_carry32(uint32 a, uint32 b) {{ return (uint32)(a + b) < a; }}
static inline bool __fission_carry64(uint64 a, uint64 b) {{ return (uint64)(a + b) < a; }}
static inline bool __fission_scarry32(uint32 a, uint32 b) {{
    int32 sa = (int32)a, sb = (int32)b, sr = (int32)(a + b);
    return ((sa ^ sr) & (sb ^ sr)) < 0;
}}
static inline bool __fission_scarry64(uint64 a, uint64 b) {{
    int64 sa = (int64)a, sb = (int64)b, sr = (int64)(a + b);
    return ((sa ^ sr) & (sb ^ sr)) < 0;
}}
static inline bool __fission_sborrow32(uint32 a, uint32 b) {{
    int32 sa = (int32)a, sb = (int32)b, sr = (int32)(a - b);
    return ((sa ^ sb) & (sa ^ sr)) < 0;
}}
static inline bool __fission_sborrow64(uint64 a, uint64 b) {{
    int64 sa = (int64)a, sb = (int64)b, sr = (int64)(a - b);
    return ((sa ^ sb) & (sa ^ sr)) < 0;
}}
#define __carry(a, b) (sizeof(a) <= 4 ? __fission_carry32((uint32)(a), (uint32)(b)) : __fission_carry64((uint64)(a), (uint64)(b)))
#define __scarry(a, b) (sizeof(a) <= 4 ? __fission_scarry32((uint32)(a), (uint32)(b)) : __fission_scarry64((uint64)(a), (uint64)(b)))
#define __sborrow(a, b) (sizeof(a) <= 4 ? __fission_sborrow32((uint32)(a), (uint32)(b)) : __fission_sborrow64((uint64)(a), (uint64)(b)))
{globals_decl}
{support_code}
{candidate_code}
int main(void) {{
{calls}
    return 0;
}}
"""


def render_case_call(func: SourceFunction, case: tuple[int, ...] | dict[str, Any], index: int) -> str:
    if isinstance(case, dict):
        return render_explicit_case_call(func, case, index)
    args = ", ".join(str(v) for v in case)
    return f'    printf("%lld\\n", (long long){func.name}({args}));\n    fflush(stdout);'


def c_int_array(values: list[int]) -> str:
    return ", ".join(str(v) for v in values) or "0"


def collect_observed_globals(cases: list[tuple[int, ...]] | list[dict[str, Any]]) -> list[dict[str, Any]]:
    observed: dict[str, dict[str, Any]] = {}
    for case in cases:
        if not isinstance(case, dict):
            continue
        for spec in case.get("globals") or []:
            name = spec["name"]
            observed.setdefault(
                name,
                {
                    "name": name,
                    "ctype": spec.get("ctype", "unsigned int"),
                    "reset": spec.get("reset", 0),
                },
            )
    return [observed[name] for name in sorted(observed)]


def render_candidate_global_decl(spec: dict[str, Any]) -> str:
    return f"volatile {spec.get('ctype', 'unsigned int')} {spec['name']} = {int(spec.get('reset', 0))};"


def candidate_support_code_blocks(cases: list[tuple[int, ...]] | list[dict[str, Any]]) -> list[str]:
    blocks: list[str] = []
    seen: set[str] = set()
    for case in cases:
        if not isinstance(case, dict):
            continue
        support_code = case.get("candidate_support_code")
        if not isinstance(support_code, str):
            continue
        normalized = support_code.strip()
        if not normalized or normalized in seen:
            continue
        blocks.append(normalized)
        seen.add(normalized)
    return blocks


def remove_duplicate_candidate_global_decls(candidate_code: str, observed_globals: list[dict[str, Any]]) -> str:
    names = {
        spec["name"]
        for spec in observed_globals
        if isinstance(spec.get("name"), str) and WORD_BOUNDARY_RE.fullmatch(spec["name"])
    }
    if not names:
        return candidate_code

    def is_duplicate_decl(line: str) -> bool:
        stripped = line.strip()
        if not stripped.endswith(";"):
            return False
        if "(" in stripped:
            return False
        for name in names:
            if re.fullmatch(
                rf"(?:extern\s+)?(?:volatile\s+)?[A-Za-z_][A-Za-z0-9_\s\*]*\b{re.escape(name)}\s*(?:=\s*[^;]+)?;",
                stripped,
            ):
                return True
        return False

    return "\n".join(line for line in candidate_code.splitlines() if not is_duplicate_decl(line))


def render_explicit_case_call(func: SourceFunction, case: dict[str, Any], index: int) -> str:
    args = case["args"]
    setup: list[str] = []
    call_args: list[str] = []
    pointer_arrays: list[tuple[int, str, int]] = []
    for arg_index, (arg, kind) in enumerate(zip(args, func.param_kinds)):
        if kind in {"int", "uint"}:
            call_args.append(str(arg))
            continue
        if kind == "int_ptr":
            values = arg["int_array"]
            name = f"case_{index}_arg_{arg_index}"
            setup.append(f"    int {name}[] = {{{c_int_array(values)}}};")
            call_args.append(name)
            pointer_arrays.append((arg_index, name, len(values)))
            continue
        if kind == "aggregate_or_pointer":
            call_args.append(arg)
            continue
        raise AssertionError(f"unsupported explicit behavior kind: {kind}")

    joined_args = ", ".join(call_args)
    globals_to_observe = case.get("globals") or []
    lines = setup
    for spec in globals_to_observe:
        lines.append(f"    {spec['name']} = {int(spec.get('reset', 0))};")
    if func.return_kind == "void":
        lines.append(f"    {func.name}({joined_args});")
        lines.append('    printf("ret=void");')
    else:
        lines.append(f"    long long case_{index}_ret = (long long){func.name}({joined_args});")
        lines.append(f'    printf("ret=%lld", case_{index}_ret);')
    for arg_index, array_name, length in pointer_arrays:
        lines.append(f'    printf(" arg{arg_index}=");')
        lines.append(f"    for (int i = 0; i < {length}; ++i) {{")
        lines.append(f'        printf("%s%d", i ? "," : "", {array_name}[i]);')
        lines.append("    }")
    for spec in globals_to_observe:
        lines.append(f'    printf(" {spec["name"]}=%lld", (long long){spec["name"]});')
    lines.append('    printf("\\n");')
    lines.append("    fflush(stdout);")
    return "\n".join(lines)


def serialize_behavior_cases(cases: list[tuple[int, ...]] | list[dict[str, Any]]) -> list[Any]:
    serialized: list[Any] = []
    for case in cases:
        if isinstance(case, dict):
            serialized.append(case)
        else:
            serialized.append(list(case))
    return serialized


def compile_and_run_c(code: str, cwd: Path, name: str, timeout_sec: int) -> dict[str, Any]:
    wall_start = time.perf_counter()
    source = cwd / f"{name}.c"
    binary = cwd / name
    source.write_text(code, encoding="utf-8")
    clang = os.environ.get("CLANG") or shutil.which("clang") or "/opt/homebrew/opt/llvm/bin/clang"
    cmd = [clang, "-x", "c", "-std=c11", "-Wno-everything", str(source), "-o", str(binary)]
    compile_start = time.perf_counter()
    try:
        compile_res = subprocess.run(
            cmd,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
            check=True,
        )
    except subprocess.CalledProcessError as exc:
        return {
            "status": "compile_failed",
            "detail": (exc.stderr or exc.stdout or str(exc))[-4000:],
            "compile_sec": round(time.perf_counter() - compile_start, 6),
            "wall_sec": round(time.perf_counter() - wall_start, 6),
        }
    except subprocess.TimeoutExpired:
        return {
            "status": "compile_timeout",
            "compile_sec": round(time.perf_counter() - compile_start, 6),
            "wall_sec": round(time.perf_counter() - wall_start, 6),
        }

    compile_sec = round(time.perf_counter() - compile_start, 6)
    run_start = time.perf_counter()
    try:
        run_res = subprocess.run(
            [str(binary)],
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
            check=True,
        )
    except subprocess.CalledProcessError as exc:
        return {
            "status": "run_failed",
            "detail": (exc.stderr or exc.stdout or str(exc))[-4000:],
            "compile_sec": compile_sec,
            "run_sec": round(time.perf_counter() - run_start, 6),
            "wall_sec": round(time.perf_counter() - wall_start, 6),
        }
    except subprocess.TimeoutExpired as exc:
        stdout = exc.stdout.decode("utf-8", errors="replace") if isinstance(exc.stdout, bytes) else (exc.stdout or "")
        stderr = exc.stderr.decode("utf-8", errors="replace") if isinstance(exc.stderr, bytes) else (exc.stderr or "")
        return {
            "status": "run_timeout",
            "partial_stdout": stdout[-4000:],
            "partial_stderr": stderr[-4000:],
            "compile_sec": compile_sec,
            "run_sec": round(time.perf_counter() - run_start, 6),
            "wall_sec": round(time.perf_counter() - wall_start, 6),
        }

    return {
        "status": "ok",
        "stdout": run_res.stdout,
        "compile_stdout": compile_res.stdout,
        "compile_sec": compile_sec,
        "run_sec": round(time.perf_counter() - run_start, 6),
        "wall_sec": round(time.perf_counter() - wall_start, 6),
    }


def behavior_output_lines(stdout: Any) -> list[str]:
    if not isinstance(stdout, str):
        return []
    return [line.strip() for line in stdout.splitlines() if line.strip()]


def partial_behavior_progress(
    oracle: dict[str, Any],
    candidate: dict[str, Any],
    cases: list[tuple[int, ...]] | list[dict[str, Any]],
) -> dict[str, Any]:
    oracle_lines = behavior_output_lines(oracle.get("stdout"))
    candidate_lines = behavior_output_lines(
        candidate.get("partial_stdout") or candidate.get("stdout")
    )
    compared_cases = max(len(oracle_lines), len(candidate_lines), len(cases))
    matched_cases = sum(
        1
        for expected, actual in zip(oracle_lines, candidate_lines, strict=False)
        if expected == actual
    )
    first_mismatch_index = next(
        (
            index
            for index in range(compared_cases)
            if (oracle_lines[index] if index < len(oracle_lines) else None)
            != (candidate_lines[index] if index < len(candidate_lines) else None)
        ),
        None,
    )
    return {
        "case_pass_count": matched_cases,
        "case_fail_count": max(0, compared_cases - matched_cases),
        "compared_case_count": compared_cases,
        "case_pass_rate": round(matched_cases / compared_cases, 6) if compared_cases else 0.0,
        "first_mismatch_index": first_mismatch_index,
        "oracle_line_count": len(oracle_lines),
        "candidate_partial_line_count": len(candidate_lines),
        "candidate_missing_line_count": max(0, len(oracle_lines) - len(candidate_lines)),
        "candidate_extra_line_count": max(0, len(candidate_lines) - len(oracle_lines)),
        "oracle": oracle_lines,
        "candidate": candidate_lines,
    }


def compile_and_run_c_cached(
    code: str,
    cwd: Path,
    name: str,
    timeout_sec: int,
    cache: dict[str, dict[str, Any]] | None,
    cache_lock: threading.Lock | None,
    cache_stats: Counter[str] | None,
) -> dict[str, Any]:
    clang = os.environ.get("CLANG") or shutil.which("clang") or "/opt/homebrew/opt/llvm/bin/clang"
    key = behavior_cache_key(code, clang, timeout_sec)
    if cache is not None and cache_lock is not None:
        with cache_lock:
            cached = cache.get(key)
        if cached is not None:
            if cache_stats is not None:
                cache_stats["hit"] += 1
            result = dict(cached)
            result["behavior_cache_status"] = "hit"
            return result

    if cache_stats is not None:
        cache_stats["miss"] += 1
    result = compile_and_run_c(code, cwd, name, timeout_sec)
    if cache is not None and cache_lock is not None:
        stored = dict(result)
        stored["behavior_cache_status"] = "stored"
        with cache_lock:
            cache[key] = stored
        if cache_stats is not None:
            cache_stats["stored"] += 1
    result = dict(result)
    result["behavior_cache_status"] = "miss"
    return result


def candidate_timeout_sec(timeout_sec: int, oracle: dict[str, Any]) -> int:
    oracle_run_sec = float(oracle.get("run_sec", 0.0) or 0.0)
    measured_cap = int(oracle_run_sec * CANDIDATE_TIMEOUT_ORACLE_MULTIPLIER) + 1
    bounded = max(CANDIDATE_TIMEOUT_MIN_SEC, measured_cap)
    return max(1, min(timeout_sec, bounded))


def c_host_execution_probe(timeout_sec: int) -> dict[str, Any]:
    code = """
#include <stdio.h>
int main(void) {
    puts("source-semantic-host-ok");
    return 0;
}
"""
    with tempfile.TemporaryDirectory(prefix="source-semantic-host-") as tmp:
        result = compile_and_run_c(code, Path(tmp), "host_probe", timeout_sec)
    if result.get("status") == "ok" and result.get("stdout", "").strip() == "source-semantic-host-ok":
        return {"status": "ok"}
    return {
        "status": f"host_{result.get('status', 'unknown')}",
        "detail": result.get("detail"),
    }


def run_behavior_check(
    entry: BenchmarkEntry,
    func: SourceFunction,
    decomp_code: str | None,
    timeout_sec: int,
    host_execution: dict[str, Any],
    behavior_cache: dict[str, dict[str, Any]] | None = None,
    behavior_cache_lock: threading.Lock | None = None,
    behavior_cache_stats: Counter[str] | None = None,
    output_dir: Path | None = None,
    address: str | None = None,
) -> dict[str, Any]:
    behavior_start = time.perf_counter()
    explicit_cases = explicit_behavior_cases(entry, func)
    case_source = "explicit" if explicit_cases is not None else "default"
    supported, reason = behavior_supported(entry, func, explicit_cases)
    if not supported:
        return {
            "status": "unsupported_signature",
            "score": 0.0,
            "reason": reason,
            "eligible": False,
            "case_source": case_source,
            "case_count": 0,
            "wall_sec": round(time.perf_counter() - behavior_start, 6),
        }
    if host_execution.get("status") != "ok":
        return {
            "status": "host_execution_unavailable",
            "score": 0.0,
            "reason": host_execution.get("status"),
            "detail": host_execution.get("detail"),
            "eligible": True,
            "case_source": case_source,
            "case_count": 0,
            "wall_sec": round(time.perf_counter() - behavior_start, 6),
        }
    if not decomp_code:
        return {
            "status": "decomp_failed",
            "score": 0.0,
            "eligible": True,
            "case_source": case_source,
            "case_count": 0,
            "wall_sec": round(time.perf_counter() - behavior_start, 6),
        }

    cases = behavior_cases_for(entry, func)
    oracle_code = source_harness(entry.source_path, func, cases)
    candidate_code = candidate_harness(decomp_code, func, cases)
    artifact_dir = (
        behavior_artifact_dir_for_row(output_dir, entry, func, address)
        if output_dir is not None
        else None
    )

    def maybe_attach_artifacts(result: dict[str, Any], oracle: dict[str, Any] | None, candidate: dict[str, Any] | None) -> dict[str, Any]:
        result.setdefault("eligible", True)
        result.setdefault("case_source", case_source)
        result.setdefault("case_count", len(cases))
        result["wall_sec"] = round(time.perf_counter() - behavior_start, 6)
        result["oracle_cache_status"] = (oracle or {}).get("behavior_cache_status")
        result["candidate_cache_status"] = (candidate or {}).get("behavior_cache_status")
        result["compile_sec"] = round(
            float((oracle or {}).get("compile_sec", 0.0) or 0.0)
            + float((candidate or {}).get("compile_sec", 0.0) or 0.0),
            6,
        )
        result["run_sec"] = round(
            float((oracle or {}).get("run_sec", 0.0) or 0.0)
            + float((candidate or {}).get("run_sec", 0.0) or 0.0),
            6,
        )
        if artifact_dir is None or result.get("status") == "pass":
            return result
        write_behavior_artifacts(artifact_dir, oracle_code, candidate_code, oracle, candidate)
        result["artifact_dir"] = rel(artifact_dir)
        result["oracle_source_path"] = rel(artifact_dir / "oracle.c")
        result["candidate_source_path"] = rel(artifact_dir / "candidate.c")
        result["result_path"] = rel(artifact_dir / "result.json")
        return result

    with tempfile.TemporaryDirectory(prefix="source-semantic-") as tmp:
        tmp_path = Path(tmp)
        oracle = compile_and_run_c_cached(
            oracle_code,
            tmp_path,
            "oracle",
            timeout_sec,
            behavior_cache,
            behavior_cache_lock,
            behavior_cache_stats,
        )
        if oracle.get("status") != "ok":
            return maybe_attach_artifacts(
                {"status": f"oracle_{oracle.get('status')}", "score": 0.0, "detail": oracle.get("detail")},
                oracle,
                None,
            )
        candidate_timeout = candidate_timeout_sec(timeout_sec, oracle)
        candidate = compile_and_run_c_cached(
            candidate_code,
            tmp_path,
            "candidate",
            candidate_timeout,
            behavior_cache,
            behavior_cache_lock,
            behavior_cache_stats,
        )
        if candidate.get("status") != "ok":
            progress = partial_behavior_progress(oracle, candidate, cases)
            return maybe_attach_artifacts(
                {
                    "status": f"candidate_{candidate.get('status')}",
                    "score": 0.0,
                    "detail": candidate.get("detail"),
                    "candidate_timeout_sec": candidate_timeout,
                    "cases": serialize_behavior_cases(cases),
                    **progress,
                },
                oracle,
                candidate,
            )
        oracle_lines = behavior_output_lines(oracle["stdout"])
        candidate_lines = behavior_output_lines(candidate["stdout"])
        passed = oracle_lines == candidate_lines
        matched_cases = sum(
            1
            for expected, actual in zip(oracle_lines, candidate_lines, strict=False)
            if expected == actual
        )
        compared_cases = max(len(oracle_lines), len(candidate_lines), len(cases))
        first_mismatch_index = next(
            (
                index
                for index in range(compared_cases)
                if (oracle_lines[index] if index < len(oracle_lines) else None)
                != (candidate_lines[index] if index < len(candidate_lines) else None)
            ),
            None,
        )
        return maybe_attach_artifacts({
            "status": "pass" if passed else "mismatch",
            "score": 1.0 if passed else 0.0,
            "case_pass_count": matched_cases,
            "case_fail_count": max(0, compared_cases - matched_cases),
            "compared_case_count": compared_cases,
            "case_pass_rate": round(matched_cases / compared_cases, 6) if compared_cases else 0.0,
            "first_mismatch_index": first_mismatch_index,
            "candidate_timeout_sec": candidate_timeout,
            "cases": serialize_behavior_cases(cases),
            "oracle": oracle_lines,
            "candidate": candidate_lines,
        }, oracle, candidate)


def summarize(rows: list[dict[str, Any]], manifest_name: str, entries: list[BenchmarkEntry]) -> dict[str, Any]:
    total = len(rows)
    mapped = sum(1 for row in rows if row["mapping_status"] == "matched")
    decomp_ok = sum(1 for row in rows if row.get("decomp_success"))
    compile_ok = sum(1 for row in rows if row.get("behavior", {}).get("status") in {"pass", "mismatch"})
    behavior_pass = sum(1 for row in rows if row.get("behavior", {}).get("status") == "pass")
    behavior_expected = sum(1 for row in rows if row.get("behavior", {}).get("eligible") is True)
    behavior_executed = sum(1 for row in rows if row.get("behavior", {}).get("status") in {"pass", "mismatch"})
    score_values = [float(row.get("semantic_score", 0.0) or 0.0) for row in rows]
    mapping_status_counts = Counter(row.get("mapping_status", "unknown") for row in rows)
    decomp_failure_counts = Counter(
        row.get("decomp_failure_kind", "unknown")
        for row in rows
        if not row.get("decomp_success")
    )
    behavior_status_counts = Counter(row.get("behavior", {}).get("status", "unknown") for row in rows)
    behavior_cache_status_counts = Counter(
        status
        for row in rows
        for status in [
            row.get("behavior", {}).get("oracle_cache_status"),
            row.get("behavior", {}).get("candidate_cache_status"),
        ]
        if status
    )
    behavior_case_source_counts = Counter(
        row.get("behavior", {}).get("case_source", "unknown")
        for row in rows
        if isinstance(row.get("behavior"), dict)
    )
    behavior_unsupported_reason_counts = Counter(
        row.get("behavior", {}).get("reason", "unknown")
        for row in rows
        if row.get("behavior", {}).get("status") == "unsupported_signature"
    )
    decomp_cache_status_counts = Counter(row.get("decomp_cache_status", "not_requested") for row in rows)
    zero_credit_breakdown = Counter(
        row_zero_credit_reason(row)
        for row in rows
        if float(row.get("semantic_score", 0.0) or 0.0) == 0.0
    )
    stage_first_failure_counts = Counter(
        row.get("stage_first_failure") or "none"
        for row in rows
        if row.get("mapping_status") == "matched"
    )
    static_component_sums: Counter[str] = Counter()
    static_gap_totals: Counter[str] = Counter()
    static_gap_component_totals: dict[str, Counter[str]] = {
        component: Counter() for component in STATIC_SIMILARITY_COMPONENTS
    }
    static_gap_component_missing_features: dict[str, Counter[str]] = {
        component: Counter() for component in STATIC_SIMILARITY_COMPONENTS
    }
    static_gap_component_extra_features: dict[str, Counter[str]] = {
        component: Counter() for component in STATIC_SIMILARITY_COMPONENTS
    }
    static_missing_feature_counts: Counter[str] = Counter()
    static_extra_feature_counts: Counter[str] = Counter()
    score_distribution = Counter()
    debug_owner_bucket_counts: Counter[str] = Counter()
    debug_stage_status_counts: Counter[str] = Counter()
    debug_stage_status_matrix: dict[str, Counter[str]] = {
        stage: Counter() for stage in STAGE_FAILURE_ORDER
    }
    debug_quality_evidence_totals: Counter[str] = Counter()
    debug_quality_evidence_nonzero_rows: Counter[str] = Counter()
    debug_template_source_totals: Counter[str] = Counter()
    behavior_first_mismatch_index_counts: Counter[str] = Counter()
    behavior_output_length_delta_counts: Counter[str] = Counter()
    behavior_mismatch_kind_counts: Counter[str] = Counter()
    behavior_case_pass_rates: list[float] = []
    behavior_missing_candidate_line_total = 0
    behavior_extra_candidate_line_total = 0
    behavior_partial_progress_rows: list[dict[str, Any]] = []
    behavior_status_by_stage_first_failure: dict[str, Counter[str]] = {}
    behavior_status_by_zero_credit_reason: dict[str, Counter[str]] = {}
    score_values_by_behavior_status: dict[str, list[float]] = {}
    score_values_by_stage_first_failure: dict[str, list[float]] = {}
    behavior_score_values: list[float] = []
    static_score_values: list[float] = []
    source_body_line_counts: list[float] = []
    decomp_line_counts: list[float] = []
    source_body_byte_counts: list[float] = []
    decomp_byte_counts: list[float] = []
    decomp_to_source_line_ratios: list[float] = []
    decomp_to_source_byte_ratios: list[float] = []
    source_feature_total_values: list[float] = []
    source_feature_total_direct_values: list[float] = []
    source_feature_total_inline_expanded_values: list[float] = []
    decomp_feature_total_values: list[float] = []
    static_intersection_feature_total_values: list[float] = []
    static_union_feature_total_values: list[float] = []
    static_component_source_feature_values: dict[str, list[float]] = {
        component: [] for component in STATIC_SIMILARITY_COMPONENTS
    }
    static_component_decomp_feature_values: dict[str, list[float]] = {
        component: [] for component in STATIC_SIMILARITY_COMPONENTS
    }
    static_component_absence_counts: dict[str, Counter[str]] = {
        component: Counter() for component in STATIC_SIMILARITY_COMPONENTS
    }
    static_component_absence_rows: dict[str, dict[str, list[dict[str, Any]]]] = {
        component: {
            "source_only_rows": [],
            "decomp_only_rows": [],
            "zero_intersection_rows": [],
        }
        for component in STATIC_SIMILARITY_COMPONENTS
    }
    source_decomp_size_hot_rows: list[dict[str, Any]] = []
    source_feature_rows = 0
    decomp_feature_rows = 0
    static_missing_feature_rows = 0
    static_extra_feature_rows = 0
    static_zero_similarity_rows = 0
    static_decomp_absent_feature_rows = 0
    static_component_missing_row_counts: Counter[str] = Counter()
    static_component_zero_similarity_row_counts: Counter[str] = Counter()
    missing_feature_count_values: list[float] = []
    extra_feature_count_values: list[float] = []
    semantic_loss_by_behavior_status: Counter[str] = Counter()
    semantic_loss_by_stage_first_failure: Counter[str] = Counter()
    semantic_loss_by_zero_credit_reason: Counter[str] = Counter()
    semantic_loss_hot_rows: list[dict[str, Any]] = []
    cost_hot_rows: list[dict[str, Any]] = []
    debug_decomp_row_count = 0
    debug_stage_status_row_count = 0
    stage_status_metrics: dict[str, Counter[str]] = {
        stage: Counter() for stage in STAGE_FAILURE_ORDER
    }
    nir_build_stats_row_count = 0
    nir_build_stats_numeric_totals: Counter[str] = Counter()
    nir_build_stats_nonzero_rows: Counter[str] = Counter()
    nir_build_stats_values: dict[str, list[float]] = {}
    nir_build_stats_debt_hot_rows: list[dict[str, Any]] = []
    nir_debt_row_count = 0
    nir_debt_score_values: list[float] = []
    nir_no_debt_score_values: list[float] = []
    nir_debt_behavior_status_counts: Counter[str] = Counter()
    nir_debt_stage_first_failure_counts: Counter[str] = Counter()
    debug_pipeline_numeric_values: dict[str, list[float]] = {}
    improvement_axis_metrics: dict[str, dict[str, Any]] = {}
    complexity_buckets: dict[str, dict[str, Any]] = {}
    cost_values_by_behavior_status: dict[str, list[float]] = {}
    cost_values_by_stage_first_failure: dict[str, list[float]] = {}
    cost_values_by_score_bucket: dict[str, list[float]] = {}
    scores_by_cost_bucket: dict[str, list[float]] = {}
    lost_score_by_cost_bucket: Counter[str] = Counter()
    stage_funnel_counts: Counter[str] = Counter()
    stage_furthest_ok_counts: Counter[str] = Counter()
    stage_first_blocker_lost_score: Counter[str] = Counter()
    admission_gate_counts: Counter[str] = Counter()
    behavior_failure_owner_counts: Counter[str] = Counter()
    behavior_failure_detail_counts: Counter[str] = Counter()
    behavior_failure_detail_rows: dict[str, list[dict[str, Any]]] = {}
    static_gap_density_rows: dict[str, dict[str, Any]] = {}
    static_missing_density_values: list[float] = []
    static_extra_density_values: list[float] = []
    static_score_by_missing_gap_bucket: dict[str, list[float]] = {}
    static_gap_hot_rows: list[dict[str, Any]] = []
    hard_function_rows: list[dict[str, Any]] = []
    static_source_variant_counts: Counter[str] = Counter()
    inline_expanded_static_score_deltas: list[float] = []
    inline_expanded_static_hot_rows: list[dict[str, Any]] = []
    semantic_quality_quadrants: dict[str, dict[str, Any]] = {}
    semantic_readiness_counts: Counter[str] = Counter()
    sleigh_blocker_rows: list[dict[str, Any]] = []
    coverage_blind_spot_rows: dict[str, list[dict[str, Any]]] = {}
    coverage_blind_spot_counts: Counter[str] = Counter()
    focus_area_metrics: dict[str, dict[str, Any]] = {}
    roadmap_priority_metrics: dict[str, dict[str, Any]] = {}
    signature_gap_rows: list[dict[str, Any]] = []
    memory_gap_rows: list[dict[str, Any]] = []
    call_gap_rows: list[dict[str, Any]] = []
    control_flow_gap_rows: list[dict[str, Any]] = []
    signedness_only_signature_gap_rows: list[dict[str, Any]] = []
    signedness_only_signature_gap_totals: Counter[str] = Counter()
    signature_return_pair_counts: Counter[str] = Counter()
    signature_param_pair_counts: Counter[str] = Counter()
    signature_pair_gap_rows: list[dict[str, Any]] = []
    signature_param_arity_mismatch_rows: list[dict[str, Any]] = []
    name_recovery_rows: list[dict[str, Any]] = []
    architecture_stage_metrics: dict[str, dict[str, Any]] = {}
    outcome_matrix_counts: Counter[str] = Counter()
    outcome_matrix_lost_score: Counter[str] = Counter()
    outcome_matrix_rows: dict[str, list[dict[str, Any]]] = {}
    by_language: dict[str, dict[str, Any]] = {}
    by_arch: dict[str, dict[str, Any]] = {}
    by_source_return_kind: dict[str, dict[str, Any]] = {}
    by_source_param_shape: dict[str, dict[str, Any]] = {}
    by_tag: dict[str, dict[str, Any]] = {}
    by_entry: dict[str, dict[str, Any]] = {}

    def add_bucket(bucket: dict[str, Any], row: dict[str, Any]) -> None:
        bucket["row_count"] += 1
        bucket["mapped"] += int(row["mapping_status"] == "matched")
        bucket["decomp_success"] += int(bool(row.get("decomp_success")))
        bucket["behavior_pass"] += int(row.get("behavior", {}).get("status") == "pass")
        bucket["score_sum"] += float(row.get("semantic_score", 0.0) or 0.0)

    def improvement_axis_for(row: dict[str, Any], behavior: dict[str, Any], first_stage: str) -> str:
        if row.get("mapping_status") != "matched":
            return "mapping"
        if first_stage.startswith("decode:") or first_stage.startswith("raw_pcode:"):
            return "sleigh_decode_lift"
        if first_stage.startswith("nir_build:") or first_stage.startswith("normalize:"):
            return "nir_build_normalize"
        if first_stage.startswith("structuring:") or first_stage.startswith("render:"):
            return "structuring_render"
        if not row.get("decomp_success"):
            return "decompile_orchestration"
        behavior_status = str(behavior.get("status", "unknown"))
        if behavior_status == "unsupported_signature":
            return "behavior_coverage"
        if behavior_status in {
            "candidate_compile_failed",
            "candidate_compile_timeout",
            "candidate_run_failed",
            "candidate_run_timeout",
            "oracle_compile_failed",
            "oracle_compile_timeout",
            "oracle_run_failed",
            "oracle_run_timeout",
            "host_execution_unavailable",
        }:
            return "behavior_harness"
        if behavior_status == "mismatch":
            return "dynamic_semantics"
        static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        if float(static_gaps.get("missing_feature_total", 0.0) or 0.0) > 0.0:
            return "static_semantic_gaps"
        preview_stats = row.get("preview_build_stats")
        if isinstance(preview_stats, dict) and any(
            is_debt_metric_name(key) and value != 0
            for key, value in numeric_items(preview_stats)
        ):
            return "nir_telemetry_debt"
        if float(row.get("semantic_score", 0.0) or 0.0) < 1.0:
            return "partial_quality"
        return "passing"

    def focus_areas_for(
        row: dict[str, Any],
        behavior: dict[str, Any],
        first_stage: str,
        preview_stats: dict[str, Any] | None,
        debug_decomp: dict[str, Any] | None,
    ) -> set[str]:
        areas: set[str] = set()
        behavior_status = str(behavior.get("status", "unknown"))
        static_components = (
            row.get("static_similarity_gap_components")
            if isinstance(row.get("static_similarity_gap_components"), dict)
            else {}
        )
        quality = (
            debug_decomp.get("quality_evidence")
            if isinstance(debug_decomp, dict) and isinstance(debug_decomp.get("quality_evidence"), dict)
            else {}
        )
        owner_buckets = {
            str(bucket)
            for bucket in (debug_decomp.get("owner_buckets") if isinstance(debug_decomp, dict) else []) or []
        }

        if row.get("mapping_status") != "matched":
            areas.add("mapping_name_recovery")
        if first_stage.startswith("decode:") or first_stage.startswith("raw_pcode:"):
            areas.add("sleigh_runtime_lift")
        if any("sleigh" in bucket or "raw_pcode" in bucket or "decode" in bucket for bucket in owner_buckets):
            areas.add("sleigh_runtime_lift")
        if first_stage.startswith("nir_build:") or first_stage.startswith("normalize:"):
            areas.add("nir_builder_dataflow")
        if isinstance(preview_stats, dict) and any(
            is_debt_metric_name(key) and value != 0 for key, value in numeric_items(preview_stats)
        ):
            areas.add("nir_builder_dataflow")
        if first_stage.startswith("structuring:") or first_stage.startswith("render:"):
            areas.add("structuring_render")
        if any("structuring" in bucket or "region" in bucket for bucket in owner_buckets):
            areas.add("structuring_render")
        if any(
            isinstance(value, int | float) and value != 0
            for key, value in quality.items()
            if key.startswith("structuring_") or key.startswith("region_")
        ):
            areas.add("structuring_render")

        type_component_missing = 0.0
        for component in ["memory", "signature", "call"]:
            details = static_components.get(component)
            if isinstance(details, dict):
                type_component_missing += float(details.get("missing_feature_total", 0.0) or 0.0)
        if type_component_missing > 0.0:
            areas.add("type_data_abstraction")
        if any(
            isinstance(value, int | float) and value != 0
            for key, value in quality.items()
            if key.startswith("typed_") or key.startswith("call_") or "prototype" in key
        ):
            areas.add("type_data_abstraction")

        if behavior_status in {
            "candidate_compile_failed",
            "candidate_compile_timeout",
            "candidate_run_failed",
            "candidate_run_timeout",
            "oracle_compile_failed",
            "oracle_compile_timeout",
            "oracle_run_failed",
            "oracle_run_timeout",
            "host_execution_unavailable",
            "unsupported_signature",
        }:
            areas.add("behavior_harness_coverage")
        if behavior_status == "mismatch":
            areas.add("dynamic_semantics")
        if not areas and float(row.get("semantic_score", 0.0) or 0.0) < 1.0:
            areas.add("unclassified_quality_loss")
        if not areas:
            areas.add("passing")
        return areas

    def add_focus_area_row(
        area: str,
        row: dict[str, Any],
        behavior_status: str,
        first_stage: str,
        score: float,
    ) -> None:
        static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        bucket = focus_area_metrics.setdefault(
            area,
            {
                "row_count": 0,
                "score_sum": 0.0,
                "lost_score_sum": 0.0,
                "behavior_status_counts": Counter(),
                "stage_first_failure_counts": Counter(),
                "missing_feature_total": 0.0,
                "top_rows": [],
            },
        )
        bucket["row_count"] += 1
        bucket["score_sum"] += score
        bucket["lost_score_sum"] += max(0.0, 1.0 - score)
        bucket["behavior_status_counts"][behavior_status] += 1
        bucket["stage_first_failure_counts"][first_stage] += 1
        bucket["missing_feature_total"] += float(static_gaps.get("missing_feature_total", 0.0) or 0.0)
        bucket["top_rows"].append(triage_row_summary(row))

    def add_axis_row(axis: str, row: dict[str, Any], behavior_status: str, first_stage: str, score: float) -> None:
        static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        metrics = improvement_axis_metrics.setdefault(
            axis,
            {
                "row_count": 0,
                "score_sum": 0.0,
                "lost_score_sum": 0.0,
                "behavior_status_counts": Counter(),
                "stage_first_failure_counts": Counter(),
                "missing_feature_total": 0.0,
                "top_rows": [],
            },
        )
        metrics["row_count"] += 1
        metrics["score_sum"] += score
        metrics["lost_score_sum"] += max(0.0, 1.0 - score)
        metrics["behavior_status_counts"][behavior_status] += 1
        metrics["stage_first_failure_counts"][first_stage] += 1
        metrics["missing_feature_total"] += float(static_gaps.get("missing_feature_total", 0.0) or 0.0)
        metrics["top_rows"].append(triage_row_summary(row))

    def add_complexity_row(bucket_name: str, row: dict[str, Any], score: float, behavior_status: str) -> None:
        static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        bucket = complexity_buckets.setdefault(
            bucket_name,
            {
                "row_count": 0,
                "score_sum": 0.0,
                "behavior_pass_count": 0,
                "missing_feature_total": 0.0,
                "zero_score_count": 0,
                "source_line_counts": [],
                "source_feature_counts": [],
            },
        )
        bucket["row_count"] += 1
        bucket["score_sum"] += score
        bucket["behavior_pass_count"] += int(behavior_status == "pass")
        bucket["missing_feature_total"] += float(static_gaps.get("missing_feature_total", 0.0) or 0.0)
        bucket["zero_score_count"] += int(score == 0.0)
        source_lines = row.get("source_body_line_count")
        source_features = row.get("source_static_feature_count")
        if isinstance(source_lines, int | float):
            bucket["source_line_counts"].append(float(source_lines))
        if isinstance(source_features, int | float):
            bucket["source_feature_counts"].append(float(source_features))

    def dynamic_semantic_axis(behavior_status: str) -> str:
        if behavior_status == "pass":
            return "dynamic_pass"
        if behavior_status == "mismatch":
            return "dynamic_mismatch"
        if behavior_status == "unsupported_signature":
            return "dynamic_unsupported"
        if behavior_status in {
            "candidate_compile_failed",
            "candidate_compile_timeout",
            "candidate_run_failed",
            "candidate_run_timeout",
            "oracle_compile_failed",
            "oracle_compile_timeout",
            "oracle_run_failed",
            "oracle_run_timeout",
            "host_execution_unavailable",
            "decomp_failed",
        }:
            return "dynamic_harness_or_decomp_blocked"
        return "dynamic_unknown"

    def static_semantic_axis(row: dict[str, Any]) -> str:
        static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        source_total = float(static_gaps.get("source_feature_total", 0.0) or 0.0)
        decomp_total = float(static_gaps.get("decomp_feature_total", 0.0) or 0.0)
        if source_total == 0.0:
            return "static_no_source_features"
        if decomp_total == 0.0:
            return "static_no_decomp_features"
        if float(row.get("static_semantic_score", 0.0) or 0.0) >= 1.0:
            return "static_perfect"
        return "static_gap"

    def add_semantic_quality_quadrant(row: dict[str, Any], behavior_status: str, score: float) -> None:
        static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        quadrant = f"{dynamic_semantic_axis(behavior_status)}|{static_semantic_axis(row)}"
        bucket = semantic_quality_quadrants.setdefault(
            quadrant,
            {
                "row_count": 0,
                "score_sum": 0.0,
                "lost_score_sum": 0.0,
                "missing_feature_total": 0.0,
                "extra_feature_total": 0.0,
                "top_rows": [],
            },
        )
        bucket["row_count"] += 1
        bucket["score_sum"] += score
        bucket["lost_score_sum"] += max(0.0, 1.0 - score)
        bucket["missing_feature_total"] += float(static_gaps.get("missing_feature_total", 0.0) or 0.0)
        bucket["extra_feature_total"] += float(static_gaps.get("extra_feature_total", 0.0) or 0.0)
        if score < 1.0:
            bucket["top_rows"].append(triage_row_summary(row))

    def add_coverage_blind_spot(kind: str, row: dict[str, Any]) -> None:
        coverage_blind_spot_counts[kind] += 1
        coverage_blind_spot_rows.setdefault(kind, []).append(triage_row_summary(row))

    for row in rows:
        score = float(row.get("semantic_score", 0.0) or 0.0)
        behavior = row.get("behavior") if isinstance(row.get("behavior"), dict) else {}
        behavior_status = str(behavior.get("status", "unknown"))
        raw_behavior_score = behavior.get("score")
        behavior_score = (
            float(raw_behavior_score)
            if isinstance(raw_behavior_score, int | float)
            else (1.0 if behavior_status == "pass" else 0.0)
        )
        static_score = float(row.get("static_semantic_score", 0.0) or 0.0)
        behavior_score_values.append(behavior_score)
        static_score_values.append(static_score)
        static_source_variant = str(row.get("static_similarity_source_variant") or "direct_source")
        static_source_variant_counts[static_source_variant] += 1
        direct_static_score = row.get("static_semantic_score_direct")
        expanded_static_score = row.get("static_semantic_score_inline_expanded")
        if isinstance(direct_static_score, int | float) and isinstance(expanded_static_score, int | float):
            delta = round(float(expanded_static_score) - float(direct_static_score), 6)
            if delta > 0.0:
                inline_expanded_static_score_deltas.append(delta)
                inline_expanded_static_hot_rows.append(
                    {
                        "entry_id": row.get("entry_id"),
                        "function_name": row.get("function_name"),
                        "address": row.get("address"),
                        "direct_static_semantic_score_percent": percent(float(direct_static_score)),
                        "inline_expanded_static_semantic_score_percent": percent(float(expanded_static_score)),
                        "static_score_delta_percent": percent(delta),
                        "semantic_score_percent": row.get("semantic_score_percent"),
                    }
                )
        first_stage = str(row.get("stage_first_failure") or "none")
        score_loss = round(max(0.0, 1.0 - score), 6)
        preview_stats = row.get("preview_build_stats")
        preview_stats_dict = preview_stats if isinstance(preview_stats, dict) else None
        debug_decomp = row.get("debug_decomp")
        debug_decomp_dict = debug_decomp if isinstance(debug_decomp, dict) else None
        stage_status_for_readiness = (
            debug_decomp.get("stage_status")
            if isinstance(debug_decomp, dict) and isinstance(debug_decomp.get("stage_status"), dict)
            else {}
        )
        all_pipeline_stages_ok = bool(stage_status_for_readiness) and all(
            stage_status_for_readiness.get(stage) == "ok"
            for stage in STAGE_FAILURE_ORDER
            if stage != "load"
        )
        semantic_readiness_counts["manifest_rows"] += 1
        semantic_readiness_counts["fully_perfect_rows"] += int(score == 1.0)
        semantic_readiness_counts["partial_credit_rows"] += int(0.0 < score < 1.0)
        semantic_readiness_counts["zero_credit_rows"] += int(score == 0.0)
        semantic_readiness_counts["behavior_pass_static_perfect_rows"] += int(
            behavior_status == "pass" and static_score == 1.0
        )
        semantic_readiness_counts["behavior_pass_static_gap_rows"] += int(
            behavior_status == "pass" and static_score < 1.0
        )
        semantic_readiness_counts["static_perfect_behavior_nonpass_rows"] += int(
            behavior_status != "pass" and static_score == 1.0
        )
        semantic_readiness_counts["pipeline_ok_behavior_nonpass_rows"] += int(
            all_pipeline_stages_ok and behavior_status != "pass"
        )
        semantic_readiness_counts["pipeline_blocked_rows"] += int(first_stage != "none")
        add_semantic_quality_quadrant(row, behavior_status, score)
        if first_stage.startswith("decode:") or first_stage.startswith("raw_pcode:"):
            sleigh_blocker_rows.append(triage_row_summary(row))
        if row.get("mapping_status") != "matched":
            add_coverage_blind_spot("unmapped_source_function", row)
        if row.get("mapping_status") == "matched" and not row.get("decomp_success"):
            add_coverage_blind_spot("mapped_but_decompile_failed", row)
        if not isinstance(debug_decomp, dict):
            add_coverage_blind_spot("missing_debug_decomp_evidence", row)
        if behavior_status == "unsupported_signature":
            add_coverage_blind_spot("unsupported_behavior_signature", row)
        if row.get("behavior", {}).get("eligible") is True and behavior_status not in {"pass", "mismatch"}:
            add_coverage_blind_spot("eligible_behavior_not_executed", row)
        if score_loss > 0.0:
            loss_reason = row_zero_credit_reason(row) if score == 0.0 else "partial_credit"
            semantic_loss_by_behavior_status[behavior_status] += score_loss
            semantic_loss_by_stage_first_failure[first_stage] += score_loss
            semantic_loss_by_zero_credit_reason[loss_reason] += score_loss
            semantic_loss_hot_rows.append(
                {
                    "entry_id": row.get("entry_id"),
                    "function_name": row.get("function_name"),
                    "address": row.get("address"),
                    "semantic_score_percent": row.get("semantic_score_percent"),
                    "lost_score": score_loss,
                    "behavior_status": behavior_status,
                    "stage_first_failure": first_stage,
                    "zero_credit_reason": loss_reason,
                }
            )
        static_gaps_for_outcome = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        static_missing_for_outcome = float(static_gaps_for_outcome.get("missing_feature_total", 0.0) or 0.0)
        static_extra_for_outcome = float(static_gaps_for_outcome.get("extra_feature_total", 0.0) or 0.0)
        static_gap_bucket_for_outcome = (
            "static_perfect"
            if static_missing_for_outcome == 0.0 and static_extra_for_outcome == 0.0
            else f"missing:{feature_gap_bucket(static_missing_for_outcome)}|extra:{feature_gap_bucket(static_extra_for_outcome)}"
        )
        outcome_key = (
            f"mapping:{row.get('mapping_status', 'unknown')}|"
            f"stage:{first_stage}|behavior:{behavior_status}|static:{static_gap_bucket_for_outcome}"
        )
        outcome_matrix_counts[outcome_key] += 1
        outcome_matrix_lost_score[outcome_key] += score_loss
        if score_loss > 0.0:
            outcome_matrix_rows.setdefault(outcome_key, []).append(triage_row_summary(row))
        score_values_by_behavior_status.setdefault(behavior_status, []).append(score)
        score_values_by_stage_first_failure.setdefault(first_stage, []).append(score)
        axis = improvement_axis_for(row, behavior, first_stage)
        add_axis_row(axis, row, behavior_status, first_stage, score)
        areas = focus_areas_for(row, behavior, first_stage, preview_stats_dict, debug_decomp_dict)
        for area in areas:
            add_focus_area_row(area, row, behavior_status, first_stage, score)
        if "sleigh_runtime_lift" in areas:
            add_priority_bucket_row(
                roadmap_priority_metrics,
                "p1_sleigh_lift_correctness",
                row,
                behavior_status,
                first_stage,
                score,
            )
        if "type_data_abstraction" in areas:
            add_priority_bucket_row(
                roadmap_priority_metrics,
                "p2_type_data_abstraction",
                row,
                behavior_status,
                first_stage,
                score,
            )
        if "structuring_render" in areas:
            add_priority_bucket_row(
                roadmap_priority_metrics,
                "p3_structuring_hard_cases",
                row,
                behavior_status,
                first_stage,
                score,
            )
        if "mapping_name_recovery" in areas:
            add_priority_bucket_row(
                roadmap_priority_metrics,
                "p4_fid_name_recovery",
                row,
                behavior_status,
                first_stage,
                score,
            )
        if row.get("binary_arch") not in {None, "unknown"} and score < 1.0:
            add_priority_bucket_row(
                roadmap_priority_metrics,
                "p5_architecture_breadth",
                row,
                behavior_status,
                first_stage,
                score,
            )
        source_complexity_value = float(row.get("source_static_feature_count") or 0.0)
        add_complexity_row(complexity_bucket(source_complexity_value), row, score, behavior_status)
        if isinstance(row.get("decomp_wall_sec"), int | float):
            decompile_sec = float(row.get("decomp_wall_sec") or 0.0)
            cost_values_by_behavior_status.setdefault(behavior_status, []).append(decompile_sec)
            cost_values_by_stage_first_failure.setdefault(first_stage, []).append(decompile_sec)
            score_bucket_name = (
                "perfect"
                if score == 1.0
                else "zero"
                if score == 0.0
                else "low"
                if score < 0.25
                else "medium"
                if score < 0.75
                else "high"
            )
            cost_values_by_score_bucket.setdefault(score_bucket_name, []).append(decompile_sec)
            cost_bucket_name = cost_bucket(decompile_sec)
            scores_by_cost_bucket.setdefault(cost_bucket_name, []).append(score)
            lost_score_by_cost_bucket[cost_bucket_name] += score_loss
        behavior_status_by_stage_first_failure.setdefault(first_stage, Counter())[behavior_status] += 1
        zero_reason = row_zero_credit_reason(row) if score == 0.0 else "nonzero"
        behavior_status_by_zero_credit_reason.setdefault(zero_reason, Counter())[behavior_status] += 1
        if behavior_status != "pass":
            failure_owner = behavior_failure_owner(behavior_status)
            behavior_failure_owner_counts[failure_owner] += 1
            detail_signature = behavior_detail_signature(behavior.get("detail"))
            if detail_signature != "none":
                behavior_failure_detail_counts[detail_signature] += 1
                behavior_failure_detail_rows.setdefault(detail_signature, []).append(triage_row_summary(row))
        source_lines = row.get("source_body_line_count")
        decomp_lines = row.get("decomp_line_count")
        source_bytes = row.get("source_body_byte_count")
        decomp_bytes = row.get("decomp_byte_count")
        if isinstance(source_lines, int | float):
            source_body_line_counts.append(float(source_lines))
        if isinstance(decomp_lines, int | float):
            decomp_line_counts.append(float(decomp_lines))
        if isinstance(source_bytes, int | float):
            source_body_byte_counts.append(float(source_bytes))
        if isinstance(decomp_bytes, int | float):
            decomp_byte_counts.append(float(decomp_bytes))
        if isinstance(source_lines, int | float) and isinstance(decomp_lines, int | float) and source_lines > 0:
            decomp_to_source_line_ratios.append(float(decomp_lines) / float(source_lines))
        if isinstance(source_bytes, int | float) and isinstance(decomp_bytes, int | float) and source_bytes > 0:
            decomp_to_source_byte_ratios.append(float(decomp_bytes) / float(source_bytes))
        if isinstance(source_lines, int | float) or isinstance(decomp_lines, int | float):
            source_decomp_size_hot_rows.append(
                {
                    "entry_id": row.get("entry_id"),
                    "function_name": row.get("function_name"),
                    "address": row.get("address"),
                    "semantic_score_percent": row.get("semantic_score_percent"),
                    "behavior_status": behavior_status,
                    "source_body_line_count": source_lines,
                    "decomp_line_count": decomp_lines,
                    "decomp_to_source_line_ratio": round(float(decomp_lines) / float(source_lines), 6)
                    if isinstance(source_lines, int | float)
                    and isinstance(decomp_lines, int | float)
                    and source_lines > 0
                    else None,
                }
            )
        case_pass_rate = behavior.get("case_pass_rate")
        if isinstance(case_pass_rate, int | float):
            behavior_case_pass_rates.append(float(case_pass_rate))
        case_pass_count = int(behavior.get("case_pass_count") or 0)
        compared_case_count = int(behavior.get("compared_case_count") or behavior.get("case_count") or 0)
        if behavior_status != "pass" and case_pass_count > 0:
            behavior_partial_progress_rows.append(
                {
                    **triage_row_summary(row),
                    "behavior_status": behavior_status,
                    "case_pass_count": case_pass_count,
                    "case_fail_count": int(behavior.get("case_fail_count") or 0),
                    "compared_case_count": compared_case_count,
                    "case_pass_rate": round(case_pass_count / compared_case_count, 6)
                    if compared_case_count
                    else 0.0,
                    "first_mismatch_index": behavior.get("first_mismatch_index"),
                    "candidate_missing_line_count": int(behavior.get("candidate_missing_line_count") or 0),
                    "candidate_extra_line_count": int(behavior.get("candidate_extra_line_count") or 0),
                }
            )
        cost_hot_rows.append(
            {
                "entry_id": row.get("entry_id"),
                "function_name": row.get("function_name"),
                "address": row.get("address"),
                "semantic_score_percent": row.get("semantic_score_percent"),
                "behavior_status": behavior_status,
                "decompile_sec": row.get("decomp_wall_sec"),
                "behavior_wall_sec": behavior.get("wall_sec"),
            }
        )
        if score == 1.0:
            score_distribution["perfect"] += 1
        elif score == 0.0:
            score_distribution["zero"] += 1
        elif score < 0.25:
            score_distribution["low"] += 1
        elif score < 0.75:
            score_distribution["medium"] += 1
        else:
            score_distribution["high"] += 1
        for component, value in (row.get("static_similarity_components") or {}).items():
            if isinstance(value, int | float):
                static_component_sums[component] += float(value)
        static_gaps = row.get("static_similarity_gaps")
        if isinstance(static_gaps, dict):
            row_source_total = float(static_gaps.get("source_feature_total", 0.0) or 0.0)
            row_decomp_total = float(static_gaps.get("decomp_feature_total", 0.0) or 0.0)
            row_intersection_total = float(static_gaps.get("intersection_feature_total", 0.0) or 0.0)
            row_union_total = float(static_gaps.get("union_feature_total", 0.0) or 0.0)
            row_missing_total = float(static_gaps.get("missing_feature_total", 0.0) or 0.0)
            row_extra_total = float(static_gaps.get("extra_feature_total", 0.0) or 0.0)
            missing_density = round(row_missing_total / row_source_total, 6) if row_source_total else 0.0
            extra_density = round(row_extra_total / row_decomp_total, 6) if row_decomp_total else 0.0
            static_missing_density_values.append(missing_density)
            static_extra_density_values.append(extra_density)
            missing_gap_bucket = feature_gap_bucket(row_missing_total)
            static_score_by_missing_gap_bucket.setdefault(missing_gap_bucket, []).append(static_score)
            if row_missing_total > 0.0 or row_extra_total > 0.0:
                static_gap_hot_rows.append(
                    {
                        **triage_row_summary(row),
                        "static_semantic_score_percent": row.get("static_semantic_score_percent"),
                        "source_feature_total": row_source_total,
                        "decomp_feature_total": row_decomp_total,
                        "intersection_feature_total": row_intersection_total,
                        "union_feature_total": row_union_total,
                        "missing_feature_total": row_missing_total,
                        "extra_feature_total": row_extra_total,
                        "missing_density": missing_density,
                        "extra_density": extra_density,
                        "top_missing_features": (static_gaps.get("top_missing_features") or [])[:5],
                        "top_extra_features": (static_gaps.get("top_extra_features") or [])[:5],
                    }
                )
                density_key = f"missing:{missing_gap_bucket}|extra:{feature_gap_bucket(row_extra_total)}"
                density_bucket = static_gap_density_rows.setdefault(
                    density_key,
                    {
                        "row_count": 0,
                        "score_sum": 0.0,
                        "missing_feature_total": 0.0,
                        "extra_feature_total": 0.0,
                        "top_rows": [],
                    },
                )
                density_bucket["row_count"] += 1
                density_bucket["score_sum"] += score
                density_bucket["missing_feature_total"] += row_missing_total
                density_bucket["extra_feature_total"] += row_extra_total
                density_bucket["top_rows"].append(triage_row_summary(row))
            source_feature_total_values.append(row_source_total)
            direct_feature_total = row.get("source_static_feature_count_direct")
            expanded_feature_total = row.get("source_static_feature_count_inline_expanded")
            if isinstance(direct_feature_total, int | float):
                source_feature_total_direct_values.append(float(direct_feature_total))
            if isinstance(expanded_feature_total, int | float):
                source_feature_total_inline_expanded_values.append(float(expanded_feature_total))
            decomp_feature_total_values.append(row_decomp_total)
            static_intersection_feature_total_values.append(row_intersection_total)
            static_union_feature_total_values.append(row_union_total)
            if (
                float(row.get("semantic_score", 0.0) or 0.0) < 1.0
                and (row_source_total >= 40.0 or float(row.get("source_body_line_count") or 0.0) >= 40.0)
            ):
                hard_function_rows.append(
                    {
                        **triage_row_summary(row),
                        "source_feature_total": row_source_total,
                        "source_body_line_count": row.get("source_body_line_count"),
                        "decomp_wall_sec": row.get("decomp_wall_sec"),
                    }
                )
            source_feature_rows += int(row_source_total > 0.0)
            decomp_feature_rows += int(row_decomp_total > 0.0)
            static_decomp_absent_feature_rows += int(row_source_total > 0.0 and row_decomp_total == 0.0)
            if row_source_total > 0.0 and row_decomp_total == 0.0:
                add_coverage_blind_spot("source_features_without_decomp_features", row)
            static_missing_feature_rows += int(row_missing_total > 0.0)
            static_extra_feature_rows += int(row_extra_total > 0.0)
            static_zero_similarity_rows += int(row_source_total > 0.0 and row_intersection_total == 0.0)
            missing_feature_count_values.append(row_missing_total)
            extra_feature_count_values.append(row_extra_total)
            for key in [
                "source_feature_total",
                "decomp_feature_total",
                "intersection_feature_total",
                "union_feature_total",
                "missing_feature_total",
                "extra_feature_total",
            ]:
                value = static_gaps.get(key)
                if isinstance(value, int | float):
                    static_gap_totals[key] += value
            for item in static_gaps.get("top_missing_features") or []:
                if isinstance(item, dict) and isinstance(item.get("feature"), str) and isinstance(item.get("count"), int | float):
                    static_missing_feature_counts[item["feature"]] += item["count"]
            for item in static_gaps.get("top_extra_features") or []:
                if isinstance(item, dict) and isinstance(item.get("feature"), str) and isinstance(item.get("count"), int | float):
                    static_extra_feature_counts[item["feature"]] += item["count"]
        gap_components = row.get("static_similarity_gap_components")
        if isinstance(gap_components, dict):
            for component, details in gap_components.items():
                if component not in static_gap_component_totals or not isinstance(details, dict):
                    continue
                component_missing_total = float(details.get("missing_feature_total", 0.0) or 0.0)
                component_extra_total = float(details.get("extra_feature_total", 0.0) or 0.0)
                component_source_total = float(details.get("source_feature_total", 0.0) or 0.0)
                component_decomp_total = float(details.get("decomp_feature_total", 0.0) or 0.0)
                component_intersection_total = float(details.get("intersection_feature_total", 0.0) or 0.0)
                component_source_present = component_source_total > 0.0
                component_decomp_present = component_decomp_total > 0.0
                component_intersection_present = component_intersection_total > 0.0
                static_component_source_feature_values[component].append(component_source_total)
                static_component_decomp_feature_values[component].append(component_decomp_total)
                static_component_missing_row_counts[component] += int(component_missing_total > 0.0)
                static_component_zero_similarity_row_counts[component] += int(
                    component_source_total > 0.0 and component_intersection_total == 0.0
                )
                absence_counts = static_component_absence_counts[component]
                absence_counts["observed_row_count"] += 1
                absence_counts["source_present_row_count"] += int(component_source_present)
                absence_counts["decomp_present_row_count"] += int(component_decomp_present)
                absence_counts["intersection_present_row_count"] += int(component_intersection_present)
                if component_source_present and component_decomp_present:
                    absence_counts["both_present_row_count"] += 1
                elif component_source_present:
                    absence_counts["source_only_row_count"] += 1
                elif component_decomp_present:
                    absence_counts["decomp_only_row_count"] += 1
                else:
                    absence_counts["both_absent_row_count"] += 1
                if component_source_present and not component_intersection_present:
                    absence_counts["zero_intersection_source_present_row_count"] += 1
                if component_source_present and not component_decomp_present:
                    static_component_absence_rows[component]["source_only_rows"].append(
                        {
                            **triage_row_summary(row),
                            "component": component,
                            "component_source_feature_total": component_source_total,
                            "component_decomp_feature_total": component_decomp_total,
                            "component_intersection_feature_total": component_intersection_total,
                        }
                    )
                if component_decomp_present and not component_source_present:
                    static_component_absence_rows[component]["decomp_only_rows"].append(
                        {
                            **triage_row_summary(row),
                            "component": component,
                            "component_source_feature_total": component_source_total,
                            "component_decomp_feature_total": component_decomp_total,
                            "component_intersection_feature_total": component_intersection_total,
                        }
                    )
                if component_source_present and not component_intersection_present:
                    static_component_absence_rows[component]["zero_intersection_rows"].append(
                        {
                            **triage_row_summary(row),
                            "component": component,
                            "component_source_feature_total": component_source_total,
                            "component_decomp_feature_total": component_decomp_total,
                            "component_intersection_feature_total": component_intersection_total,
                        }
                    )
                for key in [
                    "source_feature_total",
                    "decomp_feature_total",
                    "intersection_feature_total",
                    "union_feature_total",
                    "missing_feature_total",
                    "extra_feature_total",
                ]:
                    value = details.get(key)
                    if isinstance(value, int | float):
                        static_gap_component_totals[component][key] += value
                for item in details.get("top_missing_features") or []:
                    if (
                        isinstance(item, dict)
                        and isinstance(item.get("feature"), str)
                        and isinstance(item.get("count"), int | float)
                    ):
                        static_gap_component_missing_features[component][item["feature"]] += item["count"]
                for item in details.get("top_extra_features") or []:
                    if (
                        isinstance(item, dict)
                        and isinstance(item.get("feature"), str)
                        and isinstance(item.get("count"), int | float)
                    ):
                        static_gap_component_extra_features[component][item["feature"]] += item["count"]
                if component == "signature":
                    signedness_gap = signedness_only_signature_gap(details)
                    total_signedness_gap = signedness_gap["param_pair_count"] + signedness_gap["return_pair_count"]
                    if total_signedness_gap > 0.0:
                        for key, value in signedness_gap.items():
                            signedness_only_signature_gap_totals[key] += value
                        signedness_only_signature_gap_rows.append(
                            {
                                **triage_row_summary(row),
                                **{
                                    key: round(value, 6)
                                    for key, value in signedness_gap.items()
                                    if value > 0.0
                                },
                            }
                        )
                if component_missing_total > 0.0 or component_extra_total > 0.0:
                    component_row = {
                        **triage_row_summary(row),
                        "component": component,
                        "component_missing_feature_total": component_missing_total,
                        "component_extra_feature_total": component_extra_total,
                        "component_source_feature_total": component_source_total,
                        "component_decomp_feature_total": component_decomp_total,
                    }
                    if component == "signature":
                        signature_gap_rows.append(component_row)
                    elif component == "memory":
                        memory_gap_rows.append(component_row)
                    elif component == "call":
                        call_gap_rows.append(component_row)
                    elif component == "control_flow":
                        control_flow_gap_rows.append(component_row)
        source_return_kind = str(row.get("source_return_kind") or "unknown")
        decomp_return_kind = str(row.get("decomp_return_kind") or "missing")
        source_param_kinds = row.get("source_param_kinds") if isinstance(row.get("source_param_kinds"), list) else []
        decomp_param_kinds = row.get("decomp_param_kinds") if isinstance(row.get("decomp_param_kinds"), list) else []
        signature_return_pair_counts[f"{source_return_kind}->{decomp_return_kind}"] += 1
        max_param_count = max(len(source_param_kinds), len(decomp_param_kinds))
        param_mismatch_count = 0
        missing_param_count = 0
        extra_param_count = 0
        for index in range(max_param_count):
            source_kind = str(source_param_kinds[index]) if index < len(source_param_kinds) else "missing"
            decomp_kind = str(decomp_param_kinds[index]) if index < len(decomp_param_kinds) else "missing"
            signature_param_pair_counts[f"{source_kind}->{decomp_kind}"] += 1
            param_mismatch_count += int(source_kind != decomp_kind)
            missing_param_count += int(source_kind != "missing" and decomp_kind == "missing")
            extra_param_count += int(source_kind == "missing" and decomp_kind != "missing")
        return_mismatch = source_return_kind != decomp_return_kind
        arity_mismatch = len(source_param_kinds) != len(decomp_param_kinds)
        if return_mismatch or param_mismatch_count > 0:
            signature_pair_gap_rows.append(
                {
                    **triage_row_summary(row),
                    "source_return_kind": source_return_kind,
                    "decomp_return_kind": decomp_return_kind,
                    "source_param_kinds": source_param_kinds,
                    "decomp_param_kinds": decomp_param_kinds,
                    "param_mismatch_count": param_mismatch_count,
                    "missing_param_count": missing_param_count,
                    "extra_param_count": extra_param_count,
                    "return_mismatch": return_mismatch,
                }
            )
        if arity_mismatch:
            signature_param_arity_mismatch_rows.append(
                {
                    **triage_row_summary(row),
                    "source_param_count": len(source_param_kinds),
                    "decomp_param_count": len(decomp_param_kinds),
                    "missing_param_count": missing_param_count,
                    "extra_param_count": extra_param_count,
                    "source_param_kinds": source_param_kinds,
                    "decomp_param_kinds": decomp_param_kinds,
                }
            )
        lang = row["language"]
        bucket = by_language.setdefault(
            lang,
            {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
        )
        add_bucket(bucket, row)

        arch = str(row.get("binary_arch") or "unknown")
        arch_bucket = by_arch.setdefault(
            arch,
            {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
        )
        add_bucket(arch_bucket, row)
        arch_metrics = architecture_stage_metrics.setdefault(
            arch,
            {
                "row_count": 0,
                "score_sum": 0.0,
                "lost_score_sum": 0.0,
                "missing_feature_total": 0.0,
                "extra_feature_total": 0.0,
                "behavior_status_counts": Counter(),
                "stage_first_failure_counts": Counter(),
                "top_rows": [],
            },
        )
        arch_metrics["row_count"] += 1
        arch_metrics["score_sum"] += score
        arch_metrics["lost_score_sum"] += score_loss
        static_gaps_for_arch = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
        arch_metrics["missing_feature_total"] += float(static_gaps_for_arch.get("missing_feature_total", 0.0) or 0.0)
        arch_metrics["extra_feature_total"] += float(static_gaps_for_arch.get("extra_feature_total", 0.0) or 0.0)
        arch_metrics["behavior_status_counts"][behavior_status] += 1
        arch_metrics["stage_first_failure_counts"][first_stage] += 1
        if score < 1.0:
            arch_metrics["top_rows"].append(triage_row_summary(row))

        return_kind = str(row.get("source_return_kind") or "unknown")
        return_bucket = by_source_return_kind.setdefault(
            return_kind,
            {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
        )
        add_bucket(return_bucket, row)

        param_shape = str(row.get("source_param_shape") or "unknown")
        param_bucket = by_source_param_shape.setdefault(
            param_shape,
            {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
        )
        add_bucket(param_bucket, row)

        entry_bucket = by_entry.setdefault(
            row["entry_id"],
            {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
        )
        add_bucket(entry_bucket, row)
        if row.get("mapping_status") != "matched" or (
            row.get("fission_name")
            and row.get("function_name")
            and normalize_name(str(row.get("fission_name"))) != normalize_name(str(row.get("function_name")))
        ):
            name_recovery_rows.append(triage_row_summary(row))
            if "mapping_name_recovery" not in areas:
                add_priority_bucket_row(
                    roadmap_priority_metrics,
                    "p4_fid_name_recovery",
                    row,
                    behavior_status,
                    first_stage,
                    score,
                )

        if isinstance(debug_decomp, dict):
            debug_decomp_row_count += 1
            debug_owner_bucket_counts.update(debug_decomp.get("owner_buckets") or [])
            stage_status = debug_decomp.get("stage_status")
            if isinstance(stage_status, dict):
                debug_stage_status_row_count += 1
                furthest_stage = furthest_ok_stage(stage_status)
                stage_furthest_ok_counts[furthest_stage] += 1
                if first_stage != "none":
                    stage_first_blocker_lost_score[first_stage] += score_loss
                pipeline_statuses = [
                    stage_status.get(stage)
                    for stage in STAGE_FAILURE_ORDER
                    if stage_status.get(stage) is not None
                ]
                pipeline_ok = bool(pipeline_statuses) and all(status == "ok" for status in pipeline_statuses)
                stage_funnel_counts["mapped_with_debug_stage_status"] += 1
                if pipeline_ok:
                    stage_funnel_counts["all_pipeline_stages_ok"] += 1
                for stage in STAGE_FAILURE_ORDER:
                    if stage_status.get(stage) == "ok":
                        stage_funnel_counts[f"{stage}_ok"] += 1
                debug_stage_status_counts.update(
                    f"{stage}:{status}"
                    for stage, status in stage_status.items()
                    if status is not None
                )
                for stage in STAGE_FAILURE_ORDER:
                    status = stage_status.get(stage)
                    stage_status_metrics[stage][str(status if status is not None else "missing")] += 1
                    if status is not None:
                        debug_stage_status_matrix[stage][str(status)] += 1
            quality = debug_decomp.get("quality_evidence")
            if isinstance(quality, dict):
                for key, value in numeric_items(quality):
                    debug_quality_evidence_totals[key] += value
                    if value != 0:
                        debug_quality_evidence_nonzero_rows[key] += 1
            pipeline = debug_decomp.get("rust_sleigh_pipeline")
            add_numeric_debug_pipeline_values(debug_pipeline_numeric_values, pipeline)
            template_sources = (
                pipeline.get("template_source_counts")
                if isinstance(pipeline, dict)
                and isinstance(pipeline.get("template_source_counts"), dict)
                else {}
            )
            for key, value in template_sources.items():
                if isinstance(value, int | float):
                    debug_template_source_totals[canonical_sleigh_template_source(str(key))] += value

        if isinstance(preview_stats, dict):
            nir_build_stats_row_count += 1
            row_debt_total = 0.0
            row_debt_metrics: dict[str, float] = {}
            for key, value in numeric_items(preview_stats):
                nir_build_stats_numeric_totals[key] += value
                nir_build_stats_values.setdefault(key, []).append(value)
                if value != 0:
                    nir_build_stats_nonzero_rows[key] += 1
                if is_debt_metric_name(key) and value != 0:
                    row_debt_metrics[key] = value
                    row_debt_total += value
            if row_debt_metrics:
                nir_build_stats_debt_hot_rows.append(
                    {
                        "entry_id": row.get("entry_id"),
                        "function_name": row.get("function_name"),
                        "address": row.get("address"),
                        "semantic_score_percent": row.get("semantic_score_percent"),
                        "behavior_status": behavior_status,
                        "stage_first_failure": first_stage,
                        "debt_metric_total": round(row_debt_total, 6),
                        "top_debt_metrics": [
                            {"metric": key, "value": value}
                            for key, value in sorted(
                                row_debt_metrics.items(),
                                key=lambda item: (item[1], item[0]),
                                reverse=True,
                            )[:10]
                        ],
                    }
                )
                nir_debt_row_count += 1
                nir_debt_score_values.append(score)
                nir_debt_behavior_status_counts[behavior_status] += 1
                nir_debt_stage_first_failure_counts[first_stage] += 1
            else:
                nir_no_debt_score_values.append(score)

        for tag in row.get("tags") or []:
            tag_bucket = by_tag.setdefault(
                tag,
                {"row_count": 0, "mapped": 0, "decomp_success": 0, "behavior_pass": 0, "score_sum": 0.0},
            )
            add_bucket(tag_bucket, row)

        if isinstance(behavior, dict) and behavior.get("status") == "mismatch":
            first_mismatch = behavior.get("first_mismatch_index")
            behavior_first_mismatch_index_counts[str(first_mismatch)] += 1
            oracle = behavior.get("oracle")
            candidate = behavior.get("candidate")
            if isinstance(oracle, list) and isinstance(candidate, list):
                length_delta = len(candidate) - len(oracle)
                behavior_output_length_delta_counts[str(length_delta)] += 1
                if length_delta != 0:
                    behavior_mismatch_kind_counts["output_length"] += 1
                    if length_delta < 0:
                        behavior_missing_candidate_line_total += abs(length_delta)
                    else:
                        behavior_extra_candidate_line_total += length_delta
                else:
                    behavior_mismatch_kind_counts["wrong_value"] += 1
            else:
                behavior_mismatch_kind_counts["unknown"] += 1

    for bucket in (
        list(by_language.values())
        + list(by_arch.values())
        + list(by_source_return_kind.values())
        + list(by_source_param_shape.values())
        + list(by_tag.values())
        + list(by_entry.values())
    ):
        count = max(1, bucket["row_count"])
        avg_score = round(bucket.pop("score_sum") / count, 6)
        bucket["avg_semantic_score"] = avg_score
        bucket["avg_semantic_score_percent"] = percent(avg_score)
    host_statuses = Counter(
        row.get("behavior", {}).get("reason")
        for row in rows
        if row.get("behavior", {}).get("status") == "host_execution_unavailable"
    )
    weighted_semantic_similarity = round(sum(score_values) / total, 6) if total else 0.0
    decomp_times = [float(row.get("decomp_wall_sec") or 0.0) for row in rows if isinstance(row.get("decomp_wall_sec"), int | float)]
    behavior_compile_times = [
        float(row.get("behavior", {}).get("compile_sec") or 0.0)
        for row in rows
        if isinstance(row.get("behavior", {}).get("compile_sec"), int | float)
    ]
    behavior_run_times = [
        float(row.get("behavior", {}).get("run_sec") or 0.0)
        for row in rows
        if isinstance(row.get("behavior", {}).get("run_sec"), int | float)
    ]
    behavior_wall_times = [
        float(row.get("behavior", {}).get("wall_sec") or 0.0)
        for row in rows
        if isinstance(row.get("behavior", {}).get("wall_sec"), int | float)
    ]
    behavior_case_total = sum(
        int(row.get("behavior", {}).get("case_count") or 0)
        for row in rows
        if isinstance(row.get("behavior"), dict)
    )
    behavior_compared_case_total = sum(
        int(row.get("behavior", {}).get("compared_case_count") or row.get("behavior", {}).get("case_count") or 0)
        for row in rows
        if isinstance(row.get("behavior"), dict)
    )
    behavior_case_pass_total = sum(
        int(row.get("behavior", {}).get("case_pass_count") or 0)
        for row in rows
        if isinstance(row.get("behavior"), dict)
    )
    behavior_case_fail_total = sum(
        int(row.get("behavior", {}).get("case_fail_count") or 0)
        for row in rows
        if isinstance(row.get("behavior"), dict)
    )
    partial_mismatch_rows = sum(
        1
        for row in rows
        if row.get("behavior", {}).get("status") == "mismatch"
        and int(row.get("behavior", {}).get("case_pass_count") or 0) > 0
    )
    partial_progress_rows = sum(
        1
        for row in rows
        if row.get("behavior", {}).get("status")
        in {"mismatch", "candidate_run_timeout", "candidate_run_failed"}
        and int(row.get("behavior", {}).get("case_pass_count") or 0) > 0
    )
    partial_timeout_rows = [
        row
        for row in rows
        if row.get("behavior", {}).get("status") == "candidate_run_timeout"
        and int(row.get("behavior", {}).get("case_pass_count") or 0) > 0
    ]
    partial_timeout_case_pass_total = sum(
        int(row.get("behavior", {}).get("case_pass_count") or 0)
        for row in partial_timeout_rows
    )
    partial_timeout_compared_case_total = sum(
        int(
            row.get("behavior", {}).get("compared_case_count")
            or row.get("behavior", {}).get("case_count")
            or 0
        )
        for row in partial_timeout_rows
    )
    partial_timeout_missing_line_total = sum(
        int(row.get("behavior", {}).get("candidate_missing_line_count") or 0)
        for row in partial_timeout_rows
    )
    static_source_total = float(static_gap_totals.get("source_feature_total", 0.0) or 0.0)
    static_decomp_total = float(static_gap_totals.get("decomp_feature_total", 0.0) or 0.0)
    static_gap_summary = dict(sorted(static_gap_totals.items()))
    static_gap_summary["missing_feature_rate"] = round(
        float(static_gap_totals.get("missing_feature_total", 0.0) or 0.0) / static_source_total,
        6,
    ) if static_source_total else 0.0
    static_gap_summary["extra_feature_rate"] = round(
        float(static_gap_totals.get("extra_feature_total", 0.0) or 0.0) / static_decomp_total,
        6,
    ) if static_decomp_total else 0.0
    static_gap_summary["top_missing_features"] = [
        {"feature": feature, "count": count}
        for feature, count in static_missing_feature_counts.most_common(20)
    ]
    static_gap_summary["top_extra_features"] = [
        {"feature": feature, "count": count}
        for feature, count in static_extra_feature_counts.most_common(20)
    ]
    static_intersection_total = float(static_gap_totals.get("intersection_feature_total", 0.0) or 0.0)
    static_union_total = float(static_gap_totals.get("union_feature_total", 0.0) or 0.0)
    static_missing_total = float(static_gap_totals.get("missing_feature_total", 0.0) or 0.0)
    static_extra_total = float(static_gap_totals.get("extra_feature_total", 0.0) or 0.0)
    static_gap_component_summary: dict[str, dict[str, Any]] = {}
    static_gap_component_top_summary: dict[str, dict[str, Any]] = {}
    static_component_precision_recall_summary: dict[str, dict[str, Any]] = {}
    for component, totals in static_gap_component_totals.items():
        component_source_total = float(totals.get("source_feature_total", 0.0) or 0.0)
        component_decomp_total = float(totals.get("decomp_feature_total", 0.0) or 0.0)
        component_intersection_total = float(totals.get("intersection_feature_total", 0.0) or 0.0)
        component_precision = (
            round(component_intersection_total / component_decomp_total, 6)
            if component_decomp_total
            else 0.0
        )
        component_recall = (
            round(component_intersection_total / component_source_total, 6)
            if component_source_total
            else 0.0
        )
        component_f1 = (
            round((2.0 * component_precision * component_recall) / (component_precision + component_recall), 6)
            if component_precision + component_recall
            else 0.0
        )
        static_gap_component_summary[component] = dict(sorted(totals.items()))
        static_gap_component_summary[component]["missing_feature_rate"] = round(
            float(totals.get("missing_feature_total", 0.0) or 0.0) / component_source_total,
            6,
        ) if component_source_total else 0.0
        static_gap_component_summary[component]["extra_feature_rate"] = round(
            float(totals.get("extra_feature_total", 0.0) or 0.0) / component_decomp_total,
            6,
        ) if component_decomp_total else 0.0
        static_component_precision_recall_summary[component] = {
            "source_feature_total": component_source_total,
            "decomp_feature_total": component_decomp_total,
            "intersection_feature_total": component_intersection_total,
            "precision": component_precision,
            "precision_percent": percent(component_precision),
            "recall": component_recall,
            "recall_percent": percent(component_recall),
            "f1": component_f1,
            "f1_percent": percent(component_f1),
        }
        static_gap_component_top_summary[component] = {
            "top_missing_features": [
                {"feature": feature, "count": count}
                for feature, count in static_gap_component_missing_features[component].most_common(12)
            ],
            "top_extra_features": [
                {"feature": feature, "count": count}
                for feature, count in static_gap_component_extra_features[component].most_common(12)
            ],
        }
    static_component_absence_export: dict[str, dict[str, Any]] = {}

    def top_absence_rows(component: str, row_kind: str) -> list[dict[str, Any]]:
        return sorted(
            static_component_absence_rows.get(component, {}).get(row_kind) or [],
            key=lambda row: (
                -float(row.get("component_source_feature_total") or 0.0),
                -float(row.get("component_decomp_feature_total") or 0.0),
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:8]

    for component, counts in sorted(static_component_absence_counts.items()):
        observed = int(counts.get("observed_row_count", 0) or 0)
        static_component_absence_export[component] = {
            "observed_row_count": observed,
            "observed_row_rate_total_denominator": round(observed / total, 6) if total else 0.0,
            "source_present_row_count": int(counts.get("source_present_row_count", 0) or 0),
            "source_present_row_rate_observed_denominator": round(
                float(counts.get("source_present_row_count", 0) or 0) / observed,
                6,
            ) if observed else 0.0,
            "decomp_present_row_count": int(counts.get("decomp_present_row_count", 0) or 0),
            "decomp_present_row_rate_observed_denominator": round(
                float(counts.get("decomp_present_row_count", 0) or 0) / observed,
                6,
            ) if observed else 0.0,
            "intersection_present_row_count": int(counts.get("intersection_present_row_count", 0) or 0),
            "intersection_present_row_rate_observed_denominator": round(
                float(counts.get("intersection_present_row_count", 0) or 0) / observed,
                6,
            ) if observed else 0.0,
            "both_present_row_count": int(counts.get("both_present_row_count", 0) or 0),
            "source_only_row_count": int(counts.get("source_only_row_count", 0) or 0),
            "decomp_only_row_count": int(counts.get("decomp_only_row_count", 0) or 0),
            "both_absent_row_count": int(counts.get("both_absent_row_count", 0) or 0),
            "zero_intersection_source_present_row_count": int(
                counts.get("zero_intersection_source_present_row_count", 0) or 0
            ),
            "source_only_rows": top_absence_rows(component, "source_only_rows"),
            "decomp_only_rows": top_absence_rows(component, "decomp_only_rows"),
            "zero_intersection_rows": top_absence_rows(component, "zero_intersection_rows"),
        }
    triage_priority_rows = [
        triage_row_summary(row)
        for row in sorted(rows, key=row_triage_priority)
        if float(row.get("semantic_score", 0.0) or 0.0) < 1.0
    ][:20]
    mapped_debug_denominator = max(1, mapped)
    debug_stage_status_matrix_export = {
        stage: dict(sorted(counts.items()))
        for stage, counts in debug_stage_status_matrix.items()
        if counts
    }
    score_by_behavior_status = {
        status: numeric_distribution(values)
        for status, values in sorted(score_values_by_behavior_status.items())
    }
    score_by_stage_first_failure = {
        stage: numeric_distribution(values)
        for stage, values in sorted(score_values_by_stage_first_failure.items())
    }
    pipeline_stage_metrics = {
        stage: {
            "row_count": sum(counts.values()),
            "ok_count": int(counts.get("ok", 0)),
            "missing_count": int(counts.get("missing", 0)),
            "non_ok_count": sum(count for status, count in counts.items() if status != "ok"),
            "ok_rate": round(float(counts.get("ok", 0)) / sum(counts.values()), 6)
            if sum(counts.values())
            else 0.0,
            "status_counts": dict(sorted(counts.items())),
        }
        for stage, counts in stage_status_metrics.items()
        if counts
    }
    nir_debt_totals = {
        key: value
        for key, value in sorted(nir_build_stats_numeric_totals.items())
        if is_debt_metric_name(key) and value != 0
    }
    nir_build_stats_distributions = {
        key: numeric_distribution(values)
        for key, values in sorted(nir_build_stats_values.items())
        if key in nir_debt_totals
    }
    nir_build_stats_debt_hot_rows = sorted(
        nir_build_stats_debt_hot_rows,
        key=lambda row: (float(row.get("debt_metric_total") or 0.0), row.get("function_name") or ""),
        reverse=True,
    )[:20]
    cost_hot_rows_by_decompile = sorted(
        (
            row
            for row in cost_hot_rows
            if isinstance(row.get("decompile_sec"), int | float)
        ),
        key=lambda row: float(row.get("decompile_sec") or 0.0),
        reverse=True,
    )[:12]
    cost_hot_rows_by_behavior_wall = sorted(
        (
            row
            for row in cost_hot_rows
            if isinstance(row.get("behavior_wall_sec"), int | float)
        ),
        key=lambda row: float(row.get("behavior_wall_sec") or 0.0),
        reverse=True,
    )[:12]
    semantic_loss_hot_rows = sorted(
        semantic_loss_hot_rows,
        key=lambda row: (float(row.get("lost_score") or 0.0), row.get("function_name") or ""),
        reverse=True,
    )[:20]
    source_decomp_size_hot_rows = sorted(
        source_decomp_size_hot_rows,
        key=lambda row: (
            float(row.get("decomp_to_source_line_ratio") or 0.0),
            float(row.get("decomp_line_count") or 0.0),
            row.get("function_name") or "",
        ),
        reverse=True,
    )[:20]
    inline_expanded_static_hot_rows = sorted(
        inline_expanded_static_hot_rows,
        key=lambda row: (
            float(row.get("static_score_delta_percent") or 0.0),
            str(row.get("function_name") or ""),
        ),
        reverse=True,
    )[:20]
    improvement_axis_export: dict[str, dict[str, Any]] = {}
    for axis, metrics in sorted(improvement_axis_metrics.items()):
        row_count = int(metrics.get("row_count", 0) or 0)
        top_rows = sorted(
            metrics.get("top_rows") or [],
            key=lambda row: (
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:12]
        improvement_axis_export[axis] = {
            "row_count": row_count,
            "row_rate": round(row_count / total, 6) if total else 0.0,
            "avg_semantic_score": round(float(metrics.get("score_sum", 0.0) or 0.0) / row_count, 6)
            if row_count
            else 0.0,
            "avg_semantic_score_percent": percent(
                round(float(metrics.get("score_sum", 0.0) or 0.0) / row_count, 6)
            ) if row_count else 0.0,
            "lost_score_sum": round(float(metrics.get("lost_score_sum", 0.0) or 0.0), 6),
            "missing_feature_total": round(float(metrics.get("missing_feature_total", 0.0) or 0.0), 6),
            "behavior_status_counts": dict(sorted(metrics.get("behavior_status_counts", Counter()).items())),
            "stage_first_failure_counts": dict(sorted(metrics.get("stage_first_failure_counts", Counter()).items())),
            "top_rows": top_rows,
        }
    focus_area_export: dict[str, dict[str, Any]] = {}
    for area, metrics in sorted(focus_area_metrics.items()):
        row_count = int(metrics.get("row_count", 0) or 0)
        top_rows = sorted(
            metrics.get("top_rows") or [],
            key=lambda row: (
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:12]
        focus_area_export[area] = {
            "row_count": row_count,
            "row_rate_total_denominator": round(row_count / total, 6) if total else 0.0,
            "avg_semantic_score": round(float(metrics.get("score_sum", 0.0) or 0.0) / row_count, 6)
            if row_count
            else 0.0,
            "avg_semantic_score_percent": percent(
                round(float(metrics.get("score_sum", 0.0) or 0.0) / row_count, 6)
            ) if row_count else 0.0,
            "lost_score_sum": round(float(metrics.get("lost_score_sum", 0.0) or 0.0), 6),
            "missing_feature_total": round(float(metrics.get("missing_feature_total", 0.0) or 0.0), 6),
            "behavior_status_counts": dict(sorted(metrics.get("behavior_status_counts", Counter()).items())),
            "stage_first_failure_counts": dict(sorted(metrics.get("stage_first_failure_counts", Counter()).items())),
            "top_rows": top_rows,
        }
    complexity_export: dict[str, dict[str, Any]] = {}
    for bucket_name, bucket in sorted(complexity_buckets.items()):
        row_count = int(bucket.get("row_count", 0) or 0)
        score_sum_for_bucket = float(bucket.get("score_sum", 0.0) or 0.0)
        complexity_export[bucket_name] = {
            "row_count": row_count,
            "row_rate": round(row_count / total, 6) if total else 0.0,
            "avg_semantic_score": round(score_sum_for_bucket / row_count, 6) if row_count else 0.0,
            "avg_semantic_score_percent": percent(round(score_sum_for_bucket / row_count, 6)) if row_count else 0.0,
            "behavior_pass_count": int(bucket.get("behavior_pass_count", 0) or 0),
            "behavior_pass_rate": round(float(bucket.get("behavior_pass_count", 0) or 0) / row_count, 6)
            if row_count
            else 0.0,
            "zero_score_count": int(bucket.get("zero_score_count", 0) or 0),
            "missing_feature_total": round(float(bucket.get("missing_feature_total", 0.0) or 0.0), 6),
            "source_line_count_distribution": numeric_distribution(bucket.get("source_line_counts") or []),
            "source_feature_count_distribution": numeric_distribution(bucket.get("source_feature_counts") or []),
        }
    hard_function_rows = sorted(
        hard_function_rows,
        key=lambda row: (
            float(row.get("semantic_score_percent") or 0.0),
            -float(row.get("source_feature_total") or 0.0),
            str(row.get("function_name") or ""),
        ),
    )[:20]
    score_sum = round(sum(score_values), 6)
    behavior_score_sum = round(sum(behavior_score_values), 6)
    static_score_sum = round(sum(static_score_values), 6)
    behavior_component_score_sum = round(0.65 * behavior_score_sum, 6)
    static_component_score_sum = round(0.35 * static_score_sum, 6)
    zero_score_count = int(score_distribution.get("zero", 0))
    nonzero_score_count = sum(1 for score in score_values if score > 0.0)
    perfect_score_count = sum(1 for score in score_values if score == 1.0)

    def row_stage_ok(row: dict[str, Any], stage: str) -> bool:
        debug_decomp = row.get("debug_decomp")
        if not isinstance(debug_decomp, dict):
            return False
        stage_status = debug_decomp.get("stage_status")
        return isinstance(stage_status, dict) and stage_status.get(stage) == "ok"

    admission_gate_counts = Counter(
        {
            "manifest_rows": total,
            "mapped_rows": mapped,
            "decompiled_rows": decomp_ok,
            "decode_ok_rows": sum(1 for row in rows if row_stage_ok(row, "decode")),
            "raw_pcode_ok_rows": sum(1 for row in rows if row_stage_ok(row, "raw_pcode")),
            "nir_build_ok_rows": sum(1 for row in rows if row_stage_ok(row, "nir_build")),
            "normalize_ok_rows": sum(1 for row in rows if row_stage_ok(row, "normalize")),
            "structuring_ok_rows": sum(1 for row in rows if row_stage_ok(row, "structuring")),
            "render_ok_rows": sum(1 for row in rows if row_stage_ok(row, "render")),
            "full_pipeline_ok_rows": sum(
                1
                for row in rows
                if all(row_stage_ok(row, stage) for stage in STAGE_FAILURE_ORDER if stage != "load")
            ),
            "candidate_compiled_rows": compile_ok,
            "behavior_pass_rows": behavior_pass,
            "static_perfect_rows": sum(
                1 for row in rows if float(row.get("static_semantic_score", 0.0) or 0.0) == 1.0
            ),
            "semantic_perfect_rows": perfect_score_count,
        }
    )
    admission_gate_metrics = {
        "gate_order": [
            "manifest_rows",
            "mapped_rows",
            "decompiled_rows",
            "decode_ok_rows",
            "raw_pcode_ok_rows",
            "nir_build_ok_rows",
            "normalize_ok_rows",
            "structuring_ok_rows",
            "render_ok_rows",
            "full_pipeline_ok_rows",
            "candidate_compiled_rows",
            "behavior_pass_rows",
            "static_perfect_rows",
            "semantic_perfect_rows",
        ],
        "counts": dict(admission_gate_counts),
        "rates_total_denominator": {
            key: round(float(value) / total, 6) if total else 0.0
            for key, value in sorted(admission_gate_counts.items())
        },
    }
    stage_transition_metrics = {
        "stage_ok_funnel_counts": dict(sorted(stage_funnel_counts.items())),
        "furthest_ok_stage_counts": dict(sorted(stage_furthest_ok_counts.items())),
        "lost_score_by_first_stage_blocker": {
            key: round(float(value), 6)
            for key, value in sorted(stage_first_blocker_lost_score.items())
        },
    }
    gate_order = [
        "manifest_rows",
        "mapped_rows",
        "decompiled_rows",
        "decode_ok_rows",
        "raw_pcode_ok_rows",
        "nir_build_ok_rows",
        "normalize_ok_rows",
        "structuring_ok_rows",
        "render_ok_rows",
        "full_pipeline_ok_rows",
        "candidate_compiled_rows",
        "behavior_pass_rows",
        "static_perfect_rows",
        "semantic_perfect_rows",
    ]
    gate_drop_rows: dict[str, int] = {}
    gate_retention_rates: dict[str, float] = {}
    previous_gate: str | None = None
    for gate in gate_order:
        count = int(admission_gate_counts.get(gate, 0))
        if previous_gate is not None:
            previous_count = int(admission_gate_counts.get(previous_gate, 0))
            gate_drop_rows[f"{previous_gate}->{gate}"] = max(0, previous_count - count)
            gate_retention_rates[f"{previous_gate}->{gate}"] = round(count / previous_count, 6) if previous_count else 0.0
        previous_gate = gate
    behavior_failure_detail_top_rows = {
        signature: rows_for_signature[:5]
        for signature, rows_for_signature in sorted(
            behavior_failure_detail_rows.items(),
            key=lambda item: (len(item[1]), item[0]),
            reverse=True,
        )[:12]
    }
    static_gap_density_export: dict[str, dict[str, Any]] = {}
    for bucket_name, bucket in sorted(static_gap_density_rows.items()):
        row_count = int(bucket.get("row_count", 0) or 0)
        top_rows = sorted(
            bucket.get("top_rows") or [],
            key=lambda row: (
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:8]
        static_gap_density_export[bucket_name] = {
            "row_count": row_count,
            "row_rate": round(row_count / total, 6) if total else 0.0,
            "avg_semantic_score": round(float(bucket.get("score_sum", 0.0) or 0.0) / row_count, 6)
            if row_count
            else 0.0,
            "avg_semantic_score_percent": percent(
                round(float(bucket.get("score_sum", 0.0) or 0.0) / row_count, 6)
            ) if row_count else 0.0,
            "missing_feature_total": round(float(bucket.get("missing_feature_total", 0.0) or 0.0), 6),
            "extra_feature_total": round(float(bucket.get("extra_feature_total", 0.0) or 0.0), 6),
            "top_rows": top_rows,
        }
    static_gap_hot_row_metrics = {
        "top_missing_feature_rows": sorted(
            static_gap_hot_rows,
            key=lambda row: (
                -float(row.get("missing_feature_total") or 0.0),
                float(row.get("static_semantic_score_percent") or row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:20],
        "top_extra_feature_rows": sorted(
            static_gap_hot_rows,
            key=lambda row: (
                -float(row.get("extra_feature_total") or 0.0),
                float(row.get("static_semantic_score_percent") or row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:20],
        "top_zero_intersection_rows": sorted(
            [
                row
                for row in static_gap_hot_rows
                if float(row.get("source_feature_total") or 0.0) > 0.0
                and float(row.get("intersection_feature_total") or 0.0) == 0.0
            ],
            key=lambda row: (
                -float(row.get("source_feature_total") or 0.0),
                float(row.get("static_semantic_score_percent") or row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:20],
    }
    semantic_readiness_metrics = {
        **{key: int(value) for key, value in sorted(semantic_readiness_counts.items())},
        "fully_perfect_rate": round(
            float(semantic_readiness_counts.get("fully_perfect_rows", 0)) / total,
            6,
        ) if total else 0.0,
        "behavior_pass_static_perfect_rate": round(
            float(semantic_readiness_counts.get("behavior_pass_static_perfect_rows", 0)) / total,
            6,
        ) if total else 0.0,
        "behavior_pass_static_gap_rate": round(
            float(semantic_readiness_counts.get("behavior_pass_static_gap_rows", 0)) / total,
            6,
        ) if total else 0.0,
        "static_perfect_behavior_nonpass_rate": round(
            float(semantic_readiness_counts.get("static_perfect_behavior_nonpass_rows", 0)) / total,
            6,
        ) if total else 0.0,
        "pipeline_ok_behavior_nonpass_rate": round(
            float(semantic_readiness_counts.get("pipeline_ok_behavior_nonpass_rows", 0)) / total,
            6,
        ) if total else 0.0,
    }
    benchmark_integrity_metrics = {
        "score_denominator_row_count": total,
        "row_count": total,
        "rows_excluded_from_semantic_score_denominator": 0,
        "rows_excluded_from_static_similarity_denominator": 0,
        "missing_source_features_penalized": True,
        "extra_decompiler_features_penalized": True,
        "behavior_missing_or_unsupported_rows_fail_closed": True,
        "unmapped_or_failed_rows_remain_in_denominator": True,
        "static_missing_feature_row_count": static_missing_feature_rows,
        "static_decomp_absent_feature_row_count": static_decomp_absent_feature_rows,
        "behavior_unsupported_or_ineligible_row_count": max(0, total - behavior_expected),
        "behavior_expected_but_not_executed_row_count": max(0, behavior_expected - behavior_executed),
    }
    semantic_quality_quadrant_export: dict[str, dict[str, Any]] = {}
    for quadrant, bucket in sorted(semantic_quality_quadrants.items()):
        row_count = int(bucket.get("row_count", 0) or 0)
        top_rows = sorted(
            bucket.get("top_rows") or [],
            key=lambda row: (
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:10]
        semantic_quality_quadrant_export[quadrant] = {
            "row_count": row_count,
            "row_rate_total_denominator": round(row_count / total, 6) if total else 0.0,
            "avg_semantic_score": round(float(bucket.get("score_sum", 0.0) or 0.0) / row_count, 6)
            if row_count
            else 0.0,
            "avg_semantic_score_percent": percent(
                round(float(bucket.get("score_sum", 0.0) or 0.0) / row_count, 6)
            ) if row_count else 0.0,
            "lost_score_sum": round(float(bucket.get("lost_score_sum", 0.0) or 0.0), 6),
            "missing_feature_total": round(float(bucket.get("missing_feature_total", 0.0) or 0.0), 6),
            "extra_feature_total": round(float(bucket.get("extra_feature_total", 0.0) or 0.0), 6),
            "top_rows": top_rows,
        }
    coverage_blind_spot_export = {
        kind: {
            "row_count": int(count),
            "row_rate_total_denominator": round(float(count) / total, 6) if total else 0.0,
            "top_rows": sorted(
                coverage_blind_spot_rows.get(kind) or [],
                key=lambda row: (
                    float(row.get("semantic_score_percent") or 0.0),
                    str(row.get("function_name") or ""),
                ),
            )[:8],
        }
        for kind, count in sorted(coverage_blind_spot_counts.items())
    }
    raw_pcode_compat_total = float(nir_build_stats_numeric_totals.get("raw_pcode_compat_import_count", 0.0) or 0.0)
    invalid_pcode_shape_total = float(debug_quality_evidence_totals.get("invalid_pcode_shape_count", 0.0) or 0.0)
    sleigh_blocker_row_count = len(sleigh_blocker_rows)
    sleigh_blocker_rows = sorted(
        sleigh_blocker_rows,
        key=lambda row: (
            float(row.get("semantic_score_percent") or 0.0),
            str(row.get("function_name") or ""),
        ),
    )[:12]
    behavior_partial_progress_row_count = len(behavior_partial_progress_rows)
    behavior_partial_progress_case_pass_total = sum(
        int(row.get("case_pass_count") or 0) for row in behavior_partial_progress_rows
    )
    behavior_partial_progress_compared_case_total = sum(
        int(row.get("compared_case_count") or 0) for row in behavior_partial_progress_rows
    )
    behavior_partial_progress_case_pass_rates = [
        float(row.get("case_pass_rate") or 0.0) for row in behavior_partial_progress_rows
    ]
    behavior_partial_progress_rows = sorted(
        behavior_partial_progress_rows,
        key=lambda row: (
            -float(row.get("case_pass_count") or 0),
            float(row.get("semantic_score_percent") or 0.0),
            str(row.get("function_name") or ""),
        ),
    )[:20]
    outcome_matrix_top = {
        key: {
            "row_count": int(outcome_matrix_counts.get(key, 0)),
            "lost_score_sum": round(float(outcome_matrix_lost_score.get(key, 0.0) or 0.0), 6),
            "top_rows": sorted(
                outcome_matrix_rows.get(key) or [],
                key=lambda row: (
                    float(row.get("semantic_score_percent") or 0.0),
                    str(row.get("function_name") or ""),
                ),
            )[:5],
        }
        for key, _count in sorted(
            outcome_matrix_counts.items(),
            key=lambda item: (
                float(outcome_matrix_lost_score.get(item[0], 0.0) or 0.0),
                item[1],
                item[0],
            ),
            reverse=True,
        )[:20]
    }

    roadmap_priority_export: dict[str, dict[str, Any]] = {}
    for priority in ROADMAP_PRIORITY_ORDER:
        if priority in roadmap_priority_metrics:
            roadmap_priority_export[priority] = metric_bucket_export(roadmap_priority_metrics[priority], total)
        else:
            roadmap_priority_export[priority] = metric_bucket_export({}, total)

    def top_component_gap_rows(component_rows: list[dict[str, Any]], limit: int = 12) -> list[dict[str, Any]]:
        return sorted(
            component_rows,
            key=lambda row: (
                -float(row.get("component_missing_feature_total") or 0.0),
                -float(row.get("component_extra_feature_total") or 0.0),
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:limit]

    type_data_gap_metrics = {
        "signature_gap_row_count": len(signature_gap_rows),
        "memory_gap_row_count": len(memory_gap_rows),
        "call_gap_row_count": len(call_gap_rows),
        "signature_gap_rows": top_component_gap_rows(signature_gap_rows),
        "memory_gap_rows": top_component_gap_rows(memory_gap_rows),
        "call_gap_rows": top_component_gap_rows(call_gap_rows),
    }
    signedness_only_signature_gap_metrics = {
        "row_count": len(signedness_only_signature_gap_rows),
        "total_pair_count": round(
            float(signedness_only_signature_gap_totals.get("param_pair_count", 0.0))
            + float(signedness_only_signature_gap_totals.get("return_pair_count", 0.0)),
            6,
        ),
        "param_pair_count": round(float(signedness_only_signature_gap_totals.get("param_pair_count", 0.0)), 6),
        "return_pair_count": round(float(signedness_only_signature_gap_totals.get("return_pair_count", 0.0)), 6),
        "source_int_param_decomp_uint_count": round(
            float(signedness_only_signature_gap_totals.get("source_int_param_decomp_uint_count", 0.0)),
            6,
        ),
        "source_uint_param_decomp_int_count": round(
            float(signedness_only_signature_gap_totals.get("source_uint_param_decomp_int_count", 0.0)),
            6,
        ),
        "source_int_return_decomp_uint_count": round(
            float(signedness_only_signature_gap_totals.get("source_int_return_decomp_uint_count", 0.0)),
            6,
        ),
        "source_uint_return_decomp_int_count": round(
            float(signedness_only_signature_gap_totals.get("source_uint_return_decomp_int_count", 0.0)),
            6,
        ),
        "top_rows": sorted(
            signedness_only_signature_gap_rows,
            key=lambda row: (
                -float(row.get("param_pair_count", 0.0) or 0.0),
                -float(row.get("return_pair_count", 0.0) or 0.0),
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:12],
    }
    signature_return_pair_total = sum(signature_return_pair_counts.values())
    signature_param_pair_total = sum(signature_param_pair_counts.values())
    signature_return_mismatch_count = sum(
        count
        for pair, count in signature_return_pair_counts.items()
        if pair.split("->", 1)[0] != pair.split("->", 1)[1]
    )
    signature_param_mismatch_count = sum(
        count
        for pair, count in signature_param_pair_counts.items()
        if pair.split("->", 1)[0] != pair.split("->", 1)[1]
    )
    signature_kind_confusion_metrics = {
        "return_pair_count": signature_return_pair_total,
        "return_mismatch_count": signature_return_mismatch_count,
        "return_match_rate": round(
            (signature_return_pair_total - signature_return_mismatch_count) / signature_return_pair_total,
            6,
        )
        if signature_return_pair_total
        else 0.0,
        "param_pair_count": signature_param_pair_total,
        "param_mismatch_count": signature_param_mismatch_count,
        "param_match_rate": round(
            (signature_param_pair_total - signature_param_mismatch_count) / signature_param_pair_total,
            6,
        )
        if signature_param_pair_total
        else 0.0,
        "param_arity_mismatch_row_count": len(signature_param_arity_mismatch_rows),
        "return_pair_counts": dict(sorted(signature_return_pair_counts.items())),
        "param_pair_counts": dict(sorted(signature_param_pair_counts.items())),
        "top_signature_pair_gap_rows": sorted(
            signature_pair_gap_rows,
            key=lambda row: (
                -int(bool(row.get("return_mismatch"))),
                -int(row.get("param_mismatch_count") or 0),
                -int(row.get("missing_param_count") or 0),
                -int(row.get("extra_param_count") or 0),
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:12],
        "top_param_arity_mismatch_rows": sorted(
            signature_param_arity_mismatch_rows,
            key=lambda row: (
                -int(row.get("missing_param_count") or 0),
                -int(row.get("extra_param_count") or 0),
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:12],
    }
    structuring_gap_metrics = {
        "control_flow_gap_row_count": len(control_flow_gap_rows),
        "hard_nonperfect_row_count": len(hard_function_rows),
        "control_flow_gap_rows": top_component_gap_rows(control_flow_gap_rows),
        "hard_nonperfect_rows": hard_function_rows[:12],
    }
    fid_name_recovery_metrics = {
        "name_or_mapping_gap_row_count": len(name_recovery_rows),
        "top_name_or_mapping_gap_rows": sorted(
            name_recovery_rows,
            key=lambda row: (
                float(row.get("semantic_score_percent") or 0.0),
                str(row.get("function_name") or ""),
            ),
        )[:12],
    }
    architecture_support_metrics = {
        arch: metric_bucket_export(metrics, total)
        for arch, metrics in sorted(architecture_stage_metrics.items())
    }
    return {
        "manifest": manifest_name,
        "entry_count": len(entries),
        "row_count": total,
        "function_mapping_rate": round(mapped / total, 6) if total else 0.0,
        "decomp_success_rate": round(decomp_ok / total, 6) if total else 0.0,
        "candidate_compile_rate": round(compile_ok / total, 6) if total else 0.0,
        "behavior_pass_rate": round(behavior_pass / total, 6) if total else 0.0,
        "effective_coverage": {
            "mapped_rows": mapped,
            "mapped_rate": round(mapped / total, 6) if total else 0.0,
            "decompiled_rows": decomp_ok,
            "decompiled_rate": round(decomp_ok / total, 6) if total else 0.0,
            "behavior_expected_rows": behavior_expected,
            "behavior_expected_rate": round(behavior_expected / total, 6) if total else 0.0,
            "behavior_executed_rows": behavior_executed,
            "behavior_executed_rate": round(behavior_executed / total, 6) if total else 0.0,
        },
        "behavior_eligibility": {
            "eligible_rows": behavior_expected,
            "eligible_rate": round(behavior_expected / total, 6) if total else 0.0,
            "executed_rows": behavior_executed,
            "execution_rate": round(behavior_executed / behavior_expected, 6) if behavior_expected else 0.0,
            "pass_rate_eligible_denominator": round(behavior_pass / behavior_expected, 6) if behavior_expected else 0.0,
            "pass_rate_total_denominator": round(behavior_pass / total, 6) if total else 0.0,
        },
        "weighted_semantic_similarity": weighted_semantic_similarity,
        "weighted_semantic_similarity_percent": percent(weighted_semantic_similarity),
        "scoring_contract": {
            "semantic_score_denominator": "all manifest rows",
            "semantic_score_formula": "0.65 * behavior_score + 0.35 * static_multiset_jaccard",
            "behavior_score_policy": "pass=1.0; mismatch, unsupported, decomp failure, compile/run failure, and missing output=0.0",
            "static_similarity_policy": "multiset Jaccard over source and decompiler feature union; missing and extra features are included in the denominator",
            "unmapped_or_failed_row_policy": "row remains in denominator with zero semantic score unless another component earns credit",
        },
        "semantic_score_stats": {
            **numeric_distribution(score_values),
            "nonzero_count": nonzero_score_count,
            "nonzero_rate": round(
                nonzero_score_count / total,
                6,
            ) if total else 0.0,
        },
        "score_component_metrics": {
            "row_count": total,
            "behavior_weight": 0.65,
            "static_weight": 0.35,
            "behavior_score_sum": behavior_score_sum,
            "static_score_sum": static_score_sum,
            "behavior_component_score_sum": behavior_component_score_sum,
            "static_component_score_sum": static_component_score_sum,
            "weighted_score_sum": score_sum,
            "behavior_component_possible_score_sum": round(0.65 * total, 6),
            "static_component_possible_score_sum": round(0.35 * total, 6),
            "behavior_component_lost_score_sum": round((0.65 * total) - behavior_component_score_sum, 6),
            "static_component_lost_score_sum": round((0.35 * total) - static_component_score_sum, 6),
            "behavior_score_distribution": numeric_distribution(behavior_score_values),
            "static_score_distribution": numeric_distribution(static_score_values),
        },
        "score_denominator_metrics": {
            "score_denominator_row_count": total,
            "score_denominator_policy": "all_rows_including_unmapped_unsupported_and_failed",
            "score_sum": score_sum,
            "possible_score_sum": float(total),
            "lost_score_sum": round(float(total) - score_sum, 6),
            "zero_score_row_count": zero_score_count,
            "nonzero_score_row_count": nonzero_score_count,
            "perfect_score_row_count": perfect_score_count,
            "unmapped_row_count": max(0, total - mapped),
            "decomp_failed_or_unmapped_row_count": max(0, total - decomp_ok),
            "behavior_not_pass_row_count": max(0, total - behavior_pass),
        },
        "semantic_loss_metrics": {
            "total_lost_score": round(float(total) - score_sum, 6),
            "avg_lost_score_per_row": round((float(total) - score_sum) / total, 6) if total else 0.0,
            "lost_score_by_behavior_status": {
                key: round(float(value), 6)
                for key, value in sorted(semantic_loss_by_behavior_status.items())
            },
            "lost_score_by_stage_first_failure": {
                key: round(float(value), 6)
                for key, value in sorted(semantic_loss_by_stage_first_failure.items())
            },
            "lost_score_by_zero_credit_reason": {
                key: round(float(value), 6)
                for key, value in sorted(semantic_loss_by_zero_credit_reason.items())
            },
            "top_lost_score_rows": semantic_loss_hot_rows,
        },
        "semantic_readiness_metrics": semantic_readiness_metrics,
        "benchmark_integrity_metrics": benchmark_integrity_metrics,
        "improvement_axis_metrics": improvement_axis_export,
        "focus_area_metrics": focus_area_export,
        "roadmap_priority_metrics": {
            "priority_order": ROADMAP_PRIORITY_ORDER,
            "buckets": roadmap_priority_export,
        },
        "type_data_gap_metrics": type_data_gap_metrics,
        "signedness_only_signature_gap_metrics": signedness_only_signature_gap_metrics,
        "signature_kind_confusion_metrics": signature_kind_confusion_metrics,
        "structuring_gap_metrics": structuring_gap_metrics,
        "fid_name_recovery_metrics": fid_name_recovery_metrics,
        "architecture_support_metrics": architecture_support_metrics,
        "complexity_quality_metrics": {
            "source_feature_bucket_policy": "tiny<=5, small<=15, medium<=40, large>40 source static features",
            "by_source_feature_bucket": complexity_export,
            "hard_nonperfect_rows": hard_function_rows,
        },
        "stage_cost_correlation_metrics": {
            "decompile_wall_by_behavior_status": {
                status: numeric_distribution(values)
                for status, values in sorted(cost_values_by_behavior_status.items())
            },
            "decompile_wall_by_stage_first_failure": {
                stage: numeric_distribution(values)
                for stage, values in sorted(cost_values_by_stage_first_failure.items())
            },
            "decompile_wall_by_score_bucket": {
                bucket: numeric_distribution(values)
                for bucket, values in sorted(cost_values_by_score_bucket.items())
            },
            "score_by_decompile_cost_bucket": {
                bucket: numeric_distribution(values)
                for bucket, values in sorted(scores_by_cost_bucket.items())
            },
            "lost_score_by_decompile_cost_bucket": {
                bucket: round(float(value), 6)
                for bucket, value in sorted(lost_score_by_cost_bucket.items())
            },
        },
        "admission_gate_metrics": admission_gate_metrics,
        "quality_gate_funnel_metrics": {
            "gate_order": gate_order,
            "counts": {key: int(admission_gate_counts.get(key, 0)) for key in gate_order},
            "drop_rows_from_previous_gate": gate_drop_rows,
            "retention_rate_from_previous_gate": gate_retention_rates,
            "rates_total_denominator": {
                key: round(float(admission_gate_counts.get(key, 0)) / total, 6) if total else 0.0
                for key in gate_order
            },
        },
        "stage_transition_metrics": stage_transition_metrics,
        "sleigh_lift_health_metrics": {
            "mapped_rows": mapped,
            "debug_stage_status_rows": debug_stage_status_row_count,
            "decode_ok_rows": int(admission_gate_counts.get("decode_ok_rows", 0)),
            "raw_pcode_ok_rows": int(admission_gate_counts.get("raw_pcode_ok_rows", 0)),
            "decode_ok_rate_mapped_denominator": round(
                float(admission_gate_counts.get("decode_ok_rows", 0)) / mapped,
                6,
            ) if mapped else 0.0,
            "raw_pcode_ok_rate_mapped_denominator": round(
                float(admission_gate_counts.get("raw_pcode_ok_rows", 0)) / mapped,
                6,
            ) if mapped else 0.0,
            "template_source_totals": dict(sorted(debug_template_source_totals.items())),
            "raw_pcode_compat_import_total": raw_pcode_compat_total,
            "invalid_pcode_shape_total": invalid_pcode_shape_total,
            "sleigh_first_blocker_row_count": sleigh_blocker_row_count,
            "sleigh_first_blocker_lost_score": round(
                sum(
                    float(value)
                    for key, value in stage_first_blocker_lost_score.items()
                    if key.startswith("decode:") or key.startswith("raw_pcode:")
                ),
                6,
            ),
            "top_sleigh_blocker_rows": sleigh_blocker_rows,
        },
        "behavior_failure_diagnostics": {
            "owner_counts": dict(sorted(behavior_failure_owner_counts.items())),
            "detail_signature_counts": dict(behavior_failure_detail_counts.most_common(20)),
            "top_detail_rows": behavior_failure_detail_top_rows,
        },
        "semantic_quality_quadrant_metrics": semantic_quality_quadrant_export,
        "outcome_matrix_metrics": {
            "top_outcomes_by_lost_score": outcome_matrix_top,
            "outcome_count": len(outcome_matrix_counts),
        },
        "coverage_blind_spot_metrics": {
            "counts": dict(sorted(coverage_blind_spot_counts.items())),
            "details": coverage_blind_spot_export,
        },
        "static_gap_density_metrics": {
            "missing_density_distribution": numeric_distribution(static_missing_density_values),
            "extra_density_distribution": numeric_distribution(static_extra_density_values),
            "static_score_by_missing_gap_bucket": {
                bucket: numeric_distribution(values)
                for bucket, values in sorted(static_score_by_missing_gap_bucket.items())
            },
            "gap_bucket_rows": static_gap_density_export,
        },
        "static_gap_hot_row_metrics": static_gap_hot_row_metrics,
        "perfect_row_count": perfect_score_count,
        "supported_behavior_row_count": sum(
            1 for row in rows if row.get("behavior", {}).get("status") != "unsupported_signature"
        ),
        "mapping_status_counts": dict(sorted(mapping_status_counts.items())),
        "decomp_failure_counts": dict(sorted(decomp_failure_counts.items())),
        "behavior_status_counts": dict(sorted(behavior_status_counts.items())),
        "decomp_cache_status_counts": dict(sorted(decomp_cache_status_counts.items())),
        "behavior_cache_status_counts": dict(sorted(behavior_cache_status_counts.items())),
        "zero_credit_breakdown": dict(sorted(zero_credit_breakdown.items())),
        "score_distribution": dict(sorted(score_distribution.items())),
        "stage_first_failure_counts": dict(sorted(stage_first_failure_counts.items())),
        "static_similarity_component_averages": {
            component: round(static_component_sums[component] / total, 6) if total else 0.0
            for component in sorted(STATIC_SIMILARITY_COMPONENTS)
        },
        "static_similarity_component_average_percent": {
            component: percent(round(static_component_sums[component] / total, 6) if total else 0.0)
            for component in sorted(STATIC_SIMILARITY_COMPONENTS)
        },
        "static_similarity_gap_totals": static_gap_summary,
        "static_similarity_gap_component_totals": static_gap_component_summary,
        "static_similarity_gap_component_top_features": static_gap_component_top_summary,
        "static_component_precision_recall_metrics": static_component_precision_recall_summary,
        "behavior_case_metrics": {
            "case_count": behavior_case_total,
            "compared_case_count": behavior_compared_case_total,
            "case_pass_count": behavior_case_pass_total,
            "case_fail_count": behavior_case_fail_total,
            "case_pass_rate": round(behavior_case_pass_total / behavior_compared_case_total, 6)
            if behavior_compared_case_total
            else 0.0,
            "partial_mismatch_row_count": partial_mismatch_rows,
            "partial_progress_row_count": partial_progress_rows,
        },
        "behavior_timeout_progress_metrics": {
            "partial_timeout_row_count": len(partial_timeout_rows),
            "partial_timeout_case_pass_count": partial_timeout_case_pass_total,
            "partial_timeout_compared_case_count": partial_timeout_compared_case_total,
            "partial_timeout_case_pass_rate": round(
                partial_timeout_case_pass_total / partial_timeout_compared_case_total,
                6,
            )
            if partial_timeout_compared_case_total
            else 0.0,
            "partial_timeout_missing_candidate_line_total": partial_timeout_missing_line_total,
        },
        "behavior_partial_progress_metrics": {
            "row_count": behavior_partial_progress_row_count,
            "case_pass_count": behavior_partial_progress_case_pass_total,
            "compared_case_count": behavior_partial_progress_compared_case_total,
            "case_pass_rate_distribution": numeric_distribution(behavior_partial_progress_case_pass_rates),
            "top_rows": behavior_partial_progress_rows,
        },
        "behavior_support_metrics": {
            "case_source_counts": dict(sorted(behavior_case_source_counts.items())),
            "unsupported_reason_counts": dict(sorted(behavior_unsupported_reason_counts.items())),
            "unsupported_signature_row_count": int(behavior_status_counts.get("unsupported_signature", 0)),
            "eligible_row_count": behavior_expected,
            "executed_row_count": behavior_executed,
        },
        "behavior_denominator_metrics": {
            "row_denominator_count": total,
            "eligible_row_count": behavior_expected,
            "eligible_row_rate": round(behavior_expected / total, 6) if total else 0.0,
            "executed_row_count": behavior_executed,
            "executed_row_rate_total_denominator": round(behavior_executed / total, 6) if total else 0.0,
            "executed_row_rate_eligible_denominator": round(behavior_executed / behavior_expected, 6)
            if behavior_expected
            else 0.0,
            "pass_row_count": behavior_pass,
            "pass_row_rate_total_denominator": round(behavior_pass / total, 6) if total else 0.0,
            "pass_row_rate_eligible_denominator": round(behavior_pass / behavior_expected, 6)
            if behavior_expected
            else 0.0,
            "pass_row_rate_executed_denominator": round(behavior_pass / behavior_executed, 6)
            if behavior_executed
            else 0.0,
            "not_executed_row_count": max(0, behavior_expected - behavior_executed),
            "unsupported_or_ineligible_row_count": max(0, total - behavior_expected),
            "case_denominator_count": behavior_compared_case_total,
            "case_pass_count": behavior_case_pass_total,
            "case_fail_count": behavior_case_fail_total,
            "case_pass_rate": round(behavior_case_pass_total / behavior_compared_case_total, 6)
            if behavior_compared_case_total
            else 0.0,
        },
        "behavior_mismatch_metrics": {
            "mismatch_row_count": int(behavior_status_counts.get("mismatch", 0)),
            "first_mismatch_index_counts": dict(sorted(behavior_first_mismatch_index_counts.items())),
            "output_length_delta_counts": dict(sorted(behavior_output_length_delta_counts.items())),
            "mismatch_kind_counts": dict(sorted(behavior_mismatch_kind_counts.items())),
        },
        "behavior_distance_metrics": {
            "case_pass_rate_distribution": numeric_distribution(behavior_case_pass_rates),
            "missing_candidate_line_total": behavior_missing_candidate_line_total,
            "extra_candidate_line_total": behavior_extra_candidate_line_total,
            "output_length_delta_counts": dict(sorted(behavior_output_length_delta_counts.items())),
            "first_mismatch_index_counts": dict(sorted(behavior_first_mismatch_index_counts.items())),
        },
        "denominator_accounting_metrics": {
            "row_count": total,
            "mapped_row_count": mapped,
            "unmapped_row_count": max(0, total - mapped),
            "decompiled_row_count": decomp_ok,
            "mapped_but_not_decompiled_row_count": max(0, mapped - decomp_ok),
            "behavior_expected_row_count": behavior_expected,
            "behavior_not_expected_row_count": max(0, total - behavior_expected),
            "behavior_executed_row_count": behavior_executed,
            "behavior_expected_but_not_executed_row_count": max(0, behavior_expected - behavior_executed),
            "behavior_pass_row_count": behavior_pass,
            "behavior_nonpass_row_count": max(0, total - behavior_pass),
            "static_missing_feature_row_count": static_missing_feature_rows,
            "zero_score_row_count": int(score_distribution.get("zero", 0)),
            "nonzero_score_row_count": nonzero_score_count,
            "perfect_score_row_count": perfect_score_count,
            "semantic_score_denominator_row_count": total,
            "semantic_score_zero_fill_row_count": zero_score_count,
        },
        "score_by_behavior_status": score_by_behavior_status,
        "score_by_stage_first_failure": score_by_stage_first_failure,
        "behavior_status_by_stage_first_failure": {
            stage: dict(sorted(counts.items()))
            for stage, counts in sorted(behavior_status_by_stage_first_failure.items())
        },
        "behavior_status_by_zero_credit_reason": {
            reason: dict(sorted(counts.items()))
            for reason, counts in sorted(behavior_status_by_zero_credit_reason.items())
        },
        "static_gap_row_metrics": {
            "source_feature_row_count": source_feature_rows,
            "decomp_feature_row_count": decomp_feature_rows,
            "decomp_feature_row_rate": round(decomp_feature_rows / total, 6) if total else 0.0,
            "decomp_absent_feature_row_count": static_decomp_absent_feature_rows,
            "decomp_absent_feature_row_rate": round(static_decomp_absent_feature_rows / total, 6) if total else 0.0,
            "missing_feature_row_count": static_missing_feature_rows,
            "missing_feature_row_rate": round(static_missing_feature_rows / total, 6) if total else 0.0,
            "extra_feature_row_count": static_extra_feature_rows,
            "extra_feature_row_rate": round(static_extra_feature_rows / total, 6) if total else 0.0,
            "zero_static_intersection_row_count": static_zero_similarity_rows,
            "zero_static_intersection_row_rate": round(static_zero_similarity_rows / total, 6) if total else 0.0,
            "missing_feature_count_distribution": numeric_distribution(missing_feature_count_values),
            "extra_feature_count_distribution": numeric_distribution(extra_feature_count_values),
            "component_missing_row_counts": dict(sorted(static_component_missing_row_counts.items())),
            "component_zero_similarity_row_counts": dict(sorted(static_component_zero_similarity_row_counts.items())),
        },
        "source_feature_metrics": {
            "source_feature_total_distribution": numeric_distribution(source_feature_total_values),
            "source_feature_total_direct_distribution": numeric_distribution(source_feature_total_direct_values),
            "source_feature_total_inline_expanded_distribution": numeric_distribution(
                source_feature_total_inline_expanded_values
            ),
            "decomp_feature_total_distribution": numeric_distribution(decomp_feature_total_values),
            "intersection_feature_total_distribution": numeric_distribution(static_intersection_feature_total_values),
            "union_feature_total_distribution": numeric_distribution(static_union_feature_total_values),
            "component_source_feature_distributions": {
                component: numeric_distribution(values)
                for component, values in sorted(static_component_source_feature_values.items())
            },
            "component_decomp_feature_distributions": {
                component: numeric_distribution(values)
                for component, values in sorted(static_component_decomp_feature_values.items())
            },
        },
        "static_source_variant_metrics": {
            "variant_counts": dict(sorted(static_source_variant_counts.items())),
            "inline_expanded_static_score_delta_distribution": numeric_distribution(
                inline_expanded_static_score_deltas
            ),
            "top_inline_expanded_static_rows": inline_expanded_static_hot_rows,
        },
        "static_absence_penalty_metrics": {
            "source_feature_total": static_source_total,
            "decomp_feature_total": static_decomp_total,
            "intersection_feature_total": static_intersection_total,
            "union_feature_total": static_union_total,
            "missing_feature_total": static_missing_total,
            "extra_feature_total": static_extra_total,
            "source_recall": round(static_intersection_total / static_source_total, 6)
            if static_source_total
            else 0.0,
            "decomp_precision": round(static_intersection_total / static_decomp_total, 6)
            if static_decomp_total
            else 0.0,
            "union_jaccard": round(static_intersection_total / static_union_total, 6)
            if static_union_total
            else 1.0,
            "missing_feature_rate": round(static_missing_total / static_source_total, 6)
            if static_source_total
            else 0.0,
            "extra_feature_rate": round(static_extra_total / static_decomp_total, 6)
            if static_decomp_total
            else 0.0,
            "rows_with_source_features": source_feature_rows,
            "rows_with_decomp_features": decomp_feature_rows,
            "rows_with_no_decomp_features_despite_source": static_decomp_absent_feature_rows,
            "rows_with_missing_features": static_missing_feature_rows,
            "rows_with_zero_static_intersection": static_zero_similarity_rows,
        },
        "static_component_absence_matrix_metrics": static_component_absence_export,
        "source_decomp_size_metrics": {
            "source_body_line_count_distribution": numeric_distribution(source_body_line_counts),
            "decomp_line_count_distribution": numeric_distribution(decomp_line_counts),
            "source_body_byte_count_distribution": numeric_distribution(source_body_byte_counts),
            "decomp_byte_count_distribution": numeric_distribution(decomp_byte_counts),
            "decomp_to_source_line_ratio_distribution": numeric_distribution(decomp_to_source_line_ratios),
            "decomp_to_source_byte_ratio_distribution": numeric_distribution(decomp_to_source_byte_ratios),
            "top_decomp_to_source_line_ratio_rows": source_decomp_size_hot_rows,
        },
        "harness_cost_metrics": {
            "decompile_total_sec": round(sum(decomp_times), 6),
            "decompile_avg_sec": round(sum(decomp_times) / len(decomp_times), 6) if decomp_times else 0.0,
            "decompile_p50_sec": numeric_distribution(decomp_times)["p50"],
            "decompile_p90_sec": numeric_distribution(decomp_times)["p90"],
            "decompile_p95_sec": numeric_distribution(decomp_times)["p95"],
            "decompile_max_sec": numeric_distribution(decomp_times)["max"],
            "behavior_compile_total_sec": round(sum(behavior_compile_times), 6),
            "behavior_compile_avg_sec": round(sum(behavior_compile_times) / len(behavior_compile_times), 6)
            if behavior_compile_times
            else 0.0,
            "behavior_compile_p50_sec": numeric_distribution(behavior_compile_times)["p50"],
            "behavior_compile_p90_sec": numeric_distribution(behavior_compile_times)["p90"],
            "behavior_compile_p95_sec": numeric_distribution(behavior_compile_times)["p95"],
            "behavior_compile_max_sec": numeric_distribution(behavior_compile_times)["max"],
            "behavior_run_total_sec": round(sum(behavior_run_times), 6),
            "behavior_run_avg_sec": round(sum(behavior_run_times) / len(behavior_run_times), 6)
            if behavior_run_times
            else 0.0,
            "behavior_run_p50_sec": numeric_distribution(behavior_run_times)["p50"],
            "behavior_run_p90_sec": numeric_distribution(behavior_run_times)["p90"],
            "behavior_run_p95_sec": numeric_distribution(behavior_run_times)["p95"],
            "behavior_run_max_sec": numeric_distribution(behavior_run_times)["max"],
            "behavior_wall_total_sec": round(sum(behavior_wall_times), 6),
            "behavior_wall_avg_sec": round(sum(behavior_wall_times) / len(behavior_wall_times), 6)
            if behavior_wall_times
            else 0.0,
            "behavior_wall_p50_sec": numeric_distribution(behavior_wall_times)["p50"],
            "behavior_wall_p90_sec": numeric_distribution(behavior_wall_times)["p90"],
            "behavior_wall_p95_sec": numeric_distribution(behavior_wall_times)["p95"],
            "behavior_wall_max_sec": numeric_distribution(behavior_wall_times)["max"],
        },
        "cost_hot_rows": {
            "top_decompile_wall_rows": cost_hot_rows_by_decompile,
            "top_behavior_wall_rows": cost_hot_rows_by_behavior_wall,
        },
        "debug_coverage_metrics": {
            "debug_decomp_rows": debug_decomp_row_count,
            "debug_decomp_rate_mapped_denominator": round(debug_decomp_row_count / mapped_debug_denominator, 6)
            if mapped
            else 0.0,
            "debug_stage_status_rows": debug_stage_status_row_count,
            "debug_stage_status_rate_mapped_denominator": round(
                debug_stage_status_row_count / mapped_debug_denominator,
                6,
            ) if mapped else 0.0,
        },
        "pipeline_stage_metrics": pipeline_stage_metrics,
        "debug_pipeline_numeric_metrics": {
            key: numeric_distribution(values)
            for key, values in sorted(debug_pipeline_numeric_values.items())
        },
        "nir_build_stats_metrics": {
            "stats_row_count": nir_build_stats_row_count,
            "stats_row_rate_mapped_denominator": round(nir_build_stats_row_count / mapped_debug_denominator, 6)
            if mapped
            else 0.0,
            "numeric_totals": dict(sorted(nir_build_stats_numeric_totals.items())),
            "nonzero_row_counts": dict(sorted(nir_build_stats_nonzero_rows.items())),
            "debt_metric_totals": nir_debt_totals,
            "debt_metric_distributions": nir_build_stats_distributions,
            "top_debt_rows": nir_build_stats_debt_hot_rows,
        },
        "nir_debt_correlation_metrics": {
            "stats_row_count": nir_build_stats_row_count,
            "debt_row_count": nir_debt_row_count,
            "debt_row_rate_stats_denominator": round(nir_debt_row_count / nir_build_stats_row_count, 6)
            if nir_build_stats_row_count
            else 0.0,
            "score_distribution_debt_rows": numeric_distribution(nir_debt_score_values),
            "score_distribution_no_debt_rows": numeric_distribution(nir_no_debt_score_values),
            "behavior_status_counts_debt_rows": dict(sorted(nir_debt_behavior_status_counts.items())),
            "stage_first_failure_counts_debt_rows": dict(sorted(nir_debt_stage_first_failure_counts.items())),
        },
        "debug_owner_bucket_counts": dict(sorted(debug_owner_bucket_counts.items())),
        "debug_stage_status_counts": dict(sorted(debug_stage_status_counts.items())),
        "debug_stage_status_matrix": debug_stage_status_matrix_export,
        "debug_quality_evidence_totals": dict(sorted(debug_quality_evidence_totals.items())),
        "debug_quality_evidence_nonzero_rows": dict(sorted(debug_quality_evidence_nonzero_rows.items())),
        "debug_template_source_totals": dict(sorted(debug_template_source_totals.items())),
        "triage_priority_rows": triage_priority_rows,
        "host_execution_unavailable_count": sum(host_statuses.values()),
        "host_execution_unavailable_reasons": dict(host_statuses),
        "by_language": by_language,
        "by_arch": by_arch,
        "by_source_return_kind": by_source_return_kind,
        "by_source_param_shape": by_source_param_shape,
        "by_tag": by_tag,
        "by_entry": by_entry,
    }


def canonical_sleigh_template_source(source: str) -> str:
    if source in {"spec_derived", "SpecDerived"}:
        return "sla_construct_tpl"
    return source


def row_key(row: dict[str, Any]) -> str:
    return "::".join(
        [
            str(row.get("entry_id") or ""),
            str(row.get("source_path") or ""),
            str(row.get("function_name") or ""),
        ]
    )


def sleigh_template_source_gate(summary: dict[str, Any], required_source: str) -> dict[str, Any]:
    raw_template_totals = summary.get("debug_template_source_totals")
    if not isinstance(raw_template_totals, dict):
        raw_template_totals = {}
    template_totals: dict[str, int] = {}
    for source, value in raw_template_totals.items():
        if isinstance(value, int | float):
            canonical = canonical_sleigh_template_source(str(source))
            template_totals[canonical] = template_totals.get(canonical, 0) + int(value)
    stage_counts = summary.get("debug_stage_status_counts")
    if not isinstance(stage_counts, dict):
        stage_counts = {}
    quality_totals = summary.get("debug_quality_evidence_totals")
    if not isinstance(quality_totals, dict):
        quality_totals = {}
    sleigh_health = summary.get("sleigh_lift_health_metrics")
    if not isinstance(sleigh_health, dict):
        sleigh_health = {}
    nir_stats = summary.get("nir_build_stats_metrics")
    if not isinstance(nir_stats, dict):
        nir_stats = {}
    nir_numeric_totals = nir_stats.get("numeric_totals")
    if not isinstance(nir_numeric_totals, dict):
        nir_numeric_totals = {}

    failures: list[str] = []
    row_count = int(summary.get("row_count", 0) or 0)
    mapping_counts = summary.get("mapping_status_counts")
    if not isinstance(mapping_counts, dict):
        mapping_counts = {}
    mapped_row_count = int(mapping_counts.get("matched", row_count) or 0)
    unmapped_row_count = max(0, row_count - mapped_row_count)
    decode_ok = int(stage_counts.get("decode:ok", 0) or 0)
    raw_pcode_ok = int(stage_counts.get("raw_pcode:ok", 0) or 0)
    invalid_pcode_shape_count = int(quality_totals.get("invalid_pcode_shape_count", 0) or 0)
    raw_pcode_compat_import_count = int(
        sleigh_health.get(
            "raw_pcode_compat_import_total",
            nir_numeric_totals.get("raw_pcode_compat_import_count", 0),
        )
        or 0
    )
    total_templates = sum(
        int(value) for value in template_totals.values() if isinstance(value, int | float)
    )
    failed_sleigh_stages = {
        stage: int(value)
        for stage, value in stage_counts.items()
        if (
            stage.startswith("decode:")
            or stage.startswith("raw_pcode:")
        )
        and stage not in {"decode:ok", "raw_pcode:ok"}
        and isinstance(value, int | float)
        and int(value) != 0
    }
    unexpected_sources = {
        source: int(value)
        for source, value in template_totals.items()
        if source != required_source and isinstance(value, int | float) and int(value) != 0
    }

    if mapped_row_count > 0 and total_templates == 0:
        failures.append(
            "SLEIGH template source gate requires debug_template_source_totals; run with --include-debug-decomp"
        )
    if raw_pcode_ok > 0 and total_templates < raw_pcode_ok:
        failures.append(
            f"SLEIGH template source evidence must cover every raw_pcode:ok row ({total_templates}/{raw_pcode_ok})"
        )
    if mapped_row_count > 0 and decode_ok != mapped_row_count:
        failures.append(
            f"SLEIGH decode must be ok for every mapped row ({decode_ok}/{mapped_row_count})"
        )
    if mapped_row_count > 0 and raw_pcode_ok != mapped_row_count:
        failures.append(
            f"SLEIGH raw_pcode must be ok for every mapped row ({raw_pcode_ok}/{mapped_row_count})"
        )
    if unexpected_sources:
        failures.append(
            f"SLEIGH template sources must be only {required_source!r} "
            f"(unexpected {unexpected_sources})"
        )
    if failed_sleigh_stages:
        failures.append(f"SLEIGH decode/raw_pcode stages must be ok (got {failed_sleigh_stages})")
    if invalid_pcode_shape_count != 0:
        failures.append(f"SLEIGH invalid_pcode_shape_count must be 0 (got {invalid_pcode_shape_count})")
    if raw_pcode_compat_import_count != 0:
        failures.append(
            "SLEIGH raw_pcode_compat_import_count must be 0 "
            f"(got {raw_pcode_compat_import_count})"
        )

    return {
        "required_source": required_source,
        "status": "passed" if not failures else "failed",
        "failures": failures,
        "template_source_totals": dict(sorted(template_totals.items())),
        "template_source_count": total_templates,
        "row_count": row_count,
        "mapped_row_count": mapped_row_count,
        "unmapped_row_count": unmapped_row_count,
        "decode_ok_rows": decode_ok,
        "raw_pcode_ok_rows": raw_pcode_ok,
        "invalid_pcode_shape_count": invalid_pcode_shape_count,
        "raw_pcode_compat_import_count": raw_pcode_compat_import_count,
    }


def load_baseline_artifacts(path: Path) -> tuple[dict[str, Any], list[dict[str, Any]], Path]:
    summary_path = path
    if path.is_dir():
        summary_path = path / "source_semantic_summary.json"
    summary = load_json(summary_path)
    rows_path = summary_path.parent / "source_semantic_rows.json"
    rows = load_json(rows_path) if rows_path.exists() else []
    if not isinstance(rows, list):
        rows = []
    return summary, rows, summary_path


def find_latest_baseline_dir(
    output_dir: Path,
    manifest_name: str,
    current_row_keys: set[str],
) -> Path | None:
    root = DEFAULT_ARTIFACT_ROOT
    if not root.exists():
        return None
    output_resolved = output_dir.resolve()
    candidates: list[tuple[float, Path]] = []
    for summary_path in root.rglob("source_semantic_summary.json"):
        try:
            parent_resolved = summary_path.parent.resolve()
        except OSError:
            continue
        if parent_resolved == output_resolved:
            continue
        try:
            summary = load_json(summary_path)
        except Exception:
            continue
        if summary.get("manifest") != manifest_name:
            continue
        rows_path = summary_path.parent / "source_semantic_rows.json"
        baseline_keys: set[str] = set()
        if rows_path.exists():
            try:
                raw_rows = load_json(rows_path)
                if isinstance(raw_rows, list):
                    baseline_keys = {row_key(row) for row in raw_rows if isinstance(row, dict)}
            except Exception:
                baseline_keys = set()
        try:
            mtime = summary_path.stat().st_mtime
        except OSError:
            continue
        if current_row_keys and baseline_keys != current_row_keys:
            continue
        if not current_row_keys and summary.get("row_count") != 0:
            continue
        candidates.append((mtime, summary_path.parent))
    if not candidates:
        return None
    return max(candidates, key=lambda item: item[0])[1]


def metric_delta(current: dict[str, Any], baseline: dict[str, Any], key: str) -> dict[str, Any]:
    current_value = current.get(key)
    baseline_value = baseline.get(key)
    if key.endswith("_percent"):
        raw_key = key.removesuffix("_percent")
        if not isinstance(current_value, (int, float)) and isinstance(current.get(raw_key), (int, float)):
            current_value = percent(float(current[raw_key]))
        if not isinstance(baseline_value, (int, float)) and isinstance(baseline.get(raw_key), (int, float)):
            baseline_value = percent(float(baseline[raw_key]))
    if not isinstance(current_value, (int, float)) or not isinstance(baseline_value, (int, float)):
        return {"current": current_value, "baseline": baseline_value, "delta": None}
    return {
        "current": current_value,
        "baseline": baseline_value,
        "delta": round(float(current_value) - float(baseline_value), 6),
    }


def compare_to_baseline(
    summary: dict[str, Any],
    rows: list[dict[str, Any]],
    baseline_summary: dict[str, Any],
    baseline_rows: list[dict[str, Any]],
    baseline_path: Path,
) -> dict[str, Any]:
    current_by_key = {row_key(row): row for row in rows}
    baseline_by_key = {row_key(row): row for row in baseline_rows}
    shared_keys = sorted(set(current_by_key) & set(baseline_by_key))
    new_keys = sorted(set(current_by_key) - set(baseline_by_key))
    missing_keys = sorted(set(baseline_by_key) - set(current_by_key))

    row_deltas: list[dict[str, Any]] = []
    improved = 0
    regressed = 0
    unchanged = 0
    behavior_improved = 0
    behavior_regressed = 0
    score_delta_sum_negative = 0.0
    score_delta_sum_positive = 0.0
    new_zero_score_rows = 0
    new_unmapped_rows = 0
    new_behavior_fail_rows = 0
    for key in shared_keys:
        current = current_by_key[key]
        baseline = baseline_by_key[key]
        current_score = float(current.get("semantic_score", 0.0) or 0.0)
        baseline_score = float(baseline.get("semantic_score", 0.0) or 0.0)
        delta = round(current_score - baseline_score, 6)
        if delta > 0:
            improved += 1
            score_delta_sum_positive += delta
        elif delta < 0:
            regressed += 1
            score_delta_sum_negative += delta
        else:
            unchanged += 1

        current_behavior = current.get("behavior", {}).get("status")
        baseline_behavior = baseline.get("behavior", {}).get("status")
        if current_behavior == "pass" and baseline_behavior != "pass":
            behavior_improved += 1
        elif current_behavior != "pass" and baseline_behavior == "pass":
            behavior_regressed += 1
            new_behavior_fail_rows += 1
        if current_score == 0.0 and baseline_score > 0.0:
            new_zero_score_rows += 1
        if current.get("mapping_status") != "matched" and baseline.get("mapping_status") == "matched":
            new_unmapped_rows += 1

        if delta != 0 or current_behavior != baseline_behavior:
            row_deltas.append(
                {
                    "key": key,
                    "entry_id": current.get("entry_id"),
                    "function_name": current.get("function_name"),
                    "current_score": current_score,
                    "baseline_score": baseline_score,
                    "delta": delta,
                    "current_score_percent": percent(current_score),
                    "baseline_score_percent": percent(baseline_score),
                    "delta_percent": percent(delta),
                    "current_behavior": current_behavior,
                    "baseline_behavior": baseline_behavior,
                    "current_mapping_status": current.get("mapping_status"),
                    "baseline_mapping_status": baseline.get("mapping_status"),
                    "current_decomp_failure_kind": current.get("decomp_failure_kind"),
                    "baseline_decomp_failure_kind": baseline.get("decomp_failure_kind"),
                }
            )

    row_deltas.sort(key=lambda row: (abs(float(row["delta"])), row["function_name"] or ""), reverse=True)
    top_improvements = sorted(
        (row for row in row_deltas if float(row.get("delta", 0.0) or 0.0) > 0.0),
        key=lambda row: (float(row["delta"]), row["function_name"] or ""),
        reverse=True,
    )[:10]
    top_regressions = sorted(
        (row for row in row_deltas if float(row.get("delta", 0.0) or 0.0) < 0.0),
        key=lambda row: (float(row["delta"]), row["function_name"] or ""),
    )[:10]
    metric_keys = [
        "weighted_semantic_similarity",
        "weighted_semantic_similarity_percent",
        "semantic_score_nonzero_rate",
        "function_mapping_rate",
        "decomp_success_rate",
        "candidate_compile_rate",
        "behavior_pass_rate",
        "behavior_pass_rate_total_denominator",
        "behavior_case_pass_rate",
        "behavior_pass_row_rate_executed_denominator",
        "behavior_mismatch_row_count",
        "behavior_expected_but_not_executed_row_count",
        "behavior_expected_rate",
        "behavior_executed_rate",
        "static_source_recall",
        "static_decomp_precision",
        "static_union_jaccard",
        "static_missing_feature_rate",
        "static_missing_feature_row_rate",
        "static_decomp_absent_feature_row_rate",
        "zero_static_intersection_row_rate",
        "fully_perfect_rate",
        "behavior_pass_static_perfect_rate",
        "behavior_pass_static_gap_rate",
        "static_perfect_behavior_nonpass_rate",
        "pipeline_ok_behavior_nonpass_rate",
        "rows_excluded_from_semantic_score_denominator",
        "source_extracted_function_count",
        "source_selected_function_count",
        "source_suppressed_static_inline_helper_count",
        "source_suppressed_static_inline_helper_rate",
        "lost_score_sum",
        "perfect_row_count",
        "supported_behavior_row_count",
        "row_count",
    ]
    effective = summary.get("effective_coverage") if isinstance(summary.get("effective_coverage"), dict) else {}
    behavior_eligibility = (
        summary.get("behavior_eligibility") if isinstance(summary.get("behavior_eligibility"), dict) else {}
    )
    baseline_effective = (
        baseline_summary.get("effective_coverage")
        if isinstance(baseline_summary.get("effective_coverage"), dict)
        else {}
    )
    baseline_behavior_eligibility = (
        baseline_summary.get("behavior_eligibility")
        if isinstance(baseline_summary.get("behavior_eligibility"), dict)
        else {}
    )
    metric_source = dict(summary)
    semantic_stats = summary.get("semantic_score_stats") if isinstance(summary.get("semantic_score_stats"), dict) else {}
    behavior_cases = summary.get("behavior_case_metrics") if isinstance(summary.get("behavior_case_metrics"), dict) else {}
    behavior_mismatches = (
        summary.get("behavior_mismatch_metrics")
        if isinstance(summary.get("behavior_mismatch_metrics"), dict)
        else {}
    )
    static_gaps = (
        summary.get("static_similarity_gap_totals")
        if isinstance(summary.get("static_similarity_gap_totals"), dict)
        else {}
    )
    static_gap_rows = (
        summary.get("static_gap_row_metrics")
        if isinstance(summary.get("static_gap_row_metrics"), dict)
        else {}
    )
    denominator_accounting = (
        summary.get("denominator_accounting_metrics")
        if isinstance(summary.get("denominator_accounting_metrics"), dict)
        else {}
    )
    behavior_denominators = (
        summary.get("behavior_denominator_metrics")
        if isinstance(summary.get("behavior_denominator_metrics"), dict)
        else {}
    )
    static_absence = (
        summary.get("static_absence_penalty_metrics")
        if isinstance(summary.get("static_absence_penalty_metrics"), dict)
        else {}
    )
    score_denominators = (
        summary.get("score_denominator_metrics")
        if isinstance(summary.get("score_denominator_metrics"), dict)
        else {}
    )
    readiness_metrics = (
        summary.get("semantic_readiness_metrics")
        if isinstance(summary.get("semantic_readiness_metrics"), dict)
        else {}
    )
    integrity_metrics = (
        summary.get("benchmark_integrity_metrics")
        if isinstance(summary.get("benchmark_integrity_metrics"), dict)
        else {}
    )
    source_row_selection = (
        summary.get("source_row_selection_metrics")
        if isinstance(summary.get("source_row_selection_metrics"), dict)
        else {}
    )
    metric_source.update(
        {
            "semantic_score_nonzero_rate": semantic_stats.get("nonzero_rate"),
            "behavior_pass_rate_total_denominator": behavior_eligibility.get("pass_rate_total_denominator"),
            "behavior_case_pass_rate": behavior_cases.get("case_pass_rate"),
            "behavior_pass_row_rate_executed_denominator": behavior_denominators.get(
                "pass_row_rate_executed_denominator"
            ),
            "behavior_mismatch_row_count": behavior_mismatches.get("mismatch_row_count"),
            "behavior_expected_but_not_executed_row_count": denominator_accounting.get(
                "behavior_expected_but_not_executed_row_count"
            ),
            "behavior_expected_rate": effective.get("behavior_expected_rate"),
            "behavior_executed_rate": effective.get("behavior_executed_rate"),
            "static_source_recall": static_absence.get("source_recall"),
            "static_decomp_precision": static_absence.get("decomp_precision"),
            "static_union_jaccard": static_absence.get("union_jaccard"),
            "static_missing_feature_rate": static_gaps.get("missing_feature_rate"),
            "static_missing_feature_row_rate": static_gap_rows.get("missing_feature_row_rate"),
            "static_decomp_absent_feature_row_rate": static_gap_rows.get("decomp_absent_feature_row_rate"),
            "zero_static_intersection_row_rate": static_gap_rows.get("zero_static_intersection_row_rate"),
            "fully_perfect_rate": readiness_metrics.get("fully_perfect_rate"),
            "behavior_pass_static_perfect_rate": readiness_metrics.get("behavior_pass_static_perfect_rate"),
            "behavior_pass_static_gap_rate": readiness_metrics.get("behavior_pass_static_gap_rate"),
            "static_perfect_behavior_nonpass_rate": readiness_metrics.get("static_perfect_behavior_nonpass_rate"),
            "pipeline_ok_behavior_nonpass_rate": readiness_metrics.get("pipeline_ok_behavior_nonpass_rate"),
            "rows_excluded_from_semantic_score_denominator": integrity_metrics.get(
                "rows_excluded_from_semantic_score_denominator"
            ),
            "source_extracted_function_count": source_row_selection.get("extracted_source_function_count"),
            "source_selected_function_count": source_row_selection.get("selected_source_function_count"),
            "source_suppressed_static_inline_helper_count": source_row_selection.get(
                "suppressed_static_inline_helper_count"
            ),
            "source_suppressed_static_inline_helper_rate": source_row_selection.get(
                "suppressed_static_inline_helper_rate_filtered_denominator"
            ),
            "lost_score_sum": score_denominators.get("lost_score_sum"),
        }
    )
    baseline_metric_source = dict(baseline_summary)
    baseline_semantic_stats = (
        baseline_summary.get("semantic_score_stats")
        if isinstance(baseline_summary.get("semantic_score_stats"), dict)
        else {}
    )
    baseline_behavior_cases = (
        baseline_summary.get("behavior_case_metrics")
        if isinstance(baseline_summary.get("behavior_case_metrics"), dict)
        else {}
    )
    baseline_behavior_mismatches = (
        baseline_summary.get("behavior_mismatch_metrics")
        if isinstance(baseline_summary.get("behavior_mismatch_metrics"), dict)
        else {}
    )
    baseline_static_gaps = (
        baseline_summary.get("static_similarity_gap_totals")
        if isinstance(baseline_summary.get("static_similarity_gap_totals"), dict)
        else {}
    )
    baseline_static_gap_rows = (
        baseline_summary.get("static_gap_row_metrics")
        if isinstance(baseline_summary.get("static_gap_row_metrics"), dict)
        else {}
    )
    baseline_denominator_accounting = (
        baseline_summary.get("denominator_accounting_metrics")
        if isinstance(baseline_summary.get("denominator_accounting_metrics"), dict)
        else {}
    )
    baseline_behavior_denominators = (
        baseline_summary.get("behavior_denominator_metrics")
        if isinstance(baseline_summary.get("behavior_denominator_metrics"), dict)
        else {}
    )
    baseline_static_absence = (
        baseline_summary.get("static_absence_penalty_metrics")
        if isinstance(baseline_summary.get("static_absence_penalty_metrics"), dict)
        else {}
    )
    baseline_score_denominators = (
        baseline_summary.get("score_denominator_metrics")
        if isinstance(baseline_summary.get("score_denominator_metrics"), dict)
        else {}
    )
    baseline_readiness_metrics = (
        baseline_summary.get("semantic_readiness_metrics")
        if isinstance(baseline_summary.get("semantic_readiness_metrics"), dict)
        else {}
    )
    baseline_integrity_metrics = (
        baseline_summary.get("benchmark_integrity_metrics")
        if isinstance(baseline_summary.get("benchmark_integrity_metrics"), dict)
        else {}
    )
    baseline_source_row_selection = (
        baseline_summary.get("source_row_selection_metrics")
        if isinstance(baseline_summary.get("source_row_selection_metrics"), dict)
        else {}
    )
    baseline_metric_source.update(
        {
            "semantic_score_nonzero_rate": baseline_semantic_stats.get("nonzero_rate"),
            "behavior_pass_rate_total_denominator": baseline_behavior_eligibility.get("pass_rate_total_denominator"),
            "behavior_case_pass_rate": baseline_behavior_cases.get("case_pass_rate"),
            "behavior_pass_row_rate_executed_denominator": baseline_behavior_denominators.get(
                "pass_row_rate_executed_denominator"
            ),
            "behavior_mismatch_row_count": baseline_behavior_mismatches.get("mismatch_row_count"),
            "behavior_expected_but_not_executed_row_count": baseline_denominator_accounting.get(
                "behavior_expected_but_not_executed_row_count"
            ),
            "behavior_expected_rate": baseline_effective.get("behavior_expected_rate"),
            "behavior_executed_rate": baseline_effective.get("behavior_executed_rate"),
            "static_source_recall": baseline_static_absence.get("source_recall"),
            "static_decomp_precision": baseline_static_absence.get("decomp_precision"),
            "static_union_jaccard": baseline_static_absence.get("union_jaccard"),
            "static_missing_feature_rate": baseline_static_gaps.get("missing_feature_rate"),
            "static_missing_feature_row_rate": baseline_static_gap_rows.get("missing_feature_row_rate"),
            "static_decomp_absent_feature_row_rate": baseline_static_gap_rows.get("decomp_absent_feature_row_rate"),
            "zero_static_intersection_row_rate": baseline_static_gap_rows.get("zero_static_intersection_row_rate"),
            "fully_perfect_rate": baseline_readiness_metrics.get("fully_perfect_rate"),
            "behavior_pass_static_perfect_rate": baseline_readiness_metrics.get("behavior_pass_static_perfect_rate"),
            "behavior_pass_static_gap_rate": baseline_readiness_metrics.get("behavior_pass_static_gap_rate"),
            "static_perfect_behavior_nonpass_rate": baseline_readiness_metrics.get(
                "static_perfect_behavior_nonpass_rate"
            ),
            "pipeline_ok_behavior_nonpass_rate": baseline_readiness_metrics.get("pipeline_ok_behavior_nonpass_rate"),
            "rows_excluded_from_semantic_score_denominator": baseline_integrity_metrics.get(
                "rows_excluded_from_semantic_score_denominator"
            ),
            "source_extracted_function_count": baseline_source_row_selection.get("extracted_source_function_count"),
            "source_selected_function_count": baseline_source_row_selection.get("selected_source_function_count"),
            "source_suppressed_static_inline_helper_count": baseline_source_row_selection.get(
                "suppressed_static_inline_helper_count"
            ),
            "source_suppressed_static_inline_helper_rate": baseline_source_row_selection.get(
                "suppressed_static_inline_helper_rate_filtered_denominator"
            ),
            "lost_score_sum": baseline_score_denominators.get("lost_score_sum"),
        }
    )
    return {
        "baseline_summary_path": rel(baseline_path),
        "shared_row_count": len(shared_keys),
        "new_row_count": len(new_keys),
        "missing_row_count": len(missing_keys),
        "improved_row_count": improved,
        "regressed_row_count": regressed,
        "unchanged_row_count": unchanged,
        "behavior_improved_row_count": behavior_improved,
        "behavior_regressed_row_count": behavior_regressed,
        "regression_severity": {
            "score_delta_sum_negative": round(score_delta_sum_negative, 6),
            "score_delta_sum_positive": round(score_delta_sum_positive, 6),
            "new_zero_score_rows": new_zero_score_rows,
            "new_unmapped_rows": new_unmapped_rows,
            "new_behavior_fail_rows": new_behavior_fail_rows,
        },
        "metric_deltas": {key: metric_delta(metric_source, baseline_metric_source, key) for key in metric_keys},
        "top_row_deltas": row_deltas[:20],
        "top_improvements": top_improvements,
        "top_regressions": top_regressions,
        "new_rows": [current_by_key[key].get("function_name") for key in new_keys[:20]],
        "missing_rows": [baseline_by_key[key].get("function_name") for key in missing_keys[:20]],
    }


def comparison_outcome(comparison: dict[str, Any]) -> dict[str, Any]:
    weighted_delta = (
        comparison.get("metric_deltas", {})
        .get("weighted_semantic_similarity_percent", {})
        .get("delta")
    )
    behavior_improved = int(comparison.get("behavior_improved_row_count") or 0)
    behavior_regressed = int(comparison.get("behavior_regressed_row_count") or 0)
    improved = int(comparison.get("improved_row_count") or 0)
    regressed = int(comparison.get("regressed_row_count") or 0)
    shape_changed = bool(comparison.get("new_row_count") or comparison.get("missing_row_count"))
    if shape_changed:
        direction = "mixed"
    elif isinstance(weighted_delta, (int, float)) and weighted_delta > 0 and behavior_regressed == 0:
        direction = "improved"
    elif isinstance(weighted_delta, (int, float)) and weighted_delta < 0 and behavior_improved == 0:
        direction = "regressed"
    elif improved == 0 and regressed == 0 and behavior_improved == 0 and behavior_regressed == 0:
        direction = "unchanged"
    else:
        direction = "mixed"
    delta_text = "n/a" if not isinstance(weighted_delta, (int, float)) else f"{weighted_delta:+.3f}%"
    return {
        "direction": direction,
        "weighted_semantic_similarity_percent_delta": weighted_delta,
        "headline": (
            f"{direction}: weighted semantic similarity {delta_text}, "
            f"rows +{improved}/-{regressed}, behavior +{behavior_improved}/-{behavior_regressed}"
        ),
    }


def improvement_summary(comparison: dict[str, Any]) -> dict[str, Any]:
    metric_deltas = comparison.get("metric_deltas") if isinstance(comparison.get("metric_deltas"), dict) else {}

    def delta_for(key: str) -> float | None:
        metric = metric_deltas.get(key)
        if not isinstance(metric, dict):
            return None
        delta = metric.get("delta")
        return float(delta) if isinstance(delta, int | float) else None

    improved_metrics: list[dict[str, Any]] = []
    regressed_metrics: list[dict[str, Any]] = []
    for key in [
        "weighted_semantic_similarity_percent",
        "semantic_score_nonzero_rate",
        "function_mapping_rate",
        "decomp_success_rate",
        "candidate_compile_rate",
        "behavior_pass_rate",
        "behavior_case_pass_rate",
        "perfect_row_count",
        "supported_behavior_row_count",
    ]:
        delta = delta_for(key)
        if delta is None or delta == 0:
            continue
        metric = {
            "metric": key,
            "delta": delta,
            "current": metric_deltas.get(key, {}).get("current"),
            "baseline": metric_deltas.get(key, {}).get("baseline"),
        }
        if delta > 0:
            improved_metrics.append(metric)
        else:
            regressed_metrics.append(metric)

    return {
        "headline": comparison_outcome(comparison)["headline"],
        "improved_metric_count": len(improved_metrics),
        "regressed_metric_count": len(regressed_metrics),
        "improved_metrics": improved_metrics,
        "regressed_metrics": regressed_metrics,
        "top_improved_functions": [
            {
                "function_name": row.get("function_name"),
                "delta_percent": row.get("delta_percent"),
                "baseline_score_percent": row.get("baseline_score_percent"),
                "current_score_percent": row.get("current_score_percent"),
                "baseline_behavior": row.get("baseline_behavior"),
                "current_behavior": row.get("current_behavior"),
            }
            for row in (comparison.get("top_improvements") or [])[:10]
        ],
        "top_regressed_functions": [
            {
                "function_name": row.get("function_name"),
                "delta_percent": row.get("delta_percent"),
                "baseline_score_percent": row.get("baseline_score_percent"),
                "current_score_percent": row.get("current_score_percent"),
                "baseline_behavior": row.get("baseline_behavior"),
                "current_behavior": row.get("current_behavior"),
            }
            for row in (comparison.get("top_regressions") or [])[:10]
        ],
    }


def snapshot_baseline_artifacts(
    output_dir: Path,
    baseline_summary_path: Path,
    baseline_summary: dict[str, Any],
    baseline_rows: list[dict[str, Any]],
    comparison: dict[str, Any],
) -> dict[str, Any]:
    snapshot_dir = output_dir / "baseline_snapshot"
    snapshot_dir.mkdir(parents=True, exist_ok=True)
    summary_snapshot_path = snapshot_dir / "source_semantic_summary.json"
    rows_snapshot_path = snapshot_dir / "source_semantic_rows.json"
    comparison_snapshot_path = snapshot_dir / "source_semantic_comparison.json"
    manifest_path = snapshot_dir / "snapshot.json"
    summary_snapshot_path.write_text(dump_json_pretty(baseline_summary), encoding="utf-8")
    rows_snapshot_path.write_text(dump_json_pretty(baseline_rows), encoding="utf-8")
    comparison_snapshot_path.write_text(dump_json_pretty(comparison), encoding="utf-8")
    manifest = {
        "format": "source-semantic-baseline-snapshot-v1",
        "created_at_utc": utc_isoformat(utc_now()),
        "baseline_summary_path": rel(baseline_summary_path),
        "baseline_artifact_dir": rel(baseline_summary_path.parent),
        "summary_snapshot_path": rel(summary_snapshot_path),
        "rows_snapshot_path": rel(rows_snapshot_path),
        "comparison_snapshot_path": rel(comparison_snapshot_path),
    }
    manifest_path.write_text(dump_json_pretty(manifest), encoding="utf-8")
    manifest["snapshot_manifest_path"] = rel(manifest_path)
    return manifest


def append_history_record(path: Path, summary: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    comparison = summary.get("comparison") if isinstance(summary.get("comparison"), dict) else {}
    weighted_delta = (
        comparison.get("metric_deltas", {})
        .get("weighted_semantic_similarity_percent", {})
        .get("delta")
        if isinstance(comparison, dict)
        else None
    )
    record = {
        "run_id": summary.get("run_id"),
        "created_at_utc": summary.get("created_at_utc"),
        "artifact_dir": summary.get("artifact_dir"),
        "manifest": summary.get("manifest"),
        "row_count": summary.get("row_count"),
        "weighted_semantic_similarity_percent": summary.get("weighted_semantic_similarity_percent"),
        "weighted_semantic_similarity_percent_delta": weighted_delta,
        "comparison_outcome": summary.get("comparison_outcome"),
        "behavior_pass_rate": summary.get("behavior_pass_rate"),
        "candidate_compile_rate": summary.get("candidate_compile_rate"),
        "decomp_success_rate": summary.get("decomp_success_rate"),
        "baseline_summary_path": comparison.get("baseline_summary_path") if isinstance(comparison, dict) else None,
        "decomp_cache_hit_count": summary.get("decomp_cache_hit_count"),
        "decomp_cache_miss_count": summary.get("decomp_cache_miss_count"),
        "list_cache_hit_count": summary.get("list_cache_hit_count"),
        "list_cache_miss_count": summary.get("list_cache_miss_count"),
        "wall_sec": summary.get("wall_sec"),
    }
    with path.open("a", encoding="utf-8") as handle:
        handle.write(dump_json_line(record))


def update_latest_index(path: Path, summary: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        index = load_json(path) if path.exists() else {}
    except Exception:
        index = {}
    if not isinstance(index, dict):
        index = {}
    manifest = str(summary.get("manifest") or "unknown")
    index[manifest] = {
        "run_id": summary.get("run_id"),
        "created_at_utc": summary.get("created_at_utc"),
        "artifact_dir": summary.get("artifact_dir"),
        "summary_path": str(Path(str(summary.get("artifact_dir") or "")) / "source_semantic_summary.json"),
        "row_count": summary.get("row_count"),
        "weighted_semantic_similarity_percent": summary.get("weighted_semantic_similarity_percent"),
        "comparison_outcome": summary.get("comparison_outcome"),
        "decomp_cache_file": summary.get("decomp_cache_file"),
        "list_cache_file": summary.get("list_cache_file"),
        "history_file": summary.get("history_file"),
    }
    path.write_text(dump_json_pretty(index), encoding="utf-8")


def load_history_records(path: Path, manifest_name: str, limit: int = 12) -> list[dict[str, Any]]:
    if not path.exists():
        return []
    records: list[dict[str, Any]] = []
    try:
        with path.open("r", encoding="utf-8") as handle:
            for line in handle:
                line = line.strip()
                if not line:
                    continue
                try:
                    record = json.loads(line)
                except json.JSONDecodeError:
                    continue
                if isinstance(record, dict) and record.get("manifest") == manifest_name:
                    records.append(record)
    except OSError:
        return []
    return records[-limit:]


def history_snapshot(path: Path, summary: dict[str, Any]) -> dict[str, Any] | None:
    records = load_history_records(path, str(summary.get("manifest") or ""))
    if not records:
        return None
    same_shape_records = [record for record in records if record.get("row_count") == summary.get("row_count")]
    comparison_record = same_shape_records[-1] if same_shape_records else records[-1]
    latest_record = records[-1]
    current_similarity = summary.get("weighted_semantic_similarity_percent")
    comparison_similarity = comparison_record.get("weighted_semantic_similarity_percent")
    latest_similarity = latest_record.get("weighted_semantic_similarity_percent")
    comparable_shape = comparison_record.get("row_count") == summary.get("row_count")
    comparison_delta = (
        round(float(current_similarity) - float(comparison_similarity), 6)
        if comparable_shape
        and isinstance(current_similarity, (int, float))
        and isinstance(comparison_similarity, (int, float))
        else None
    )
    latest_delta = (
        round(float(current_similarity) - float(latest_similarity), 6)
        if latest_record.get("row_count") == summary.get("row_count")
        and isinstance(current_similarity, (int, float))
        and isinstance(latest_similarity, (int, float))
        else None
    )
    return {
        "history_file": rel(path),
        "previous_run_count": len(records),
        "latest_previous_run": latest_record,
        "comparison_previous_run": comparison_record,
        "comparison_shape_matches": comparable_shape,
        "latest_shape_matches": latest_record.get("row_count") == summary.get("row_count"),
        "weighted_semantic_similarity_percent_delta_vs_comparison": comparison_delta,
        "weighted_semantic_similarity_percent_delta_vs_latest": latest_delta,
        "recent_runs": records,
    }


def render_markdown(summary: dict[str, Any], rows: list[dict[str, Any]]) -> str:
    lines = [
        f"# Source Semantic Benchmark: {summary['manifest']}",
        "",
        f"- Run ID: `{summary.get('run_id', 'unknown')}`",
        f"- Artifact dir: `{summary.get('artifact_dir', 'unknown')}`",
        f"- Entries: {summary['entry_count']}",
        f"- Rows: {summary['row_count']}",
        f"- Function mapping rate: {summary['function_mapping_rate']:.3f}",
        f"- Decompile success rate: {summary['decomp_success_rate']:.3f}",
        f"- Candidate compile rate: {summary['candidate_compile_rate']:.3f}",
        f"- Behavior pass rate: {summary['behavior_pass_rate']:.3f}",
        f"- Weighted semantic similarity: {summary['weighted_semantic_similarity_percent']:.3f}%",
        f"- Perfect rows: {summary['perfect_row_count']}",
        f"- Supported behavior rows: {summary['supported_behavior_row_count']}",
        f"- Host execution unavailable rows: {summary['host_execution_unavailable_count']}",
    ]
    effective = summary.get("effective_coverage") if isinstance(summary.get("effective_coverage"), dict) else {}
    behavior_eligibility = (
        summary.get("behavior_eligibility") if isinstance(summary.get("behavior_eligibility"), dict) else {}
    )
    if effective:
        lines.append(
            "- Effective coverage: "
            f"mapped {effective.get('mapped_rows', 0)}/{summary['row_count']} "
            f"({float(effective.get('mapped_rate', 0.0) or 0.0):.3f}), "
            f"decompiled {effective.get('decompiled_rows', 0)}/{summary['row_count']} "
            f"({float(effective.get('decompiled_rate', 0.0) or 0.0):.3f}), "
            f"behavior executed {effective.get('behavior_executed_rows', 0)}/{summary['row_count']} "
            f"({float(effective.get('behavior_executed_rate', 0.0) or 0.0):.3f})"
        )
    if behavior_eligibility:
        lines.append(
            "- Behavior eligibility: "
            f"eligible {behavior_eligibility.get('eligible_rows', 0)}, "
            f"execution rate {float(behavior_eligibility.get('execution_rate', 0.0) or 0.0):.3f}, "
            f"pass/eligible {float(behavior_eligibility.get('pass_rate_eligible_denominator', 0.0) or 0.0):.3f}, "
            f"pass/total {float(behavior_eligibility.get('pass_rate_total_denominator', 0.0) or 0.0):.3f}"
        )
    if "wall_sec" in summary:
        lines.append(f"- Wall time: {summary['wall_sec']:.3f}s")
    if summary.get("decomp_cache_file"):
        lines.append(f"- Decomp cache: `{summary['decomp_cache_file']}`")
        lines.append(
            f"- Decomp cache hits/misses: {summary.get('decomp_cache_hit_count', 0)}/"
            f"{summary.get('decomp_cache_miss_count', 0)}"
        )
    if summary.get("list_cache_file"):
        lines.append(f"- List cache: `{summary['list_cache_file']}`")
        lines.append(
            f"- List cache hits/misses: {summary.get('list_cache_hit_count', 0)}/"
            f"{summary.get('list_cache_miss_count', 0)}"
        )
    cache_efficiency = summary.get("cache_efficiency_metrics")
    if isinstance(cache_efficiency, dict):
        lines.append(
            "- Cache hit rates: "
            f"decomp {float(cache_efficiency.get('decomp_cache_hit_rate', 0.0) or 0.0):.3f}, "
            f"list {float(cache_efficiency.get('list_cache_hit_rate', 0.0) or 0.0):.3f}, "
            f"behavior {float(cache_efficiency.get('behavior_cache_hit_rate', 0.0) or 0.0):.3f}"
        )
    if summary.get("history_file"):
        lines.append(f"- History: `{summary['history_file']}`")
    if summary.get("latest_index_file"):
        lines.append(f"- Latest index: `{summary['latest_index_file']}`")
    history = summary.get("history")
    if isinstance(history, dict):
        comparison_record = (
            history.get("comparison_previous_run")
            if isinstance(history.get("comparison_previous_run"), dict)
            else {}
        )
        latest_record = (
            history.get("latest_previous_run")
            if isinstance(history.get("latest_previous_run"), dict)
            else {}
        )
        comparison_delta = history.get("weighted_semantic_similarity_percent_delta_vs_comparison")
        latest_delta = history.get("weighted_semantic_similarity_percent_delta_vs_latest")
        comparison_delta_text = "n/a" if comparison_delta is None else f"{comparison_delta:+.3f}%"
        latest_delta_text = "n/a" if latest_delta is None else f"{latest_delta:+.3f}%"
        lines.append(
            f"- Latest comparable history delta: {comparison_delta_text} "
            f"(previous run `{comparison_record.get('run_id', 'unknown')}`)"
        )
        lines.append(
            f"- Latest history delta: {latest_delta_text} "
            f"(previous run `{latest_record.get('run_id', 'unknown')}`)"
        )
    if summary.get("baseline_snapshot"):
        snapshot = summary["baseline_snapshot"]
        lines.append(f"- Baseline snapshot: `{snapshot.get('snapshot_manifest_path')}`")
    improvement = summary.get("improvement_summary")
    if isinstance(improvement, dict):
        lines.extend(["", "## Improvement Summary", "", f"- {improvement.get('headline', 'n/a')}"])
        improved_metrics = improvement.get("improved_metrics") or []
        regressed_metrics = improvement.get("regressed_metrics") or []
        if improved_metrics:
            lines.extend(["", "### Improved Metrics", "", "| Metric | Delta | Baseline | Current |", "|---|---:|---:|---:|"])
            for metric in improved_metrics:
                lines.append(
                    f"| `{metric.get('metric')}` | {float(metric.get('delta', 0.0) or 0.0):+.3f} | "
                    f"{metric.get('baseline')} | {metric.get('current')} |"
                )
        if regressed_metrics:
            lines.extend(["", "### Regressed Metrics", "", "| Metric | Delta | Baseline | Current |", "|---|---:|---:|---:|"])
            for metric in regressed_metrics:
                lines.append(
                    f"| `{metric.get('metric')}` | {float(metric.get('delta', 0.0) or 0.0):+.3f} | "
                    f"{metric.get('baseline')} | {metric.get('current')} |"
                )
        top_improved = improvement.get("top_improved_functions") or []
        if top_improved:
            lines.extend(["", "### Improved Functions", "", "| Function | Delta | Baseline | Current | Behavior |", "|---|---:|---:|---:|---|"])
            for row in top_improved[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | {float(row.get('delta_percent', 0.0) or 0.0):+.3f}% | "
                    f"{float(row.get('baseline_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(row.get('current_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{row.get('baseline_behavior')} -> {row.get('current_behavior')} |"
                )
    lines.extend([
        "",
        "## By Language",
        "",
        "| Language | Rows | Mapped | Decomp OK | Behavior Pass | Avg Similarity |",
        "|---|---:|---:|---:|---:|---:|",
    ])
    for lang, bucket in sorted(summary["by_language"].items()):
        lines.append(
            f"| {lang} | {bucket['row_count']} | {bucket['mapped']} | {bucket['decomp_success']} | "
            f"{bucket['behavior_pass']} | {bucket['avg_semantic_score_percent']:.3f}% |"
        )
    if summary.get("by_arch"):
        lines.extend([
            "",
            "## By Architecture",
            "",
            "| Architecture | Rows | Mapped | Decomp OK | Behavior Pass | Avg Similarity |",
            "|---|---:|---:|---:|---:|---:|",
        ])
        for arch, bucket in sorted(summary["by_arch"].items()):
            lines.append(
                f"| {arch} | {bucket['row_count']} | {bucket['mapped']} | {bucket['decomp_success']} | "
                f"{bucket['behavior_pass']} | {bucket['avg_semantic_score_percent']:.3f}% |"
            )
    if summary.get("by_source_return_kind"):
        lines.extend([
            "",
            "## By Source Signature",
            "",
            "| Return Kind | Rows | Mapped | Decomp OK | Behavior Pass | Avg Similarity |",
            "|---|---:|---:|---:|---:|---:|",
        ])
        for return_kind, bucket in sorted(summary["by_source_return_kind"].items()):
            lines.append(
                f"| {return_kind} | {bucket['row_count']} | {bucket['mapped']} | {bucket['decomp_success']} | "
                f"{bucket['behavior_pass']} | {bucket['avg_semantic_score_percent']:.3f}% |"
            )
        param_shapes = summary.get("by_source_param_shape")
        if isinstance(param_shapes, dict) and param_shapes:
            lines.extend(["", "| Param Shape | Rows | Mapped | Decomp OK | Behavior Pass | Avg Similarity |", "|---|---:|---:|---:|---:|---:|"])
            for param_shape, bucket in sorted(param_shapes.items()):
                lines.append(
                    f"| {param_shape} | {bucket['row_count']} | {bucket['mapped']} | {bucket['decomp_success']} | "
                    f"{bucket['behavior_pass']} | {bucket['avg_semantic_score_percent']:.3f}% |"
                )
    if summary.get("behavior_status_counts"):
        lines.extend(["", "## Behavior Status", "", "| Status | Rows |", "|---|---:|"])
        for status, count in sorted(summary["behavior_status_counts"].items()):
            lines.append(f"| {status} | {count} |")
    if summary.get("zero_credit_breakdown"):
        lines.extend(["", "## Zero-Credit Breakdown", "", "| Reason | Rows |", "|---|---:|"])
        for reason, count in sorted(summary["zero_credit_breakdown"].items()):
            lines.append(f"| {reason} | {count} |")
    if summary.get("score_distribution"):
        lines.extend(["", "## Score Distribution", "", "| Bucket | Rows |", "|---|---:|"])
        for bucket, count in sorted(summary["score_distribution"].items()):
            lines.append(f"| {bucket} | {count} |")
    if summary.get("semantic_score_stats"):
        stats = summary["semantic_score_stats"]
        lines.extend(["", "## Semantic Score Stats", ""])
        lines.append(
            f"- Avg {float(stats.get('avg', 0.0) or 0.0):.6f}, "
            f"p50 {float(stats.get('p50', 0.0) or 0.0):.6f}, "
            f"p90 {float(stats.get('p90', 0.0) or 0.0):.6f}, "
            f"nonzero {stats.get('nonzero_count', 0)}/{stats.get('count', 0)} "
            f"({float(stats.get('nonzero_rate', 0.0) or 0.0):.3f})"
        )
    score_components = summary.get("score_component_metrics")
    if isinstance(score_components, dict):
        lines.extend(["", "## Score Component Metrics", ""])
        lines.append(
            f"- Behavior contribution {float(score_components.get('behavior_component_score_sum', 0.0) or 0.0):.6f}/"
            f"{float(score_components.get('behavior_component_possible_score_sum', 0.0) or 0.0):.6f}, "
            f"static contribution {float(score_components.get('static_component_score_sum', 0.0) or 0.0):.6f}/"
            f"{float(score_components.get('static_component_possible_score_sum', 0.0) or 0.0):.6f}"
        )
        lines.append(
            f"- Behavior lost {float(score_components.get('behavior_component_lost_score_sum', 0.0) or 0.0):.6f}, "
            f"static lost {float(score_components.get('static_component_lost_score_sum', 0.0) or 0.0):.6f}"
        )
    scoring_contract = summary.get("scoring_contract")
    if isinstance(scoring_contract, dict):
        lines.extend(["", "## Scoring Contract", ""])
        for key, value in sorted(scoring_contract.items()):
            lines.append(f"- `{key}`: {value}")
    score_denominators = summary.get("score_denominator_metrics")
    if isinstance(score_denominators, dict):
        lines.extend(["", "## Score Denominator Metrics", "", "| Metric | Value |", "|---|---:|"])
        for key, value in sorted(score_denominators.items()):
            if isinstance(value, str):
                lines.append(f"| {key} | `{value}` |")
            else:
                lines.append(f"| {key} | {value} |")
    semantic_loss = summary.get("semantic_loss_metrics")
    if isinstance(semantic_loss, dict):
        lines.extend(["", "## Semantic Loss Metrics", ""])
        lines.append(
            f"- Lost score: {float(semantic_loss.get('total_lost_score', 0.0) or 0.0):.6f}, "
            f"avg per row {float(semantic_loss.get('avg_lost_score_per_row', 0.0) or 0.0):.6f}"
        )
        loss_by_stage = semantic_loss.get("lost_score_by_stage_first_failure")
        if isinstance(loss_by_stage, dict) and loss_by_stage:
            lines.extend(["", "| First Stage Failure | Lost Score |", "|---|---:|"])
            for stage, value in sorted(loss_by_stage.items()):
                lines.append(f"| {stage} | {float(value or 0.0):.6f} |")
    readiness = summary.get("semantic_readiness_metrics")
    if isinstance(readiness, dict):
        lines.extend(["", "## Semantic Readiness Metrics", ""])
        lines.append(
            f"- Fully perfect {readiness.get('fully_perfect_rows', 0)}/{readiness.get('manifest_rows', 0)} "
            f"({float(readiness.get('fully_perfect_rate', 0.0) or 0.0):.3f}), "
            f"behavior pass + static perfect {readiness.get('behavior_pass_static_perfect_rows', 0)} "
            f"({float(readiness.get('behavior_pass_static_perfect_rate', 0.0) or 0.0):.3f})"
        )
        lines.append(
            f"- Behavior pass with static gap {readiness.get('behavior_pass_static_gap_rows', 0)}, "
            f"static perfect but behavior non-pass {readiness.get('static_perfect_behavior_nonpass_rows', 0)}, "
            f"pipeline OK but behavior non-pass {readiness.get('pipeline_ok_behavior_nonpass_rows', 0)}"
        )
    integrity = summary.get("benchmark_integrity_metrics")
    if isinstance(integrity, dict):
        lines.extend(["", "## Benchmark Integrity Metrics", ""])
        lines.append(
            f"- Score denominator rows {integrity.get('score_denominator_row_count', 0)}, "
            f"excluded rows {integrity.get('rows_excluded_from_semantic_score_denominator', 0)}, "
            f"static excluded rows {integrity.get('rows_excluded_from_static_similarity_denominator', 0)}"
        )
        lines.append(
            f"- Missing source features penalized: {integrity.get('missing_source_features_penalized')}; "
            f"extra decompiler features penalized: {integrity.get('extra_decompiler_features_penalized')}; "
            f"behavior unsupported/missing fail closed: {integrity.get('behavior_missing_or_unsupported_rows_fail_closed')}"
        )
    improvement_axes = summary.get("improvement_axis_metrics")
    if isinstance(improvement_axes, dict) and improvement_axes:
        lines.extend([
            "",
            "## Improvement Axis Metrics",
            "",
            "| Axis | Rows | Avg Similarity | Lost Score | Missing Features |",
            "|---|---:|---:|---:|---:|",
        ])
        for axis, metrics in sorted(
            improvement_axes.items(),
            key=lambda item: float((item[1] or {}).get("lost_score_sum", 0.0) or 0.0),
            reverse=True,
        ):
            if not isinstance(metrics, dict):
                continue
            lines.append(
                f"| {axis} | {metrics.get('row_count', 0)} | "
                f"{float(metrics.get('avg_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                f"{float(metrics.get('lost_score_sum', 0.0) or 0.0):.6f} | "
                f"{float(metrics.get('missing_feature_total', 0.0) or 0.0):.0f} |"
            )
    focus_areas = summary.get("focus_area_metrics")
    if isinstance(focus_areas, dict) and focus_areas:
        lines.extend([
            "",
            "## Focus Area Metrics",
            "",
            "| Focus Area | Rows | Avg Similarity | Lost Score | Missing Features |",
            "|---|---:|---:|---:|---:|",
        ])
        for area, metrics in sorted(
            focus_areas.items(),
            key=lambda item: float((item[1] or {}).get("lost_score_sum", 0.0) or 0.0),
            reverse=True,
        ):
            if not isinstance(metrics, dict):
                continue
            lines.append(
                f"| {area} | {metrics.get('row_count', 0)} | "
                f"{float(metrics.get('avg_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                f"{float(metrics.get('lost_score_sum', 0.0) or 0.0):.6f} | "
                f"{float(metrics.get('missing_feature_total', 0.0) or 0.0):.0f} |"
            )
    roadmap = summary.get("roadmap_priority_metrics")
    if isinstance(roadmap, dict):
        buckets = roadmap.get("buckets") if isinstance(roadmap.get("buckets"), dict) else {}
        order = roadmap.get("priority_order") if isinstance(roadmap.get("priority_order"), list) else sorted(buckets)
        if buckets:
            lines.extend([
                "",
                "## Roadmap Priority Metrics",
                "",
                "| Priority | Rows | Avg Similarity | Lost Score | Missing | Extra |",
                "|---|---:|---:|---:|---:|---:|",
            ])
            for priority in order:
                metrics = buckets.get(priority)
                if not isinstance(metrics, dict):
                    continue
                lines.append(
                    f"| {priority} | {metrics.get('row_count', 0)} | "
                    f"{float(metrics.get('avg_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(metrics.get('lost_score_sum', 0.0) or 0.0):.6f} | "
                    f"{float(metrics.get('missing_feature_total', 0.0) or 0.0):.0f} | "
                    f"{float(metrics.get('extra_feature_total', 0.0) or 0.0):.0f} |"
                )
    type_data_gaps = summary.get("type_data_gap_metrics")
    if isinstance(type_data_gaps, dict):
        lines.extend(["", "## Type/Data Gap Metrics", ""])
        lines.append(
            f"- Signature gap rows {type_data_gaps.get('signature_gap_row_count', 0)}, "
            f"memory gap rows {type_data_gaps.get('memory_gap_row_count', 0)}, "
            f"call gap rows {type_data_gaps.get('call_gap_row_count', 0)}"
        )
    signedness_gaps = summary.get("signedness_only_signature_gap_metrics")
    if isinstance(signedness_gaps, dict):
        lines.extend(["", "## Signedness-Only Signature Gap Metrics", ""])
        lines.append(
            f"- Rows {signedness_gaps.get('row_count', 0)}, "
            f"pairs {float(signedness_gaps.get('total_pair_count', 0.0) or 0.0):.0f}, "
            f"param pairs {float(signedness_gaps.get('param_pair_count', 0.0) or 0.0):.0f}, "
            f"return pairs {float(signedness_gaps.get('return_pair_count', 0.0) or 0.0):.0f}"
        )
        top_rows = signedness_gaps.get("top_rows")
        if isinstance(top_rows, list) and top_rows:
            lines.extend(["", "| Function | Score | Param Pairs | Return Pairs |", "|---|---:|---:|---:|"])
            for row in top_rows[:8]:
                if not isinstance(row, dict):
                    continue
                lines.append(
                    f"| `{row.get('function_name')}` | "
                    f"{float(row.get('semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(row.get('param_pair_count', 0.0) or 0.0):.0f} | "
                    f"{float(row.get('return_pair_count', 0.0) or 0.0):.0f} |"
                )
    signature_confusion = summary.get("signature_kind_confusion_metrics")
    if isinstance(signature_confusion, dict):
        lines.extend(["", "## Signature Kind Confusion Metrics", ""])
        lines.append(
            f"- Return match rate {float(signature_confusion.get('return_match_rate', 0.0) or 0.0):.3f}, "
            f"param match rate {float(signature_confusion.get('param_match_rate', 0.0) or 0.0):.3f}, "
            f"arity mismatch rows {signature_confusion.get('param_arity_mismatch_row_count', 0)}"
        )
        param_pairs = signature_confusion.get("param_pair_counts")
        if isinstance(param_pairs, dict) and param_pairs:
            lines.extend(["", "| Param Pair | Count |", "|---|---:|"])
            for pair, count in sorted(param_pairs.items(), key=lambda item: (-int(item[1]), str(item[0])))[:10]:
                lines.append(f"| `{pair}` | {count} |")
        gap_rows = signature_confusion.get("top_signature_pair_gap_rows")
        if isinstance(gap_rows, list) and gap_rows:
            lines.extend(["", "| Function | Score | Return | Params |", "|---|---:|---|---|"])
            for row in gap_rows[:8]:
                if not isinstance(row, dict):
                    continue
                lines.append(
                    f"| `{row.get('function_name')}` | "
                    f"{float(row.get('semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"`{row.get('source_return_kind')}->{row.get('decomp_return_kind')}` | "
                    f"{int(row.get('param_mismatch_count') or 0)} mismatches |"
                )
    structuring_gaps = summary.get("structuring_gap_metrics")
    if isinstance(structuring_gaps, dict):
        lines.extend(["", "## Structuring Gap Metrics", ""])
        lines.append(
            f"- Control-flow gap rows {structuring_gaps.get('control_flow_gap_row_count', 0)}, "
            f"hard non-perfect rows {structuring_gaps.get('hard_nonperfect_row_count', 0)}"
        )
    fid_name = summary.get("fid_name_recovery_metrics")
    if isinstance(fid_name, dict):
        lines.extend(["", "## FID/Name Recovery Metrics", ""])
        lines.append(f"- Name or mapping gap rows {fid_name.get('name_or_mapping_gap_row_count', 0)}")
    arch_support = summary.get("architecture_support_metrics")
    if isinstance(arch_support, dict) and arch_support:
        lines.extend([
            "",
            "## Architecture Support Metrics",
            "",
            "| Architecture | Rows | Avg Similarity | Lost Score | Top First Failure |",
            "|---|---:|---:|---:|---|",
        ])
        for arch, metrics in sorted(arch_support.items()):
            if not isinstance(metrics, dict):
                continue
            stage_counts = metrics.get("stage_first_failure_counts")
            top_stage = "none"
            if isinstance(stage_counts, dict) and stage_counts:
                top_stage = sorted(stage_counts.items(), key=lambda item: (item[1], item[0]), reverse=True)[0][0]
            lines.append(
                f"| {arch} | {metrics.get('row_count', 0)} | "
                f"{float(metrics.get('avg_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                f"{float(metrics.get('lost_score_sum', 0.0) or 0.0):.6f} | {top_stage} |"
            )
    complexity_quality = summary.get("complexity_quality_metrics")
    if isinstance(complexity_quality, dict):
        buckets = complexity_quality.get("by_source_feature_bucket")
        if isinstance(buckets, dict) and buckets:
            lines.extend([
                "",
                "## Complexity Quality Metrics",
                "",
                "| Source Feature Bucket | Rows | Avg Similarity | Behavior Pass Rate | Zero Rows | Missing Features |",
                "|---|---:|---:|---:|---:|---:|",
            ])
            for bucket_name, bucket in sorted(buckets.items()):
                if not isinstance(bucket, dict):
                    continue
                lines.append(
                    f"| {bucket_name} | {bucket.get('row_count', 0)} | "
                    f"{float(bucket.get('avg_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(bucket.get('behavior_pass_rate', 0.0) or 0.0):.3f} | "
                    f"{bucket.get('zero_score_count', 0)} | "
                    f"{float(bucket.get('missing_feature_total', 0.0) or 0.0):.0f} |"
                )
        hard_rows = complexity_quality.get("hard_nonperfect_rows") or []
        if hard_rows:
            lines.extend(["", "### Hard Non-Perfect Rows", "", "| Function | Score | Behavior | Stage | Source Features | Missing |", "|---|---:|---|---|---:|---:|"])
            for row in hard_rows[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | "
                    f"{float(row.get('semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{row.get('behavior_status')} | {row.get('stage_first_failure')} | "
                    f"{float(row.get('source_feature_total', 0.0) or 0.0):.0f} | "
                    f"{float(row.get('missing_feature_total', 0.0) or 0.0):.0f} |"
                )
    stage_costs = summary.get("stage_cost_correlation_metrics")
    if isinstance(stage_costs, dict):
        by_stage = stage_costs.get("decompile_wall_by_stage_first_failure")
        if isinstance(by_stage, dict) and by_stage:
            lines.extend(["", "## Stage Cost Correlation Metrics", "", "| First Stage Failure | Rows | Avg Decompile Sec | P95 | Max |", "|---|---:|---:|---:|---:|"])
            for stage, stats in sorted(by_stage.items()):
                if not isinstance(stats, dict):
                    continue
                lines.append(
                    f"| {stage} | {stats.get('count', 0)} | "
                    f"{float(stats.get('avg', 0.0) or 0.0):.6f} | "
                    f"{float(stats.get('p95', 0.0) or 0.0):.6f} | "
                    f"{float(stats.get('max', 0.0) or 0.0):.6f} |"
                )
        score_by_cost = stage_costs.get("score_by_decompile_cost_bucket")
        if isinstance(score_by_cost, dict) and score_by_cost:
            lines.extend(["", "| Cost Bucket | Rows | Avg Score | P50 | Lost Score |", "|---|---:|---:|---:|---:|"])
            lost_by_cost = stage_costs.get("lost_score_by_decompile_cost_bucket")
            if not isinstance(lost_by_cost, dict):
                lost_by_cost = {}
            for bucket, stats in sorted(score_by_cost.items()):
                if not isinstance(stats, dict):
                    continue
                lines.append(
                    f"| {bucket} | {stats.get('count', 0)} | "
                    f"{float(stats.get('avg', 0.0) or 0.0):.6f} | "
                    f"{float(stats.get('p50', 0.0) or 0.0):.6f} | "
                    f"{float(lost_by_cost.get(bucket, 0.0) or 0.0):.6f} |"
                )
    admission = summary.get("admission_gate_metrics")
    if isinstance(admission, dict):
        counts = admission.get("counts") if isinstance(admission.get("counts"), dict) else {}
        rates = (
            admission.get("rates_total_denominator")
            if isinstance(admission.get("rates_total_denominator"), dict)
            else {}
        )
        order = admission.get("gate_order") if isinstance(admission.get("gate_order"), list) else sorted(counts)
        lines.extend(["", "## Admission Gate Metrics", "", "| Gate | Rows | Total Rate |", "|---|---:|---:|"])
        for gate_name in order:
            if gate_name not in counts:
                continue
            lines.append(
                f"| {gate_name} | {counts.get(gate_name, 0)} | "
                f"{float(rates.get(gate_name, 0.0) or 0.0):.3f} |"
            )
    quality_funnel = summary.get("quality_gate_funnel_metrics")
    if isinstance(quality_funnel, dict):
        counts = quality_funnel.get("counts") if isinstance(quality_funnel.get("counts"), dict) else {}
        drops = (
            quality_funnel.get("drop_rows_from_previous_gate")
            if isinstance(quality_funnel.get("drop_rows_from_previous_gate"), dict)
            else {}
        )
        retention = (
            quality_funnel.get("retention_rate_from_previous_gate")
            if isinstance(quality_funnel.get("retention_rate_from_previous_gate"), dict)
            else {}
        )
        order = quality_funnel.get("gate_order") if isinstance(quality_funnel.get("gate_order"), list) else sorted(counts)
        if counts:
            lines.extend(["", "## Quality Gate Funnel Metrics", "", "| Gate | Rows | Drop From Previous | Retention |", "|---|---:|---:|---:|"])
            previous_gate = None
            for gate_name in order:
                if gate_name not in counts:
                    continue
                edge = f"{previous_gate}->{gate_name}" if previous_gate is not None else ""
                lines.append(
                    f"| {gate_name} | {counts.get(gate_name, 0)} | "
                    f"{drops.get(edge, 0) if edge else 0} | "
                    f"{float(retention.get(edge, 1.0 if previous_gate is None else 0.0) or 0.0):.3f} |"
                )
                previous_gate = gate_name
    stage_transitions = summary.get("stage_transition_metrics")
    if isinstance(stage_transitions, dict):
        furthest = stage_transitions.get("furthest_ok_stage_counts")
        if isinstance(furthest, dict) and furthest:
            lines.extend(["", "## Stage Transition Metrics", "", "| Furthest OK Stage | Rows |", "|---|---:|"])
            for stage, count in sorted(furthest.items()):
                lines.append(f"| {stage} | {count} |")
    sleigh_health = summary.get("sleigh_lift_health_metrics")
    if isinstance(sleigh_health, dict):
        lines.extend(["", "## SLEIGH Lift Health Metrics", ""])
        lines.append(
            f"- Decode OK {sleigh_health.get('decode_ok_rows', 0)}/{sleigh_health.get('mapped_rows', 0)} "
            f"({float(sleigh_health.get('decode_ok_rate_mapped_denominator', 0.0) or 0.0):.3f} mapped denominator), "
            f"raw p-code OK {sleigh_health.get('raw_pcode_ok_rows', 0)}/{sleigh_health.get('mapped_rows', 0)} "
            f"({float(sleigh_health.get('raw_pcode_ok_rate_mapped_denominator', 0.0) or 0.0):.3f})"
        )
        lines.append(
            f"- Compat imports {float(sleigh_health.get('raw_pcode_compat_import_total', 0.0) or 0.0):.0f}, "
            f"invalid p-code shapes {float(sleigh_health.get('invalid_pcode_shape_total', 0.0) or 0.0):.0f}, "
            f"SLEIGH first-blocker rows {sleigh_health.get('sleigh_first_blocker_row_count', 0)}"
        )
        blocker_rows = sleigh_health.get("top_sleigh_blocker_rows") or []
        if blocker_rows:
            lines.extend(["", "| Function | Address | Score | First Failure |", "|---|---|---:|---|"])
            for row in blocker_rows[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | `{row.get('address')}` | "
                    f"{float(row.get('semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{row.get('stage_first_failure')} |"
                )
    failure_diagnostics = summary.get("behavior_failure_diagnostics")
    if isinstance(failure_diagnostics, dict):
        owner_counts = failure_diagnostics.get("owner_counts")
        if isinstance(owner_counts, dict) and owner_counts:
            lines.extend(["", "## Behavior Failure Diagnostics", "", "| Owner | Rows |", "|---|---:|"])
            for owner, count in sorted(owner_counts.items()):
                lines.append(f"| {owner} | {count} |")
        detail_counts = failure_diagnostics.get("detail_signature_counts")
        if isinstance(detail_counts, dict) and detail_counts:
            lines.extend(["", "| Detail Signature | Rows |", "|---|---:|"])
            for signature, count in list(detail_counts.items())[:8]:
                lines.append(f"| `{signature}` | {count} |")
    quadrants = summary.get("semantic_quality_quadrant_metrics")
    if isinstance(quadrants, dict) and quadrants:
        lines.extend([
            "",
            "## Semantic Quality Quadrants",
            "",
            "| Quadrant | Rows | Avg Similarity | Lost Score | Missing | Extra |",
            "|---|---:|---:|---:|---:|---:|",
        ])
        for quadrant, metrics in sorted(
            quadrants.items(),
            key=lambda item: float((item[1] or {}).get("lost_score_sum", 0.0) or 0.0),
            reverse=True,
        ):
            if not isinstance(metrics, dict):
                continue
            lines.append(
                f"| `{quadrant}` | {metrics.get('row_count', 0)} | "
                f"{float(metrics.get('avg_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                f"{float(metrics.get('lost_score_sum', 0.0) or 0.0):.6f} | "
                f"{float(metrics.get('missing_feature_total', 0.0) or 0.0):.0f} | "
                f"{float(metrics.get('extra_feature_total', 0.0) or 0.0):.0f} |"
            )
    outcome_matrix = summary.get("outcome_matrix_metrics")
    if isinstance(outcome_matrix, dict):
        outcomes = outcome_matrix.get("top_outcomes_by_lost_score")
        if isinstance(outcomes, dict) and outcomes:
            lines.extend(["", "## Outcome Matrix Metrics", "", "| Outcome | Rows | Lost Score |", "|---|---:|---:|"])
            for outcome, metrics in outcomes.items():
                if not isinstance(metrics, dict):
                    continue
                lines.append(
                    f"| `{outcome}` | {metrics.get('row_count', 0)} | "
                    f"{float(metrics.get('lost_score_sum', 0.0) or 0.0):.6f} |"
                )
    blind_spots = summary.get("coverage_blind_spot_metrics")
    if isinstance(blind_spots, dict):
        counts = blind_spots.get("counts")
        if isinstance(counts, dict) and counts:
            lines.extend(["", "## Coverage Blind-Spot Metrics", "", "| Blind Spot | Rows |", "|---|---:|"])
            for kind, count in sorted(counts.items()):
                lines.append(f"| {kind} | {count} |")
    gap_density = summary.get("static_gap_density_metrics")
    if isinstance(gap_density, dict):
        missing_density = gap_density.get("missing_density_distribution")
        extra_density = gap_density.get("extra_density_distribution")
        if isinstance(missing_density, dict) and isinstance(extra_density, dict):
            lines.extend(["", "## Static Gap Density Metrics", ""])
            lines.append(
                f"- Missing density avg {float(missing_density.get('avg', 0.0) or 0.0):.6f}, "
                f"p95 {float(missing_density.get('p95', 0.0) or 0.0):.6f}; "
                f"extra density avg {float(extra_density.get('avg', 0.0) or 0.0):.6f}, "
                f"p95 {float(extra_density.get('p95', 0.0) or 0.0):.6f}"
            )
        gap_buckets = gap_density.get("gap_bucket_rows")
        if isinstance(gap_buckets, dict) and gap_buckets:
            lines.extend(["", "| Gap Bucket | Rows | Avg Similarity | Missing | Extra |", "|---|---:|---:|---:|---:|"])
            for bucket, metrics in sorted(gap_buckets.items()):
                if not isinstance(metrics, dict):
                    continue
                lines.append(
                    f"| `{bucket}` | {metrics.get('row_count', 0)} | "
                    f"{float(metrics.get('avg_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(metrics.get('missing_feature_total', 0.0) or 0.0):.0f} | "
                    f"{float(metrics.get('extra_feature_total', 0.0) or 0.0):.0f} |"
                )
    static_hot_rows = summary.get("static_gap_hot_row_metrics")
    if isinstance(static_hot_rows, dict):
        missing_rows = static_hot_rows.get("top_missing_feature_rows")
        if isinstance(missing_rows, list) and missing_rows:
            lines.extend(["", "## Static Gap Hot Rows", "", "| Function | Score | Missing | Extra | Top Missing Features |", "|---|---:|---:|---:|---|"])
            for row in missing_rows[:10]:
                if not isinstance(row, dict):
                    continue
                top_missing = ", ".join(
                    f"{item.get('feature')}:{item.get('count')}"
                    for item in (row.get("top_missing_features") or [])[:3]
                    if isinstance(item, dict)
                )
                lines.append(
                    f"| `{row.get('function_name')}` | "
                    f"{float(row.get('static_semantic_score_percent', row.get('semantic_score_percent', 0.0)) or 0.0):.3f}% | "
                    f"{float(row.get('missing_feature_total', 0.0) or 0.0):.0f} | "
                    f"{float(row.get('extra_feature_total', 0.0) or 0.0):.0f} | "
                    f"`{top_missing}` |"
                )
    denominator_accounting = summary.get("denominator_accounting_metrics")
    if isinstance(denominator_accounting, dict):
        lines.extend(["", "## Denominator Accounting", "", "| Metric | Rows |", "|---|---:|"])
        for key, value in sorted(denominator_accounting.items()):
            lines.append(f"| {key} | {value} |")
    source_selection = summary.get("source_row_selection_metrics")
    if isinstance(source_selection, dict):
        lines.extend(["", "## Source Row Selection Metrics", ""])
        lines.append(
            f"- Extracted {source_selection.get('extracted_source_function_count', 0)} source functions, "
            f"filtered {source_selection.get('filtered_source_function_count', 0)}, "
            f"selected {source_selection.get('selected_source_function_count', 0)}, "
            f"suppressed static inline helpers "
            f"{source_selection.get('suppressed_static_inline_helper_count', 0)}"
        )
        suppressed = source_selection.get("suppressed_static_inline_helpers") or []
        if suppressed:
            lines.extend(["", "| Suppressed Helper | Entry | Callers | Reason |", "|---|---|---|---|"])
            for row in suppressed[:12]:
                callers = ", ".join(str(name) for name in (row.get("matched_callers") or []))
                lines.append(
                    f"| `{row.get('function_name')}` | `{row.get('entry_id')}` | `{callers}` | "
                    f"{row.get('reason')} |"
                )
    score_by_behavior = summary.get("score_by_behavior_status")
    if isinstance(score_by_behavior, dict) and score_by_behavior:
        lines.extend(["", "## Score By Behavior Status", "", "| Status | Rows | Avg | P50 | P90 |", "|---|---:|---:|---:|---:|"])
        for status, stats in sorted(score_by_behavior.items()):
            if not isinstance(stats, dict):
                continue
            lines.append(
                f"| {status} | {stats.get('count', 0)} | "
                f"{float(stats.get('avg', 0.0) or 0.0):.6f} | "
                f"{float(stats.get('p50', 0.0) or 0.0):.6f} | "
                f"{float(stats.get('p90', 0.0) or 0.0):.6f} |"
            )
    behavior_by_stage = summary.get("behavior_status_by_stage_first_failure")
    if isinstance(behavior_by_stage, dict) and behavior_by_stage:
        lines.extend(["", "## Behavior By First Stage Failure", "", "| Stage | Behavior | Rows |", "|---|---|---:|"])
        for stage, counts in sorted(behavior_by_stage.items()):
            if not isinstance(counts, dict):
                continue
            for status, count in sorted(counts.items()):
                lines.append(f"| {stage} | {status} | {count} |")
    if summary.get("stage_first_failure_counts"):
        lines.extend(["", "## First Stage Failure", "", "| Stage Status | Rows |", "|---|---:|"])
        for status, count in sorted(summary["stage_first_failure_counts"].items()):
            lines.append(f"| {status} | {count} |")
    if summary.get("debug_stage_status_matrix"):
        lines.extend(["", "## Debug Stage Status Matrix", "", "| Stage | Status | Rows |", "|---|---|---:|"])
        for stage, counts in sorted(summary["debug_stage_status_matrix"].items()):
            if not isinstance(counts, dict):
                continue
            for status, count in sorted(counts.items()):
                lines.append(f"| {stage} | {status} | {count} |")
    if summary.get("static_similarity_component_average_percent"):
        lines.extend(["", "## Static Similarity Components", "", "| Component | Avg Similarity |", "|---|---:|"])
        for component, avg in sorted(summary["static_similarity_component_average_percent"].items()):
            lines.append(f"| {component} | {float(avg or 0.0):.3f}% |")
    static_gaps = summary.get("static_similarity_gap_totals")
    if isinstance(static_gaps, dict):
        lines.extend(["", "## Static Similarity Gaps", ""])
        lines.append(
            f"- Source features: {int(static_gaps.get('source_feature_total', 0) or 0)}, "
            f"decomp features: {int(static_gaps.get('decomp_feature_total', 0) or 0)}, "
            f"missing: {int(static_gaps.get('missing_feature_total', 0) or 0)} "
            f"({float(static_gaps.get('missing_feature_rate', 0.0) or 0.0):.3f}), "
            f"extra: {int(static_gaps.get('extra_feature_total', 0) or 0)} "
            f"({float(static_gaps.get('extra_feature_rate', 0.0) or 0.0):.3f})"
        )
        missing = static_gaps.get("top_missing_features") or []
        if missing:
            lines.extend(["", "| Top Missing Feature | Count |", "|---|---:|"])
            for item in missing[:10]:
                lines.append(f"| `{item.get('feature')}` | {item.get('count')} |")
        extra = static_gaps.get("top_extra_features") or []
        if extra:
            lines.extend(["", "| Top Extra Feature | Count |", "|---|---:|"])
            for item in extra[:10]:
                lines.append(f"| `{item.get('feature')}` | {item.get('count')} |")
    static_gap_rows = summary.get("static_gap_row_metrics")
    if isinstance(static_gap_rows, dict):
        lines.extend(["", "## Static Gap Row Metrics", ""])
        lines.append(
            f"- Rows with missing features: {static_gap_rows.get('missing_feature_row_count', 0)} "
            f"({float(static_gap_rows.get('missing_feature_row_rate', 0.0) or 0.0):.3f}), "
            f"rows with extra features: {static_gap_rows.get('extra_feature_row_count', 0)} "
            f"({float(static_gap_rows.get('extra_feature_row_rate', 0.0) or 0.0):.3f}), "
            f"zero static intersection rows: {static_gap_rows.get('zero_static_intersection_row_count', 0)} "
            f"({float(static_gap_rows.get('zero_static_intersection_row_rate', 0.0) or 0.0):.3f})"
        )
        component_missing = static_gap_rows.get("component_missing_row_counts")
        if isinstance(component_missing, dict) and component_missing:
            lines.extend(["", "| Component | Missing Rows |", "|---|---:|"])
            for component, count in sorted(component_missing.items()):
                lines.append(f"| {component} | {count} |")
    source_features = summary.get("source_feature_metrics")
    if isinstance(source_features, dict):
        source_dist = source_features.get("source_feature_total_distribution")
        decomp_dist = source_features.get("decomp_feature_total_distribution")
        union_dist = source_features.get("union_feature_total_distribution")
        if isinstance(source_dist, dict) and isinstance(decomp_dist, dict) and isinstance(union_dist, dict):
            lines.extend(["", "## Source Feature Metrics", ""])
            lines.append(
                f"- Source feature avg {float(source_dist.get('avg', 0.0) or 0.0):.6f}, "
                f"decomp feature avg {float(decomp_dist.get('avg', 0.0) or 0.0):.6f}, "
                f"union feature p95 {float(union_dist.get('p95', 0.0) or 0.0):.6f}"
            )
        component_source = source_features.get("component_source_feature_distributions")
        if isinstance(component_source, dict) and component_source:
            lines.extend(["", "| Component | Source Avg Features | Decomp Avg Features |", "|---|---:|---:|"])
            component_decomp = (
                source_features.get("component_decomp_feature_distributions")
                if isinstance(source_features.get("component_decomp_feature_distributions"), dict)
                else {}
            )
            for component, stats in sorted(component_source.items()):
                decomp_stats = component_decomp.get(component) if isinstance(component_decomp, dict) else {}
                lines.append(
                    f"| {component} | {float((stats or {}).get('avg', 0.0) or 0.0):.6f} | "
                    f"{float((decomp_stats or {}).get('avg', 0.0) or 0.0):.6f} |"
                )
    source_variants = summary.get("static_source_variant_metrics")
    if isinstance(source_variants, dict):
        counts = source_variants.get("variant_counts")
        delta_dist = source_variants.get("inline_expanded_static_score_delta_distribution")
        if isinstance(counts, dict) and counts:
            lines.extend(["", "## Static Source Variant Metrics", "", "| Variant | Rows |", "|---|---:|"])
            for variant, count in sorted(counts.items()):
                lines.append(f"| {variant} | {count} |")
        if isinstance(delta_dist, dict) and delta_dist.get("count"):
            lines.append(
                f"- Inline-expanded static score delta avg "
                f"{float(delta_dist.get('avg', 0.0) or 0.0):.6f}, "
                f"max {float(delta_dist.get('max', 0.0) or 0.0):.6f}"
            )
        hot_rows = source_variants.get("top_inline_expanded_static_rows") or []
        if hot_rows:
            lines.extend(["", "| Function | Direct Static | Inline Expanded | Delta |", "|---|---:|---:|---:|"])
            for row in hot_rows[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | "
                    f"{float(row.get('direct_static_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(row.get('inline_expanded_static_semantic_score_percent', 0.0) or 0.0):.3f}% | "
                    f"{float(row.get('static_score_delta_percent', 0.0) or 0.0):.3f}% |"
                )
    static_absence = summary.get("static_absence_penalty_metrics")
    if isinstance(static_absence, dict):
        lines.extend(["", "## Static Absence Penalty Metrics", ""])
        lines.append(
            f"- Source recall {float(static_absence.get('source_recall', 0.0) or 0.0):.6f}, "
            f"decomp precision {float(static_absence.get('decomp_precision', 0.0) or 0.0):.6f}, "
            f"union Jaccard {float(static_absence.get('union_jaccard', 0.0) or 0.0):.6f}"
        )
        lines.append(
            f"- Missing features {int(static_absence.get('missing_feature_total', 0) or 0)}, "
            f"extra features {int(static_absence.get('extra_feature_total', 0) or 0)}, "
            f"rows with no decomp features despite source "
            f"{int(static_absence.get('rows_with_no_decomp_features_despite_source', 0) or 0)}"
        )
    component_absence = summary.get("static_component_absence_matrix_metrics")
    if isinstance(component_absence, dict) and component_absence:
        lines.extend(["", "## Static Component Absence Matrix", ""])
        lines.extend(
            [
                "| Component | Observed Rows | Source Present | Decomp Present | Both Present | Source Only | Decomp Only | Zero Intersection |",
                "|---|---:|---:|---:|---:|---:|---:|---:|",
            ]
        )
        for component, metrics in sorted(component_absence.items()):
            if not isinstance(metrics, dict):
                continue
            lines.append(
                f"| {component} | {metrics.get('observed_row_count', 0)} | "
                f"{metrics.get('source_present_row_count', 0)} | "
                f"{metrics.get('decomp_present_row_count', 0)} | "
                f"{metrics.get('both_present_row_count', 0)} | "
                f"{metrics.get('source_only_row_count', 0)} | "
                f"{metrics.get('decomp_only_row_count', 0)} | "
                f"{metrics.get('zero_intersection_source_present_row_count', 0)} |"
            )
    component_pr = summary.get("static_component_precision_recall_metrics")
    if isinstance(component_pr, dict) and component_pr:
        lines.extend(["", "## Static Component Precision/Recall", "", "| Component | Precision | Recall | F1 | Source | Decomp | Intersection |", "|---|---:|---:|---:|---:|---:|---:|"])
        for component, metrics in sorted(component_pr.items()):
            if not isinstance(metrics, dict):
                continue
            lines.append(
                f"| {component} | {float(metrics.get('precision', 0.0) or 0.0):.3f} | "
                f"{float(metrics.get('recall', 0.0) or 0.0):.3f} | "
                f"{float(metrics.get('f1', 0.0) or 0.0):.3f} | "
                f"{float(metrics.get('source_feature_total', 0.0) or 0.0):.0f} | "
                f"{float(metrics.get('decomp_feature_total', 0.0) or 0.0):.0f} | "
                f"{float(metrics.get('intersection_feature_total', 0.0) or 0.0):.0f} |"
            )
    size_metrics = summary.get("source_decomp_size_metrics")
    if isinstance(size_metrics, dict):
        line_ratio = size_metrics.get("decomp_to_source_line_ratio_distribution")
        source_lines = size_metrics.get("source_body_line_count_distribution")
        decomp_lines = size_metrics.get("decomp_line_count_distribution")
        if isinstance(line_ratio, dict) and isinstance(source_lines, dict) and isinstance(decomp_lines, dict):
            lines.extend(["", "## Source/Decompiler Size Metrics", ""])
            lines.append(
                f"- Source lines avg {float(source_lines.get('avg', 0.0) or 0.0):.6f}, "
                f"decomp lines avg {float(decomp_lines.get('avg', 0.0) or 0.0):.6f}, "
                f"decomp/source line ratio p95 {float(line_ratio.get('p95', 0.0) or 0.0):.6f}"
            )
        hot_size_rows = size_metrics.get("top_decomp_to_source_line_ratio_rows") or []
        if hot_size_rows:
            lines.extend(["", "| Function | Address | Source Lines | Decomp Lines | Ratio | Behavior |", "|---|---|---:|---:|---:|---|"])
            for row in hot_size_rows[:8]:
                ratio = row.get("decomp_to_source_line_ratio")
                ratio_text = "n/a" if ratio is None else f"{float(ratio or 0.0):.6f}"
                lines.append(
                    f"| `{row.get('function_name')}` | `{row.get('address')}` | "
                    f"{row.get('source_body_line_count') or 0} | {row.get('decomp_line_count') or 0} | "
                    f"{ratio_text} | {row.get('behavior_status')} |"
                )
    behavior_cases = summary.get("behavior_case_metrics")
    if isinstance(behavior_cases, dict):
        lines.extend(["", "## Behavior Case Metrics", ""])
        lines.append(
            f"- Cases: pass {behavior_cases.get('case_pass_count', 0)}/"
            f"{behavior_cases.get('compared_case_count', behavior_cases.get('case_count', 0))} "
            f"({float(behavior_cases.get('case_pass_rate', 0.0) or 0.0):.3f}), "
            f"failed {behavior_cases.get('case_fail_count', 0)}, "
            f"partial mismatch rows {behavior_cases.get('partial_mismatch_row_count', 0)}, "
            f"partial progress rows {behavior_cases.get('partial_progress_row_count', 0)}"
        )
    behavior_timeouts = summary.get("behavior_timeout_progress_metrics")
    if isinstance(behavior_timeouts, dict) and behavior_timeouts.get("partial_timeout_row_count"):
        lines.extend(["", "## Behavior Timeout Progress Metrics", ""])
        lines.append(
            f"- Partial timeout rows {behavior_timeouts.get('partial_timeout_row_count', 0)}, "
            f"cases passed before timeout {behavior_timeouts.get('partial_timeout_case_pass_count', 0)}/"
            f"{behavior_timeouts.get('partial_timeout_compared_case_count', 0)} "
            f"({float(behavior_timeouts.get('partial_timeout_case_pass_rate', 0.0) or 0.0):.3f}), "
            f"missing candidate lines {behavior_timeouts.get('partial_timeout_missing_candidate_line_total', 0)}"
        )
    partial_progress = summary.get("behavior_partial_progress_metrics")
    if isinstance(partial_progress, dict) and partial_progress.get("row_count"):
        lines.extend(["", "## Behavior Partial Progress Metrics", ""])
        lines.append(
            f"- Partial progress rows {partial_progress.get('row_count', 0)}, "
            f"cases passed {partial_progress.get('case_pass_count', 0)}/"
            f"{partial_progress.get('compared_case_count', 0)}"
        )
        top_rows = partial_progress.get("top_rows")
        if isinstance(top_rows, list) and top_rows:
            lines.extend(["", "| Function | Address | Behavior | Passed Cases | First Mismatch | Score |", "|---|---|---|---:|---:|---:|"])
            for row in top_rows[:8]:
                if not isinstance(row, dict):
                    continue
                lines.append(
                    f"| `{row.get('function_name')}` | `{row.get('address')}` | {row.get('behavior_status')} | "
                    f"{row.get('case_pass_count', 0)}/{row.get('compared_case_count', 0)} | "
                    f"{row.get('first_mismatch_index')} | "
                    f"{float(row.get('semantic_score_percent', 0.0) or 0.0):.3f}% |"
                )
    behavior_support = summary.get("behavior_support_metrics")
    if isinstance(behavior_support, dict):
        lines.extend(["", "## Behavior Support Metrics", ""])
        lines.append(
            f"- Eligible rows {behavior_support.get('eligible_row_count', 0)}, "
            f"executed rows {behavior_support.get('executed_row_count', 0)}, "
            f"unsupported signature rows {behavior_support.get('unsupported_signature_row_count', 0)}"
        )
        case_sources = behavior_support.get("case_source_counts")
        if isinstance(case_sources, dict) and case_sources:
            lines.extend(["", "| Case Source | Rows |", "|---|---:|"])
            for source, count in sorted(case_sources.items()):
                lines.append(f"| {source} | {count} |")
        unsupported_reasons = behavior_support.get("unsupported_reason_counts")
        if isinstance(unsupported_reasons, dict) and unsupported_reasons:
            lines.extend(["", "| Unsupported Reason | Rows |", "|---|---:|"])
            for reason, count in sorted(unsupported_reasons.items()):
                lines.append(f"| `{reason}` | {count} |")
    behavior_denominators = summary.get("behavior_denominator_metrics")
    if isinstance(behavior_denominators, dict):
        lines.extend(["", "## Behavior Denominator Metrics", "", "| Metric | Value |", "|---|---:|"])
        for key, value in sorted(behavior_denominators.items()):
            lines.append(f"| {key} | {value} |")
    behavior_mismatches = summary.get("behavior_mismatch_metrics")
    if isinstance(behavior_mismatches, dict) and behavior_mismatches.get("mismatch_row_count"):
        lines.extend(["", "## Behavior Mismatch Metrics", ""])
        lines.append(f"- Mismatch rows: {behavior_mismatches.get('mismatch_row_count', 0)}")
        kinds = behavior_mismatches.get("mismatch_kind_counts")
        if isinstance(kinds, dict) and kinds:
            lines.extend(["", "| Kind | Rows |", "|---|---:|"])
            for kind, count in sorted(kinds.items()):
                lines.append(f"| {kind} | {count} |")
    behavior_distance = summary.get("behavior_distance_metrics")
    if isinstance(behavior_distance, dict):
        case_pass_rate = behavior_distance.get("case_pass_rate_distribution")
        if isinstance(case_pass_rate, dict) and case_pass_rate.get("count"):
            lines.extend(["", "## Behavior Distance Metrics", ""])
            lines.append(
                f"- Case pass rate avg {float(case_pass_rate.get('avg', 0.0) or 0.0):.6f}, "
                f"p50 {float(case_pass_rate.get('p50', 0.0) or 0.0):.6f}, "
                f"p90 {float(case_pass_rate.get('p90', 0.0) or 0.0):.6f}"
            )
            lines.append(
                f"- Missing candidate lines: {behavior_distance.get('missing_candidate_line_total', 0)}, "
                f"extra candidate lines: {behavior_distance.get('extra_candidate_line_total', 0)}"
            )
    if summary.get("harness_cost_metrics"):
        costs = summary["harness_cost_metrics"]
        lines.extend(["", "## Harness Cost Metrics", "", "| Metric | Seconds |", "|---|---:|"])
        for key, value in sorted(costs.items()):
            lines.append(f"| {key} | {float(value or 0.0):.6f} |")
    cost_hot_rows = summary.get("cost_hot_rows")
    if isinstance(cost_hot_rows, dict):
        top_decompile = cost_hot_rows.get("top_decompile_wall_rows") or []
        top_behavior = cost_hot_rows.get("top_behavior_wall_rows") or []
        if top_decompile:
            lines.extend(["", "## Cost Hot Rows", "", "| Function | Address | Decompile Sec | Behavior Wall Sec | Behavior |", "|---|---|---:|---:|---|"])
            for row in top_decompile[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | `{row.get('address')}` | "
                    f"{float(row.get('decompile_sec') or 0.0):.6f} | "
                    f"{float(row.get('behavior_wall_sec') or 0.0):.6f} | "
                    f"{row.get('behavior_status')} |"
                )
        if top_behavior:
            lines.extend(["", "### Behavior Wall Hot Rows", "", "| Function | Address | Behavior Wall Sec | Decompile Sec | Behavior |", "|---|---|---:|---:|---|"])
            for row in top_behavior[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | `{row.get('address')}` | "
                    f"{float(row.get('behavior_wall_sec') or 0.0):.6f} | "
                    f"{float(row.get('decompile_sec') or 0.0):.6f} | "
                    f"{row.get('behavior_status')} |"
                )
    debug_coverage = summary.get("debug_coverage_metrics")
    if isinstance(debug_coverage, dict):
        lines.extend(["", "## Debug Coverage Metrics", ""])
        lines.append(
            f"- Debug decomp rows: {debug_coverage.get('debug_decomp_rows', 0)} "
            f"({float(debug_coverage.get('debug_decomp_rate_mapped_denominator', 0.0) or 0.0):.3f} mapped denominator), "
            f"stage status rows: {debug_coverage.get('debug_stage_status_rows', 0)} "
            f"({float(debug_coverage.get('debug_stage_status_rate_mapped_denominator', 0.0) or 0.0):.3f} mapped denominator)"
        )
    pipeline_stage_metrics = summary.get("pipeline_stage_metrics")
    if isinstance(pipeline_stage_metrics, dict) and pipeline_stage_metrics:
        lines.extend(["", "## Pipeline Stage Metrics", "", "| Stage | Rows | OK | Non-OK | Missing | OK Rate |", "|---|---:|---:|---:|---:|---:|"])
        for stage, metrics in sorted(pipeline_stage_metrics.items()):
            if not isinstance(metrics, dict):
                continue
            lines.append(
                f"| {stage} | {metrics.get('row_count', 0)} | {metrics.get('ok_count', 0)} | "
                f"{metrics.get('non_ok_count', 0)} | {metrics.get('missing_count', 0)} | "
                f"{float(metrics.get('ok_rate', 0.0) or 0.0):.3f} |"
            )
    debug_pipeline_numeric = summary.get("debug_pipeline_numeric_metrics")
    if isinstance(debug_pipeline_numeric, dict) and debug_pipeline_numeric:
        lines.extend(["", "## Debug Pipeline Numeric Metrics", "", "| Metric | Rows | Avg | P95 | Max |", "|---|---:|---:|---:|---:|"])
        for metric, stats in sorted(debug_pipeline_numeric.items()):
            if not isinstance(stats, dict):
                continue
            lines.append(
                f"| {metric} | {stats.get('count', 0)} | "
                f"{float(stats.get('avg', 0.0) or 0.0):.6f} | "
                f"{float(stats.get('p95', 0.0) or 0.0):.6f} | "
                f"{float(stats.get('max', 0.0) or 0.0):.6f} |"
            )
    nir_stats = summary.get("nir_build_stats_metrics")
    if isinstance(nir_stats, dict) and nir_stats.get("stats_row_count"):
        lines.extend(["", "## NIR Build Stats Metrics", ""])
        lines.append(
            f"- Stats rows: {nir_stats.get('stats_row_count', 0)} "
            f"({float(nir_stats.get('stats_row_rate_mapped_denominator', 0.0) or 0.0):.3f} mapped denominator)"
        )
        debt_totals = nir_stats.get("debt_metric_totals")
        if isinstance(debt_totals, dict) and debt_totals:
            lines.extend(["", "| Debt Metric | Total | Nonzero Rows |", "|---|---:|---:|"])
            nonzero_rows = nir_stats.get("nonzero_row_counts") if isinstance(nir_stats.get("nonzero_row_counts"), dict) else {}
            for metric, total_value in sorted(debt_totals.items()):
                lines.append(f"| {metric} | {total_value} | {nonzero_rows.get(metric, 0)} |")
        top_debt_rows = nir_stats.get("top_debt_rows") or []
        if top_debt_rows:
            lines.extend(["", "### NIR Debt Hot Rows", "", "| Function | Address | Debt Total | Behavior | First Failure |", "|---|---|---:|---|---|"])
            for row in top_debt_rows[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | `{row.get('address')}` | "
                    f"{float(row.get('debt_metric_total') or 0.0):.6f} | "
                    f"{row.get('behavior_status')} | {row.get('stage_first_failure')} |"
                )
    debt_correlation = summary.get("nir_debt_correlation_metrics")
    if isinstance(debt_correlation, dict) and debt_correlation.get("stats_row_count"):
        debt_scores = debt_correlation.get("score_distribution_debt_rows")
        no_debt_scores = debt_correlation.get("score_distribution_no_debt_rows")
        lines.extend(["", "## NIR Debt Correlation Metrics", ""])
        lines.append(
            f"- Debt rows: {debt_correlation.get('debt_row_count', 0)}/"
            f"{debt_correlation.get('stats_row_count', 0)} "
            f"({float(debt_correlation.get('debt_row_rate_stats_denominator', 0.0) or 0.0):.3f})"
        )
        if isinstance(debt_scores, dict) and isinstance(no_debt_scores, dict):
            lines.append(
                f"- Avg score with debt {float(debt_scores.get('avg', 0.0) or 0.0):.6f}, "
                f"without debt {float(no_debt_scores.get('avg', 0.0) or 0.0):.6f}"
            )
    if summary.get("decomp_cache_status_counts"):
        lines.extend(["", "## Decompile Cache Status", "", "| Status | Rows |", "|---|---:|"])
        for status, count in sorted(summary["decomp_cache_status_counts"].items()):
            lines.append(f"| {status} | {count} |")
    if summary.get("behavior_cache_status_counts"):
        lines.extend(["", "## Behavior Cache Status", "", "| Status | Hits |", "|---|---:|"])
        for status, count in sorted(summary["behavior_cache_status_counts"].items()):
            lines.append(f"| {status} | {count} |")
    if summary.get("decomp_failure_counts"):
        lines.extend(["", "## Decompile Failures", "", "| Failure | Rows |", "|---|---:|"])
        for failure, count in sorted(summary["decomp_failure_counts"].items()):
            lines.append(f"| {failure} | {count} |")
    if summary.get("debug_owner_bucket_counts"):
        lines.extend(["", "## Debug Owner Buckets", "", "| Bucket | Rows |", "|---|---:|"])
        for bucket, count in sorted(summary["debug_owner_bucket_counts"].items()):
            lines.append(f"| {bucket} | {count} |")
    if summary.get("debug_quality_evidence_totals"):
        lines.extend(["", "## Debug Quality Evidence", "", "| Metric | Total |", "|---|---:|"])
        for metric, total_value in sorted(summary["debug_quality_evidence_totals"].items()):
            lines.append(f"| {metric} | {total_value} |")
    if summary.get("debug_quality_evidence_nonzero_rows"):
        lines.extend(["", "## Debug Quality Evidence Nonzero Rows", "", "| Metric | Rows |", "|---|---:|"])
        for metric, count in sorted(summary["debug_quality_evidence_nonzero_rows"].items()):
            lines.append(f"| {metric} | {count} |")
    if summary.get("debug_template_source_totals"):
        lines.extend(["", "## Debug SLEIGH Template Sources", "", "| Source | Total |", "|---|---:|"])
        for source, total_value in sorted(summary["debug_template_source_totals"].items()):
            lines.append(f"| {source} | {total_value} |")
    gate = summary.get("sleigh_template_source_gate")
    if isinstance(gate, dict):
        lines.extend(["", "## SLEIGH Template Source Gate", ""])
        lines.append(f"- Status: `{gate.get('status')}`")
        lines.append(f"- Required source: `{gate.get('required_source')}`")
        if gate.get("mapped_row_count") is not None:
            lines.append(
                f"- Rows: mapped `{gate.get('mapped_row_count')}` / total `{gate.get('row_count')}`"
            )
        if gate.get("unmapped_row_count"):
            lines.append(
                f"- Unmapped rows ignored by SLEIGH gate: `{gate.get('unmapped_row_count')}`"
            )
        for failure in gate.get("failures") or []:
            lines.append(f"- Failure: {failure}")
    triage_rows = summary.get("triage_priority_rows") or []
    if triage_rows:
        lines.extend(["", "## Triage Priority Rows", "", "| Function | Score | Behavior | Stage | Missing | Artifact |", "|---|---:|---|---|---:|---|"])
        for row in triage_rows[:12]:
            artifact = row.get("behavior_artifact_dir") or row.get("debug_decomp_bundle_path") or ""
            lines.append(
                f"| `{row.get('function_name')}` | "
                f"{float(row.get('semantic_score_percent', 0.0) or 0.0):.3f}% | "
                f"{row.get('behavior_status')} | {row.get('stage_first_failure')} | "
                f"{row.get('missing_feature_total') or 0} | `{artifact}` |"
            )
    comparison = summary.get("comparison")
    if isinstance(comparison, dict):
        outcome = summary.get("comparison_outcome") if isinstance(summary.get("comparison_outcome"), dict) else {}
        weighted = comparison.get("metric_deltas", {}).get("weighted_semantic_similarity_percent", {})
        delta = weighted.get("delta")
        delta_text = "n/a" if delta is None else f"{delta:+.3f}%"
        lines.extend(
            [
                "",
                "## Baseline Comparison",
                "",
                f"- Baseline: `{comparison.get('baseline_summary_path')}`",
                f"- Outcome: {outcome.get('headline', 'n/a')}",
                f"- Weighted semantic similarity delta: {delta_text}",
                f"- Improved rows: {comparison.get('improved_row_count', 0)}",
                f"- Regressed rows: {comparison.get('regressed_row_count', 0)}",
                f"- Behavior improved rows: {comparison.get('behavior_improved_row_count', 0)}",
                f"- Behavior regressed rows: {comparison.get('behavior_regressed_row_count', 0)}",
                f"- New rows: {comparison.get('new_row_count', 0)}",
                f"- Missing rows: {comparison.get('missing_row_count', 0)}",
            ]
        )
        severity = comparison.get("regression_severity") if isinstance(comparison.get("regression_severity"), dict) else {}
        if severity:
            lines.extend(
                [
                    f"- Negative score delta sum: {float(severity.get('score_delta_sum_negative', 0.0) or 0.0):+.6f}",
                    f"- New zero-score rows: {severity.get('new_zero_score_rows', 0)}",
                    f"- New unmapped rows: {severity.get('new_unmapped_rows', 0)}",
                    f"- New behavior-fail rows: {severity.get('new_behavior_fail_rows', 0)}",
                ]
            )
        top_improvements = comparison.get("top_improvements") or []
        if top_improvements:
            lines.extend(["", "### Top Improvements", "", "| Function | Delta | Baseline | Current | Behavior |", "|---|---:|---:|---:|---|"])
            for row in top_improvements[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | {row.get('delta_percent', 0.0):+.3f}% | "
                    f"{row.get('baseline_score_percent', 0.0):.3f}% | {row.get('current_score_percent', 0.0):.3f}% | "
                    f"{row.get('baseline_behavior')} -> {row.get('current_behavior')} |"
                )
        top_regressions = comparison.get("top_regressions") or []
        if top_regressions:
            lines.extend(["", "### Top Regressions", "", "| Function | Delta | Baseline | Current | Behavior |", "|---|---:|---:|---:|---|"])
            for row in top_regressions[:8]:
                lines.append(
                    f"| `{row.get('function_name')}` | {row.get('delta_percent', 0.0):+.3f}% | "
                    f"{row.get('baseline_score_percent', 0.0):.3f}% | {row.get('current_score_percent', 0.0):.3f}% | "
                    f"{row.get('baseline_behavior')} -> {row.get('current_behavior')} |"
                )
        top_deltas = comparison.get("top_row_deltas") or []
        if top_deltas:
            lines.extend(["", "| Function | Delta | Baseline | Current | Behavior |", "|---|---:|---:|---:|---|"])
            for row in top_deltas[:10]:
                lines.append(
                    f"| `{row.get('function_name')}` | {row.get('delta_percent', 0.0):+.3f}% | "
                    f"{row.get('baseline_score_percent', 0.0):.3f}% | {row.get('current_score_percent', 0.0):.3f}% | "
                    f"{row.get('baseline_behavior')} -> {row.get('current_behavior')} |"
                )
    debug_triage = summary.get("debug_triage") or []
    if debug_triage:
        lines.extend(["", "## Materialized Debug Triage", "", "| Function | Score | Debug Bundle | Disasm | Xrefs | Facts |", "|---|---:|---|---|---|---|"])
        for row in debug_triage[:12]:
            lines.append(
                f"| `{row.get('function_name')}` | {row.get('semantic_score_percent', 0.0):.3f}% | "
                f"`{row.get('debug_decomp_bundle_path')}` | `{row.get('disasm_capture_path')}` | "
                f"`{row.get('xrefs_capture_path')}` | `{row.get('function_facts_summary_path')}` |"
            )
    regression_debug_triage = summary.get("regression_debug_triage") or []
    if regression_debug_triage:
        lines.extend(["", "## Regression Debug Triage", "", "| Function | Delta | Score | Debug Bundle | Disasm | Xrefs | Facts |", "|---|---:|---:|---|---|---|---|"])
        for row in regression_debug_triage[:12]:
            regression = row.get("baseline_regression") if isinstance(row.get("baseline_regression"), dict) else {}
            delta = regression.get("delta_percent")
            delta_text = "n/a" if delta is None else f"{delta:+.3f}%"
            lines.append(
                f"| `{row.get('function_name')}` | {delta_text} | {row.get('semantic_score_percent', 0.0):.3f}% | "
                f"`{row.get('debug_decomp_bundle_path')}` | `{row.get('disasm_capture_path')}` | "
                f"`{row.get('xrefs_capture_path')}` | `{row.get('function_facts_summary_path')}` |"
            )
    debug_commands = summary.get("debug_repro_commands") or []
    if debug_commands:
        lines.extend(["", "## Debug Repro Commands", ""])
        for row in debug_commands[:8]:
            lines.append(
                f"- `{row.get('entry_id')}` `{row.get('function_name')}` "
                f"({row.get('semantic_score_percent', 0.0):.3f}%, {row.get('behavior_status')}):"
            )
            lines.append("")
            lines.append("  ```bash")
            lines.append(f"  {row.get('debug_decomp_command')}")
            lines.append("  ```")
            if row.get("disasm_function_command"):
                lines.append("  ```bash")
                lines.append(f"  {row.get('disasm_function_command')}")
                lines.append("  ```")
            if row.get("xrefs_function_command"):
                lines.append("  ```bash")
                lines.append(f"  {row.get('xrefs_function_command')}")
                lines.append("  ```")
            if row.get("preview_candidate_command"):
                lines.append("  ```bash")
                lines.append(f"  {row.get('preview_candidate_command')}")
                lines.append("  ```")
            if row.get("function_facts_command"):
                lines.append("  ```bash")
                lines.append(f"  {row.get('function_facts_command')}")
                lines.append("  ```")
            if row.get("behavior_artifact_dir"):
                lines.append(f"  Behavior artifacts: `{row.get('behavior_artifact_dir')}`")
    failing = [row for row in rows if row.get("semantic_score", 0.0) < 1.0][:20]
    if failing:
        lines.extend(["", "## First Non-Perfect Rows", ""])
        for row in failing:
            behavior = row.get("behavior", {})
            lines.append(
                f"- `{row['entry_id']}` `{row['function_name']}`: score={row['semantic_score']:.3f}, "
                f"similarity={row['semantic_score_percent']:.3f}%, "
                f"map={row['mapping_status']}, behavior={behavior.get('status')}"
            )
            if behavior.get("artifact_dir"):
                lines.append(f"  behavior artifacts: `{behavior.get('artifact_dir')}`")
    lines.append("")
    return "\n".join(lines)


STAGE_FAILURE_ORDER = ["load", "decode", "raw_pcode", "nir_build", "normalize", "structuring", "render"]


def stage_first_failure(debug_decomp: Any) -> str | None:
    if not isinstance(debug_decomp, dict):
        return None
    stage_status = debug_decomp.get("stage_status")
    if not isinstance(stage_status, dict):
        return None
    for stage in STAGE_FAILURE_ORDER:
        status = stage_status.get(stage)
        if status not in {None, "ok"}:
            return f"{stage}:{status}"
    return None


def zero_credit_reason(
    mapping_status: str,
    decomp: dict[str, Any],
    behavior: dict[str, Any],
    static_score: float,
    semantic_score: float,
) -> str | None:
    if semantic_score > 0.0:
        return None
    if mapping_status != "matched":
        return mapping_status
    if not decomp.get("success"):
        return f"decomp:{decomp.get('failure_kind', 'unknown')}"
    behavior_status = behavior.get("status", "unknown")
    if behavior_status not in {"unsupported_signature", "pass"}:
        return f"behavior:{behavior_status}"
    if static_score == 0.0:
        return "static_zero"
    return "weighted_zero"


def row_zero_credit_reason(row: dict[str, Any]) -> str:
    explicit = row.get("zero_credit_reason")
    if explicit:
        return str(explicit)
    mapping_status = str(row.get("mapping_status") or "unknown")
    if mapping_status != "matched":
        return mapping_status
    if not row.get("decomp_success"):
        return f"decomp:{row.get('decomp_failure_kind', 'unknown')}"
    behavior_status = row.get("behavior", {}).get("status", "unknown")
    if behavior_status not in {"unsupported_signature", "pass"}:
        return f"behavior:{behavior_status}"
    if float(row.get("static_semantic_score", 0.0) or 0.0) == 0.0:
        return "static_zero"
    return "weighted_zero"


def row_triage_priority(row: dict[str, Any]) -> tuple[int, float, int, str]:
    score = float(row.get("semantic_score", 0.0) or 0.0)
    behavior = row.get("behavior") if isinstance(row.get("behavior"), dict) else {}
    static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
    missing_total = int(static_gaps.get("missing_feature_total") or 0)
    if row.get("mapping_status") != "matched":
        severity = 0
    elif not row.get("decomp_success"):
        severity = 1
    elif behavior.get("status") not in {"pass", "unsupported_signature"}:
        severity = 2
    elif missing_total > 0:
        severity = 3
    else:
        severity = 4
    return (severity, score, -missing_total, str(row.get("function_name") or ""))


def triage_row_summary(row: dict[str, Any]) -> dict[str, Any]:
    behavior = row.get("behavior") if isinstance(row.get("behavior"), dict) else {}
    static_gaps = row.get("static_similarity_gaps") if isinstance(row.get("static_similarity_gaps"), dict) else {}
    debug_decomp = row.get("debug_decomp") if isinstance(row.get("debug_decomp"), dict) else {}
    return {
        "entry_id": row.get("entry_id"),
        "function_name": row.get("function_name"),
        "address": row.get("address"),
        "semantic_score_percent": row.get("semantic_score_percent"),
        "static_semantic_score_percent": row.get("static_semantic_score_percent"),
        "static_similarity_source_variant": row.get("static_similarity_source_variant"),
        "static_semantic_score_direct_percent": percent(float(row.get("static_semantic_score_direct", 0.0) or 0.0)),
        "static_semantic_score_inline_expanded_percent": percent(
            float(row.get("static_semantic_score_inline_expanded", 0.0) or 0.0)
        ),
        "source_static_feature_count_direct": row.get("source_static_feature_count_direct"),
        "source_static_feature_count_inline_expanded": row.get("source_static_feature_count_inline_expanded"),
        "mapping_status": row.get("mapping_status"),
        "decomp_success": bool(row.get("decomp_success")),
        "decomp_failure_kind": row.get("decomp_failure_kind"),
        "behavior_status": behavior.get("status"),
        "case_pass_count": behavior.get("case_pass_count"),
        "case_fail_count": behavior.get("case_fail_count"),
        "first_mismatch_index": behavior.get("first_mismatch_index"),
        "zero_credit_reason": row_zero_credit_reason(row)
        if float(row.get("semantic_score", 0.0) or 0.0) == 0.0
        else row.get("zero_credit_reason"),
        "stage_first_failure": row.get("stage_first_failure"),
        "debug_owner_buckets": debug_decomp.get("owner_buckets") if isinstance(debug_decomp, dict) else None,
        "missing_feature_total": static_gaps.get("missing_feature_total"),
        "extra_feature_total": static_gaps.get("extra_feature_total"),
        "top_missing_features": (static_gaps.get("top_missing_features") or [])[:5],
        "top_extra_features": (static_gaps.get("top_extra_features") or [])[:5],
        "debug_decomp_bundle_path": row.get("debug_decomp_bundle_path"),
        "behavior_artifact_dir": behavior.get("artifact_dir"),
    }


def row_for_function(
    entry: BenchmarkEntry,
    func: SourceFunction,
    source_functions_by_name: dict[str, SourceFunction],
    fission_funcs: list[FissionFunction],
    fission_error: str | None,
    fission_bin: Path,
    timeout_sec: int,
    host_execution: dict[str, Any],
    decomp_cache: dict[str, dict[str, Any]],
    decomp_cache_lock: threading.Lock,
    decomp_cache_stats: Counter[str],
    behavior_cache: dict[str, dict[str, Any]] | None,
    behavior_cache_lock: threading.Lock | None,
    behavior_cache_stats: Counter[str],
    include_debug_decomp: bool,
    output_dir: Path | None = None,
) -> dict[str, Any]:
    source_fp_direct = code_fingerprint(func.body, func)
    source_fp_inline_expanded = inline_expanded_source_fingerprint(func, source_functions_by_name)
    mapping_status, matched, candidates = match_function(func, fission_funcs) if not fission_error else ("list_failed", None, [])
    decomp: dict[str, Any] = {"success": False, "failure_kind": mapping_status}
    if matched is not None:
        debug_decomp_bundle_path = (
            debug_bundle_path_for_parts(output_dir, entry.id, func.name, matched.address)
            if include_debug_decomp and output_dir is not None
            else None
        )
        decomp = run_fission_decomp_cached(
            entry.binary_path,
            matched.address,
            fission_bin,
            timeout_sec,
            include_debug_decomp,
            debug_decomp_bundle_path,
            decomp_cache,
            decomp_cache_lock,
            decomp_cache_stats,
        )
    else:
        decomp["decomp_cache_status"] = "not_requested"
    decomp_code = decomp.get("code") if decomp.get("success") else None
    source_body_lines = func.body.splitlines()
    decomp_lines = decomp_code.splitlines() if decomp_code else []
    decomp_signature = rendered_signature_kinds(decomp_code) if decomp_code else None
    decomp_return_kind = decomp_signature[0] if decomp_signature is not None else None
    decomp_param_kinds = decomp_signature[1] if decomp_signature is not None else None
    decomp_fp = code_fingerprint(decomp_code or "") if decomp_code else Counter()
    static_score_direct = multiset_jaccard(source_fp_direct, decomp_fp) if decomp_code else 0.0
    static_score_inline_expanded = (
        multiset_jaccard(source_fp_inline_expanded, decomp_fp)
        if decomp_code and source_fp_inline_expanded != source_fp_direct
        else static_score_direct
    )
    if static_score_inline_expanded > static_score_direct:
        source_fp = source_fp_inline_expanded
        static_score = static_score_inline_expanded
        static_source_variant = "same_source_inline_expanded"
    else:
        source_fp = source_fp_direct
        static_score = static_score_direct
        static_source_variant = "direct_source"
    static_components = static_similarity_components(source_fp, decomp_fp) if decomp_code else {
        name: 0.0 for name in STATIC_SIMILARITY_COMPONENTS
    }
    static_gaps = multiset_gap_details(source_fp, decomp_fp) if decomp_code else multiset_gap_details(source_fp, Counter())
    static_gap_components = (
        static_similarity_gap_components(source_fp, decomp_fp)
        if decomp_code
        else static_similarity_gap_components(source_fp, Counter())
    )
    behavior = run_behavior_check(
        entry,
        func,
        decomp_code,
        timeout_sec,
        host_execution,
        behavior_cache,
        behavior_cache_lock,
        behavior_cache_stats,
        output_dir=output_dir,
        address=matched.address if matched else None,
    )
    semantic_score = round(0.65 * float(behavior.get("score", 0.0)) + 0.35 * static_score, 6)
    debug_decomp = decomp.get("debug_decomp")
    return {
        "entry_id": entry.id,
        "binary_path": rel(entry.binary_path),
        "binary_arch": infer_entry_arch(entry),
        "source_path": rel(entry.source_path),
        "language": entry.language,
        "tags": entry.tags,
        "function_name": func.name,
        "source_line": func.line,
        "source_signature": func.signature,
        "source_is_static": func.is_static,
        "source_return_kind": func.return_kind,
        "source_param_kinds": func.param_kinds,
        "source_param_shape": source_param_shape(func.param_kinds),
        "source_body_line_count": len(source_body_lines),
        "source_body_byte_count": len(func.body.encode("utf-8")),
        "source_static_feature_count": sum(source_fp.values()),
        "source_static_feature_count_direct": sum(source_fp_direct.values()),
        "source_static_feature_count_inline_expanded": sum(source_fp_inline_expanded.values()),
        "address": matched.address if matched else None,
        "fission_name": matched.name if matched else None,
        "mapping_status": mapping_status,
        "mapping_candidates": candidates,
        "list_error": fission_error,
        "decomp_success": bool(decomp.get("success")),
        "decomp_failure_kind": decomp.get("failure_kind"),
        "decomp_failure_detail": decomp.get("failure_detail"),
        "engine_used": decomp.get("engine_used"),
        "preview_build_stats": decomp.get("preview_build_stats"),
        "debug_decomp_bundle_path": decomp.get("debug_decomp_bundle_path"),
        "debug_decomp": debug_decomp,
        "decomp_cache_status": decomp.get("decomp_cache_status", "not_requested"),
        "decomp_wall_sec": decomp.get("wall_sec"),
        "decomp_line_count": len(decomp_lines),
        "decomp_byte_count": len(decomp_code.encode("utf-8")) if decomp_code else 0,
        "decomp_return_kind": decomp_return_kind,
        "decomp_param_kinds": decomp_param_kinds,
        "decomp_param_shape": source_param_shape(decomp_param_kinds or []) if decomp_param_kinds is not None else None,
        "decomp_static_feature_count": sum(decomp_fp.values()),
        "static_semantic_score": static_score,
        "static_semantic_score_percent": percent(static_score),
        "static_semantic_score_direct": static_score_direct,
        "static_semantic_score_inline_expanded": static_score_inline_expanded,
        "static_similarity_source_variant": static_source_variant,
        "static_similarity_components": static_components,
        "static_similarity_gaps": static_gaps,
        "static_similarity_gap_components": static_gap_components,
        "behavior": behavior,
        "semantic_score": semantic_score,
        "semantic_score_percent": percent(semantic_score),
        "zero_credit_reason": zero_credit_reason(mapping_status, decomp, behavior, static_score, semantic_score),
        "stage_first_failure": stage_first_failure(debug_decomp),
    }


def run_benchmark(args: argparse.Namespace) -> int:
    start = time.perf_counter()
    created_at = utc_now()
    manifest_path = resolve_path(args.manifest)
    manifest = load_json(manifest_path)
    manifest_name = manifest.get("name", manifest_path.stem)
    run_id = f"{sanitize_id(manifest_name)}-{utc_timestamp_slug(created_at)}"
    entries = discover_source_entries(manifest)
    entries = filter_entries(entries, args.entry_id, args.tag)
    if args.limit_binaries is not None:
        entries = entries[: args.limit_binaries]

    output_dir = resolve_path(args.output_dir) if args.output_dir else DEFAULT_ARTIFACT_ROOT / run_id
    output_dir.mkdir(parents=True, exist_ok=True)
    fission_bin = resolve_path(args.fission_bin)
    host_execution = c_host_execution_probe(args.timeout_sec)

    rows: list[dict[str, Any]] = []
    jobs = max(1, int(args.jobs or 1))
    decomp_cache_path = None if args.no_decomp_cache else resolve_path(args.decomp_cache_file)
    decomp_cache: dict[str, dict[str, Any]] = load_decomp_cache(decomp_cache_path)
    decomp_cache_lock = threading.Lock()
    decomp_cache_stats: Counter[str] = Counter()
    decomp_cache_initial_entry_count = len(decomp_cache)
    list_cache_path = None if args.no_list_cache else resolve_path(args.list_cache_file)
    list_cache: dict[str, dict[str, Any]] = load_list_cache(list_cache_path)
    list_cache_stats: Counter[str] = Counter()
    list_cache_initial_entry_count = len(list_cache)
    behavior_cache_path = None if args.no_behavior_cache else resolve_path(args.behavior_cache_file)
    behavior_cache: dict[str, dict[str, Any]] | None = load_behavior_cache(behavior_cache_path)
    behavior_cache_lock = threading.Lock()
    behavior_cache_stats: Counter[str] = Counter()
    behavior_cache_initial_entry_count = len(behavior_cache or {})
    source_row_selection_entries: list[dict[str, Any]] = []
    suppressed_static_inline_rows: list[dict[str, Any]] = []
    for entry in entries:
        all_source_functions = extract_source_functions(entry.source_path, entry.language)
        source_functions = filter_source_functions(all_source_functions, args.function_name)
        source_functions_by_name = {
            normalize_name(func.name): func
            for func in all_source_functions
        }
        fission_funcs, fission_error = run_fission_list_cached(
            entry.binary_path,
            fission_bin,
            args.timeout_sec,
            list_cache,
            list_cache_stats,
        )
        source_functions, suppressed_for_entry = filter_inlined_static_source_functions(
            source_functions,
            all_source_functions,
            fission_funcs,
            explicit_function_filter=bool(args.function_name),
            fission_error=fission_error,
        )
        for suppressed in suppressed_for_entry:
            suppressed_static_inline_rows.append(
                {
                    "entry_id": entry.id,
                    "source_path": rel(entry.source_path),
                    "binary_path": rel(entry.binary_path),
                    **suppressed,
                }
            )
        source_functions = select_source_functions(
            source_functions,
            fission_funcs,
            args.limit_functions,
            fission_error,
        )
        source_row_selection_entries.append(
            {
                "entry_id": entry.id,
                "source_path": rel(entry.source_path),
                "binary_path": rel(entry.binary_path),
                "extracted_source_function_count": len(all_source_functions),
                "extracted_static_source_function_count": sum(1 for func in all_source_functions if func.is_static),
                "filtered_source_function_count": len(filter_source_functions(all_source_functions, args.function_name)),
                "suppressed_static_inline_helper_count": len(suppressed_for_entry),
                "selected_source_function_count": len(source_functions),
                "listed_binary_function_count": len(fission_funcs),
                "list_error": fission_error,
            }
        )
        prewarm_decomp_cache_for_entry(
            entry,
            source_functions,
            fission_funcs,
            fission_error,
            fission_bin,
            args.timeout_sec,
            args.include_debug_decomp,
            output_dir,
            decomp_cache,
            decomp_cache_lock,
            decomp_cache_stats,
        )
        if jobs == 1 or len(source_functions) <= 1:
            for func in source_functions:
                rows.append(
                    row_for_function(
                        entry,
                        func,
                        source_functions_by_name,
                        fission_funcs,
                        fission_error,
                        fission_bin,
                        args.timeout_sec,
                        host_execution,
                        decomp_cache,
                        decomp_cache_lock,
                        decomp_cache_stats,
                        behavior_cache,
                        behavior_cache_lock,
                        behavior_cache_stats,
                        args.include_debug_decomp,
                        output_dir,
                    )
                )
            continue

        entry_rows: list[tuple[int, dict[str, Any]]] = []
        with ThreadPoolExecutor(max_workers=jobs) as executor:
            futures = {
                executor.submit(
                    row_for_function,
                    entry,
                    func,
                    source_functions_by_name,
                    fission_funcs,
                    fission_error,
                    fission_bin,
                    args.timeout_sec,
                    host_execution,
                    decomp_cache,
                    decomp_cache_lock,
                    decomp_cache_stats,
                    behavior_cache,
                    behavior_cache_lock,
                    behavior_cache_stats,
                    args.include_debug_decomp,
                    output_dir,
                ): index
                for index, func in enumerate(source_functions)
            }
            for future in as_completed(futures):
                entry_rows.append((futures[future], future.result()))
        rows.extend(row for _index, row in sorted(entry_rows, key=lambda item: item[0]))

    attach_debug_repro_commands(rows, fission_bin, output_dir)
    summary = summarize(rows, manifest_name, entries)
    summary["run_id"] = run_id
    summary["created_at_utc"] = utc_isoformat(created_at)
    summary["artifact_dir"] = rel(output_dir)
    summary["jobs"] = jobs
    summary["decomp_cache_file"] = rel(decomp_cache_path) if decomp_cache_path is not None else None
    summary["list_cache_file"] = rel(list_cache_path) if list_cache_path is not None else None
    summary["behavior_cache_file"] = rel(behavior_cache_path) if behavior_cache_path is not None else None
    summary["history_file"] = rel(DEFAULT_HISTORY_FILE)
    summary["latest_index_file"] = rel(DEFAULT_LATEST_INDEX_FILE)
    summary["decomp_cache_initial_entry_count"] = decomp_cache_initial_entry_count
    summary["decomp_cache_entry_count"] = len(decomp_cache)
    summary["decomp_cache_hit_count"] = int(decomp_cache_stats.get("hit", 0))
    summary["decomp_cache_miss_count"] = int(decomp_cache_stats.get("miss", 0))
    summary["decomp_cache_stored_count"] = int(decomp_cache_stats.get("stored", 0))
    summary["list_cache_initial_entry_count"] = list_cache_initial_entry_count
    summary["list_cache_entry_count"] = len(list_cache)
    summary["list_cache_hit_count"] = int(list_cache_stats.get("hit", 0))
    summary["list_cache_miss_count"] = int(list_cache_stats.get("miss", 0))
    summary["list_cache_stored_count"] = int(list_cache_stats.get("stored", 0))
    summary["behavior_cache_initial_entry_count"] = behavior_cache_initial_entry_count
    summary["behavior_cache_entry_count"] = len(behavior_cache or {})
    summary["behavior_cache_hit_count"] = int(behavior_cache_stats.get("hit", 0))
    summary["behavior_cache_miss_count"] = int(behavior_cache_stats.get("miss", 0))
    summary["behavior_cache_stored_count"] = int(behavior_cache_stats.get("stored", 0))
    extracted_source_function_count = sum(
        int(entry.get("extracted_source_function_count") or 0)
        for entry in source_row_selection_entries
    )
    filtered_source_function_count = sum(
        int(entry.get("filtered_source_function_count") or 0)
        for entry in source_row_selection_entries
    )
    selected_source_function_count = sum(
        int(entry.get("selected_source_function_count") or 0)
        for entry in source_row_selection_entries
    )
    suppressed_static_inline_count = len(suppressed_static_inline_rows)
    summary["source_row_selection_metrics"] = {
        "extracted_source_function_count": extracted_source_function_count,
        "extracted_static_source_function_count": sum(
            int(entry.get("extracted_static_source_function_count") or 0)
            for entry in source_row_selection_entries
        ),
        "filtered_source_function_count": filtered_source_function_count,
        "selected_source_function_count": selected_source_function_count,
        "semantic_score_denominator_row_count": summary.get("row_count"),
        "explicit_function_filter_active": bool(args.function_name),
        "limit_functions": args.limit_functions,
        "suppressed_static_inline_helper_count": suppressed_static_inline_count,
        "suppressed_static_inline_helper_rate_filtered_denominator": round(
            suppressed_static_inline_count / filtered_source_function_count,
            6,
        ) if filtered_source_function_count else 0.0,
        "suppressed_static_inline_policy": (
            "static source helpers reachable from matched source functions but absent from binary symbols "
            "are excluded from benchmark rows unless explicitly selected; they remain available for "
            "same-source inline-expanded fingerprints"
        ),
        "entries": source_row_selection_entries,
        "suppressed_static_inline_helpers": suppressed_static_inline_rows,
    }
    decomp_cache_requests = summary["decomp_cache_hit_count"] + summary["decomp_cache_miss_count"]
    list_cache_requests = summary["list_cache_hit_count"] + summary["list_cache_miss_count"]
    behavior_cache_requests = summary["behavior_cache_hit_count"] + summary["behavior_cache_miss_count"]
    summary["cache_efficiency_metrics"] = {
        "decomp_cache_request_count": decomp_cache_requests,
        "decomp_cache_hit_rate": round(summary["decomp_cache_hit_count"] / decomp_cache_requests, 6)
        if decomp_cache_requests
        else 0.0,
        "decomp_cache_stored_count": summary["decomp_cache_stored_count"],
        "list_cache_request_count": list_cache_requests,
        "list_cache_hit_rate": round(summary["list_cache_hit_count"] / list_cache_requests, 6)
        if list_cache_requests
        else 0.0,
        "list_cache_stored_count": summary["list_cache_stored_count"],
        "behavior_cache_request_count": behavior_cache_requests,
        "behavior_cache_hit_rate": round(summary["behavior_cache_hit_count"] / behavior_cache_requests, 6)
        if behavior_cache_requests
        else 0.0,
        "behavior_cache_stored_count": summary["behavior_cache_stored_count"],
    }
    summary["wall_sec"] = round(time.perf_counter() - start, 6)
    if args.require_sleigh_template_source:
        summary["sleigh_template_source_gate"] = sleigh_template_source_gate(
            summary,
            args.require_sleigh_template_source,
        )
    history = history_snapshot(DEFAULT_HISTORY_FILE, summary)
    if history is not None:
        summary["history"] = history
    save_decomp_cache(decomp_cache_path, decomp_cache)
    save_list_cache(list_cache_path, list_cache)
    save_behavior_cache(behavior_cache_path, behavior_cache or {})
    baseline_path: Path | None = None
    if not args.no_baseline_compare:
        baseline_path = resolve_path(args.baseline_dir) if args.baseline_dir else find_latest_baseline_dir(
            output_dir,
            summary["manifest"],
            {row_key(row) for row in rows},
        )
    if baseline_path is not None:
        try:
            baseline_summary, baseline_rows, baseline_summary_path = load_baseline_artifacts(baseline_path)
            summary["comparison"] = compare_to_baseline(
                summary,
                rows,
                baseline_summary,
                baseline_rows,
                baseline_summary_path,
            )
            summary["comparison_outcome"] = comparison_outcome(summary["comparison"])
            summary["improvement_summary"] = improvement_summary(summary["comparison"])
            if not args.no_baseline_snapshot:
                summary["baseline_snapshot"] = snapshot_baseline_artifacts(
                    output_dir,
                    baseline_summary_path,
                    baseline_summary,
                    baseline_rows,
                    summary["comparison"],
                )
        except Exception as exc:
            summary["comparison_error"] = {
                "baseline": str(baseline_path),
                "error": str(exc),
            }
    if args.materialize_debug_triage:
        triage_rows = materialize_debug_triage(
            rows,
            fission_bin,
            output_dir,
            args.timeout_sec,
            args.debug_triage_limit,
        )
        summary["debug_triage"] = triage_rows
        summary["debug_triage_count"] = len(triage_rows)
    if args.materialize_regression_debug_triage and isinstance(summary.get("comparison"), dict):
        regression_triage_rows = materialize_regression_debug_triage(
            rows,
            summary["comparison"],
            fission_bin,
            output_dir,
            args.timeout_sec,
            args.regression_debug_triage_limit,
        )
        summary["regression_debug_triage"] = regression_triage_rows
        summary["regression_debug_triage_count"] = len(regression_triage_rows)
    debug_commands = top_debug_commands(rows)
    if debug_commands:
        summary["debug_repro_commands"] = debug_commands
    (output_dir / "source_semantic_rows.json").write_text(
        dump_json_pretty(rows), encoding="utf-8"
    )
    (output_dir / "source_semantic_summary.json").write_text(
        dump_json_pretty(summary), encoding="utf-8"
    )
    if "comparison" in summary:
        (output_dir / "source_semantic_comparison.json").write_text(
            dump_json_pretty(summary["comparison"]), encoding="utf-8"
        )
    (output_dir / "source_semantic_summary.md").write_text(render_markdown(summary, rows), encoding="utf-8")
    gate = summary.get("sleigh_template_source_gate")
    gate_failed = isinstance(gate, dict) and gate.get("status") == "failed"
    if not gate_failed:
        append_history_record(DEFAULT_HISTORY_FILE, summary)
        update_latest_index(DEFAULT_LATEST_INDEX_FILE, summary)
    print(dump_json_pretty(summary), end="")
    return 1 if gate_failed else 0


def run_self_test() -> int:
    sample = """
static int helper(int x) { return x + 1; }
int add(int a, int b) { return a + b; }
int max(int a, int b) { if (a > b) return a; return b; }
"""
    with tempfile.TemporaryDirectory(prefix="source-semantic-selftest-") as tmp:
        path = Path(tmp) / "sample.c"
        path.write_text(sample, encoding="utf-8")
        funcs = extract_source_functions(path, "c")
        assert [f.name for f in funcs] == ["helper", "add", "max"]
        assert funcs[0].is_static
        assert funcs[1].return_kind == "int"
        assert funcs[1].param_kinds == ["int", "int"]
        funcs_by_name = {normalize_name(f.name): f for f in funcs}
        caller = SourceFunction(
            name="caller",
            signature="int caller(int a, int b)",
            body="return add(a, b);",
            return_kind="int",
            param_kinds=["int", "int"],
            param_names=["a", "b"],
            line=1,
        )
        expanded_fp = inline_expanded_source_fingerprint(caller, funcs_by_name)
        assert expanded_fp["call:add"] == 0
        assert expanded_fp["op:+"] >= 1
        assert multiset_jaccard(code_fingerprint(funcs[1].body, funcs[1]), code_fingerprint(funcs[1].body, funcs[1])) == 1.0
        missing_feature_score = multiset_jaccard(
            code_fingerprint("if (a > b) return helper(a + b);", funcs[1]),
            code_fingerprint("return a;", funcs[1]),
        )
        assert 0.0 < missing_feature_score < 1.0, missing_feature_score
        gap_details = multiset_gap_details(
            code_fingerprint("if (a > b) return helper(a + b);", funcs[1]),
            code_fingerprint("return a;", funcs[1]),
        )
        assert gap_details["missing_feature_total"] > 0
        assert gap_details["union_feature_total"] >= gap_details["intersection_feature_total"]
        rendered_sig_fp = code_fingerprint("uint test_switch(uint param_1) { return param_1; }")
        assert rendered_sig_fp["sig:return:int"] == 1
        assert rendered_sig_fp["sig:param_count:1"] == 1
        assert rendered_sig_fp["sig:param:uint"] == 1
        assert rendered_sig_fp["call:testswitch"] == 0
        rendered_call_fp = code_fingerprint(
            "uint caller(uint param_1) { return helper(param_1); }"
        )
        assert rendered_call_fp["call:caller"] == 0
        assert rendered_call_fp["call:helper"] == 1
        function_pointer_source = SourceFunction(
            name="apply_op",
            signature="u32 apply_op(op_func f, u32 a, u32 b)",
            body="return f(a, b);",
            return_kind="int",
            param_kinds=["aggregate_or_pointer", "uint", "uint"],
            param_names=["f", "a", "b"],
            line=1,
        )
        function_pointer_source_fp = code_fingerprint(
            function_pointer_source.body,
            function_pointer_source,
        )
        function_pointer_decomp_fp = code_fingerprint(
            "uint apply_op(void * param_1, uint param_2, uint param_3) { "
            "return ((uint (*)(uint, uint))param_1)(param_2, param_3); }"
        )
        assert function_pointer_source_fp["call:indirect_param"] == 1
        assert function_pointer_source_fp["call:f"] == 0
        assert function_pointer_decomp_fp["call:indirect_param"] == 1
        status, matched, _ = match_function(funcs[1], [FissionFunction("0x1000", "add [export]")])
        assert status == "matched"
        assert matched is not None
        status, matched, candidates = match_function(
            SourceFunction(
                name="main",
                signature="int main()",
                body="return 0;",
                return_kind="int",
                param_kinds=[],
                param_names=[],
                line=1,
            ),
            [
                FissionFunction("0x1000", "__main"),
                FissionFunction("0x2000", "main"),
            ],
        )
        assert status == "matched"
        assert matched is not None
        assert matched.address == "0x2000"
        assert candidates == []
        limited = select_source_functions(
            [
                SourceFunction(
                    name="helper",
                    signature="static int helper(int x)",
                    body="return x + 1;",
                    return_kind="int",
                    param_kinds=["int"],
                    param_names=["x"],
                    line=1,
                    is_static=True,
                ),
                SourceFunction(
                    name="entry",
                    signature="int entry(int x)",
                    body="return helper(x);",
                    return_kind="int",
                    param_kinds=["int"],
                    param_names=["x"],
                    line=2,
                ),
            ],
            [FissionFunction("0x3000", "entry")],
            1,
        )
        assert [func.name for func in limited] == ["entry"]
        static_funcs = [
            SourceFunction(
                name="helper",
                signature="static int helper(int x)",
                body="return x + 1;",
                return_kind="int",
                param_kinds=["int"],
                param_names=["x"],
                line=1,
                is_static=True,
            ),
            SourceFunction(
                name="entry",
                signature="int entry(int x)",
                body="return helper(x);",
                return_kind="int",
                param_kinds=["int"],
                param_names=["x"],
                line=2,
            ),
        ]
        filtered_static, suppressed_static = filter_inlined_static_source_functions(
            static_funcs,
            static_funcs,
            [FissionFunction("0x3000", "entry")],
            explicit_function_filter=False,
        )
        assert [func.name for func in filtered_static] == ["entry"]
        assert [row["function_name"] for row in suppressed_static] == ["helper"]
        explicit_static, explicit_suppressed = filter_inlined_static_source_functions(
            [static_funcs[0]],
            static_funcs,
            [FissionFunction("0x3000", "entry")],
            explicit_function_filter=True,
        )
        assert [func.name for func in explicit_static] == ["helper"]
        assert explicit_suppressed == []
        entries = [
            BenchmarkEntry(
                id="x86-smoke",
                binary_path=Path("/tmp/x86.exe"),
                source_path=Path("/tmp/x86.c"),
                language="c",
                tags=["smoke", "x86-64"],
            ),
            BenchmarkEntry(
                id="aarch64-control",
                binary_path=Path("/tmp/aarch64.o"),
                source_path=Path("/tmp/aarch64.c"),
                language="c",
                tags=["smoke", "aarch64", "control-flow"],
            ),
        ]
        assert [entry.id for entry in filter_entries(entries, ["aarch64-control"], None)] == [
            "aarch64-control"
        ]
        assert [entry.id for entry in filter_entries(entries, None, ["smoke", "aarch64"])] == [
            "aarch64-control"
        ]
        assert [
            func.name for func in filter_source_functions(funcs, ["max"])
        ] == ["max"]
        assert classify_return("u64 wide(unsigned int seed)", "wide", "unsigned int seed", "c") == "int"
        assert classify_return("uint64_t wide(unsigned int seed)", "wide", "unsigned int seed", "c") == "int"
        assert classify_return("longlong wide(longlong seed)", "wide", "longlong seed", "c") == "int"
        assert classify_param("ulonglong count", "c") == "uint"
        assert classify_param("ushort flags", "c") == "uint"
        assert classify_param("uchar byte", "c") == "uint"
        summary = summarize(
            [
                {
                    "language": "c",
                    "tags": [],
                    "entry_id": "selftest",
                    "mapping_status": "matched",
                    "decomp_success": True,
                    "behavior": {"status": "pass", "case_count": 2, "case_pass_count": 2, "case_fail_count": 0},
                    "semantic_score": 1.0,
                    "static_semantic_score": 1.0,
                    "static_semantic_score_direct": 1.0,
                    "static_semantic_score_inline_expanded": 1.0,
                    "static_similarity_source_variant": "direct_source",
                    "source_body_line_count": 1,
                    "source_body_byte_count": 12,
                    "source_return_kind": "int",
                    "source_param_kinds": ["int", "int"],
                    "decomp_return_kind": "int",
                    "decomp_param_kinds": ["uint", "uint"],
                    "source_static_feature_count_direct": 2,
                    "source_static_feature_count_inline_expanded": 2,
                    "decomp_line_count": 2,
                    "decomp_byte_count": 24,
                    "static_similarity_gaps": {
                        "source_feature_total": 2,
                        "decomp_feature_total": 2,
                        "intersection_feature_total": 2,
                        "union_feature_total": 2,
                        "missing_feature_total": 0,
                        "extra_feature_total": 0,
                    },
                    "static_similarity_gap_components": {
                        "signature": {
                            "source_feature_total": 2,
                            "decomp_feature_total": 2,
                            "intersection_feature_total": 0,
                            "union_feature_total": 4,
                            "missing_feature_total": 2,
                            "extra_feature_total": 2,
                            "top_missing_features": [{"feature": "sig:param:int", "count": 2}],
                            "top_extra_features": [{"feature": "sig:param:uint", "count": 2}],
                        }
                    },
                    "preview_build_stats": {
                        "validated_pcode_op_count": 10,
                        "replacement_plan_rejected_missing_merge_count": 2,
                    },
                    "debug_decomp": {
                        "stage_status": {
                            "decode": "ok",
                            "raw_pcode": "ok",
                            "nir_build": "ok",
                            "normalize": "ok",
                            "structuring": "ok",
                            "render": "ok",
                        },
                        "quality_evidence": {
                            "region_emit_ready_failed_count": 1,
                        },
                    },
                },
                {
                    "language": "c",
                    "tags": [],
                    "entry_id": "selftest",
                    "mapping_status": "unmapped",
                    "decomp_success": False,
                    "behavior": {"status": "decomp_failed", "score": 0.0, "case_count": 1},
                    "semantic_score": 0.0,
                    "static_semantic_score": 0.0,
                    "static_semantic_score_direct": 0.0,
                    "static_semantic_score_inline_expanded": 0.0,
                    "static_similarity_source_variant": "direct_source",
                    "source_body_line_count": 1,
                    "source_body_byte_count": 12,
                    "source_return_kind": "int",
                    "source_param_kinds": ["int"],
                    "decomp_return_kind": None,
                    "decomp_param_kinds": None,
                    "source_static_feature_count_direct": 2,
                    "source_static_feature_count_inline_expanded": 2,
                    "decomp_line_count": 0,
                    "decomp_byte_count": 0,
                    "static_similarity_gaps": {
                        "source_feature_total": 2,
                        "decomp_feature_total": 0,
                        "intersection_feature_total": 0,
                        "union_feature_total": 2,
                        "missing_feature_total": 2,
                        "extra_feature_total": 0,
                        "top_missing_features": [{"feature": "op:+", "count": 1}],
                    },
                    "static_similarity_gap_components": {},
                },
            ],
            "selftest",
            [],
        )
        assert summary["row_count"] == 2
        assert summary["function_mapping_rate"] == 0.5
        assert summary["decomp_success_rate"] == 0.5
        assert summary["weighted_semantic_similarity"] == 0.5
        assert summary["effective_coverage"]["mapped_rows"] == 1
        assert summary["zero_credit_breakdown"]["unmapped"] == 1
        assert "control_flow" in summary["static_similarity_component_averages"]
        assert summary["static_similarity_gap_totals"]["missing_feature_total"] == 2
        assert summary["static_similarity_gap_totals"]["top_missing_features"][0]["feature"] == "op:+"
        assert summary["behavior_case_metrics"]["case_pass_count"] == 2
        assert summary["score_distribution"]["perfect"] == 1
        assert summary["score_distribution"]["zero"] == 1
        assert summary["semantic_score_stats"]["nonzero_count"] == 1
        assert summary["scoring_contract"]["semantic_score_denominator"] == "all manifest rows"
        assert summary["score_component_metrics"]["behavior_component_score_sum"] == 0.65
        assert summary["score_component_metrics"]["static_component_score_sum"] == 0.35
        assert summary["score_denominator_metrics"]["score_denominator_row_count"] == 2
        assert summary["score_denominator_metrics"]["lost_score_sum"] == 1.0
        assert summary["semantic_loss_metrics"]["lost_score_by_zero_credit_reason"]["unmapped"] == 1.0
        assert summary["semantic_readiness_metrics"]["fully_perfect_rows"] == 1
        assert summary["semantic_readiness_metrics"]["behavior_pass_static_perfect_rows"] == 1
        assert summary["semantic_readiness_metrics"]["pipeline_ok_behavior_nonpass_rows"] == 0
        assert summary["benchmark_integrity_metrics"]["rows_excluded_from_semantic_score_denominator"] == 0
        assert summary["benchmark_integrity_metrics"]["missing_source_features_penalized"] is True
        assert summary["behavior_mismatch_metrics"]["mismatch_row_count"] == 0
        assert summary["behavior_case_metrics"]["compared_case_count"] == 3
        assert summary["behavior_denominator_metrics"]["case_denominator_count"] == 3
        assert summary["behavior_timeout_progress_metrics"]["partial_timeout_row_count"] == 0
        assert summary["behavior_case_metrics"]["partial_progress_row_count"] == 0
        assert summary["denominator_accounting_metrics"]["unmapped_row_count"] == 1
        assert summary["denominator_accounting_metrics"]["semantic_score_denominator_row_count"] == 2
        assert summary["static_gap_row_metrics"]["missing_feature_row_count"] == 1
        assert summary["static_absence_penalty_metrics"]["missing_feature_total"] == 2.0
        assert summary["static_absence_penalty_metrics"]["rows_with_no_decomp_features_despite_source"] == 1
        assert summary["static_component_absence_matrix_metrics"]["signature"]["both_present_row_count"] == 1
        assert (
            summary["static_component_absence_matrix_metrics"]["signature"][
                "zero_intersection_source_present_row_count"
            ]
            == 1
        )
        assert summary["source_decomp_size_metrics"]["decomp_to_source_line_ratio_distribution"]["max"] == 2.0
        assert summary["static_source_variant_metrics"]["variant_counts"]["direct_source"] == 2
        assert summary["score_by_behavior_status"]["pass"]["count"] == 1
        assert summary["behavior_status_by_zero_credit_reason"]["unmapped"]["decomp_failed"] == 1
        assert "top_decompile_wall_rows" in summary["cost_hot_rows"]
        assert "debug_coverage_metrics" in summary
        assert summary["triage_priority_rows"][0]["function_name"] is None
        assert "decompile_avg_sec" in summary["harness_cost_metrics"]
        assert "decompile_p95_sec" in summary["harness_cost_metrics"]
        assert summary["pipeline_stage_metrics"]["decode"]["ok_count"] == 1
        assert summary["nir_build_stats_metrics"]["stats_row_count"] == 1
        assert summary["nir_build_stats_metrics"]["debt_metric_totals"]["replacement_plan_rejected_missing_merge_count"] == 2.0
        assert summary["nir_debt_correlation_metrics"]["debt_row_count"] == 1
        assert summary["debug_quality_evidence_totals"]["region_emit_ready_failed_count"] == 1.0
        assert summary["behavior_distance_metrics"]["case_pass_rate_distribution"]["count"] == 0
        assert summary["improvement_axis_metrics"]["nir_telemetry_debt"]["row_count"] == 1
        assert summary["improvement_axis_metrics"]["mapping"]["lost_score_sum"] == 1.0
        assert summary["admission_gate_metrics"]["counts"]["manifest_rows"] == 2
        assert summary["admission_gate_metrics"]["counts"]["raw_pcode_ok_rows"] == 1
        assert summary["quality_gate_funnel_metrics"]["drop_rows_from_previous_gate"]["manifest_rows->mapped_rows"] == 1
        assert summary["stage_transition_metrics"]["furthest_ok_stage_counts"]["render"] == 1
        assert summary["sleigh_lift_health_metrics"]["decode_ok_rows"] == 1
        assert summary["sleigh_lift_health_metrics"]["raw_pcode_ok_rows"] == 1
        assert summary["sleigh_lift_health_metrics"]["raw_pcode_compat_import_total"] == 0.0
        assert summary["behavior_failure_diagnostics"]["owner_counts"]["decomp_failed"] == 1
        assert summary["semantic_quality_quadrant_metrics"]["dynamic_pass|static_perfect"]["row_count"] == 1
        assert (
            summary["semantic_quality_quadrant_metrics"][
                "dynamic_harness_or_decomp_blocked|static_no_decomp_features"
            ]["row_count"]
            == 1
        )
        assert summary["outcome_matrix_metrics"]["outcome_count"] == 2
        assert summary["coverage_blind_spot_metrics"]["counts"]["unmapped_source_function"] == 1
        assert summary["coverage_blind_spot_metrics"]["counts"]["source_features_without_decomp_features"] == 1
        assert summary["static_gap_density_metrics"]["gap_bucket_rows"]["missing:small|extra:none"]["row_count"] == 1
        assert summary["static_gap_hot_row_metrics"]["top_missing_feature_rows"][0]["missing_feature_total"] == 2.0
        assert summary["static_component_precision_recall_metrics"]["signature"]["precision"] == 0.0
        assert summary["behavior_partial_progress_metrics"]["row_count"] == 0
        assert summary["focus_area_metrics"]["nir_builder_dataflow"]["row_count"] == 1
        assert summary["focus_area_metrics"]["mapping_name_recovery"]["lost_score_sum"] == 1.0
        assert summary["roadmap_priority_metrics"]["priority_order"][0] == "p1_sleigh_lift_correctness"
        assert summary["roadmap_priority_metrics"]["buckets"]["p3_structuring_hard_cases"]["row_count"] == 1
        assert summary["roadmap_priority_metrics"]["buckets"]["p4_fid_name_recovery"]["lost_score_sum"] == 1.0
        assert "signature_gap_rows" in summary["type_data_gap_metrics"]
        assert summary["signedness_only_signature_gap_metrics"]["row_count"] == 1
        assert summary["signedness_only_signature_gap_metrics"]["param_pair_count"] == 2.0
        assert summary["signature_kind_confusion_metrics"]["return_pair_counts"]["int->missing"] == 1
        assert summary["signature_kind_confusion_metrics"]["param_pair_counts"]["int->missing"] == 1
        assert summary["signature_kind_confusion_metrics"]["param_pair_counts"]["int->uint"] == 2
        assert summary["signature_kind_confusion_metrics"]["param_arity_mismatch_row_count"] == 1
        aliases = c_like_pointer_typedef_aliases(
            "typedef unsigned int (*op_func)(unsigned int, unsigned int);\n"
            "typedef unsigned char *byte_ptr;\n"
        )
        assert aliases == {"op_func", "byte_ptr"}
        parsed = extract_c_like_functions(
            "typedef unsigned int (*op_func)(unsigned int, unsigned int);\n"
            "unsigned int apply_op(op_func f, unsigned int a) { return f(a, a); }\n",
            "c",
        )
        assert parsed[0].param_kinds == ["aggregate_or_pointer", "uint"]
        assert "uint" not in call_names_for_fingerprint(
            "return ((uint (*)(uint, uint))param_1)(param_2, param_3);"
        )
        fp_cases = [
            {
                "args": ["op_add", 2],
                "candidate_support_code": "uint op_add(uint a, uint b) { return a + b; }",
            }
        ]
        valid, reason = validate_explicit_behavior_cases(parsed[0], fp_cases)
        assert valid, reason
        rendered = render_explicit_case_call(parsed[0], fp_cases[0], 0)
        assert "apply_op(op_add, 2)" in rendered
        assert candidate_support_code_blocks(fp_cases) == [
            "uint op_add(uint a, uint b) { return a + b; }"
        ]
        progress = partial_behavior_progress(
            {"stdout": "ret=0\nret=1\nret=1\nret=5\n"},
            {"partial_stdout": "ret=0\nret=1\nret=1\n"},
            [(0,), (1,), (2,), (5,)],
        )
        assert progress["case_pass_count"] == 3
        assert progress["case_fail_count"] == 1
        assert progress["compared_case_count"] == 4
        assert progress["first_mismatch_index"] == 3
        assert progress["candidate_missing_line_count"] == 1
        assert candidate_timeout_sec(20, {"run_sec": 0.01}) == CANDIDATE_TIMEOUT_MIN_SEC
        assert candidate_timeout_sec(20, {"run_sec": 0.25}) == 3
        assert candidate_timeout_sec(20, {"run_sec": 0.32}) == 4
        assert "timeout_sec=7" in behavior_cache_key("int main(void){return 0;}", "/bin/clang", 7)
        assert "control_flow_gap_rows" in summary["structuring_gap_metrics"]
        assert summary["fid_name_recovery_metrics"]["name_or_mapping_gap_row_count"] == 1
        assert "unknown" in summary["architecture_support_metrics"]
        assert summary["complexity_quality_metrics"]["by_source_feature_bucket"]["tiny"]["row_count"] == 2
        assert "decompile_wall_by_stage_first_failure" in summary["stage_cost_correlation_metrics"]
        assert "score_by_decompile_cost_bucket" in summary["stage_cost_correlation_metrics"]
        void_func = SourceFunction(
            name="touch",
            signature="void touch(unsigned int seed)",
            body="control_sink = seed;",
            return_kind="void",
            param_kinds=["uint"],
            param_names=["seed"],
            line=1,
        )
        global_cases = [
            {
                "args": [7],
                "globals": [{"name": "control_sink", "ctype": "unsigned int", "reset": 0}],
            }
        ]
        valid, reason = validate_explicit_behavior_cases(void_func, global_cases)
        assert valid, reason
        rendered = render_explicit_case_call(void_func, global_cases[0], 0)
        assert "control_sink = 0;" in rendered
        assert "control_sink=%lld" in rendered
        assert "touch(7);" in rendered
        deduped_candidate = candidate_harness(
            "uint control_sink;\nuint math_sink;\nvoid touch(unsigned int seed) { control_sink = seed; }",
            void_func,
            global_cases,
        )
        assert "volatile unsigned int control_sink = 0;" in deduped_candidate
        assert "\nuint control_sink;\n" not in deduped_candidate
        assert "\nuint math_sink;\n" in deduped_candidate
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:ok": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"sla_construct_tpl": 2},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "passed"
        assert gate["raw_pcode_compat_import_count"] == 0
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:ok": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"spec_derived": 2},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "passed"
        assert gate["template_source_totals"] == {"sla_construct_tpl": 2}
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:ok": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"compatibility_lowered": 1},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "failed"
        assert "compatibility_lowered" in gate["failures"][0]
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:failed": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"sla_construct_tpl": 1},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "failed"
        assert any("raw_pcode:failed" in failure for failure in gate["failures"])
        gate = sleigh_template_source_gate(
            {
                "row_count": 2,
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:ok": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"sla_construct_tpl": 1},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "failed"
        assert any(
            "decode must be ok for every mapped row (1/2)" in failure
            for failure in gate["failures"]
        )
        gate = sleigh_template_source_gate(
            {
                "row_count": 2,
                "mapping_status_counts": {"matched": 1, "unmapped": 1},
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:ok": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"sla_construct_tpl": 1},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "passed"
        assert gate["mapped_row_count"] == 1
        assert gate["unmapped_row_count"] == 1
        gate = sleigh_template_source_gate(
            {
                "row_count": 2,
                "debug_stage_status_counts": {"decode:ok": 2, "raw_pcode:ok": 2},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"sla_construct_tpl": 1},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "failed"
        assert any("template source evidence must cover every raw_pcode:ok row (1/2)" in failure for failure in gate["failures"])
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:ok": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 1},
                "debug_template_source_totals": {"sla_construct_tpl": 1},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "failed"
        assert any("invalid_pcode_shape_count" in failure for failure in gate["failures"])
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {"decode:ok": 1, "raw_pcode:ok": 1},
                "debug_quality_evidence_totals": {"invalid_pcode_shape_count": 0},
                "debug_template_source_totals": {"sla_construct_tpl": 1},
                "nir_build_stats_metrics": {
                    "numeric_totals": {"raw_pcode_compat_import_count": 1}
                },
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "failed"
        assert any("raw_pcode_compat_import_count" in failure for failure in gate["failures"])
        gate = sleigh_template_source_gate(
            {
                "row_count": 1,
                "debug_stage_status_counts": {},
                "debug_template_source_totals": {},
            },
            "sla_construct_tpl",
        )
        assert gate["status"] == "failed"
        assert "--include-debug-decomp" in gate["failures"][0]
    print("self-test ok")
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Benchmark Fission pseudocode against original source semantics. Ghidra is not used."
    )
    parser.add_argument("--manifest", default=str(DEFAULT_MANIFEST), help="Source semantic manifest JSON")
    parser.add_argument("--fission-bin", default=str(DEFAULT_FISSION_BIN), help="Path to fission_cli")
    parser.add_argument(
        "--output-dir",
        help="Output artifact directory; defaults to a timestamped directory under benchmark/artifacts/source_semantic_benchmark",
    )
    parser.add_argument(
        "--entry-id",
        action="append",
        help="Run only the manifest entry with this exact id; repeat to include multiple entries",
    )
    parser.add_argument(
        "--tag",
        action="append",
        help="Run only manifest entries containing this tag; repeat to require all listed tags",
    )
    parser.add_argument(
        "--function-name",
        action="append",
        help="Run only source functions with this exact name; repeat to include multiple functions",
    )
    parser.add_argument("--limit-binaries", type=int, help="Limit discovered manifest entries")
    parser.add_argument("--limit-functions", type=int, help="Limit source functions per entry")
    parser.add_argument("--timeout-sec", type=int, default=30, help="Per-command timeout")
    parser.add_argument(
        "--baseline-dir",
        help="Compare against a previous artifact directory or source_semantic_summary.json; defaults to latest matching prior run",
    )
    parser.add_argument(
        "--no-baseline-compare",
        action="store_true",
        help="Disable automatic comparison against previous source-semantic artifacts",
    )
    parser.add_argument(
        "--no-baseline-snapshot",
        action="store_true",
        help="Do not copy the selected baseline summary/rows/comparison into the current artifact directory",
    )
    parser.add_argument(
        "--include-debug-decomp",
        action="store_true",
        help="Pass fission_cli decomp --debug-decomp and attach compact stage/owner evidence to each row",
    )
    parser.add_argument(
        "--require-sleigh-template-source",
        choices=["sla_construct_tpl"],
        help=(
            "Fail the run unless all debug SLEIGH template-source evidence uses this source. "
            "Requires --include-debug-decomp for rows with raw_pcode:ok."
        ),
    )
    parser.add_argument(
        "--materialize-debug-triage",
        action="store_true",
        help="Run fission_cli decomp/disasm/xrefs/function-facts for the lowest-scoring rows and save captures",
    )
    parser.add_argument(
        "--debug-triage-limit",
        type=int,
        default=8,
        help="Maximum non-perfect rows to materialize with --materialize-debug-triage",
    )
    parser.add_argument(
        "--materialize-regression-debug-triage",
        action="store_true",
        help="Run fission_cli debug surfaces for rows that regressed versus the selected baseline",
    )
    parser.add_argument(
        "--regression-debug-triage-limit",
        type=int,
        default=8,
        help="Maximum regressed rows to materialize with --materialize-regression-debug-triage",
    )
    parser.add_argument(
        "--decomp-cache-file",
        default=str(DEFAULT_DECOMP_CACHE_FILE),
        help="Persistent decompile-result cache file keyed by input binary and fission_cli build metadata",
    )
    parser.add_argument(
        "--no-decomp-cache",
        action="store_true",
        help="Disable the persistent decompile-result cache; the in-run memory cache remains enabled",
    )
    parser.add_argument(
        "--list-cache-file",
        default=str(DEFAULT_LIST_CACHE_FILE),
        help="Persistent fission_cli list-result cache file keyed by input binary and fission_cli build metadata",
    )
    parser.add_argument(
        "--no-list-cache",
        action="store_true",
        help="Disable the persistent fission_cli list-result cache",
    )
    parser.add_argument(
        "--behavior-cache-file",
        default=str(DEFAULT_BEHAVIOR_CACHE_FILE),
        help="Persistent behavior harness cache file keyed by C harness contents and compiler metadata",
    )
    parser.add_argument(
        "--no-behavior-cache",
        action="store_true",
        help="Disable persistent behavior harness compile/run cache",
    )
    parser.add_argument(
        "--jobs",
        type=int,
        default=DEFAULT_JOBS,
        help=f"Run source-function rows in parallel per binary entry (default: {DEFAULT_JOBS}; use 1 for serial)",
    )
    parser.add_argument("--self-test", action="store_true", help="Run lightweight parser/scoring self-test")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.self_test:
        return run_self_test()
    return run_benchmark(args)


if __name__ == "__main__":
    raise SystemExit(main())
