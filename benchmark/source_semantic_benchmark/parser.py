import re
from pathlib import Path
from typing import Any

from benchmark.source_semantic_benchmark.config import (
    ACCESS_LABEL_RE,
    ARRAY_SUFFIX_RE,
    BLOCK_COMMENT_RE,
    C_LIKE_ACCESS_PREFIX_RE,
    C_LIKE_FUNCTION_RE,
    C_LIKE_TYPE_DECL_RE,
    GO_FUNCTION_RE,
    INTEGRAL_WORDS,
    LINE_COMMENT_RE,
    RETURN_ARROW_RE,
    RUST_FUNCTION_RE,
    UNSIGNED_INTEGRAL_WORDS,
    WORD_RE,
)
from benchmark.source_semantic_benchmark.models import FissionFunction, SourceFunction
from benchmark.source_semantic_benchmark.utils import normalize_name


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


def filter_source_functions(funcs: list[SourceFunction], names: list[str] | None) -> list[SourceFunction]:
    if not names:
        return funcs
    wanted = {normalize_name(name) for name in names}
    return [func for func in funcs if normalize_name(func.name) in wanted]


def filter_inlined_static_source_functions(
    funcs: list[SourceFunction],
    all_source_funcs: list[SourceFunction],
    fission_funcs: list[FissionFunction],
    explicit_function_filter: bool,
    fission_error: str | None = None,
) -> tuple[list[SourceFunction], list[dict[str, Any]]]:
    if explicit_function_filter or fission_error or not fission_funcs:
        return funcs, []

    fission_names = {normalize_name(f.name) for f in fission_funcs}
    all_names = {normalize_name(f.name) for f in all_source_funcs}

    suppressed = []
    keep = []
    for func in funcs:
        norm = normalize_name(func.name)
        if func.is_static and norm not in fission_names and norm in all_names:
            suppressed.append(
                {
                    "function_name": func.name,
                    "mapping_status": "suppressed_static_inlined",
                    "decomp_success": False,
                    "semantic_score": 0.0,
                    "static_semantic_score": 0.0,
                }
            )
        else:
            keep.append(func)
    return keep, suppressed
