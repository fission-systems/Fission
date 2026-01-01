#ifndef FISSION_UTILS_STEP_TIMER_H
#define FISSION_UTILS_STEP_TIMER_H

#include <string>
#include <chrono>
#include <iostream>

namespace fission {
namespace utils {

/**
 * RAII-based step timer for performance observability.
 * Logs step duration when destroyed.
 * 
 * Usage:
 *   {
 *       StepTimer timer("Step 4: Decompilation");
 *       arch->allacts.getCurrent()->perform(*fd);
 *   }
 *   // Automatically logs: "[TIMING] Step 4: Decompilation: 1234ms"
 */
class StepTimer {
public:
    explicit StepTimer(const std::string& step_name) 
        : name_(step_name), enabled_(true) {
        start_ = std::chrono::high_resolution_clock::now();
    }
    
    // Allow disabling timing for optional steps
    StepTimer(const std::string& step_name, bool enabled) 
        : name_(step_name), enabled_(enabled) {
        if (enabled_) {
            start_ = std::chrono::high_resolution_clock::now();
        }
    }
    
    ~StepTimer() {
        if (enabled_) {
            auto end = std::chrono::high_resolution_clock::now();
            auto duration = std::chrono::duration_cast<std::chrono::milliseconds>(end - start_);
            std::cerr << "[TIMING] " << name_ << ": " << duration.count() << "ms" << std::endl;
        }
    }
    
    // Get elapsed time without stopping
    long long elapsed_ms() const {
        auto now = std::chrono::high_resolution_clock::now();
        return std::chrono::duration_cast<std::chrono::milliseconds>(now - start_).count();
    }
    
    // Disable default copy/move
    StepTimer(const StepTimer&) = delete;
    StepTimer& operator=(const StepTimer&) = delete;
    
private:
    std::string name_;
    std::chrono::time_point<std::chrono::high_resolution_clock> start_;
    bool enabled_;
};

} // namespace utils
} // namespace fission

#endif // FISSION_UTILS_STEP_TIMER_H
