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
# Everything under `utils/source/` is a packer input and never ships: the
# `.fidbf` databases, the `.gdt` archives, the ordinal and Go JSON, and the
# signature text. Packing runs here, not on the machine that installs the
# bundle, so those inputs have no reason to travel.
#
# That is the whole exclusion rule now. It used to be a list of extensions
# paired with "drop the source only where its .fpk exists", because the tree
# mixed inputs and outputs in one directory -- signatures/fid held 228 .fpk, 57
# .fidbf and 5 .txt. The layout states it instead.
#
# Two things are dropped that are not packer inputs, each after checking
# nothing reads them:
#   *.gdt.types.json  -- unreferenced, and its enum values are wrong:
#                        PAGE_NOACCESS and FILE_SHARE_READ both recorded 264
#                        where both are 1.
#   *.exports         -- 27.4M of DLL export XML. `ghidra_no_return.rs` walks
#                        those directories but reads only `.hints`, of which
#                        there are four totalling 0.03M.
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

tar --exclude='utils/source' \
    --exclude='.DS_Store' \
    --exclude='*.gdt.types.json' \
    --exclude='*.exports' \
    -czf "${out}" utils
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
  echo "excluded=utils/source (packer inputs), *.gdt.types.json, *.exports, .DS_Store"
  echo "archive_bytes=$(wc -c < "${out}" | tr -d ' ')"
  echo
  echo "archive files by top-level directory:"
  tar tzf "${out}" | grep -v '/$' | cut -d/ -f2 | sort | uniq -c | sort -rn
} | tee "${out%.tar.gz}.inventory.txt"

ls -lh "${out}"
