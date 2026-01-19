#include "fission/utils/logger.h"
#include <fstream>
#include <memory>
#include <mutex>
#include <ctime>

namespace fission {
namespace utils {

static NullBuffer null_buffer;
static std::ostream null_stream_instance(&null_buffer);

class LoggerImpl {
public:
    std::mutex mutex;
    std::ofstream file_stream;
    bool to_console = true;
    bool to_file = false;
    
    void open(const std::string& path) {
        std::lock_guard<std::mutex> lock(mutex);
        if (file_stream.is_open()) {
            file_stream.close();
        }
        file_stream.open(path, std::ios::out | std::ios::app);
        to_file = file_stream.is_open();
    }
    

    std::string get_timestamp() {
        std::time_t now = std::time(nullptr);
        char buf[32];
        std::strftime(buf, sizeof(buf), "[%Y-%m-%d %H:%M:%S] ", std::localtime(&now));
        return std::string(buf);
    }
    
    void log(const std::string& level, const std::string& msg) {
         std::lock_guard<std::mutex> lock(mutex);
         std::string timestamp = get_timestamp();
         if (to_console) {
             std::cerr << level << msg << std::endl;
         }
         if (to_file && file_stream.is_open()) {
             file_stream << timestamp << level << msg << std::endl;
             file_stream.flush();
         }
    }
};

static LoggerImpl g_logger;

std::ostream& null_stream() {
    return null_stream_instance;
}

void Logger::initialize(const std::string& filename) {
    if (!filename.empty()) {
        g_logger.open(filename);
    }
}

void Logger::log(const std::string& msg) {
    g_logger.log("", msg);
}

void Logger::info(const std::string& msg) {
     g_logger.log("[INFO] ", msg);
}

void Logger::warn(const std::string& msg) {
     g_logger.log("[WARN] ", msg);
}

void Logger::error(const std::string& msg) {
     g_logger.log("[ERROR] ", msg);
}

std::ostream& Logger::stream() {
    return log_stream();
}

// TeeBuffer: writes to both console and file
class TeeBuffer : public std::streambuf {
public:
    TeeBuffer() {}
    
    void set_file(std::ofstream* f) { file_ = f; }
    
protected:
    int overflow(int c) override {
        if (c != EOF) {
            // Write to console
            std::cerr.put(static_cast<char>(c));
            // Write to file if available
            if (file_ && file_->is_open()) {
                file_->put(static_cast<char>(c));
            }
        }
        return c;
    }
    
    std::streamsize xsputn(const char* s, std::streamsize n) override {
        std::cerr.write(s, n);
        if (file_ && file_->is_open()) {
            file_->write(s, n);
        }
        return n;
    }
    
    int sync() override {
        std::cerr.flush();
        if (file_ && file_->is_open()) {
            file_->flush();
        }
        return 0;
    }
    
private:
    std::ofstream* file_ = nullptr;
};

static TeeBuffer tee_buffer;
static std::ostream tee_stream(&tee_buffer);

std::ostream& log_stream() {
    // Update file pointer (in case logger was initialized after first call)
    tee_buffer.set_file(&g_logger.file_stream);
    return tee_stream;
}

} // namespace utils
} // namespace fission
