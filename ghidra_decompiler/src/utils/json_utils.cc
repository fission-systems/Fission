/**
 * Fission Decompiler - JSON Utilities Implementation
 */

#include "fission/utils/json_utils.h"
#include <cctype>

namespace fission {
namespace utils {

std::string extract_json_string(const std::string& json, const std::string& key) {
    std::string search = "\"" + key + "\"";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return "";
    pos += search.length();
    
    // Find colon
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t' || json[pos] == '\n' || json[pos] == '\r')) pos++;
    if (pos >= json.length() || json[pos] != ':') return "";
    pos++;
    
    // Find opening quote
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t' || json[pos] == '\n' || json[pos] == '\r')) pos++;
    if (pos >= json.length() || json[pos] != '"') return "";
    pos++;
    
    // Handle escaped quotes
    size_t end = pos;
    while (end < json.length()) {
        if (json[end] == '"' && end > 0 && json[end-1] != '\\') {
            break;
        }
        end++;
    }
    
    if (end >= json.length()) return "";
    return json.substr(pos, end - pos);
}

int64_t extract_json_int(const std::string& json, const std::string& key) {
    std::string search = "\"" + key + "\"";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return 0;
    pos += search.length();
    
    // Find colon
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t' || json[pos] == '\n' || json[pos] == '\r')) pos++;
    if (pos >= json.length() || json[pos] != ':') return 0;
    pos++;
    
    // Skip whitespace after colon
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t' || json[pos] == '\n' || json[pos] == '\r')) pos++;
    
    std::string num;
    while (pos < json.length() && (std::isdigit(json[pos]) || json[pos] == '-')) {
        num += json[pos++];
    }
    return num.empty() ? 0 : std::stoll(num);
}

bool extract_json_bool(const std::string& json, const std::string& key) {
    std::string search = "\"" + key + "\":";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return false;
    pos += search.length();
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t')) pos++;
    return (json.substr(pos, 4) == "true");
}

std::string json_escape(const std::string& s) {
    std::string result;
    result.reserve(s.size() * 2);
    for (char c : s) {
        switch (c) {
            case '"': result += "\\\""; break;
            case '\\': result += "\\\\"; break;
            case '\n': result += "\\n"; break;
            case '\r': result += "\\r"; break;
            case '\t': result += "\\t"; break;
            default: result += c; break;
        }
    }
    return result;
}

std::map<uint64_t, std::string> parse_json_string_map(const std::string& json, size_t start_pos) {
    std::map<uint64_t, std::string> symbols;
    size_t pos = start_pos;
    
    // Skip whitespace
    while (pos < json.length() && std::isspace(json[pos])) pos++;
    
    // Ensure starts with {
    if (pos >= json.length() || json[pos] != '{') return symbols;
    pos++; // skip '{'
    
    // Parse key-value pairs until '}'
    while (pos < json.length()) {
        // Skip whitespace
        while (pos < json.length() && std::isspace(json[pos])) pos++;
        
        if (pos >= json.length() || json[pos] == '}') break;
        if (json[pos] == ',') { pos++; continue; }
        
        // Parse key
        if (json[pos] != '"') break;
        pos++; // skip opening quote
        size_t key_end = json.find('"', pos);
        if (key_end == std::string::npos) break;
        std::string addr_str = json.substr(pos, key_end - pos);
        pos = key_end + 1;
        
        // Skip ":"
        while (pos < json.length() && (std::isspace(json[pos]) || json[pos] == ':')) pos++;
        
        // Parse value
        if (pos >= json.length() || json[pos] != '"') break;
        pos++; // skip opening quote
        size_t val_end = pos;
        while (val_end < json.length()) {
            if (json[val_end] == '"' && val_end > 0 && json[val_end-1] != '\\') break;
            val_end++;
        }
        if (val_end >= json.length()) break;
        std::string func_name = json.substr(pos, val_end - pos);
        pos = val_end + 1;
        
        // Parse address (supports "0x" hex prefix)
        uint64_t addr = 0;
        try {
            if (addr_str.length() > 2 && (addr_str.substr(0, 2) == "0x" || addr_str.substr(0, 2) == "0X")) {
                addr = std::stoull(addr_str.substr(2), nullptr, 16);
            } else {
                addr = std::stoull(addr_str, nullptr, 10);
            }
        } catch (...) {
            continue; // Skip invalid addresses
        }
        
        symbols[addr] = func_name;
    }
    
    return symbols;
}

std::map<uint64_t, std::string> extract_iat_symbols(const std::string& json) {
    std::string search = "\"iat_symbols\":";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return {};
    pos += search.length();
    
    // Skip whitespace
    while (pos < json.length() && std::isspace(json[pos])) pos++;
    
    // Parse map starting at current position
    return parse_json_string_map(json, pos);
}

std::vector<std::string> extract_json_array(const std::string& json_content) {
    std::vector<std::string> elements;
    size_t pos = 0;
    
    // Skip whitespace
    while (pos < json_content.length() && std::isspace(json_content[pos])) pos++;
    
    // Must start with '['
    if (pos >= json_content.length() || json_content[pos] != '[') {
        return elements;
    }
    pos++; // skip '['
    
    while (pos < json_content.length()) {
        // Skip whitespace and commas
        while (pos < json_content.length() && 
               (std::isspace(json_content[pos]) || json_content[pos] == ',')) {
            pos++;
        }
        
        if (pos >= json_content.length() || json_content[pos] == ']') {
            break;
        }
        
        // Must be start of object '{'
        if (json_content[pos] != '{') {
            break;
        }
        
        // Find matching '}'
        size_t start = pos;
        int depth = 0;
        bool in_string = false;
        
        while (pos < json_content.length()) {
            char c = json_content[pos];
            
            if (in_string) {
                if (c == '\\' && pos + 1 < json_content.length()) {
                    pos += 2; // skip escaped char
                    continue;
                }
                if (c == '"') {
                    in_string = false;
                }
            } else {
                if (c == '"') {
                    in_string = true;
                } else if (c == '{') {
                    depth++;
                } else if (c == '}') {
                    depth--;
                    if (depth == 0) {
                        pos++; // include closing '}'
                        break;
                    }
                }
            }
            pos++;
        }
        
        if (depth == 0 && pos > start) {
            elements.push_back(json_content.substr(start, pos - start));
        }
    }
    
    return elements;
}

} // namespace utils
} // namespace fission
