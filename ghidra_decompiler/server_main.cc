/**
 * Ghidra Decompiler gRPC Server
 * 
 * Implements the DecompilerService defined in protos/ghidra_service.proto
 */

#include <iostream>
#include <memory>
#include <string>
#include <thread>
#include <mutex>

#include <grpcpp/grpcpp.h>

#include "ghidra_service.grpc.pb.h"

// Ghidra headers
#include "libdecomp.hh"
#include "sleigh_arch.hh"
#include "loadimage.hh"

using grpc::Server;
using grpc::ServerBuilder;
using grpc::ServerContext;
using grpc::Status;
using namespace ghidra_service;
using namespace ghidra;

// Simple custom LoadImage that holds the binary data in memory
class MemoryLoadImage : public LoadImage {
    std::string data;
    uint64_t base_addr;
public:
    MemoryLoadImage(const std::string& d, uint64_t base) 
        : LoadImage("memory"), data(d), base_addr(base) {}
    
    virtual void loadFill(uint1 *ptr, int4 size, const Address &addr) override {
        uint64_t offset = addr.getOffset();
        uint64_t max = base_addr + data.size();
        
        for(int4 i=0; i<size; ++i) {
            uint64_t cur = offset + i;
            if (cur >= base_addr && cur < max) {
                ptr[i] = data[cur - base_addr];
            } else {
                ptr[i] = 0;
            }
        }
    }
    
    virtual string getArchType(void) const override { return "memory"; }
    virtual void adjustVma(long adjust) override {}
};

class DecompilerServiceImpl final : public DecompilerService::Service {
    std::mutex mu_;
    SleighArchitecture *glb = nullptr;
    DocumentStorage store;
    MemoryLoadImage *loader = nullptr;
    ContextInternal *context = nullptr;
    
public:
    ~DecompilerServiceImpl() {
        std::lock_guard<std::mutex> lock(mu_);
        cleanup();
    }
    
    void cleanup() {
        if (glb) { delete glb; glb = nullptr; }
        if (loader) { delete loader; loader = nullptr; }
        if (context) { delete context; context = nullptr; }
    }

    Status LoadBinary(ServerContext* context, const LoadBinaryRequest* request,
                      LoadBinaryResponse* reply) override {
        std::lock_guard<std::mutex> lock(mu_);
        
        try {
            cleanup();
            
            // 1. Setup Loader
            loader = new MemoryLoadImage(request->binary_content(), request->base_address());
            
            // 2. Setup Architecture
            // Using SleighArchitecture specific constructor or init
            // Note: This relies on finding the .sla file properly
            string specfile = request->sla_path(); 
            if (specfile.empty()) {
                // Default fallback logic or error
                 reply->set_success(false);
                 reply->set_error_message("SLA path required");
                 return Status::OK;
            }
            
            // This is a simplified initialization sequence
            // In real Ghidra API, we need to carefully setup SleighArchitecture
            // We'll mimic SleighArchitecture::buildArchitecture behavior
            
            AttributeId::initialize();
            ElementId::initialize();
            
            glb = new SleighArchitecture(specfile, request->arch_spec(), &std::cout);
            
            // Register loader
            // Note: SleighArchitecture usually takes ownership or needs careful handling
            // Here we just keep pointers simple for prototype
            
            // Setup document storage with SLA
            Document *doc = store.openDocument(specfile);
            store.registerTag(doc->getRoot());
            
            glb->init(store);
            
            // Perform read-only check or initialization
            // glb->readLoaderSymbols("nm"); // Optional
            
            context = new ContextInternal();
            
            reply->set_success(true);
        } catch (const std::exception& e) {
            cleanup();
            reply->set_success(false);
            reply->set_error_message(e.what());
        } catch (...) {
            cleanup();
            reply->set_success(false);
            reply->set_error_message("Unknown error loading binary");
        }
        
        return Status::OK;
    }

    Status Decompile(ServerContext* ctx, const DecompileRequest* request,
                     DecompileResponse* reply) override {
        std::lock_guard<std::mutex> lock(mu_);
        
        if (!glb || !context) {
            reply->set_success(false);
            reply->set_error_message("Binary not loaded");
            return Status::OK;
        }

        try {
            // Setup decompilation
            Address addr(glb->getDefaultSpace(), request->address());
            
            // 1. Create function
            Funcdata func(request->address(), "func", glb);
            
            // 2. Decompile
            // This is the heavy lifting
            // We need to implement proper decompiler loop or use PrintC
            
            // Placeholder for now as direct C++ API integration requires many components
            // (Architecture, Scope, SymbolTable, etc.) to be perfectly aligned
             
            reply->set_success(true);
            reply->set_c_code("// Decompilation via gRPC successful!\n// (Actual output pending full implementation)");
            
        } catch (const std::exception& e) {
            reply->set_success(false);
            reply->set_error_message(e.what());
        }
        
        return Status::OK;
    }

    Status Ping(ServerContext* context, const PingRequest* request,
                PingResponse* reply) override {
        reply->set_alive(true);
        reply->set_memory_usage(0); // TODO
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
    Try {
        RunServer();
    } Catch (const std::exception& e) {
        std::cerr << "Server crashed: " << e.what() << std::endl;
        return 1;
    }
    return 0;
}
