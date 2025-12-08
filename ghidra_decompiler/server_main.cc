/**
 * Ghidra Decompiler gRPC Server
 * 
 * Implements the DecompilerService defined in protos/ghidra_service.proto
 * Full decompilation using SleighArchitecture and PrintC
 */

// gRPC and standard headers FIRST
#include <iostream>
#include <memory>
#include <string>
#include <thread>
#include <mutex>
#include <vector>
#include <sstream>
#include <set>
#include <fstream>

#include <grpcpp/grpcpp.h>
#include "ghidra_service.grpc.pb.h"

// Kill Windows LoadImage macro
#ifdef LoadImage
#undef LoadImage
#endif
#ifdef LoadImageA
#undef LoadImageA
#endif
#ifdef LoadImageW
#undef LoadImageW
#endif

// Ghidra headers for full decompilation
#include "libdecomp.hh"
#include "sleigh_arch.hh"
#include "loadimage.hh"

using grpc::Server;
using grpc::ServerBuilder;
using grpc::ServerContext;
using grpc::Status;
using namespace ghidra_service;
using namespace ghidra;

#ifdef _WIN32
#include <windows.h>
#endif

// Get directory containing the executable
static std::string getExecutableDir() {
#ifdef _WIN32
    char path[MAX_PATH];
    GetModuleFileNameA(NULL, path, MAX_PATH);
    std::string exePath(path);
    size_t lastSlash = exePath.find_last_of("\\/");
    if (lastSlash != std::string::npos) {
        return exePath.substr(0, lastSlash);
    }
    return ".";
#else
    // Linux/Mac: read /proc/self/exe or use argv[0]
    char path[4096];
    ssize_t len = readlink("/proc/self/exe", path, sizeof(path) - 1);
    if (len != -1) {
        path[len] = '\0';
        std::string exePath(path);
        size_t lastSlash = exePath.find_last_of("/");
        if (lastSlash != std::string::npos) {
            return exePath.substr(0, lastSlash);
        }
    }
    return ".";
#endif
}

// Custom LoadImage - feeds bytes to Sleigh
class MemoryLoadImage : public LoadImage {
    std::string data_;
    uint64_t base_addr_;
public:
    MemoryLoadImage(const std::string& d, uint64_t base) 
        : LoadImage("memory"), data_(d), base_addr_(base) {}
    
    virtual void loadFill(uint1 *ptr, int4 size, const Address &addr) override {
        uint64_t offset = addr.getOffset();
        uint64_t max = base_addr_ + data_.size();
        
        for(int4 i = 0; i < size; ++i) {
            uint64_t cur = offset + i;
            if (cur >= base_addr_ && cur < max) {
                ptr[i] = static_cast<uint1>(data_[cur - base_addr_]);
            } else {
                ptr[i] = 0;
            }
        }
    }
    
    virtual string getArchType(void) const override { return "memory"; }
    virtual void adjustVma(long adjust) override {}
};

// Custom Architecture that uses our MemoryLoadImage
class ServerArchitecture : public SleighArchitecture {
    MemoryLoadImage* custom_loader;
public:
    ServerArchitecture(const string& sleigh_id, MemoryLoadImage* ldr, ostream* err)
        : SleighArchitecture("", sleigh_id, err), custom_loader(ldr) {}
    
    virtual void buildLoader(DocumentStorage& store) override {
        loader = custom_loader;  // Use our custom loader
    }
};

// Assembly Emitter - captures disassembly output
class ServerAssemblyEmit : public AssemblyEmit {
public:
    string mnem;
    string body;
    
    virtual void dump(const Address &addr, const string &m, const string &b) override {
        mnem = m;
        body = b;
    }
};

class DecompilerServiceImpl final : public DecompilerService::Service {
    std::mutex mu_;
    std::unique_ptr<MemoryLoadImage> loader;
    std::unique_ptr<ServerArchitecture> arch;
    uint64_t base_address = 0;
    bool initialized = false;
    
public:
    DecompilerServiceImpl() {
        // Get executable directory and compute paths relative to it
        std::string exeDir = getExecutableDir();
        
        // Try multiple possible locations for the languages folder:
        // 1. <exe_dir>/../../ghidra_decompiler/languages (when exe is in build/Release)
        // 2. <exe_dir>/../ghidra_decompiler/languages (when exe is in build)
        // 3. <exe_dir>/languages (deployed scenario)
        std::vector<std::pair<std::string, std::string>> searchPaths = {
            {exeDir + "/../../ghidra_decompiler", exeDir + "/../../ghidra_decompiler/languages"},
            {exeDir + "/../ghidra_decompiler", exeDir + "/../ghidra_decompiler/languages"},
            {exeDir, exeDir + "/languages"}
        };
        
        std::string baseDir;
        std::string langDir;
        
        for (const auto& paths : searchPaths) {
            std::ifstream test(paths.second + "/x86.ldefs");
            if (test.good()) {
                baseDir = paths.first;
                langDir = paths.second;
                break;
            }
        }
        
        if (langDir.empty()) {
            std::cerr << "[Server] ERROR: Could not find languages directory!" << std::endl;
            std::cerr << "[Server] Searched from: " << exeDir << std::endl;
            return;
        }
        
        std::cout << "[Server] Base directory: " << baseDir << std::endl;
        std::cout << "[Server] Languages directory: " << langDir << std::endl;
        
        // Initialize Ghidra library (registers print languages, capabilities, etc.)
        startDecompilerLibrary(baseDir.c_str());
        // Manually add the languages directory to specpaths
        SleighArchitecture::specpaths.addDir2Path(langDir);
        // Parse .ldefs files by calling getDescriptions()
        try {
            SleighArchitecture::getDescriptions();
        } catch (const LowlevelError& e) {
            std::cerr << "[Server Init] Warning: " << e.explain << std::endl;
        }
    }
    
    ~DecompilerServiceImpl() {
        cleanup();
    }
    
    void cleanup() {
        arch.reset();
        loader.reset();
        initialized = false;
    }

    Status LoadBinary(ServerContext* ctx, const LoadBinaryRequest* request,
                      LoadBinaryResponse* reply) override {
        std::lock_guard<std::mutex> lock(mu_);
        
        try {
            std::cout << "[Server] Loading binary: " << request->binary_content().size() << " bytes" << std::endl;
            cleanup();
            
            base_address = request->base_address();
            
            // Create custom loader
            loader = std::make_unique<MemoryLoadImage>(request->binary_content(), base_address);
            
            // Get language ID (e.g., "x86:LE:64:default")
            string lang_id = request->arch_spec();
            if (lang_id.empty()) {
                lang_id = "x86:LE:64:default";
            }
            std::cout << "[Server] Language ID: " << lang_id << std::endl;
            
            // Create Architecture
            arch = std::make_unique<ServerArchitecture>(lang_id, loader.get(), &std::cerr);
            
            // Initialize with DocumentStorage
            DocumentStorage store;
            arch->init(store);
            
            initialized = true;
            reply->set_success(true);
            std::cout << "[Server] Binary loaded successfully" << std::endl;
            
        } catch (const LowlevelError& e) {
            std::cerr << "[Server] Ghidra error: " << e.explain << std::endl;
            cleanup();
            reply->set_success(false);
            reply->set_error_message(e.explain);
        } catch (const std::exception& e) {
            std::cerr << "[Server] Error: " << e.what() << std::endl;
            cleanup();
            reply->set_success(false);
            reply->set_error_message(e.what());
        } catch (...) {
            std::cerr << "[Server] Unknown error" << std::endl;
            cleanup();
            reply->set_success(false);
            reply->set_error_message("Unknown exception during initialization");
        }
        
        return Status::OK;
    }

    Status DecompileFunction(ServerContext* ctx, const DecompileRequest* request,
                     DecompileResponse* reply) override {
        std::lock_guard<std::mutex> lock(mu_);
        
        if (!initialized || !arch) {
            reply->set_success(false);
            reply->set_error_message("Binary not loaded");
            return Status::OK;
        }

        try {
            Address func_addr(arch->getDefaultCodeSpace(), request->address());
            std::cout << "[Server] Decompiling function at 0x" << std::hex << request->address() << std::dec << std::endl;
            
            // Create function name
            std::ostringstream fname;
            fname << "func_" << std::hex << request->address();
            
            // Find or create function in symbol table
            Scope* global_scope = arch->symboltab->getGlobalScope();
            Funcdata* fd = global_scope->findFunction(func_addr);
            
            if (fd == nullptr) {
                // Create a new function
                fd = global_scope->addFunction(func_addr, fname.str())->getFunction();
            }
            
            if (fd == nullptr) {
                reply->set_success(false);
                reply->set_error_message("Failed to create function");
                return Status::OK;
            }
            
            // Clear any previous analysis
            if (fd->isProcStarted()) {
                arch->clearAnalysis(fd);
            }
            
            // Perform decompilation
            std::cout << "[Server] Running decompile actions..." << std::endl;
            arch->allacts.getCurrent()->reset(*fd);
            int4 res = arch->allacts.getCurrent()->perform(*fd);
            
            if (res < 0) {
                std::cout << "[Server] Decompilation incomplete (break point hit)" << std::endl;
            } else {
                std::cout << "[Server] Decompilation complete" << std::endl;
            }
            
            // ===== Generate C Code =====
            std::ostringstream c_stream;
            arch->print->setOutputStream(&c_stream);
            arch->print->docFunction(fd);
            
            reply->set_c_code(c_stream.str());
            reply->set_signature(fd->getName() + "()");
            reply->set_success(true);
            
            // ===== Generate Disassembly Blocks =====
            ghidra_service::BasicBlock* pb_block = reply->add_blocks();
            pb_block->set_start_addr(func_addr.getOffset());
            pb_block->set_id(func_addr.getOffset());
            
            // Simple linear disassembly
            Address cur = func_addr;
            int instr_count = 0;
            const int MAX_INSTRS = 200;
            
            while(instr_count < MAX_INSTRS) {
                ServerAssemblyEmit emit;
                int4 length = arch->translate->printAssembly(emit, cur);
                
                if (length <= 0) break;
                
                ghidra_service::Instruction* pb_instr = pb_block->add_instructions();
                pb_instr->set_address(cur.getOffset());
                pb_instr->set_length(length);
                pb_instr->set_mnemonic(emit.mnem);
                pb_instr->set_operands(emit.body);
                
                // Stop at RET
                if (emit.mnem.find("RET") != string::npos) {
                    break;
                }
                
                cur = cur + length;
                instr_count++;
            }
            pb_block->set_end_addr(cur.getOffset());
            
            std::cout << "[Server] Generated " << instr_count << " instructions" << std::endl;
            
        } catch (const LowlevelError& e) {
            std::cerr << "[Server] Decompile error: " << e.explain << std::endl;
            reply->set_success(false);
            reply->set_error_message(e.explain);
        } catch (const std::exception& e) {
            std::cerr << "[Server] Decompile error: " << e.what() << std::endl;
            reply->set_success(false);
            reply->set_error_message(e.what());
        } catch (...) {
            std::cerr << "[Server] Unknown decompile error" << std::endl;
            reply->set_success(false);
            reply->set_error_message("Unknown exception during decompilation");
        }
        
        return Status::OK;
    }

    Status DisassembleRange(ServerContext* ctx, const DisassembleRequest* request,
                     DisassembleResponse* reply) override {
        return Status::OK;
    }

    Status Ping(ServerContext* context, const PingRequest* request,
                PingResponse* reply) override {
        reply->set_alive(true);
        return Status::OK;
    }
};

void RunServer() {
    std::string server_address("0.0.0.0:50051");
    DecompilerServiceImpl service;

    ServerBuilder builder;
    builder.AddListeningPort(server_address, grpc::InsecureServerCredentials());
    builder.RegisterService(&service);
    
    std::unique_ptr<Server> server(builder.BuildAndStart());
    std::cout << "Server listening on " << server_address << std::endl;
    server->Wait();
}

int main(int argc, char** argv) {
    if(argc > 1 && string(argv[1]) == "test") {
        return 0;
    }
    RunServer();
    return 0;
}
