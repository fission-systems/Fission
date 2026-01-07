#include "fission/core/ContextServices.h"
#include "fission/core/ArchInit.h"
#include "fission/ffi/DecompContext.h"
#include "address.hh"
#include "funcdata.hh"

#include <iostream>
#include <vector>

namespace {
int failures = 0;

void expect_true(bool condition, const char* message) {
    if (!condition) {
        std::cerr << "[FAIL] " << message << std::endl;
        ++failures;
    }
}

void test_symbols() {
    fission::ffi::DecompContext ctx(nullptr);

    fission::core::add_symbol(&ctx, 0x1000, "func_a");
    expect_true(ctx.symbols.size() == 1, "add_symbol should add entry");
    expect_true(ctx.symbols[0x1000] == "func_a", "add_symbol should store name");

    fission::core::add_global_symbol(&ctx, 0x2000, "global_a");
    expect_true(ctx.global_symbols.size() == 1, "add_global_symbol should add entry");
    expect_true(ctx.global_symbols[0x2000] == "global_a", "add_global_symbol should store name");

    fission::core::clear_symbols(&ctx);
    expect_true(ctx.symbols.empty(), "clear_symbols should clear entries");

    fission::core::clear_global_symbols(&ctx);
    expect_true(ctx.global_symbols.empty(), "clear_global_symbols should clear entries");

    DecompError rc = fission::core::add_function(&ctx, 0x3000, "func_b");
    expect_true(rc == DECOMP_OK, "add_function should succeed without arch");
    expect_true(ctx.symbols[0x3000] == "func_b", "add_function should store name");
}

void test_memory() {
    fission::ffi::DecompContext ctx(nullptr);
    std::vector<uint8_t> data = {0x90, 0x90, 0x90, 0x90};

    DecompError rc = fission::core::load_binary(&ctx, data.data(), data.size(), 0x140000000, true);
    expect_true(rc == DECOMP_OK, "load_binary should succeed");
    expect_true(ctx.binary_data.size() == data.size(), "binary_data size should match");
    expect_true(ctx.base_addr == 0x140000000, "base_addr should be set");
    expect_true(ctx.is_64bit, "is_64bit should be set");
    expect_true(ctx.memory_image != nullptr, "memory_image should be created");

    rc = fission::core::add_memory_block(
        &ctx,
        ".text",
        0x140001000,
        0x200,
        0x0,
        0x200,
        true,
        false
    );
    expect_true(rc == DECOMP_OK, "add_memory_block should succeed");
    expect_true(ctx.memory_blocks.size() == 1, "memory_blocks should have one entry");
    expect_true(ctx.memory_blocks[0].name == ".text", "memory block name should match");
    expect_true(ctx.memory_blocks[0].va_addr == 0x140001000, "memory block VA should match");
}

void test_symbol_provider_switch() {
    fission::ffi::DecompContext ctx(nullptr);
    int drop_calls = 0;

    auto drop_cb = [](void* userdata) {
        int* counter = static_cast<int*>(userdata);
        if (counter) {
            (*counter)++;
        }
    };

    DecompSymbolProvider provider{};
    provider.userdata = &drop_calls;
    provider.drop = drop_cb;

    fission::core::set_symbol_provider(&ctx, &provider);
    expect_true(ctx.symbol_provider_enabled, "symbol provider should be enabled");
    expect_true(ctx.symbol_provider != nullptr, "symbol provider instance should be created");
    expect_true(drop_calls == 0, "drop should not be called on initial set");

    fission::core::reset_symbol_provider(&ctx);
    expect_true(!ctx.symbol_provider_enabled, "symbol provider should be disabled");
    expect_true(drop_calls == 1, "drop should be called when provider cleared");
}

void test_function_registration() {
    const char* sla_dir = FISSION_SLA_DIR;
    bool initialized = fission::ffi::initialize_ghidra_library(sla_dir);
    expect_true(initialized, "initialize_ghidra_library should succeed");

    fission::ffi::DecompContext ctx(sla_dir);
    std::vector<uint8_t> data = {0x90};

    DecompError rc = fission::core::load_binary(&ctx, data.data(), data.size(), 0x1000, true);
    expect_true(rc == DECOMP_OK, "load_binary should succeed for arch init");

    rc = fission::core::add_memory_block(
        &ctx,
        ".text",
        0x1000,
        0x100,
        0x0,
        0x100,
        true,
        false
    );
    expect_true(rc == DECOMP_OK, "add_memory_block should succeed for arch init");

    fission::core::initialize_architecture(&ctx);
    expect_true(ctx.arch != nullptr, "initialize_architecture should create arch");

    rc = fission::core::add_function(&ctx, 0x1000, "func_register");
    expect_true(rc == DECOMP_OK, "add_function should succeed with arch");

    ghidra::Scope* global_scope = ctx.arch->symboltab->getGlobalScope();
    expect_true(global_scope != nullptr, "global scope should exist");

    ghidra::Address func_addr(ctx.arch->getDefaultCodeSpace(), 0x1000);
    ghidra::Funcdata* fd = global_scope->findFunction(func_addr);
    expect_true(fd != nullptr, "function should be registered in global scope");
}
}

int main() {
    test_symbols();
    test_memory();
    test_symbol_provider_switch();
    test_function_registration();

    if (failures == 0) {
        std::cout << "All ContextServices tests passed." << std::endl;
        return 0;
    }

    std::cerr << failures << " ContextServices tests failed." << std::endl;
    return 1;
}
