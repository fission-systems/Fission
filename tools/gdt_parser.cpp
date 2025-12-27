#include <cstdint>
#include <cstring>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

#include <zlib.h>

namespace {

bool read_file(const std::string &path, std::vector<uint8_t> *out) {
  std::ifstream in(path, std::ios::binary);
  if (!in) {
    return false;
  }
  in.seekg(0, std::ios::end);
  std::streamoff size = in.tellg();
  if (size < 0) {
    return false;
  }
  in.seekg(0, std::ios::beg);
  out->resize(static_cast<size_t>(size));
  if (!out->empty()) {
    in.read(reinterpret_cast<char *>(out->data()), size);
  }
  return in.good();
}

bool write_file(const std::string &path, const std::vector<uint8_t> &data) {
  std::ofstream out(path, std::ios::binary);
  if (!out) {
    return false;
  }
  out.write(reinterpret_cast<const char *>(data.data()), data.size());
  return out.good();
}

size_t find_bytes(const std::vector<uint8_t> &buf, const std::vector<uint8_t> &pat, size_t start) {
  if (pat.empty() || buf.size() < pat.size() || start >= buf.size()) {
    return std::string::npos;
  }
  for (size_t i = start; i + pat.size() <= buf.size(); ++i) {
    if (std::memcmp(buf.data() + i, pat.data(), pat.size()) == 0) {
      return i;
    }
  }
  return std::string::npos;
}

uint16_t read_le16(const std::vector<uint8_t> &buf, size_t off) {
  return static_cast<uint16_t>(buf[off] | (buf[off + 1] << 8));
}

uint32_t read_le32(const std::vector<uint8_t> &buf, size_t off) {
  return static_cast<uint32_t>(buf[off] |
                               (buf[off + 1] << 8) |
                               (buf[off + 2] << 16) |
                               (buf[off + 3] << 24));
}

uint32_t read_be32(const std::vector<uint8_t> &buf, size_t off) {
  return static_cast<uint32_t>((buf[off] << 24) |
                               (buf[off + 1] << 16) |
                               (buf[off + 2] << 8) |
                               (buf[off + 3]));
}

uint64_t read_be64(const std::vector<uint8_t> &buf, size_t off) {
  uint64_t hi = read_be32(buf, off);
  uint64_t lo = read_be32(buf, off + 4);
  return (hi << 32) | lo;
}

bool is_printable_ascii(uint8_t c) {
  return (c >= 0x20 && c <= 0x7e);
}

std::string read_ascii_z(const std::vector<uint8_t> &buf, size_t off, size_t max_len) {
  std::string out;
  for (size_t i = 0; i < max_len && off + i < buf.size(); ++i) {
    uint8_t c = buf[off + i];
    if (c == 0) {
      break;
    }
    if (!is_printable_ascii(c)) {
      return std::string();
    }
    out.push_back(static_cast<char>(c));
  }
  return out;
}

bool inflate_raw_deflate(const std::vector<uint8_t> &comp, size_t out_size, std::vector<uint8_t> *out) {
  out->assign(out_size, 0);
  z_stream zs{};
  zs.next_in = const_cast<Bytef *>(comp.data());
  zs.avail_in = static_cast<uInt>(comp.size());
  zs.next_out = out->data();
  zs.avail_out = static_cast<uInt>(out->size());

  int ret = inflateInit2(&zs, -15);
  if (ret != Z_OK) {
    return false;
  }
  ret = inflate(&zs, Z_FINISH);
  inflateEnd(&zs);
  return ret == Z_STREAM_END && zs.total_out == out->size();
}

std::string default_out_path(const std::string &in_path) {
  return in_path + ".folder_item.bin";
}

void print_usage(const char *argv0) {
  std::cerr << "Usage: " << argv0 << " <input.gdt> [--out <path>] [--no-write]\n";
}

}  // namespace

int main(int argc, char **argv) {
  if (argc < 2) {
    print_usage(argv[0]);
    return 1;
  }

  std::string input_path;
  std::string output_path;
  bool no_write = false;

  input_path = argv[1];
  for (int i = 2; i < argc; ++i) {
    std::string arg = argv[i];
    if (arg == "--out") {
      if (i + 1 >= argc) {
        print_usage(argv[0]);
        return 1;
      }
      output_path = argv[++i];
    } else if (arg == "--no-write") {
      no_write = true;
    } else {
      print_usage(argv[0]);
      return 1;
    }
  }

  std::vector<uint8_t> data;
  if (!read_file(input_path, &data)) {
    std::cerr << "Failed to read file: " << input_path << "\n";
    return 1;
  }
  if (data.size() < 8) {
    std::cerr << "File too small.\n";
    return 1;
  }

  if (!(data[0] == 0xAC && data[1] == 0xED && data[2] == 0x00 && data[3] == 0x05)) {
    std::cerr << "Missing Java serialization header (AC ED 00 05).\n";
    return 1;
  }

  const std::vector<uint8_t> pk_local = {'P', 'K', 0x03, 0x04};
  size_t local_off = find_bytes(data, pk_local, 0);
  if (local_off == std::string::npos) {
    std::cerr << "ZIP local header (PK 03 04) not found.\n";
    return 1;
  }

  if (local_off + 30 > data.size()) {
    std::cerr << "Truncated ZIP local header.\n";
    return 1;
  }

  uint16_t ver = read_le16(data, local_off + 4);
  uint16_t flags = read_le16(data, local_off + 6);
  uint16_t method = read_le16(data, local_off + 8);
  uint16_t name_len = read_le16(data, local_off + 26);
  uint16_t extra_len = read_le16(data, local_off + 28);
  size_t name_off = local_off + 30;
  size_t comp_start = name_off + name_len + extra_len;

  if (comp_start > data.size()) {
    std::cerr << "Invalid ZIP name/extra lengths.\n";
    return 1;
  }

  std::string file_name(reinterpret_cast<const char *>(&data[name_off]), name_len);
  std::cout << "ZIP entry: " << file_name << "\n";
  std::cout << "ZIP version: " << ver << " flags=0x" << std::hex << flags << std::dec
            << " method=" << method << "\n";

  const std::vector<uint8_t> pk_desc = {'P', 'K', 0x07, 0x08};
  size_t desc_off = find_bytes(data, pk_desc, comp_start);
  if (desc_off == std::string::npos) {
    std::cerr << "ZIP data descriptor (PK 07 08) not found.\n";
    return 1;
  }
  if (desc_off + 16 > data.size()) {
    std::cerr << "Truncated ZIP data descriptor.\n";
    return 1;
  }

  uint32_t crc32 = read_le32(data, desc_off + 4);
  uint32_t comp_size = read_le32(data, desc_off + 8);
  uint32_t uncomp_size = read_le32(data, desc_off + 12);
  std::cout << "Descriptor: crc32=0x" << std::hex << crc32 << std::dec
            << " comp=" << comp_size << " uncomp=" << uncomp_size << "\n";

  size_t comp_end = desc_off;
  size_t comp_len = comp_end - comp_start;
  if (comp_len != comp_size) {
    std::cerr << "Warning: compressed size mismatch (computed " << comp_len
              << ", descriptor " << comp_size << ")\n";
  }

  if (method != 8) {
    std::cerr << "Unsupported compression method: " << method << " (expected 8=deflate)\n";
    return 1;
  }

  std::vector<uint8_t> comp_data(data.begin() + static_cast<long>(comp_start),
                                 data.begin() + static_cast<long>(comp_end));
  std::vector<uint8_t> decomp;
  if (!inflate_raw_deflate(comp_data, uncomp_size, &decomp)) {
    std::cerr << "Deflate decompression failed.\n";
    return 1;
  }

  if (!no_write) {
    if (output_path.empty()) {
      output_path = default_out_path(input_path);
    }
    if (!write_file(output_path, decomp)) {
      std::cerr << "Failed to write output: " << output_path << "\n";
      return 1;
    }
    std::cout << "Wrote payload: " << output_path << "\n";
  }

  if (decomp.size() < 28) {
    std::cerr << "Decompressed payload too small for DB header.\n";
    return 1;
  }

  std::string sig(reinterpret_cast<const char *>(decomp.data()), 8);
  uint64_t db_id = read_be64(decomp, 8);
  uint32_t version = read_be32(decomp, 16);
  uint32_t page_size = read_be32(decomp, 20);
  uint32_t unknown = read_be32(decomp, 24);

  std::cout << "DB signature: " << sig << "\n";
  std::cout << "DB id: 0x" << std::hex << db_id << std::dec << "\n";
  std::cout << "DB version: " << version << "\n";
  std::cout << "DB page size: 0x" << std::hex << page_size << std::dec << "\n";
  std::cout << "DB unknown: 0x" << std::hex << unknown << std::dec << "\n";
  if (page_size != 0 && (decomp.size() % page_size) == 0) {
    std::cout << "DB pages: " << (decomp.size() / page_size) << "\n";
  } else {
    std::cerr << "Warning: payload size not aligned to page size.\n";
  }

  // Heuristic schema extraction: length-prefixed column strings ending with ';'
  std::cout << "Schema candidates:\n";
  const size_t max_scan = decomp.size();
  for (size_t off = 0; off + 4 < max_scan; ++off) {
    uint32_t len = read_be32(decomp, off);
    if (len < 3 || len > 512) {
      continue;
    }
    size_t str_off = off + 4;
    if (str_off + len > max_scan) {
      continue;
    }
    bool ok = true;
    for (size_t i = 0; i < len; ++i) {
      uint8_t c = decomp[str_off + i];
      if (!is_printable_ascii(c)) {
        ok = false;
        break;
      }
    }
    if (!ok || decomp[str_off + len - 1] != ';') {
      continue;
    }

    std::string cols(reinterpret_cast<const char *>(&decomp[str_off]), len);

    // Try to find a nearby table name within 128 bytes before this field.
    std::string table;
    size_t back_start = (off > 128) ? (off - 128) : 0;
    for (size_t p = off; p > back_start; --p) {
      if (decomp[p - 1] == 0) {
        std::string candidate = read_ascii_z(decomp, p, 64);
        if (!candidate.empty()) {
          table = candidate;
          break;
        }
      }
    }

    std::cout << "  offset=0x" << std::hex << off << std::dec;
    if (!table.empty()) {
      std::cout << " table=\"" << table << "\"";
    }
    std::cout << " cols=\"" << cols << "\"\n";
  }

  return 0;
}
