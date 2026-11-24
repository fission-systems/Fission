#!/bin/sh
# Rebuild the CSIC fixtures from this directory. Toolchain: zig 0.15.2 and
# llvm-objcopy (Homebrew LLVM). Symbols are kept and debug info dropped:
# the harness names sinks by symbol and the static baseline needs function
# bounds, but DWARF would only make the checked-in files large. UBSan is
# off because zig enables trapping UBSan at -O0, which adds branches the
# source does not have. Outputs land next to this directory.
set -eu
OUT=..
OBJCOPY=${OBJCOPY:-llvm-objcopy}
FLAGS="-fno-sanitize=undefined -static -fno-pie -no-pie -fno-stack-protector -Wl,--build-id=none"
build() { # target opt source output
    zig cc -target "$1" "$2" $FLAGS -o "$OUT/$4" "$3.c"
    "$OBJCOPY" --strip-debug "$OUT/$4"
}
for c in rfc_auth implicit_call false_alias_ptr real_alias joint_or joint_and \
         nondet_time nondet_random crash_null password branch_len; do
    build x86_64-linux-musl -O0 "$c" "x64_${c}.elf"
done
build x86_64-linux-musl -O2 rfc_auth x64_rfc_auth_o2.elf
for c in rfc_auth crash_null password; do
    build aarch64-linux-musl -O0 "$c" "a64_${c}.elf"
done
# P2 cases.
for c in and3 xor_gate segv_grant; do
    build x86_64-linux-musl -O0 "$c" "x64_${c}.elf"
done
build x86_64-linux-musl -O2 xor_gate x64_xor_gate_o2.elf
for c in and3 segv_grant; do
    build aarch64-linux-musl -O0 "$c" "a64_${c}.elf"
done
