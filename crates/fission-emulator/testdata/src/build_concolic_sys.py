#!/usr/bin/env python3
"""Build freestanding x64_concolic_branch_sys.elf (read 1 byte, branch on 'A', exit)."""
from __future__ import annotations

import struct
import sys
from pathlib import Path

code = bytes(
    [
        0x48, 0x83, 0xEC, 0x10,  # sub rsp, 0x10
        0x48, 0xC7, 0xC0, 0x00, 0x00, 0x00, 0x00,  # mov rax, 0
        0x48, 0xC7, 0xC7, 0x00, 0x00, 0x00, 0x00,  # mov rdi, 0
        0x48, 0x89, 0xE6,  # mov rsi, rsp
        0x48, 0xC7, 0xC2, 0x01, 0x00, 0x00, 0x00,  # mov rdx, 1
        0x0F, 0x05,  # syscall
        0x48, 0x83, 0xF8, 0x01,  # cmp rax, 1
        0x75, 0x10,  # jne fail
        0x80, 0x3C, 0x24, 0x41,  # cmp byte [rsp], 'A'
        0x75, 0x0A,  # jne fail
        0x48, 0xC7, 0xC7, 0x00, 0x00, 0x00, 0x00,  # mov rdi, 0
        0xEB, 0x07,  # jmp exit
        # fail:
        0x48, 0xC7, 0xC7, 0x01, 0x00, 0x00, 0x00,  # mov rdi, 1
        # exit:
        0x48, 0xC7, 0xC0, 0x3C, 0x00, 0x00, 0x00,  # mov rax, 60
        0x0F, 0x05,  # syscall
    ]
)

e_entry = 0x400078
eh = bytearray(64)
eh[0:4] = b"\x7fELF"
eh[4] = 2
eh[5] = 1
eh[6] = 1
eh[16:18] = struct.pack("<H", 2)
eh[18:20] = struct.pack("<H", 0x3E)
eh[20:24] = struct.pack("<I", 1)
eh[24:32] = struct.pack("<Q", e_entry)
eh[32:40] = struct.pack("<Q", 64)
eh[52:54] = struct.pack("<H", 64)
eh[54:56] = struct.pack("<H", 56)
eh[56:58] = struct.pack("<H", 1)

ph = bytearray(56)
ph[0:4] = struct.pack("<I", 1)
ph[4:8] = struct.pack("<I", 5)
ph[16:24] = struct.pack("<Q", 0x400000)
ph[24:32] = struct.pack("<Q", 0x400000)
filesz = 64 + 56 + len(code)
# file size covers code at 0x78
filesz = max(filesz, 0x78 + len(code))
ph[32:40] = struct.pack("<Q", filesz)
ph[40:48] = struct.pack("<Q", filesz)
ph[48:56] = struct.pack("<Q", 0x1000)

pad = 0x78 - (64 + 56)
blob = bytes(eh) + bytes(ph) + (b"\x00" * pad) + code

out = Path(__file__).resolve().parent.parent / "x64_concolic_branch_sys.elf"
out.write_bytes(blob)
print(f"wrote {out} ({len(blob)} bytes)", file=sys.stderr)
