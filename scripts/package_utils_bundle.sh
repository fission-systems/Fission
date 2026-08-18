#!/usr/bin/env bash
# Package utils/ into the fission-utils bundle published as an `assets-v*`
# release and consumed by .github/actions/setup-utils.
#
# Runs identically in CI and locally. `utils/` is gitignored, so a bundle built
# from a fresh checkout carries only what setup-utils restored -- the authoring
# tree is the only place a complete one can be built.
#
# The inventory describes the ARCHIVE, not the working tree. assets-v3's
# inventory reported file_count=3468 while its archive held 1048 files, so the
# one number available to check a bundle against described something else.
#
# Two kinds of file are excluded, each after checking it is unreachable.
#
# .gdt archives: read only by the offline extractors (scripts/gdt_extract_*.py,
# scripts/win32_enum_groups.py) which run here, never by Fission.
# `get_all_gdt_paths` has no callers and no `.gdt` path is opened anywhere
# outside path_config.rs. 20.7M.
#
# signatures/fidb_java/: `find_fid_file` falls back from `<name>.fidbf` to
# `<name>.fidb` only when the .fidbf is absent, and all 47 .fidb files have a
# .fidbf sibling -- the fallback cannot fire. 64.5M, and it barely compresses
# (1.1x) because Ghidra already stores it deflated.
set -euo pipefail

out="${1:-fission-utils.tar.gz}"
tag="${ASSETS_TAG:-unset}"
ref="${SOURCE_REF:-$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)}"

test -d utils || { echo "no utils/ directory here" >&2; exit 1; }

slaspec_count=$(find utils/sleigh-specs -name '*.slaspec' 2>/dev/null | wc -l | tr -d ' ')
if [ "${slaspec_count}" -le 10 ]; then
  echo "refusing to package: only ${slaspec_count} slaspec files, tree looks incomplete" >&2
  exit 1
fi

tar --exclude='*.gdt' --exclude='.DS_Store' \
    --exclude='utils/signatures/fidb_java' -czf "${out}" utils
shasum -a 256 "${out}" > "${out}.sha256"

archive_files=$(tar tzf "${out}" | grep -vc '/$')
tree_files=$(find utils -type f ! -name '*.gdt' ! -name '.DS_Store' \
  ! -path 'utils/signatures/fidb_java/*' | wc -l | tr -d ' ')
if [ "${archive_files}" -ne "${tree_files}" ]; then
  echo "archive holds ${archive_files} files, tree has ${tree_files} -- refusing" >&2
  exit 1
fi

{
  echo "fission-utils bundle inventory"
  echo "assets_tag=${tag}"
  echo "source_ref=${ref}"
  echo "source_sha=$(git rev-parse HEAD 2>/dev/null || echo unknown)"
  echo "slaspec_count=${slaspec_count}"
  echo "file_count=${archive_files}   # counted in the archive, not the tree"
  echo "excluded=*.gdt (build-time only), signatures/fidb_java (unreachable fallback), .DS_Store"
  echo "archive_bytes=$(wc -c < "${out}" | tr -d ' ')"
  echo
  echo "archive files by top-level directory:"
  tar tzf "${out}" | grep -v '/$' | cut -d/ -f2 | sort | uniq -c | sort -rn
} | tee "${out%.tar.gz}.inventory.txt"

ls -lh "${out}"
