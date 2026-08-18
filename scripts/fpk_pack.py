#!/usr/bin/env python3
"""
fpk_pack.py
───────────
Pack a sorted line-oriented data file into an `.fpk` container: a sparse index
plus independently compressed blocks.

Why a container at all. `win_api_signatures.txt` is 115,900 lines and 5.07M, and
parsing it costs ~120ms of the ~160ms every `fission_cli` process spends loading
resource data -- about 30% of a small decompile. Compressing the file whole
would shrink distribution and make that worse, since the parse still happens
after a decompress. Blocks give both: records are sorted and packed into ~64KB
groups, each compressed on its own, and a lookup decompresses one block.

Measured on win_api_signatures.txt:

    raw                              5.07M    parse 120ms
    whole-file gzip                  0.92M    parse 120ms + decompress
    64KB blocks + sparse index       0.95M    one 63K block decompressed

The index is one entry per block (82 of them, ~2K total), so it is read whole
and kept uncompressed. A per-record index costs 1.33M and buys nothing: records
average 46 bytes, so per-record compression would spend more on framing than on
data.

The payload inside a block is the ORIGINAL TEXT, byte for byte. Record parsing
does not change; only opening the file does. That is deliberate. Ghidra's own
formats cost two silent-corruption bugs in this repo -- a pointer table read at
the wrong stride, an enum value table read from the wrong field -- both of which
parsed successfully and returned wrong values. A block is a compressed stream:
reading it at the wrong offset fails outright rather than quietly.

Format:
    [0..4)    magic "FPK1"
    [4..6)    kind      u16  payload grammar (see KINDS)
    [6..8)    codec     u16  1 = zlib
    [8..16)   record_count      u64
    [16..24)  block_count       u64
    [24..32)  index_offset      u64
    [32..40)  index_len         u64
    [40..72)  payload sha256
    [72..)    blocks, then index

    index entry: u32 key_len | key | u64 offset | u32 comp_len | u32 raw_len

Usage:
    python3 scripts/fpk_pack.py <input> --kind pipe-text --output <out.fpk>
    python3 scripts/fpk_pack.py <input> --kind pipe-text --self-test
"""
from __future__ import annotations

import argparse
import hashlib
import struct
import sys
import zlib

MAGIC = b"FPK1"
CODEC_ZLIB = 1
HEADER_LEN = 72
BLOCK_TARGET = 64 * 1024

KINDS = {
    # `name|return_type|param:type,...`, keyed by the text before the first `|`
    "pipe-text": 1,
    # one JSON object per line, keyed by its "name" member
    "json-lines": 2,
}


def key_of(line: str, kind: str) -> str:
    if kind == "pipe-text":
        return line.split("|", 1)[0]
    if kind == "json-lines":
        import json

        return json.loads(line).get("name", "")
    raise ValueError(f"unknown kind {kind}")


def build(lines: list[str], kind: str) -> bytes:
    # Sorted so a lookup can binary-search the index and so blocks hold
    # neighbouring keys, which is also what makes them compress well.
    records = sorted(lines, key=lambda line: key_of(line, kind))

    blocks: list[bytes] = []
    current: list[bytes] = []
    size = 0
    for record in records:
        encoded = (record + "\n").encode()
        if size + len(encoded) > BLOCK_TARGET and current:
            blocks.append(b"".join(current))
            current, size = [], 0
        current.append(encoded)
        size += len(encoded)
    if current:
        blocks.append(b"".join(current))

    payload = bytearray()
    index = bytearray()
    for block in blocks:
        compressed = zlib.compress(block, 9)
        first_key = key_of(block.split(b"\n", 1)[0].decode(), kind).encode()
        index += struct.pack("<I", len(first_key)) + first_key
        index += struct.pack("<QII", HEADER_LEN + len(payload), len(compressed), len(block))
        payload += compressed

    digest = hashlib.sha256(bytes(payload)).digest()
    header = (
        MAGIC
        + struct.pack("<HH", KINDS[kind], CODEC_ZLIB)
        + struct.pack("<QQQQ", len(records), len(blocks), HEADER_LEN + len(payload), len(index))
        + digest
    )
    assert len(header) == HEADER_LEN, len(header)
    return bytes(header) + bytes(payload) + bytes(index)


def read_all(blob: bytes, kind: str) -> list[str]:
    """Decode every block. Used by the self-test to prove nothing was lost."""
    if blob[:4] != MAGIC:
        raise ValueError("not an fpk file")
    _record_count, block_count, index_offset, index_len = struct.unpack_from("<QQQQ", blob, 8)
    digest = blob[40:72]
    if hashlib.sha256(blob[HEADER_LEN:index_offset]).digest() != digest:
        raise ValueError("payload digest mismatch")
    out: list[str] = []
    pos = index_offset
    end = index_offset + index_len
    while pos < end:
        (key_len,) = struct.unpack_from("<I", blob, pos)
        pos += 4 + key_len
        offset, comp_len, raw_len = struct.unpack_from("<QII", blob, pos)
        pos += 16
        block = zlib.decompress(blob[offset : offset + comp_len])
        if len(block) != raw_len:
            raise ValueError("block length mismatch")
        out.extend(block.decode().splitlines())
    if len(out) != _record_count:
        raise ValueError(f"record count mismatch: {len(out)} != {_record_count}")
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input")
    parser.add_argument("--kind", choices=sorted(KINDS), required=True)
    parser.add_argument("--output")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    with open(args.input) as fh:
        lines = [l.rstrip("\n") for l in fh if l.strip() and not l.startswith("#")]

    blob = build(lines, args.kind)
    raw = sum(len(l) + 1 for l in lines)
    print(
        f"[+] {len(lines)} records, {raw / 1048576:.2f}M -> {len(blob) / 1048576:.2f}M "
        f"({raw / len(blob):.1f}x)",
        file=sys.stderr,
    )

    if args.self_test or args.output:
        # Round-trip as a multiset: packing sorts, so order is expected to move.
        back = read_all(blob, args.kind)
        if sorted(back) != sorted(lines):
            print("[!] round-trip lost or altered records", file=sys.stderr)
            sys.exit(1)
        print(f"[+] round-trip exact for all {len(back)} records", file=sys.stderr)

    if args.output:
        with open(args.output, "wb") as fh:
            fh.write(blob)
        print(f"[+] Wrote {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
