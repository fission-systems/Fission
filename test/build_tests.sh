#!/bin/bash
# build_tests.sh - Compile all Fission test binaries

set -e

echo "🔨 Building Fission Test Suite"
echo "================================"

# Determine architecture
if [[ $(uname -m) == "arm64" ]]; then
    echo "✨ Detected Apple Silicon - compiling for x86-64"
    ARCH_FLAG="-arch x86_64"
else
    echo "✨ Detected x86-64 architecture"
    ARCH_FLAG=""
fi

# Check if MinGW is available
if command -v x86_64-w64-mingw32-gcc &> /dev/null; then
    MINGW_AVAILABLE=true
    echo "✅ MinGW cross-compiler found"
else
    MINGW_AVAILABLE=false
    echo "⚠️  MinGW not found - Windows binaries will be skipped"
    echo "    Install with: brew install mingw-w64"
fi

# Control flow tests
echo ""
echo "📦 Building control_flow_test..."
gcc $ARCH_FLAG -O2 -o control_flow_test_x64 control_flow_test.c
echo "   ✓ control_flow_test_x64 (Mach-O/ELF)"

if [ "$MINGW_AVAILABLE" = true ]; then
    x86_64-w64-mingw32-gcc -O2 -o control_flow_test_x64.exe control_flow_test.c
    echo "   ✓ control_flow_test_x64.exe (PE 64-bit)"
    
    i686-w64-mingw32-gcc -O2 -o control_flow_test_x86.exe control_flow_test.c
    echo "   ✓ control_flow_test_x86.exe (PE 32-bit)"
fi

# Data type tests
echo ""
echo "📦 Building datatype_test..."
gcc $ARCH_FLAG -O2 -o datatype_test_x64 datatype_test.c 2>&1 | grep -v "warning: .sizeof. on array" || true
echo "   ✓ datatype_test_x64 (Mach-O/ELF)"

if [ "$MINGW_AVAILABLE" = true ]; then
    x86_64-w64-mingw32-gcc -O2 -o datatype_test_x64.exe datatype_test.c 2>&1 | grep -v "warning: .sizeof. on array" || true
    echo "   ✓ datatype_test_x64.exe (PE 64-bit)"
    
    i686-w64-mingw32-gcc -O2 -o datatype_test_x86.exe datatype_test.c 2>&1 | grep -v "warning: .sizeof. on array" || true
    echo "   ✓ datatype_test_x86.exe (PE 32-bit)"
fi

# Structure tests
echo ""
echo "📦 Building struct_test..."
gcc $ARCH_FLAG -O2 -o struct_test_x64 struct_test.c
echo "   ✓ struct_test_x64 (Mach-O/ELF)"

if [ "$MINGW_AVAILABLE" = true ]; then
    x86_64-w64-mingw32-gcc -O2 -o struct_test_x64.exe struct_test.c
    echo "   ✓ struct_test_x64.exe (PE 64-bit)"
    
    i686-w64-mingw32-gcc -O2 -o struct_test_x86.exe struct_test.c
    echo "   ✓ struct_test_x86.exe (PE 32-bit)"
fi

# Windows API tests (Windows only)
if [ "$MINGW_AVAILABLE" = true ]; then
    echo ""
    echo "📦 Building winapi_test..."
    x86_64-w64-mingw32-gcc -O2 -o winapi_test.exe winapi_test.c -ladvapi32 -luser32 -lws2_32
    echo "   ✓ winapi_test.exe (PE 64-bit)"
fi

# Summary
echo ""
echo "================================"
echo "✅ All tests built successfully!"
echo ""
echo "📊 Generated binaries:"
ls -lh *.exe *_x64 2>/dev/null | grep -E '\.exe$|_x64$|_x86\.exe$' | awk '{printf "   %s (%s)\n", $9, $5}'
echo ""
echo "🧪 Test with Fission:"
echo "   cargo run --bin fission_cli -- test/control_flow_test_x64.exe --info"
echo "   cargo run --bin fission_cli --features native_decomp -- test/control_flow_test_x64.exe --decomp 0x140001a20"
