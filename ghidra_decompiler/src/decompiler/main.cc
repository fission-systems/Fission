/**
 * Fission Decompiler CLI
 * 
 * Standalone subprocess decompiler that reads JSON from stdin and outputs C code to stdout.
 * 
 * Modes:
 *   - Single-shot (default): Process one request and exit
 *   - Server (--server): Keep running and process multiple requests (line-delimited JSON)
 * 
 * Input (stdin): {"bytes":"BASE64_ENCODED_BYTES","address":12345,"is_64bit":true,"sla_dir":"/path"}
 * Output (stdout): {"status":"ok","code":"..."} or {"status":"error","message":"..."}
 */

#include <cstring>
#include <map>
#include <cstdint>
#include <cstdlib>
#include "fission/decompiler/ServerMode.h"
#include "fission/utils/logger.h"

// Global Structure Registry - declared extern, defined in DecompilationPipeline.cc
extern std::map<uint64_t, std::map<int, std::string>> global_struct_registry;

int main(int argc, char** argv) {
    // Check for --server flag
    bool server_mode = false;
    for (int i = 1; i < argc; i++) {
        if (strcmp(argv[i], "--server") == 0 || strcmp(argv[i], "-s") == 0) {
            server_mode = true;
            break;
        }
    }

    // Initialize logger
    const char* log_file = std::getenv("FISSION_LOG_FILE");
    if (log_file) {
        fission::utils::Logger::initialize(log_file);
    }
    
    if (server_mode) {
        return fission::decompiler::ServerMode::run_server();
    } else {
        return fission::decompiler::ServerMode::run_single();
    }
}
