#include "fission/decompiler/ServerMode.h"
#include "fission/decompiler/DecompilationPipeline.h"
#include "fission/core/DecompilerContext.h"
#include <iostream>
#include <sstream>
#include <string>
#include <cstdlib>

namespace fission {
namespace decompiler {

int ServerMode::run_server() {
    std::cerr << "[fission_decomp] Server mode started" << std::endl;
    std::cout.setf(std::ios::unitbuf);
    
    core::DecompilerContext state;
    std::string line;
    
    while (std::getline(std::cin, line)) {
        if (line.empty()) continue;
        
        std::string response = DecompilationPipeline::process_request(state, line);
        
        if (response == "__QUIT__") {
            std::cout << "{\"status\":\"ok\",\"message\":\"goodbye\"}" << std::endl;
            break;
        }
        
        std::cout << response << std::endl;
        std::cout.flush();
    }
    
    std::cerr << "[fission_decomp] Server shutting down" << std::endl;
    return 0;
}

int ServerMode::run_single() {
    std::cout.setf(std::ios::unitbuf);
    
    // Read all of stdin
    std::stringstream buffer;
    buffer << std::cin.rdbuf();
    std::string input = buffer.str();
    
    if (input.empty()) {
        std::cout << "{\"status\":\"error\",\"message\":\"No input provided\"}" << std::endl;
        return 1;
    }
    
    core::DecompilerContext state;
    std::string response = DecompilationPipeline::process_request(state, input);
    std::cout << response << std::endl;
    std::cout.flush();
    _exit(0);  // Skip cleanup to avoid Ghidra memory corruption crash
}

} // namespace decompiler
} // namespace fission
