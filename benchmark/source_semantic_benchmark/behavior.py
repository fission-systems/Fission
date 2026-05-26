from __future__ import annotations
import os
import re
import shutil
import subprocess
import tempfile
import threading
import time
import random
from collections import Counter
from pathlib import Path
from typing import Any

try:
    import resource
except ImportError:
    resource = None

from benchmark.source_semantic_benchmark.config import (
    CANDIDATE_TIMEOUT_MIN_SEC,
    CANDIDATE_TIMEOUT_ORACLE_MULTIPLIER,
    CTYPE_RE,
    WORD_BOUNDARY_RE,
)
from benchmark.source_semantic_benchmark.models import BenchmarkEntry, SourceFunction
from benchmark.source_semantic_benchmark.utils import (
    dump_json_pretty,
    percent,
    sanitize_id,
)
from benchmark.source_semantic_benchmark.cache import behavior_cache_key


def fuzzed_behavior_cases(param_kinds: list[str], count: int = 30) -> list[tuple[int, ...]]:
    param_count = len(param_kinds)
    if param_count == 0:
        return [()]
    rng = random.Random(42)
    uint_candidates = [0, 1, 2, 5, 10, 100, 255, 65535, 1000000, 2147483647]
    int_candidates = [0, 1, -1, 2, -2, 5, -5, 10, -10, 100, -100, 127, -128, 32767, -32768, 2147483647, -2147483648]
    cases: list[tuple[int, ...]] = []
    for _ in range(count):
        case_args: list[int] = []
        for kind in param_kinds:
            if kind == "uint":
                if rng.random() < 0.5:
                    case_args.append(rng.choice(uint_candidates))
                else:
                    case_args.append(rng.randint(0, 10000))
            else:
                if rng.random() < 0.5:
                    case_args.append(rng.choice(int_candidates))
                else:
                    case_args.append(rng.randint(-5000, 5000))
        cases.append(tuple(case_args))
    return cases


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
        for arg_index, arg in enumerate(args):
            kind = func.param_kinds[arg_index]
            if kind in {"int", "uint"}:
                if not isinstance(arg, int):
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
    entry: BenchmarkEntry, func: SourceFunction, fuzz_cases: bool = False
) -> list[tuple[int, ...]] | list[dict[str, Any]]:
    explicit_cases = explicit_behavior_cases(entry, func)
    if explicit_cases is not None:
        return explicit_cases
    if fuzz_cases:
        return fuzzed_behavior_cases(func.param_kinds, 30)
    return default_behavior_cases_for_kinds(func.param_kinds)


def source_harness(
    source_path: Path, func: SourceFunction, cases: list[tuple[int, ...]] | list[dict[str, Any]]
) -> str:
    call_name = "source_original_main" if func.name == "main" else None
    calls = "\n".join(
        render_case_call(func, case, index, call_name=call_name)
        for index, case in enumerate(cases)
    )
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
    candidate_code: str,
    func: SourceFunction,
    cases: list[tuple[int, ...]] | list[dict[str, Any]],
    source_path: Path | None = None,
) -> str:
    call_name = "fission_candidate_main" if func.name == "main" else None
    calls = "\n".join(
        render_case_call(func, case, index, call_name=call_name)
        for index, case in enumerate(cases)
    )
    support_code = "\n".join(candidate_support_code_blocks(cases))
    observed_globals = collect_observed_globals(cases)
    globals_decl = "\n".join(render_candidate_global_decl(spec) for spec in observed_globals)
    candidate_code = remove_duplicate_candidate_global_decls(candidate_code, observed_globals)
    main_define = "#define main fission_candidate_main" if func.name == "main" else ""
    main_undef = "#undef main" if func.name == "main" else ""
    source_dependencies = (
        f'#define main source_original_main\n#include "{source_path}"\n#undef main'
        if func.name == "main" and source_path is not None
        else ""
    )
    return f"""
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
typedef unsigned char byte;
typedef unsigned char uchar;
typedef signed char schar;
typedef unsigned char undefined1;
typedef unsigned char undefined;
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
static inline ulonglong __main(void) {{ return 0; }}
#define __carry(a, b) (sizeof(a) <= 4 ? __fission_carry32((uint32)(a), (uint32)(b)) : __fission_carry64((uint64)(a), (uint64)(b)))
#define __scarry(a, b) (sizeof(a) <= 4 ? __fission_scarry32((uint32)(a), (uint32)(b)) : __fission_scarry64((uint64)(a), (uint64)(b)))
#define __sborrow(a, b) (sizeof(a) <= 4 ? __fission_sborrow32((uint32)(a), (uint32)(b)) : __fission_sborrow64((uint64)(a), (uint64)(b)))
{globals_decl}
{support_code}
{source_dependencies}
{main_define}
{candidate_code}
{main_undef}
int main(void) {{
{calls}
    return 0;
}}
"""


def render_case_call(
    func: SourceFunction,
    case: tuple[int, ...] | dict[str, Any],
    index: int,
    call_name: str | None = None,
) -> str:
    if isinstance(case, dict):
        return render_explicit_case_call(func, case, index, call_name=call_name)
    args = ", ".join(str(v) for v in case)
    target = call_name or func.name
    return f'    printf("%lld\\n", (long long){target}({args}));\n    fflush(stdout);'


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


def render_explicit_case_call(
    func: SourceFunction,
    case: dict[str, Any],
    index: int,
    call_name: str | None = None,
) -> str:
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
    target = call_name or func.name
    globals_to_observe = case.get("globals") or []
    lines = setup
    for spec in globals_to_observe:
        lines.append(f"    {spec['name']} = {int(spec.get('reset', 0))};")
    if func.return_kind == "void":
        lines.append(f"    {target}({joined_args});")
        lines.append('    printf("ret=void");')
    else:
        lines.append(f"    long long case_{index}_ret = (long long){target}({joined_args});")
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


def compile_and_run_c(
    code: str, cwd: Path, name: str, timeout_sec: int, enable_sanitizer: bool = False
) -> dict[str, Any]:
    wall_start = time.perf_counter()
    source = cwd / f"{name}.c"
    binary = cwd / name
    source.write_text(code, encoding="utf-8")
    clang = os.environ.get("CLANG") or shutil.which("clang") or "/opt/homebrew/opt/llvm/bin/clang"
    cmd = [clang, "-x", "c", "-std=c11", "-Wno-everything"]
    if enable_sanitizer:
        cmd.extend(["-fsanitize=address,undefined", "-fno-sanitize-recover=all"])
    cmd.extend([str(source), "-o", str(binary)])
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

    def limit_resources():
        if resource is not None:
            try:
                resource.setrlimit(resource.RLIMIT_AS, (128 * 1024 * 1024, 128 * 1024 * 1024))
                resource.setrlimit(resource.RLIMIT_CPU, (timeout_sec + 2, timeout_sec + 2))
            except Exception:
                pass

    run_start = time.perf_counter()
    try:
        run_res = subprocess.run(
            [str(binary)],
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=timeout_sec,
            preexec_fn=limit_resources,
            check=True,
        )
    except subprocess.CalledProcessError as exc:
        stdout = exc.stdout or ""
        stderr = exc.stderr or ""
        status = "run_failed"
        asan_keywords = ["AddressSanitizer", "UndefinedBehaviorSanitizer", "runtime error:", "ASAN:", "UBSAN:"]
        if any(kw in stderr or kw in stdout for kw in asan_keywords):
            status = "run_sanitizer_error"
        elif exc.returncode == 137 or "out of memory" in stderr.lower() or "out of memory" in stdout.lower():
            status = "run_mle"
        elif exc.returncode == -24 or (exc.returncode == -9 and (time.perf_counter() - run_start) >= timeout_sec):
            status = "run_tle"
            
        return {
            "status": status,
            "detail": (stderr or stdout or str(exc))[-4000:],
            "partial_stdout": stdout[-4000:],
            "partial_stderr": stderr[-4000:],
            "compile_sec": compile_sec,
            "run_sec": round(time.perf_counter() - run_start, 6),
            "wall_sec": round(time.perf_counter() - wall_start, 6),
        }
    except subprocess.TimeoutExpired as exc:
        stdout = exc.stdout.decode("utf-8", errors="replace") if isinstance(exc.stdout, bytes) else (exc.stdout or "")
        stderr = exc.stderr.decode("utf-8", errors="replace") if isinstance(exc.stderr, bytes) else (exc.stderr or "")
        return {
            "status": "run_tle",
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
    enable_sanitizer: bool = False,
) -> dict[str, Any]:
    clang = os.environ.get("CLANG") or shutil.which("clang") or "/opt/homebrew/opt/llvm/bin/clang"
    key = behavior_cache_key(code + f"|sanitizer={enable_sanitizer}", clang, timeout_sec)
    if cache is not None and cache_lock is not None:
        with cache_lock:
            cached = cache.get(key)
        if cached is not None and behavior_cache_entry_is_valid(cached):
            if cache_stats is not None:
                cache_stats["hit"] += 1
            result = dict(cached)
            result["behavior_cache_status"] = "hit"
            return result
        if cached is not None and cache_stats is not None:
            cache_stats["stale"] += 1

    if cache_stats is not None:
        cache_stats["miss"] += 1
    result = compile_and_run_c(code, cwd, name, timeout_sec, enable_sanitizer)
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


def behavior_cache_entry_is_valid(entry: dict[str, Any]) -> bool:
    if entry.get("status") != "run_failed":
        return True
    return isinstance(entry.get("partial_stdout"), str) or isinstance(entry.get("stdout"), str)


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
    enable_sanitizer: bool = False,
    fuzz_cases: bool = False,
) -> dict[str, Any]:
    behavior_start = time.perf_counter()
    explicit_cases = explicit_behavior_cases(entry, func)
    case_source = "explicit" if explicit_cases is not None else "default"
    if fuzz_cases and explicit_cases is None:
        case_source = "fuzzed"
    supported, reason = behavior_supported(entry, func, explicit_cases)

    if not supported:
        return {
            "status": "unsupported",
            "reason": reason,
            "case_source": case_source,
            "case_count": 0,
            "case_pass_count": 0,
            "case_fail_count": 0,
            "score": 0.0,
            "wall_sec": round(time.perf_counter() - behavior_start, 6),
        }

    if host_execution.get("status") != "ok":
        return {
            "status": "host_blocked",
            "reason": f"host execution probe blocked: {host_execution.get('status')}",
            "case_source": case_source,
            "case_count": 0,
            "case_pass_count": 0,
            "case_fail_count": 0,
            "score": 0.0,
            "wall_sec": round(time.perf_counter() - behavior_start, 6),
        }

    if decomp_code is None:
        expected_cases = behavior_cases_for(entry, func, fuzz_cases)
        return {
            "status": "decomp_failed",
            "case_source": case_source,
            "case_count": len(expected_cases),
            "case_pass_count": 0,
            "case_fail_count": len(expected_cases),
            "score": 0.0,
            "wall_sec": round(time.perf_counter() - behavior_start, 6),
        }

    cases = behavior_cases_for(entry, func, fuzz_cases)
    oracle_code = source_harness(entry.source_path, func, cases)
    candidate_code = candidate_harness(decomp_code, func, cases, entry.source_path)

    with tempfile.TemporaryDirectory(prefix="source-semantic-behavior-") as tmp:
        tmp_path = Path(tmp)
        oracle = compile_and_run_c_cached(
            oracle_code,
            tmp_path,
            "oracle",
            timeout_sec,
            behavior_cache,
            behavior_cache_lock,
            behavior_cache_stats,
            enable_sanitizer=enable_sanitizer,
        )
        if oracle.get("status") != "ok":
            status = f"oracle_{oracle.get('status')}"
            res = {
                "status": status,
                "detail": oracle.get("detail"),
                "case_source": case_source,
                "case_count": len(cases),
                "case_pass_count": 0,
                "case_fail_count": len(cases),
                "score": 0.0,
                "wall_sec": round(time.perf_counter() - behavior_start, 6),
            }
            if output_dir is not None:
                write_behavior_artifacts(
                    behavior_artifact_dir_for_row(output_dir, entry, func, address),
                    oracle_code,
                    candidate_code,
                    oracle,
                    None,
                )
            return res

        bounded_timeout = candidate_timeout_sec(timeout_sec, oracle)
        candidate = compile_and_run_c_cached(
            candidate_code,
            tmp_path,
            "candidate",
            bounded_timeout,
            behavior_cache,
            behavior_cache_lock,
            behavior_cache_stats,
            enable_sanitizer=enable_sanitizer,
        )
        if candidate.get("status") == "ok":
            oracle_stdout = oracle.get("stdout", "").strip()
            candidate_stdout = candidate.get("stdout", "").strip()
            if oracle_stdout == candidate_stdout:
                status = "pass"
                score = 1.0
            else:
                status = "mismatch"
                score = 0.0
        else:
            status = f"candidate_{candidate.get('status')}"
            score = 0.0

        progress = partial_behavior_progress(oracle, candidate, cases)
        res = {
            "status": status,
            "detail": candidate.get("detail") or candidate.get("stderr") or candidate.get("stdout"),
            "case_source": case_source,
            "case_count": progress["compared_case_count"],
            "case_pass_count": progress["case_pass_count"],
            "case_fail_count": progress["case_fail_count"],
            "first_mismatch_index": progress["first_mismatch_index"],
            "candidate_missing_line_count": progress["candidate_missing_line_count"],
            "candidate_extra_line_count": progress["candidate_extra_line_count"],
            "score": score,
            "wall_sec": round(time.perf_counter() - behavior_start, 6),
        }
        if output_dir is not None:
            write_behavior_artifacts(
                behavior_artifact_dir_for_row(output_dir, entry, func, address),
                oracle_code,
                candidate_code,
                oracle,
                candidate,
            )
        return res
