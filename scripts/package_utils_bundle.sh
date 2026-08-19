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
#
# Sources that a `.fpk` now replaces. The packed form is what the runtime reads;
# the original is what the packer reads, and packing happens here, not on the
# machine that installs the bundle. Each is excluded only where its `.fpk`
# exists, so a half-converted tree ships the source rather than nothing:
#   *.fidbf                     -> <name>.{lib,fn,rel,dom}.fpk
#   *_signatures.txt            -> <name>_signatures.fpk
#   {x86,arm}_ordinals.json     -> ordinals.fpk
#   go1.X.json                  -> go1.X.{fn,ty}.fpk
#
# windows_vs12_*.gdt.types.json is excluded outright: nothing reads it, and its
# enum values are wrong -- PAGE_NOACCESS and FILE_SHARE_READ both recorded 264
# where both are 1. `win_enum_constants.json` carries the correct table.
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

# Build the exclusion list, dropping a source only when its .fpk is present.
excludes=(--exclude='*.gdt' --exclude='.DS_Store'
          --exclude='utils/signatures/fidb_java'
          --exclude='*.gdt.types.json')
for fidbf in utils/signatures/fid/*.fidbf; do
  [ -e "${fidbf}" ] || continue
  stem="${fidbf%.fidbf}"
  if [ -f "${stem}.fn.fpk" ] && [ -f "${stem}.lib.fpk" ] \
     && [ -f "${stem}.rel.fpk" ] && [ -f "${stem}.dom.fpk" ]; then
    excludes+=(--exclude="$(basename "${fidbf}")")
  fi
done
for txt in utils/signatures/typeinfo/*/*_signatures.txt; do
  [ -e "${txt}" ] || continue
  [ -f "${txt%.txt}.fpk" ] && excludes+=(--exclude="$(basename "${txt}")")
done
if [ -f utils/signatures/ordinals/ordinals.fpk ]; then
  excludes+=(--exclude='x86_ordinals.json' --exclude='arm_ordinals.json')
fi
for snapshot in utils/signatures/typeinfo/golang/go1.*.json; do
  [ -e "${snapshot}" ] || continue
  stem="${snapshot%.json}"
  if [ -f "${stem}.fn.fpk" ] && [ -f "${stem}.ty.fpk" ]; then
    excludes+=(--exclude="$(basename "${snapshot}")")
  fi
done

tar "${excludes[@]}" -czf "${out}" utils
shasum -a 256 "${out}" > "${out}.sha256"

archive_files=$(tar tzf "${out}" | grep -vc '/$')
# The archive is now a subset of the tree by design, so the check is that it
# still carries what the runtime needs rather than that the counts agree.
for required in \
  utils/signatures/ordinals/ordinals.fpk \
  utils/signatures/typeinfo/win32/win_api_signatures.fpk \
  utils/signatures/fid/vs2019_x64.fn.fpk
do
  if [ -e "${required}" ] && ! tar tzf "${out}" | grep -qxF "${required}"; then
    echo "archive is missing ${required} -- refusing" >&2
    exit 1
  fi
done

{
  echo "fission-utils bundle inventory"
  echo "assets_tag=${tag}"
  echo "source_ref=${ref}"
  echo "source_sha=$(git rev-parse HEAD 2>/dev/null || echo unknown)"
  echo "slaspec_count=${slaspec_count}"
  echo "file_count=${archive_files}   # counted in the archive, not the tree"
  echo "excluded=*.gdt, fidb_java, *.gdt.types.json, and every source a .fpk replaces"
  echo "archive_bytes=$(wc -c < "${out}" | tr -d ' ')"
  echo
  echo "archive files by top-level directory:"
  tar tzf "${out}" | grep -v '/$' | cut -d/ -f2 | sort | uniq -c | sort -rn
} | tee "${out%.tar.gz}.inventory.txt"

ls -lh "${out}"
