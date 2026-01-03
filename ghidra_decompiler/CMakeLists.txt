# CMakeLists.txt for Ghidra Decompiler Native CLI

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

# Ghidra Core Sources (in ghidra/ subdirectory)
set(GHIDRA_DIR ${CMAKE_CURRENT_SOURCE_DIR}/decompile)
set(CORE_SOURCES
    ${GHIDRA_DIR}/xml.cc ${GHIDRA_DIR}/marshal.cc ${GHIDRA_DIR}/space.cc ${GHIDRA_DIR}/float.cc
    ${GHIDRA_DIR}/address.cc ${GHIDRA_DIR}/pcoderaw.cc ${GHIDRA_DIR}/translate.cc ${GHIDRA_DIR}/opcodes.cc
    ${GHIDRA_DIR}/globalcontext.cc ${GHIDRA_DIR}/capability.cc ${GHIDRA_DIR}/architecture.cc
    ${GHIDRA_DIR}/options.cc ${GHIDRA_DIR}/graph.cc ${GHIDRA_DIR}/cover.cc ${GHIDRA_DIR}/block.cc
    ${GHIDRA_DIR}/cast.cc ${GHIDRA_DIR}/typeop.cc ${GHIDRA_DIR}/database.cc ${GHIDRA_DIR}/cpool.cc
    ${GHIDRA_DIR}/comment.cc ${GHIDRA_DIR}/stringmanage.cc ${GHIDRA_DIR}/modelrules.cc ${GHIDRA_DIR}/fspec.cc
    ${GHIDRA_DIR}/action.cc ${GHIDRA_DIR}/loadimage.cc ${GHIDRA_DIR}/grammar.cc ${GHIDRA_DIR}/varnode.cc
    ${GHIDRA_DIR}/op.cc ${GHIDRA_DIR}/type.cc ${GHIDRA_DIR}/variable.cc ${GHIDRA_DIR}/varmap.cc
    ${GHIDRA_DIR}/jumptable.cc ${GHIDRA_DIR}/emulate.cc ${GHIDRA_DIR}/emulateutil.cc ${GHIDRA_DIR}/flow.cc
    ${GHIDRA_DIR}/userop.cc ${GHIDRA_DIR}/multiprecision.cc ${GHIDRA_DIR}/funcdata.cc
    ${GHIDRA_DIR}/funcdata_block.cc ${GHIDRA_DIR}/funcdata_op.cc ${GHIDRA_DIR}/funcdata_varnode.cc
    ${GHIDRA_DIR}/unionresolve.cc ${GHIDRA_DIR}/pcodeinject.cc ${GHIDRA_DIR}/heritage.cc
    ${GHIDRA_DIR}/prefersplit.cc ${GHIDRA_DIR}/rangeutil.cc ${GHIDRA_DIR}/ruleaction.cc ${GHIDRA_DIR}/subflow.cc
    ${GHIDRA_DIR}/blockaction.cc ${GHIDRA_DIR}/merge.cc ${GHIDRA_DIR}/double.cc ${GHIDRA_DIR}/transform.cc
    ${GHIDRA_DIR}/constseq.cc ${GHIDRA_DIR}/coreaction.cc ${GHIDRA_DIR}/condexe.cc ${GHIDRA_DIR}/override.cc
    ${GHIDRA_DIR}/dynamic.cc ${GHIDRA_DIR}/crc32.cc ${GHIDRA_DIR}/prettyprint.cc ${GHIDRA_DIR}/printlanguage.cc
    ${GHIDRA_DIR}/printc.cc ${GHIDRA_DIR}/printjava.cc ${GHIDRA_DIR}/memstate.cc ${GHIDRA_DIR}/opbehavior.cc
    ${GHIDRA_DIR}/paramid.cc ${GHIDRA_DIR}/signature.cc ${GHIDRA_DIR}/sleigh.cc ${GHIDRA_DIR}/pcodeparse.cc
    ${GHIDRA_DIR}/pcodecompile.cc ${GHIDRA_DIR}/sleighbase.cc ${GHIDRA_DIR}/slghsymbol.cc
    ${GHIDRA_DIR}/slghpatexpress.cc ${GHIDRA_DIR}/slghpattern.cc ${GHIDRA_DIR}/semantics.cc
    ${GHIDRA_DIR}/context.cc ${GHIDRA_DIR}/slaformat.cc ${GHIDRA_DIR}/compression.cc ${GHIDRA_DIR}/filemanage.cc
    ${GHIDRA_DIR}/sleigh_arch.cc ${GHIDRA_DIR}/inject_sleigh.cc ${GHIDRA_DIR}/libdecomp.cc
    ${GHIDRA_DIR}/interface.cc ${GHIDRA_DIR}/ifaceterm.cc ${GHIDRA_DIR}/ifacedecomp.cc ${GHIDRA_DIR}/callgraph.cc
    ${GHIDRA_DIR}/codedata.cc ${GHIDRA_DIR}/testfunction.cc ${GHIDRA_DIR}/rulecompile.cc
)

# Fission Modules
set(FISSION_MODULES_DIR ${CMAKE_CURRENT_SOURCE_DIR}/src)
set(FISSION_SOURCES
    ${FISSION_MODULES_DIR}/utils/encoding.cc
    ${FISSION_MODULES_DIR}/utils/json_utils.cc
    ${FISSION_MODULES_DIR}/utils/file_utils.cc
    ${FISSION_MODULES_DIR}/utils/logger.cc
    ${FISSION_MODULES_DIR}/types/GdtBinaryParser.cc
    ${FISSION_MODULES_DIR}/types/GuidParser.cc
    ${FISSION_MODULES_DIR}/types/RttiAnalyzer.cc
    ${FISSION_MODULES_DIR}/types/PatternLoader.cc
    ${FISSION_MODULES_DIR}/types/TypeManager.cc
    ${FISSION_MODULES_DIR}/types/StructureAnalyzer.cc
    ${FISSION_MODULES_DIR}/types/TypeResolver.cc
    ${FISSION_MODULES_DIR}/types/PrototypeEnforcer.cc
    ${FISSION_MODULES_DIR}/types/GlobalTypeRegistry.cc
    ${FISSION_MODULES_DIR}/analysis/EmulationAnalyzer.cc
    ${FISSION_MODULES_DIR}/analysis/CallGraphAnalyzer.cc
    ${FISSION_MODULES_DIR}/analysis/FunctionMatcher.cc
    ${FISSION_MODULES_DIR}/analysis/InternalMatcher.cc
    ${FISSION_MODULES_DIR}/analysis/FidDatabase.cc
    ${FISSION_MODULES_DIR}/analysis/TypePropagator.cc
    ${FISSION_MODULES_DIR}/analysis/CallingConvDetector.cc
    ${FISSION_MODULES_DIR}/analysis/StackFrameAnalyzer.cc
    ${FISSION_MODULES_DIR}/analysis/TypeSharing.cc
    ${FISSION_MODULES_DIR}/analysis/GlobalDataAnalyzer.cc
    ${FISSION_MODULES_DIR}/analysis/VTableAnalyzer.cc
    ${FISSION_MODULES_DIR}/core/CliArchitecture.cc
    ${FISSION_MODULES_DIR}/core/DecompilerContext.cc
    ${FISSION_MODULES_DIR}/processing/Constants.cc
    ${FISSION_MODULES_DIR}/processing/PostProcessors.cc
    ${FISSION_MODULES_DIR}/processing/StringScanner.cc
    ${FISSION_MODULES_DIR}/loader/PeHeader.cc
    ${FISSION_MODULES_DIR}/loader/SymbolLoader.cc
    ${FISSION_MODULES_DIR}/loader/BinaryDetector.cc
    # MemoryImage is header-only
)

# Fission Decompiler CLI
add_executable(fission_decomp
    fission_decomp.cpp
    ${CORE_SOURCES}
    ${FISSION_SOURCES}
)

# Fission Decompiler Shared Library (for FFI)
set(FFI_SOURCES
    ${FISSION_MODULES_DIR}/ffi/libdecomp_ffi.cpp
)

add_library(decomp SHARED
    ${CORE_SOURCES}
    ${FISSION_SOURCES}
    ${FFI_SOURCES}
)

# Export symbols for shared library
target_compile_definitions(decomp PRIVATE DECOMP_EXPORTS)

target_include_directories(fission_decomp PUBLIC 
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${GHIDRA_DIR}
    ${CMAKE_CURRENT_SOURCE_DIR}/include
)

target_include_directories(decomp PUBLIC 
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${GHIDRA_DIR}
    ${CMAKE_CURRENT_SOURCE_DIR}/include
)

target_link_libraries(fission_decomp 
    PRIVATE 
    ZLIB::ZLIB
)

target_link_libraries(decomp 
    PRIVATE 
    ZLIB::ZLIB
)

message(STATUS "Ghidra Decompiler CLI Configured")
message(STATUS "  Ghidra sources: ghidra/")
message(STATUS "  Fission modules: fission/")
