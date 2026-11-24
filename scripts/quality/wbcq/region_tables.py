# SPDX-License-Identifier: Apache-2.0
"""The bytes behind the pointer fields of `corpus/region_machines.c`.

The manifest names a table per region; the engine is handed these bytes with
`--region FIELD=HEX` (`evaluate.py`) and the native ground truth puts the same
bytes in buffers of its own (`ground_truth.py`). Neither side computes them
from the other.
"""

from __future__ import annotations

# The number lexer's classes and states (see `tdfa_step`).
OTHER, DIGIT, DOT, DELIM = range(4)
START, INT, AFTER_DOT, FRAC, BAD = range(5)
ACCEPT = 0x80


def number_classes() -> bytes:
    table = bytearray(256)
    for byte in range(ord("0"), ord("9") + 1):
        table[byte] = DIGIT
    table[ord(".")] = DOT
    for byte in b" \t\n,;":
        table[byte] = DELIM
    return bytes(table)


def number_delta() -> bytes:
    rows = {
        START: (BAD, INT, BAD, START),
        INT: (BAD, INT, AFTER_DOT, START | ACCEPT),
        AFTER_DOT: (BAD, FRAC, BAD, START),
        FRAC: (BAD, FRAC, BAD, START | ACCEPT),
        BAD: (BAD, BAD, BAD, START),
    }
    return bytes(value for state in range(5) for value in rows[state])


def helo_keyword() -> bytes:
    return b"HELO\0\0\0\0"


def ring4() -> bytes:
    return bytes(4)


TABLES = {
    "number_classes": number_classes,
    "number_delta": number_delta,
    "helo_keyword": helo_keyword,
    "ring4": ring4,
}


def region_bytes(name: str) -> bytes:
    return TABLES[name]()
