# CMakeLists.txt for Ghidra Decompiler gRPC Server

cmake_minimum_required(VERSION 3.16)
project(ghidra_server CXX)

set(CMAKE_CXX_STANDARD 17) # gRPC requires C++14 or higher
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Platform settings
if(MSVC)
    add_compile_options(/W0 /EHsc /bigobj)
    add_definitions(-D_CRT_SECURE_NO_WARNINGS)
else()
    add_compile_options(-w -Wno-sign-compare)
    add_definitions(-D__TERMINAL__)
endif()

# Find dependencie via vcpkg
find_package(gRPC CONFIG REQUIRED)
find_package(Protobuf CONFIG REQUIRED)
find_package(ZLIB REQUIRED) # Now we have zlib!

# Protocol Buffers setup
set(PROTO_DIR "../protos")
set(PROTO_SRC "${CMAKE_CURRENT_BINARY_DIR}/ghidra_service.pb.cc")
set(PROTO_HDR "${CMAKE_CURRENT_BINARY_DIR}/ghidra_service.pb.h")
set(GRPC_SRC "${CMAKE_CURRENT_BINARY_DIR}/ghidra_service.grpc.pb.cc")
set(GRPC_HDR "${CMAKE_CURRENT_BINARY_DIR}/ghidra_service.grpc.pb.h")

add_custom_command(
    OUTPUT "${PROTO_SRC}" "${PROTO_HDR}" "${GRPC_SRC}" "${GRPC_HDR}"
    COMMAND protobuf::protoc
    ARGS --grpc_out="${CMAKE_CURRENT_BINARY_DIR}"
         --cpp_out="${CMAKE_CURRENT_BINARY_DIR}"
         -I "${PROTO_DIR}"
         --plugin=protoc-gen-grpc=$<TARGET_FILE:gM::grpc_cpp_plugin>
         "${PROTO_DIR}/ghidra_service.proto"
    DEPENDS "${PROTO_DIR}/ghidra_service.proto"
)

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

# Wrapper/Server Sources
set(SERVER_SOURCES
    server_main.cc
    # loadimage_bfd.hh is a stub now, so we don't include BFD sources
)

# Executable
add_executable(ghidra_server 
    ${SERVER_SOURCES} 
    ${CORE_SOURCES}
    ${PROTO_SRC}
    ${GRPC_SRC}
)

target_include_directories(ghidra_server PUBLIC 
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${CMAKE_CURRENT_BINARY_DIR}
)

# Start with minimal linking
target_link_libraries(ghidra_server 
    PRIVATE 
    gRPC::grpc++ 
    protobuf::libprotobuf
    ZLIB::ZLIB
)

if(MSVC)
    target_link_options(ghidra_server PRIVATE /SUBSYSTEM:CONSOLE)
endif()

message(STATUS "Ghidra Server Configured")
