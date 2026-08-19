#!/usr/bin/env bash
# Move utils/ from the flat `signatures/` tree to the lifecycle split.
#
#   packed/   what the runtime reads: .fpk only
#   runtime/  what the runtime reads and is not packed yet
#   source/   what the packers read; never shipped
#
# The old tree mixed all three in one directory -- signatures/fid held 228 .fpk,
# 57 .fidbf and 5 .txt -- so nothing in the layout said which files ship. The
# bundle script had to encode that as a list of extensions and "exclude the
# source when its .fpk exists" rules; with the split it excludes one directory.
#
# Idempotent: a tree already migrated is left alone. `--undo` reverses it.
set -euo pipefail

root="${UTILS_ROOT:-utils}"
test -d "${root}" || { echo "no ${root}/ here" >&2; exit 1; }

undo=""
[ "${1:-}" = "--undo" ] && undo=1

move() { # move <from> <to>
  local from="$1" to="$2"
  [ -e "${from}" ] || return 0
  mkdir -p "$(dirname "${to}")"
  mv "${from}" "${to}"
  echo "  ${from} -> ${to}"
}

if [ -z "${undo}" ]; then
  echo "migrating ${root}/ to the split layout"
  mkdir -p "${root}"/{packed/{fid,signatures,typeinfo,ordinals},runtime/{typeinfo,die,patterns,ghidra-data,sleigh},source/{fid,gdt,ordinals,typeinfo}}

  # packed: the .fpk the runtime reads
  for f in "${root}"/signatures/fid/*.fpk;                    do move "${f}" "${root}/packed/fid/$(basename "${f}")"; done
  for f in "${root}"/signatures/typeinfo/*/*_signatures.fpk;  do move "${f}" "${root}/packed/signatures/$(basename "${f}")"; done
  for f in "${root}"/signatures/typeinfo/golang/*.fpk;        do move "${f}" "${root}/packed/typeinfo/$(basename "${f}")"; done
  for f in "${root}"/signatures/ordinals/*.fpk;               do move "${f}" "${root}/packed/ordinals/$(basename "${f}")"; done
  move "${root}/ghidra-data/ghidra_exports.fpk" "${root}/packed/ordinals/ghidra_exports.fpk"

  # source: packer inputs
  for f in "${root}"/signatures/fid/*.fidbf;                  do move "${f}" "${root}/source/fid/$(basename "${f}")"; done
  for f in "${root}"/signatures/typeinfo/*/*.gdt;             do move "${f}" "${root}/source/gdt/$(basename "${f}")"; done
  for f in "${root}"/signatures/ordinals/*_ordinals.json;     do move "${f}" "${root}/source/ordinals/$(basename "${f}")"; done
  for f in "${root}"/signatures/typeinfo/golang/go1.*.json;   do move "${f}" "${root}/source/typeinfo/$(basename "${f}")"; done
  for f in "${root}"/signatures/typeinfo/*/*_signatures.txt;  do move "${f}" "${root}/source/typeinfo/$(basename "${f}")"; done

  # runtime: read directly, not packed
  move "${root}/signatures/die"      "${root}/runtime/die"
  move "${root}/signatures/patterns" "${root}/runtime/patterns"
  move "${root}/ghidra-data"         "${root}/runtime/ghidra-data"
  move "${root}/sleigh-specs"        "${root}/runtime/sleigh"
  # what is left of typeinfo is struct layouts and enum groups the runtime reads
  if [ -d "${root}/signatures/typeinfo" ]; then
    for d in "${root}"/signatures/typeinfo/*/; do
      [ -d "${d}" ] || continue
      move "${d}" "${root}/runtime/typeinfo/$(basename "${d}")"
    done
  fi
  # fidb_java is unreachable (every .fidb has a .fidbf sibling); park it in source
  move "${root}/signatures/fidb_java" "${root}/source/fidb_java"

  find "${root}/signatures" -type d -empty -delete 2>/dev/null || true
  rmdir "${root}/signatures" 2>/dev/null || true
else
  echo "reversing the split layout"
  mkdir -p "${root}"/signatures/{fid,ordinals,typeinfo}
  for f in "${root}"/packed/fid/*;         do move "${f}" "${root}/signatures/fid/$(basename "${f}")"; done
  for f in "${root}"/source/fid/*;         do move "${f}" "${root}/signatures/fid/$(basename "${f}")"; done
  for f in "${root}"/packed/ordinals/*;    do move "${f}" "${root}/signatures/ordinals/$(basename "${f}")"; done
  for f in "${root}"/source/ordinals/*;    do move "${f}" "${root}/signatures/ordinals/$(basename "${f}")"; done
  for d in "${root}"/runtime/typeinfo/*/;  do [ -d "${d}" ] && move "${d}" "${root}/signatures/typeinfo/$(basename "${d}")"; done
  move "${root}/runtime/die"          "${root}/signatures/die"
  move "${root}/runtime/patterns"     "${root}/signatures/patterns"
  move "${root}/runtime/ghidra-data"  "${root}/ghidra-data"
  move "${root}/runtime/sleigh"       "${root}/sleigh-specs"
  move "${root}/source/fidb_java"     "${root}/signatures/fidb_java"
  find "${root}/packed" "${root}/runtime" "${root}/source" -type d -empty -delete 2>/dev/null || true
fi

echo
find "${root}" -maxdepth 2 -type d | sort | sed 's/^/  /'
