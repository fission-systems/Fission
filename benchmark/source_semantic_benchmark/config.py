import os
import re
from pathlib import Path

# Paths
ROOT_DIR = Path(__file__).resolve().parents[2]
DEFAULT_MANIFEST = Path(__file__).resolve().parent / "manifests" / "source_owned_all.json"
DEFAULT_FISSION_BIN = ROOT_DIR / "target" / "release" / "fission_cli"
DEFAULT_GHIDRA_HOME = ROOT_DIR / "vendor" / "ghidra" / "ghidra_12.0.4_PUBLIC"
DEFAULT_GHIDRA_SCRIPT_DIR = Path(__file__).resolve().parent / "ghidra_scripts"
DEFAULT_GHIDRA_EXPORT_SCRIPT = "ExportSourceSemanticDecomp.java"
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

# Regular Expressions
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

NIR_DEBT_METRIC_RE = re.compile(
    r"(rejected|failed|fallback|irreducible|invalid|missing|conflict|forced|unsupported|timeout|error)"
)

# Mappings & Lists
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
