#ifndef FISSION_DECOMPILER_SERVER_MODE_H
#define FISSION_DECOMPILER_SERVER_MODE_H

namespace fission {
namespace decompiler {

/**
 * @brief Server and single-shot mode implementations
 * 
 * Provides two operational modes:
 * - Server mode: Persistent process handling multiple requests
 * - Single-shot mode: Process one request and exit immediately
 */
class ServerMode {
public:
    /**
     * @brief Run in server mode (persistent)
     * 
     * Reads line-delimited JSON from stdin, processes each request,
     * and writes JSON responses to stdout. Continues until "quit" command.
     * 
     * @return Exit code (0 for success)
     */
    static int run_server();
    
    /**
     * @brief Run in single-shot mode (one request)
     * 
     * Reads one JSON request from stdin, processes it, writes response
     * to stdout, and exits immediately. Uses _exit() to avoid cleanup crashes.
     * 
     * @return Exit code (0 for success, 1 for error)
     */
    static int run_single();
};

} // namespace decompiler
} // namespace fission

#endif // FISSION_DECOMPILER_SERVER_MODE_H
