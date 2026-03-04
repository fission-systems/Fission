#!/usr/bin/env bash
# Generate minimal seed corpus files for fission-loader fuzz targets.
# Run once before fuzzing to give libfuzzer a starting point.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$SCRIPT_DIR/../.."
CORPUS_ROOT="$ROOT/crates/fission-loader/fuzz/corpus"

# ---------------------------------------------------------------------------
# Minimal valid PE32 DOS header + MZ magic (64 bytes)
# ---------------------------------------------------------------------------
PE_DIR="$CORPUS_ROOT/fuzz_pe_parser"
mkdir -p "$PE_DIR"

python3 - <<'PY' "$PE_DIR/minimal_pe32.bin"
import sys, struct

out = sys.argv[1]
buf = bytearray(64)
# MZ magic
buf[0:2] = b'MZ'
# e_lfanew = 0x40 (points just past this header)
struct.pack_into('<I', buf, 60, 0x40)
with open(out, 'wb') as f:
    f.write(buf)
print(f"[+] Written {out}")
PY

python3 - <<'PY' "$PE_DIR/minimal_pe64.bin"
import sys, struct

out = sys.argv[1]
# 256-byte fake PE64 skeleton (DOS hdr + PE sig + COFF + Optional hdr stub)
buf = bytearray(256)
buf[0:2] = b'MZ'
struct.pack_into('<I', buf, 60, 0x40)   # e_lfanew
buf[0x40:0x44] = b'PE\x00\x00'         # PE signature
# COFF: machine=AMD64, sections=0, SymTbl=0, NumSym=0, OptHdrSz=240, Chars=0x22
struct.pack_into('<HHIIIHH', buf, 0x44, 0x8664, 0, 0, 0, 0, 240, 0x0022)
# Optional header magic for PE32+ = 0x020b
struct.pack_into('<H', buf, 0x58, 0x020b)
with open(out, 'wb') as f:
    f.write(buf)
print(f"[+] Written {out}")
PY

# ---------------------------------------------------------------------------
# Minimal valid ELF64 header (64 bytes)
# ---------------------------------------------------------------------------
ELF_DIR="$CORPUS_ROOT/fuzz_elf_parser"
mkdir -p "$ELF_DIR"

python3 - <<'PY' "$ELF_DIR/minimal_elf64.bin"
import sys, struct

out = sys.argv[1]
buf = bytearray(64)
# e_ident
buf[0:4]  = b'\x7fELF'
buf[4]    = 2       # ELFCLASS64
buf[5]    = 1       # ELFDATA2LSB (little-endian)
buf[6]    = 1       # EV_CURRENT
# e_type = ET_EXEC, e_machine = EM_X86_64, e_version = 1
struct.pack_into('<HHIQ', buf, 16, 2, 0x3e, 1, 0)
# e_phoff=0, e_shoff=0, e_flags=0, e_ehsize=64
struct.pack_into('<QIIHHHHHH', buf, 32, 0, 0, 0, 64, 56, 0, 64, 0, 0)
with open(out, 'wb') as f:
    f.write(buf)
print(f"[+] Written {out}")
PY

python3 - <<'PY' "$ELF_DIR/minimal_elf32.bin"
import sys, struct

out = sys.argv[1]
buf = bytearray(52)
buf[0:4]  = b'\x7fELF'
buf[4]    = 1       # ELFCLASS32
buf[5]    = 1       # little-endian
buf[6]    = 1       # version
struct.pack_into('<HHIIII', buf, 16, 2, 0x03, 1, 0, 0, 0)  # ET_EXEC, EM_386
struct.pack_into('<IIHHHHHH', buf, 32, 0, 52, 0, 52, 32, 0, 28, 0)
with open(out, 'wb') as f:
    f.write(buf)
print(f"[+] Written {out}")
PY

echo "[*] Seed corpus generated:"
find "$CORPUS_ROOT" -type f | sort
