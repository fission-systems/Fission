# CMakeLists.txt for Ghidra Decompiler Native Shared Library (FFI)

cmake_minimum_required(VERSION 3.16)
project(fission_native CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Platform settings
if(MSVC)
    add_compile_options(/W0 /EHsc /bigobj)
    add_definitions(-D_CRT_SECURE_NO_WARNINGS)
    add_definitions(-DWIN32_LEAN_AND_MEAN -DNOMINMAX -DNOGDI)
else()
    add_compile_options(-w -Wno-sign-compare -fPIC)
    add_definitions(-D__TERMINAL__)
endif()

# Find dependencies via vcpkg
find_package(ZLIB REQUIRED)

# Ghidra Core Sources
set(CORE_SOURCES
    xml.cc marshal.cc space.cc float.cc address.cc pcoderaw.cc translate.cc opcodes.cc globalcontext.cc
    capability.cc architecture.cc options.cc graph.cc cover.cc block.cc cast.cc typeop.cc database.cc
    cpool.cc comment.cc stringmanage.cc modelrules.cc fspec.cc action.cc loadimage.cc grammar.cc
    varnode.cc op.cc type.cc variable.cc varmap.cc jumptable.cc emulate.cc emulateutil.cc flow.cc
    userop.cc multiprecision.cc funcdata.cc funcdata_block.cc funcdata_op.cc funcdata_varnode.cc
    unionresolve.cc pcodeinject.cc heritage.cc prefersplit.cc rangeutil.cc ruleaction.cc subflow.cc
    blockaction.cc merge.cc double.cc transform.cc constseq.cc coreaction.cc condexe.cc override.cc
    dynamic.cc crc32.cc prettyprint.cc printlanguage.cc printc.cc printjava.cc memstate.cc opbehavior.cc
    paramid.cc signature.cc sleigh.cc pcodeparse.cc pcodecompile.cc sleighbase.cc slghsymbol.cc
    slghpatexpress.cc slghpattern.cc semantics.cc context.cc slaformat.cc compression.cc filemanage.cc
    sleigh_arch.cc inject_sleigh.cc libdecomp.cc interface.cc ifaceterm.cc ifacedecomp.cc callgraph.cc
    codedata.cc testfunction.cc rulecompile.cc
)

# Shared Library (for FFI)
add_library(fission_decompiler SHARED
    wrapper.cpp
    ${CORE_SOURCES}
)

target_include_directories(fission_decompiler PUBLIC 
    ${CMAKE_CURRENT_SOURCE_DIR}
)

target_link_libraries(fission_decompiler 
    PRIVATE 
    ZLIB::ZLIB
)

message(STATUS "Ghidra FFI Library Configured")
