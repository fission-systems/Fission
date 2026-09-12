#!/usr/bin/env bash
# Build real third-party programs as Windows PEs, for the emulator to run.
#
# Why this exists: the emulator's "what is missing" sweep is only as honest as
# its input, and everything executable we had locally was written by us. The
# dev corpus is programs that exercise one construct each; `corpus/realworld`
# is the same sources stripped. A real program calls things none of them do,
# and the first sweep against two of them said so -- eighteen missing APIs,
# most of them ordinary C runtime, with the lifter and emulator core clean.
#
# Deliberately NOT the DecBench corpus. Those are real Debian binaries and are
# static-analysis only: the dataset holds malware compiled from source. These
# are built here from published sources instead, so what runs is something we
# compiled from something anyone can read.
#
# mingw is the toolchain because it cross-compiles from macOS and because the
# Windows HLE layer is the one with the gaps worth measuring. Programs that
# need a Unix host (busybox, toybox) or a configure/dependency tree (curl,
# git, openssl) are not here -- each would be a build system to babysit rather
# than a program to run, and the sweep needs breadth of *guest behaviour*, not
# of build tooling.
#
# Usage:  scripts/corpus/build_real_programs.sh [outdir]
#         default outdir: /tmp/realbin

set -uo pipefail

OUT="${1:-/tmp/realbin}"
SRC="${FISSION_REAL_SRC:-/tmp/realsrc}"
CC="${CC_MINGW:-x86_64-w64-mingw32-gcc}"

command -v "${CC}" >/dev/null 2>&1 || {
  echo "no ${CC} (brew install mingw-w64)" >&2
  exit 1
}
mkdir -p "${OUT}" "${SRC}"

built=0
skipped=0

# Fetch once; a second run reuses what is already unpacked.
fetch() {
  local url="$1" archive="$2" marker="$3"
  [[ -e "${SRC}/${marker}" ]] && return 0
  echo "  fetching $(basename "${url}")"
  curl -sSL --max-time 180 -o "${SRC}/${archive}" "${url}" || return 1
  case "${archive}" in
    *.zip) (cd "${SRC}" && unzip -qo "${archive}") ;;
    *.tar.gz | *.tgz) tar xzf "${SRC}/${archive}" -C "${SRC}" ;;
    *.tar.xz) tar xJf "${SRC}/${archive}" -C "${SRC}" ;;
    *) echo "  unknown archive type ${archive}" >&2; return 1 ;;
  esac
  [[ -e "${SRC}/${marker}" ]]
}

# Build one program, and say which it was rather than failing the whole run:
# a toolchain that cannot build one of these is still worth the others.
build() {
  local name="$1"
  shift
  printf '  %-14s ' "${name}"
  if "$@" >"${SRC}/${name}.buildlog" 2>&1 && [[ -s "${OUT}/${name}.exe" ]]; then
    echo "$(wc -c <"${OUT}/${name}.exe" | tr -d ' ') bytes"
    built=$((built + 1))
  else
    echo "SKIPPED (see ${SRC}/${name}.buildlog)"
    skipped=$((skipped + 1))
  fi
}

echo "sources → ${SRC}"

# ── SQLite: a database engine, 255k lines in one file ───────────────────────
if fetch "https://www.sqlite.org/2024/sqlite-amalgamation-3450100.zip" \
  "sqlite.zip" "sqlite-amalgamation-3450100/sqlite3.c"; then
  build sqlite3 "${CC}" -O1 -o "${OUT}/sqlite3.exe" \
    "${SRC}/sqlite-amalgamation-3450100/shell.c" \
    "${SRC}/sqlite-amalgamation-3450100/sqlite3.c" \
    -DSQLITE_THREADSAFE=0 -DSQLITE_OMIT_LOAD_EXTENSION \
    -I "${SRC}/sqlite-amalgamation-3450100"
fi

# ── Lua: a language interpreter, its own allocator and GC ───────────────────
if fetch "https://www.lua.org/ftp/lua-5.4.6.tar.gz" "lua.tar.gz" "lua-5.4.6/src/lua.c"; then
  build lua bash -c "cd '${SRC}/lua-5.4.6/src' && ${CC} -O1 -o '${OUT}/lua.exe' \
    \$(ls *.c | grep -vE '^(luac|onelua)\.c$') -DLUA_USE_C89 -lm"
fi

# ── zlib: the compressor everything links, plus its own test driver ─────────
if fetch "https://github.com/madler/zlib/releases/download/v1.3.1/zlib-1.3.1.tar.gz" \
  "zlib.tar.gz" "zlib-1.3.1/deflate.c"; then
  build minigzip bash -c "cd '${SRC}/zlib-1.3.1' && ${CC} -O1 -o '${OUT}/minigzip.exe' \
    test/minigzip.c \$(ls *.c) -I."
fi

# ── bzip2: a different compressor, heavy on tables and bit twiddling ────────
if fetch "https://sourceware.org/pub/bzip2/bzip2-1.0.8.tar.gz" "bzip2.tar.gz" \
  "bzip2-1.0.8/bzip2.c"; then
  build bzip2 bash -c "cd '${SRC}/bzip2-1.0.8' && ${CC} -O1 -o '${OUT}/bzip2.exe' \
    bzip2.c blocksort.c huffman.c crctable.c randtable.c compress.c decompress.c bzlib.c"
fi

# ── zstd: modern compression, wide integer and SIMD-friendly code ───────────
if fetch "https://github.com/facebook/zstd/releases/download/v1.5.6/zstd-1.5.6.tar.gz" \
  "zstd.tar.gz" "zstd-1.5.6/lib/compress/zstd_compress.c"; then
  build zstd bash -c "cd '${SRC}/zstd-1.5.6' && ${CC} -O1 -o '${OUT}/zstd.exe' \
    \$(find lib programs -name '*.c' -not -path '*legacy*') \
    -Ilib -Ilib/common -Ilib/compress -Ilib/decompress -Ilib/dictBuilder -Iprograms \
    -DZSTD_LEGACY_SUPPORT=0 -DZSTD_MULTITHREAD=0"
fi

# ── jq's core: JSON parsing, a bytecode VM, decimal arithmetic ──────────────
if fetch "https://github.com/DaveGamble/cJSON/archive/refs/tags/v1.7.18.tar.gz" \
  "cjson.tar.gz" "cJSON-1.7.18/cJSON.c"; then
  build cjson bash -c "cd '${SRC}/cJSON-1.7.18' && ${CC} -O1 -o '${OUT}/cjson.exe' \
    cJSON.c cJSON_Utils.c test.c -I."
fi

echo
echo "${built} built, ${skipped} skipped → ${OUT}"
ls -la "${OUT}" 2>/dev/null | tail -n +2 | awk '{printf "  %-18s %s bytes\n", $9, $5}'
