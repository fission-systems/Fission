#!/bin/bash
set -e

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${GREEN}[*] Building Ghidra Decompiler...${NC}"

# Check for CMake
if ! command -v cmake &> /dev/null; then
    echo -e "${RED}[!] CMake could not be found. Please install CMake.${NC}"
    exit 1
fi

# Check for Make or Ninja
if ! command -v make &> /dev/null && ! command -v ninja &> /dev/null; then
    echo -e "${RED}[!] Make or Ninja could not be found. Please install build tools.${NC}"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Directory setup
DECOMPILER_DIR="$REPO_ROOT/ghidra_decompiler"
BUILD_DIR="${DECOMPILER_DIR}/build"

if [ ! -d "$DECOMPILER_DIR" ]; then
    echo -e "${RED}[!] Directory $DECOMPILER_DIR does not exist.${NC}"
    exit 1
fi

mkdir -p "$BUILD_DIR"
cd "$BUILD_DIR"

# Configure
echo -e "${GREEN}[*] Configuring with CMake...${NC}"
cmake ..

# Build
echo -e "${GREEN}[*] Compiling...${NC}"
cmake --build . --config Release

echo -e "${GREEN}[✓] Decompiler built successfully!${NC}"
