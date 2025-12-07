# CMakeLists.txt for Ghidra Decompiler Library
# This builds libdecomp for use with Fission

cmake_minimum_required(VERSION 3.16)
project(ghidra_decompiler CXX)

set(CMAKE_CXX_STANDARD 11)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Platform-specific settings
if(MSVC)
    add_compile_options(/W0 /EHsc /bigobj)
    add_definitions(-D_CRT_SECURE_NO_WARNINGS)
    # Note: __TERMINAL__ requires termios.h (Unix only), so disabled on Windows
else()
    add_compile_options(-w -Wno-sign-compare)
    # Enable terminal support on Unix
    add_definitions(-D__TERMINAL__)
endif()

# Core source files (from Makefile CORE)
set(CORE_SOURCES
    xml.cc
    marshal.cc
    space.cc
    float.cc
    address.cc
    pcoderaw.cc
    translate.cc
    opcodes.cc
    globalcontext.cc
)

# Decompiler core sources (from Makefile DECCORE)
set(DECCORE_SOURCES
    capability.cc
    architecture.cc
    options.cc
    graph.cc
    cover.cc
    block.cc
    cast.cc
    typeop.cc
    database.cc
    cpool.cc
    comment.cc
    stringmanage.cc
    modelrules.cc
    fspec.cc
    action.cc
    loadimage.cc
    grammar.cc
    varnode.cc
    op.cc
    type.cc
    variable.cc
    varmap.cc
    jumptable.cc
    emulate.cc
    emulateutil.cc
    flow.cc
    userop.cc
    multiprecision.cc
    funcdata.cc
    funcdata_block.cc
    funcdata_op.cc
    funcdata_varnode.cc
    unionresolve.cc
    pcodeinject.cc
    heritage.cc
    prefersplit.cc
    rangeutil.cc
    ruleaction.cc
    subflow.cc
    blockaction.cc
    merge.cc
    double.cc
    transform.cc
    constseq.cc
    coreaction.cc
    condexe.cc
    override.cc
    dynamic.cc
    crc32.cc
    prettyprint.cc
    printlanguage.cc
    printc.cc
    printjava.cc
    memstate.cc
    opbehavior.cc
    paramid.cc
    signature.cc
)

# Sleigh sources (from Makefile SLEIGH) - WITH compression now that zlib is available
set(SLEIGH_SOURCES
    sleigh.cc
    pcodeparse.cc
    pcodecompile.cc
    sleighbase.cc
    slghsymbol.cc
    slghpatexpress.cc
    slghpattern.cc
    semantics.cc
    context.cc
    slaformat.cc
    compression.cc
    filemanage.cc
)

# Extra sources
# Note: Still excluding BFD-dependent files (not available on Windows without MinGW)
set(EXTRA_SOURCES
    sleigh_arch.cc
    inject_sleigh.cc
    # raw_arch.cc      # Requires libbfd
    # loadimage_xml.cc # Would benefit from libxml but not critical
    libdecomp.cc
    interface.cc
    ifaceterm.cc
    ifacedecomp.cc
    callgraph.cc
    codedata.cc
    testfunction.cc
    # analyzesigs.cc  # Requires libbfd
    rulecompile.cc
)

# Fission wrapper
set(WRAPPER_SOURCES
    wrapper.cpp
)

# Combine all sources
set(ALL_SOURCES
    ${CORE_SOURCES}
    ${DECCORE_SOURCES}
    ${SLEIGH_SOURCES}
    ${EXTRA_SOURCES}
    ${WRAPPER_SOURCES}
)

# Create static library
add_library(ghidra_decompiler STATIC ${ALL_SOURCES})

# Include directories
target_include_directories(ghidra_decompiler PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})

# Find and link zlib (required for compression)
find_package(ZLIB REQUIRED)
target_link_libraries(ghidra_decompiler PRIVATE ZLIB::ZLIB)

# Install
install(TARGETS ghidra_decompiler
    LIBRARY DESTINATION lib
    ARCHIVE DESTINATION lib
)

install(FILES wrapper.h DESTINATION include)

message(STATUS "Ghidra Decompiler Library configured")
message(STATUS "  Sources: ${CMAKE_CURRENT_SOURCE_DIR}")
message(STATUS "  Build type: ${CMAKE_BUILD_TYPE}")
message(STATUS "  ZLIB found: ${ZLIB_INCLUDE_DIRS}")
