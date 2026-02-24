/**
 * Fission Decompiler CLI
 *
 * Standalone subprocess decompiler that reads JSON from stdin and outputs C code to stdout.
 * Reads one JSON request from stdin, processes it, writes response to stdout, and exits.
 *
 * Input (stdin):  {"bytes":"BASE64","address":12345,"is_64bit":true,"sla_dir":"/path"}
 * Output (stdout): {"status":"ok","code":"..."} or {"status":"error","message":"..."}
 */

#include <cstdlib>
#include <iostream>
#include <map>
#include <cstdint>
#include <sstream>
#include <string>
#include "fission/decompiler/DecompilationPipeline.h"
#include "fission/core/DecompilerContext.h"
#include "fission/utils/logger.h"

// struct_registry is now a member of DecompilerContext (see DecompilerContext.h)

int main(int /*argc*/, char** /*argv*/) {
    // Initialize logger (optional file sink)
    const char* log_file = std::getenv("FISSION_LOG_FILE");
    if (log_file) {
        fission::utils::Logger::initialize(log_file);
    }

    std::cout.setf(std::ios::unitbuf);

    // Read all of stdin
    std::stringstream buffer;
    buffer << std::cin.rdbuf();
    std::string input = buffer.str();

    if (input.empty()) {
        std::cout << "{\"status\":\"error\",\"message\":\"No input provided\"}" << std::endl;
        return 1;
    }

    fission::core::DecompilerContext state;
    std::string response = fission::decompiler::DecompilationPipeline::process_request(state, input);
    std::cout << response << std::endl;
    std::cout.flush();
    _exit(0);  // Skip cleanup to avoid Ghidra memory corruption crash
}
