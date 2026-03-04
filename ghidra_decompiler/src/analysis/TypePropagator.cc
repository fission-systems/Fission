#include <cstring>
#include "fission/analysis/TypePropagator.h"
#include "fission/analysis/StackFrameAnalyzer.h"
#include "fission/types/TypeResolver.h"
#include "fission/core/ArchPolicy.h"
#include "database.hh"
#include "funcdata.hh"
#include "op.hh"
#include "varnode.hh"
#include "type.hh"
#include "architecture.hh"
#include "fspec.hh"
#include "fission/config/PathConfig.h"
#include <iostream>
#include <set>
#include "fission/utils/logger.h"

namespace fission {
namespace analysis {

using namespace ghidra;
using namespace fission::config;

namespace {

bool is_pointer_type(Datatype* type) {
    return type && type->getMetatype() == TYPE_PTR;
}

bool get_signed_offset(ghidra::Varnode* vn, int64_t& out) {
    if (!vn || !vn->isConstant()) {
        return false;
    }
    int size = vn->getSize();
    if (size <= 0) {
        return false;
    }
    int bits = (size * 8) - 1;
    ghidra::intb raw = static_cast<ghidra::intb>(vn->getOffset());
    out = static_cast<int64_t>(ghidra::sign_extend(raw, bits));
    return true;
}

bool is_stack_base(ghidra::Varnode* vn, ghidra::AddrSpace* stack_space) {
    if (!vn || !stack_space) return false;
    if (vn->getSpace() == stack_space) return true;
    if (vn->isInput() && vn->getSpace()->getName() == "register") return true;
    return false;
}

int64_t normalize_stack_offset(ghidra::AddrSpace* stack_space, uint64_t offset) {
    if (!stack_space) {
        return static_cast<int64_t>(offset);
    }
    int bytes = stack_space->getAddrSize();
    if (bytes <= 0) {
        return static_cast<int64_t>(offset);
    }
    int bits = (bytes * 8) - 1;
    ghidra::intb raw = static_cast<ghidra::intb>(offset);
    return static_cast<int64_t>(ghidra::sign_extend(raw, bits));
}

bool resolve_stack_offset(
    ghidra::Varnode* vn,
    ghidra::AddrSpace* stack_space,
    int64_t& offset,
    int depth = 6
) {
    if (!vn || depth <= 0) {
        return false;
    }
    if (is_stack_base(vn, stack_space)) {
        if (vn->getSpace() == stack_space) {
            offset += static_cast<int64_t>(vn->getOffset());
        }
        return true;
    }
    if (!vn->isWritten()) {
        return false;
    }
    ghidra::PcodeOp* def = vn->getDef();
    if (!def) {
        return false;
    }

    switch (def->code()) {
        case ghidra::CPUI_COPY:
        case ghidra::CPUI_CAST:
        case ghidra::CPUI_INT_ZEXT:
        case ghidra::CPUI_INT_SEXT:
            return resolve_stack_offset(def->getIn(0), stack_space, offset, depth - 1);
        case ghidra::CPUI_PTRSUB: {
            ghidra::Varnode* base = def->getIn(0);
            int64_t off = 0;
            if (!get_signed_offset(def->getIn(1), off)) {
                return false;
            }
            offset += off;
            return resolve_stack_offset(base, stack_space, offset, depth - 1);
        }
        case ghidra::CPUI_PTRADD: {
            ghidra::Varnode* base = def->getIn(0);
            int64_t idx = 0;
            int64_t elem = 0;
            if (!get_signed_offset(def->getIn(1), idx)) {
                return false;
            }
            if (!get_signed_offset(def->getIn(2), elem)) {
                return false;
            }
            offset += idx * elem;
            return resolve_stack_offset(base, stack_space, offset, depth - 1);
        }
        case ghidra::CPUI_INT_ADD: {
            int64_t off = 0;
            ghidra::Varnode* lhs = def->getIn(0);
            ghidra::Varnode* rhs = def->getIn(1);
            if (get_signed_offset(lhs, off)) {
                offset += off;
                return resolve_stack_offset(rhs, stack_space, offset, depth - 1);
            }
            if (get_signed_offset(rhs, off)) {
                offset += off;
                return resolve_stack_offset(lhs, stack_space, offset, depth - 1);
            }
            return false;
        }
        case ghidra::CPUI_INT_SUB: {
            int64_t off = 0;
            ghidra::Varnode* lhs = def->getIn(0);
            ghidra::Varnode* rhs = def->getIn(1);
            if (get_signed_offset(rhs, off)) {
                offset -= off;
                return resolve_stack_offset(lhs, stack_space, offset, depth - 1);
            }
            return false;
        }
        case ghidra::CPUI_MULTIEQUAL:
        case ghidra::CPUI_INDIRECT: {
            for (int slot = 0; slot < def->numInput(); ++slot) {
                int64_t candidate = offset;
                if (resolve_stack_offset(def->getIn(slot), stack_space, candidate, depth - 1)) {
                    offset = candidate;
                    return true;
                }
            }
            return false;
        }
        default:
            return false;
    }
}

} // namespace

TypePropagator::TypePropagator(Architecture* a) : arch(a), struct_registry(nullptr), local_count(0), compiler_id_("") {}

TypePropagator::TypePropagator(Architecture* a, std::map<uint64_t, std::map<int, std::string>>* registry) 
    : arch(a), struct_registry(registry), local_count(0), compiler_id_("") {}

TypePropagator::~TypePropagator() {}

uint64_t TypePropagator::get_varnode_id(Varnode* vn) {
    if (!vn) return 0;
    // Pack: [space_index:8][offset:52][size:4]
    // Previous formula had (offset << 8)|size which aliased any two offsets
    // differing by a multiple of 256. Fixed layout gives 2^52 distinct offsets.
    uint64_t space_idx = (uint64_t)vn->getSpace()->getIndex() & 0xFF;
    uint64_t offset    = vn->getOffset() & 0x000FFFFFFFFFFFFFULL;
    uint64_t sz        = (uint64_t)vn->getSize() & 0xFULL;
    return (space_idx << 56) | (offset << 4) | sz;
}

void TypePropagator::propagate_from_call(Funcdata* fd, PcodeOp* call_op) {
    if (!call_op || call_op->code() != CPUI_CALL) return;
    
    // Get call target
    Varnode* target = call_op->getIn(0);
    if (!target || !target->isConstant()) return;
    
    uint64_t target_addr = target->getOffset();
    
    // Look up function at target address
    Funcdata* target_func = arch->symboltab->getGlobalScope()->queryFunction(
        Address(arch->getDefaultCodeSpace(), target_addr));

    // --- DF-1: FuncCallSpecs fallback ---
    // Even when target_func is not yet fully analysed, the call site's FuncCallSpecs
    // (attached by Ghidra's prototype-resolution pass) may already carry concrete
    // parameter types. Use them directly so we don't miss call-site type info.
    if (!target_func) {
        FuncCallSpecs* fc = fd->getCallSpecs(call_op);
        if (fc) {
            std::string fc_name = fc->getName();
            // Apply platform API rules via name even without Funcdata.
            if (compiler_id_.empty() || compiler_id_ == "windows" ||
                compiler_id_ == "msvc"  || compiler_id_ == "mingw") {
                infer_windows_api_types(call_op, fc_name);
            } else {
                infer_posix_api_types(call_op, fc_name);
            }
            // Propagate parameter types from FuncCallSpecs prototype.
            // x86 32-bit cdecl: a double (8B) is pushed as two 4-byte stack
            // slots, so call_op->getIn() has more entries than proto params.
            // Track arg_idx and param_idx independently.
            bool is_32bit_fc = (arch->getDefaultCodeSpace()->getAddrSize() == 4);
            int nparams = fc->numParams();
            int arg_idx_fc = 1; // getIn(0) = call target
            for (int pi = 0; pi < nparams; ++pi) {
                if (arg_idx_fc >= call_op->numInput()) break;
                ProtoParameter* param = fc->getParam(pi);
                if (!param) { ++arg_idx_fc; continue; }
                Datatype* pt = param->getType();
                if (!pt || pt->getMetatype() == TYPE_UNKNOWN) { ++arg_idx_fc; continue; }
                Varnode* arg = call_op->getIn(arg_idx_fc);
                if (!arg) { ++arg_idx_fc; continue; }
                // x86 cdecl double split: 8-byte float param, two 4-byte call inputs
                if (is_32bit_fc
                    && pt->getMetatype() == TYPE_FLOAT
                    && pt->getSize() == 8
                    && arg->getSize() == 4) {
                    propagate_backwards(arg, pt);
                    ++arg_idx_fc;
                    if (arg_idx_fc < call_op->numInput()) {
                        Varnode* arg_hi = call_op->getIn(arg_idx_fc);
                        if (arg_hi && arg_hi->getSize() == 4)
                            propagate_backwards(arg_hi, pt);
                        ++arg_idx_fc;
                    }
                } else {
                    propagate_backwards(arg, pt);
                    ++arg_idx_fc;
                }
            }
        }
        return;
    }
    
    // Get function name for Windows/POSIX API inference
    std::string func_name = target_func->getName();

    // A-2: Apply platform-specific type inference rules.
    // Windows (PE / MSVC / MinGW) gets Windows API patterns.
    // Everything else (ELF gcc/clang, Mach-O) gets POSIX patterns.
    if (compiler_id_.empty() || compiler_id_ == "windows" ||
        compiler_id_ == "msvc"  || compiler_id_ == "mingw") {
        infer_windows_api_types(call_op, func_name);
    } else {
        infer_posix_api_types(call_op, func_name);
    }
    
    // Get prototype
    const FuncProto& proto = target_func->getFuncProto();
    int num_params = proto.numParams();
    
    // Map each input parameter to its type.
    // x86 32-bit cdecl: a double (8B) is pushed as two 4-byte stack slots,
    // so call_op->getIn() may have more inputs than proto.numParams().
    // Use independent arg_idx / param_idx counters and consume two call
    // inputs for every 8-byte float (double) parameter.
    bool is_32bit = (arch->getDefaultCodeSpace()->getAddrSize() == 4);
    int arg_idx = 1; // getIn(0) = call target
    for (int param_idx = 0; param_idx < num_params; ++param_idx) {
        if (arg_idx >= call_op->numInput()) break;
        ProtoParameter* param = proto.getParam(param_idx);
        if (!param) { ++arg_idx; continue; }
        Datatype* param_type = param->getType();
        if (!param_type || param_type->getMetatype() == TYPE_UNKNOWN) { ++arg_idx; continue; }
        Varnode* arg = call_op->getIn(arg_idx);
        if (!arg) { ++arg_idx; continue; }
        // x86 cdecl double split: 8-byte float param, two 4-byte call inputs
        if (is_32bit
            && param_type->getMetatype() == TYPE_FLOAT
            && param_type->getSize() == 8
            && arg->getSize() == 4) {
            propagate_backwards(arg, param_type);
            ++arg_idx;
            if (arg_idx < call_op->numInput()) {
                Varnode* arg_hi = call_op->getIn(arg_idx);
                if (arg_hi && arg_hi->getSize() == 4)
                    propagate_backwards(arg_hi, param_type);
                ++arg_idx;
            }
        } else {
            propagate_backwards(arg, param_type);
            ++arg_idx;
        }
    }
}

// ---------------------------------------------------------------------------
// merge_split_double_args
// ---------------------------------------------------------------------------
// x86 32-bit cdecl passes a double (8B IEEE 754) as two consecutive 4-byte
// stack slots.  The Pcode CPUI_CALL therefore has two 4-byte input varnodes
// where the callee prototype expects a single 8-byte float.  When both halves
// are compile-time constants we can combine them into one 8-byte constant
// so the C printer outputs a single argument (matching Ghidra's behaviour).
//
// Byte layout (little-endian x86):
//   [esp+0] = lo dword  → getIn(arg_idx)
//   [esp+4] = hi dword  → getIn(arg_idx+1)
//   combined = (hi << 32) | lo
// ---------------------------------------------------------------------------
void TypePropagator::merge_split_double_args(Funcdata* fd) {
    int addrSize = arch ? (int)arch->getDefaultCodeSpace()->getAddrSize() : -1;
    fission::utils::log_stream() << "[merge_split_double] ENTER fd="
              << (fd ? fd->getName() : "null") << " addrSize=" << addrSize << std::endl;
    // Only applicable on 32-bit targets where double fits into two 4B slots.
    if (addrSize != 4) {
        fission::utils::log_stream() << "[merge_split_double] EXIT: not 32-bit" << std::endl;
        return;
    }

    // Collect CPUI_CALL ops first — we will modify them during the loop.
    std::vector<PcodeOp*> call_ops;
    for (auto iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (op && op->code() == CPUI_CALL)
            call_ops.push_back(op);
    }

    fission::utils::log_stream() << "[merge_split_double] fd=" << fd->getName()
              << " call_ops=" << call_ops.size() << std::endl;

    for (PcodeOp* call_op : call_ops) {
        // Use FuncCallSpecs — works for both regular functions (RAM space)
        // and imported functions (fspec / external space).  This is the same
        // mechanism used by propagate_from_call().
        FuncCallSpecs* fc = fd->getCallSpecs(call_op);
        if (!fc) continue;

        // FuncCallSpecs inherits from FuncProto — use its methods directly.
        int num_params = fc->numParams();

        fission::utils::log_stream() << "[merge_split_double]  call=" << fc->getName()
                  << " numParams=" << num_params
                  << " numInput=" << call_op->numInput() << std::endl;

        int arg_idx = 1;  // getIn(0) is the call target address
        for (int pi = 0; pi < num_params; ++pi) {
            if (arg_idx >= call_op->numInput()) break;

            ProtoParameter* param = fc->getParam(pi);
            if (!param) { ++arg_idx; continue; }

            Datatype* pt = param->getType();
            if (!pt) { ++arg_idx; continue; }

            fission::utils::log_stream() << "[merge_split_double]    param[" << pi << "] size=" << pt->getSize()
                      << " meta=" << (int)pt->getMetatype()
                      << " arg_idx=" << arg_idx << " arg.size="
                      << call_op->getIn(arg_idx)->getSize()
                      << " arg.isConst=" << call_op->getIn(arg_idx)->isConstant()
                      << std::endl;

            // Is this an 8-byte parameter that may have been split into two
            // 4-byte inputs on a 32-bit stack?
            // Covers double (TYPE_FLOAT), long long (TYPE_INT), undefined8
            // (TYPE_UNKNOWN) — anything 8 bytes that is not a pointer, array,
            // or struct (those don't get split this way).
            bool is_split_param = (pt->getSize() == 8
                                   && pt->getMetatype() != TYPE_PTR
                                   && pt->getMetatype() != TYPE_ARRAY
                                   && pt->getMetatype() != TYPE_STRUCT);
            if (is_split_param && (arg_idx + 1) < call_op->numInput()) {
                Varnode* lo_vn = call_op->getIn(arg_idx);
                Varnode* hi_vn = call_op->getIn(arg_idx + 1);
                // Only merge when both halves are compile-time constants of 4B.
                if (lo_vn && hi_vn
                    && lo_vn->isConstant() && hi_vn->isConstant()
                    && lo_vn->getSize() == 4 && hi_vn->getSize() == 4) {

                    uint64_t lo_val = lo_vn->getOffset() & 0xFFFFFFFFULL;
                    uint64_t hi_val = hi_vn->getOffset() & 0xFFFFFFFFULL;
                    uint64_t combined = (hi_val << 32) | lo_val;

                    fission::utils::log_stream() << "[merge_split_double]    MERGING: lo=0x" << std::hex << lo_val
                              << " hi=0x" << hi_val << " -> 0x" << combined << std::dec << std::endl;

                    // Create an 8-byte constant and replace the two inputs.
                    Varnode* merged = fd->newConstant(8, combined);
                    fd->opSetInput(call_op, merged, arg_idx);
                    fd->opRemoveInput(call_op, arg_idx + 1);
                    // Consumed one merged slot — advance by 1.
                    ++arg_idx;
                    continue;
                }
                // Non-constant split double: consume both slots as-is.
                fission::utils::log_stream() << "[merge_split_double]    non-constant 8B param, skipping 2 slots" << std::endl;
                arg_idx += 2;
                continue;
            }

            ++arg_idx;
        }
    }
}

void TypePropagator::infer_windows_api_types(PcodeOp* call_op, const std::string& func_name) {
    if (!call_op) return;
    
    TypeFactory* tf = arch->types;
    if (!tf) return;
    
    // Get base integer type for pointer creation
    Datatype* base_int = tf->getBase(1, TYPE_INT);
    if (!base_int) return;
    
    // Handle common patterns
    // CreateFileW(lpFileName, dwDesiredAccess, dwShareMode, lpSecurityAttributes, dwCreationDisposition, dwFlagsAndAttributes, hTemplateFile)
    if (func_name.find("CreateFile") != std::string::npos) {
        if (call_op->numInput() >= 2) {
            Varnode* filename = call_op->getIn(1);
            if (filename) {
                // Try to get wchar_t pointer type
                Datatype* wchar_ptr = tf->getTypePointer(arch->getDefaultCodeSpace()->getAddrSize(), 
                                                         tf->getBase(2, TYPE_INT), 
                                                         arch->getDefaultCodeSpace()->getWordSize());
                if (wchar_ptr) propagate_backwards(filename, wchar_ptr);
            }
        }
        return;
    }
    
    // WriteFile(hFile, lpBuffer, nNumberOfBytesToWrite, lpNumberOfBytesWritten, lpOverlapped)
    if (func_name.find("WriteFile") != std::string::npos || func_name.find("ReadFile") != std::string::npos) {
        if (call_op->numInput() >= 3) {
            Varnode* buffer = call_op->getIn(2);
            if (buffer) {
                Datatype* void_type = tf->getBase(1, TYPE_VOID);
                if (void_type) {
                    Datatype* void_ptr = tf->getTypePointer(arch->getDefaultCodeSpace()->getAddrSize(), 
                                                            void_type, 
                                                            arch->getDefaultCodeSpace()->getWordSize());
                    if (void_ptr) propagate_backwards(buffer, void_ptr);
                }
            }
        }
        return;
    }
    
    // Windows CRT O2-inline decomposed printf:
    //   __acrt_iob_func(unsigned index) -> FILE*   (index 1 = stdout)
    //   __stdio_common_vfprintf(opts, FILE*, fmt, locale, ...) -> int
    // Handle before the generic printf/.find("printf") block below so the
    // more-specific names are not caught by that broad substring match.
    if (func_name == "__acrt_iob_func") {
        Varnode* output = call_op->getOut();
        if (output) {
            // Return is FILE* — represent as void* (we have no FILE typedef).
            Datatype* void_type = tf->getBase(1, TYPE_VOID);
            if (void_type) {
                Datatype* file_ptr = tf->getTypePointer(
                    arch->getDefaultCodeSpace()->getAddrSize(),
                    void_type,
                    arch->getDefaultCodeSpace()->getWordSize());
                if (file_ptr) {
                    uint64_t vid = get_varnode_id(output);
                    inferred_types[vid] = file_ptr;
                }
            }
        }
        return;
    }
    // __stdio_common_vfprintf / vfwprintf / vsprintf_s / vsnprintf_s:
    //   arg1 = uint64 options, arg2 = FILE*, arg3 = const char* format
    if (func_name == "__stdio_common_vfprintf"  ||
        func_name == "__stdio_common_vfwprintf" ||
        func_name == "__stdio_common_vsprintf"  ||
        func_name == "__stdio_common_vsnprintf_s") {
        if (call_op->numInput() >= 4) {
            Datatype* char_ptr = tf->getTypePointer(arch->getDefaultCodeSpace()->getAddrSize(),
                                                    tf->getBase(1, TYPE_INT),
                                                    arch->getDefaultCodeSpace()->getWordSize());
            if (char_ptr) propagate_backwards(call_op->getIn(3), char_ptr);
        }
        return;
    }

    // sprintf, printf family - first arg is char*
    if (func_name.find("printf") != std::string::npos || func_name.find("sprintf") != std::string::npos) {
        if (call_op->numInput() >= 2) {
            Varnode* format = call_op->getIn(1);
            if (format) {
                Datatype* char_ptr = tf->getTypePointer(arch->getDefaultCodeSpace()->getAddrSize(), 
                                                        tf->getBase(1, TYPE_INT), 
                                                        arch->getDefaultCodeSpace()->getWordSize());
                if (char_ptr) propagate_backwards(format, char_ptr);
            }
        }
        return;
    }
    
    // malloc/calloc/realloc — attempt to recover concrete pointer type from CAST uses.
    // Step 3A: When Ghidra emits (SomeType*)malloc(sz), the p-code is:
    //   output = CALL malloc(sz)
    //   cast_out = CAST(output)          <- concrete type on cast_out
    // Scan forward over descendant ops to find any CAST that already has a
    // specific pointer type, and promote the malloc output to that type.
    // This makes create_item-style functions emit typed field access instead
    // of raw pointer arithmetic.
    if (func_name == "malloc" || func_name == "calloc" || func_name == "realloc") {
        Varnode* output = call_op->getOut();
        if (output) {
            // Phase 1: scan CAST descendant uses for concrete pointer type.
            Datatype* promoted = nullptr;
            for (auto use_it = output->beginDescend();
                 use_it != output->endDescend(); ++use_it) {
                PcodeOp* use_op = *use_it;
                if (!use_op || use_op->code() != CPUI_CAST) continue;
                Varnode* cast_out = use_op->getOut();
                if (!cast_out) continue;
                // Prefer TempType (set by ActionInferTypes) over current type.
                Datatype* ct = cast_out->getTempType();
                if (!ct) ct = cast_out->getType();
                if (ct && ct->getMetatype() == TYPE_PTR && ct->getSize() > 0) {
                    // Pick the most specific (largest pointee) if multiple CASTs.
                    if (!promoted || ct->getSize() > promoted->getSize())
                        promoted = ct;
                }
            }
            if (promoted) {
                uint64_t vid = get_varnode_id(output);
                inferred_types[vid] = promoted;
                output->setTempType(promoted);
                return;
            }

            // Phase 2 (fallback): scan COPY/PTRSUB/STORE uses for concrete pointer type.
            // Handles patterns like:
            //   MyStruct* p = (MyStruct*)malloc(sz);  -- COPY(cast_out) with typed dst
            //   *p = x;                               -- STORE where input[1] is the typed ptr
            //   p->field = y;                         -- PTRSUB on the output
            //
            // NOTE: STORE ops have no output varnode (getOut() is always null for STORE).
            //       We inspect input[1] (the address operand) instead, since it is the
            //       varnode that holds the typed pointer derived from the malloc result.
            for (auto use_it = output->beginDescend();
                 use_it != output->endDescend(); ++use_it) {
                PcodeOp* use_op = *use_it;
                if (!use_op) continue;

                Varnode* candidate = nullptr;

                if (use_op->code() == CPUI_COPY) {
                    // Destination carries the type.
                    candidate = use_op->getOut();
                } else if (use_op->code() == CPUI_STORE) {
                    // input[0] = AddrSpace, input[1] = address (typed ptr), input[2] = value.
                    // The address operand (input[1]) tells us what the pointer type should be.
                    if (use_op->numInput() >= 2)
                        candidate = use_op->getIn(1);
                } else if (use_op->code() == CPUI_PTRSUB) {
                    // PTRSUB(base_ptr, offset): base_ptr is input[0].
                    candidate = use_op->getOut();
                }

                if (!candidate) continue;
                Datatype* dt = candidate->getTempType();
                if (!dt) dt = candidate->getType();
                if (dt && dt->getMetatype() == TYPE_PTR && dt->getSize() > 0) {
                    promoted = dt;
                    break;
                }
            }
            if (promoted) {
                uint64_t vid = get_varnode_id(output);
                inferred_types[vid] = promoted;
                output->setTempType(promoted);
                return;
            }

            // Phase 3 (last resort): If Ghidra already committed a concrete pointer
            // type on the call output itself, leave it alone; otherwise set void*.
            Datatype* current = output->getType();
            bool already_concrete_ptr = (current &&
                current->getMetatype() == TYPE_PTR &&
                current->getSize() != 0);
            Datatype* temp = output->getTempType();
            bool temp_concrete_ptr = (temp &&
                temp->getMetatype() == TYPE_PTR &&
                temp->getSize() != 0);
            if (!already_concrete_ptr && !temp_concrete_ptr) {
                Datatype* void_type = tf->getBase(1, TYPE_VOID);
                if (void_type) {
                    Datatype* void_ptr = tf->getTypePointer(
                        arch->getDefaultCodeSpace()->getAddrSize(),
                        void_type,
                        arch->getDefaultCodeSpace()->getWordSize());
                    if (void_ptr) {
                        uint64_t vid = get_varnode_id(output);
                        inferred_types[vid] = void_ptr;
                    }
                }
            }
        }
        return;
    }
    
    // strlen/wcslen - arg is string, returns size_t
    if (func_name == "strlen" || func_name == "wcslen") {
        if (call_op->numInput() >= 2) {
            Varnode* str = call_op->getIn(1);
            if (str) {
                bool is_wide = (func_name == "wcslen");
                int char_size = is_wide ? 2 : 1;
                Datatype* str_type = tf->getTypePointer(arch->getDefaultCodeSpace()->getAddrSize(), 
                                                        tf->getBase(char_size, TYPE_INT), 
                                                        arch->getDefaultCodeSpace()->getWordSize());
                if (str_type) propagate_backwards(str, str_type);
            }
        }
        return;
    }
}

// A-2: POSIX / standard-C API type inference (ELF and Mach-O targets).
void TypePropagator::infer_posix_api_types(PcodeOp* call_op, const std::string& func_name) {
    if (!call_op) return;

    TypeFactory* tf = arch->types;
    if (!tf) return;

    const int ptr_size  = arch->getDefaultCodeSpace()->getAddrSize();
    const int word_size = arch->getDefaultCodeSpace()->getWordSize();

    auto make_ptr = [&](type_metatype meta, int bytes) -> Datatype* {
        Datatype* base = tf->getBase(bytes, meta);
        if (!base) return nullptr;
        return tf->getTypePointer(ptr_size, base, word_size);
    };

    Datatype* void_type = tf->getBase(1, TYPE_VOID);
    Datatype* void_ptr  = void_type ? tf->getTypePointer(ptr_size, void_type, word_size) : nullptr;
    Datatype* char_ptr  = make_ptr(TYPE_INT, 1);   // char*

    // open(const char* path, int flags [, mode_t mode]) -> int fd
    if (func_name == "open" || func_name == "openat") {
        int path_arg = (func_name == "openat") ? 2 : 1;
        if (call_op->numInput() > path_arg && char_ptr) {
            propagate_backwards(call_op->getIn(path_arg), char_ptr);
        }
        return;
    }

    // fopen(const char* path, const char* mode) -> FILE*
    if (func_name == "fopen" || func_name == "fopen64") {
        if (call_op->numInput() >= 2 && char_ptr)
            propagate_backwards(call_op->getIn(1), char_ptr);
        if (call_op->numInput() >= 3 && char_ptr)
            propagate_backwards(call_op->getIn(2), char_ptr);
        return;
    }

    // read(int fd, void* buf, size_t n) / write(int fd, const void* buf, size_t n)
    if (func_name == "read"  || func_name == "write" ||
        func_name == "pread" || func_name == "pwrite") {
        if (call_op->numInput() >= 3 && void_ptr)
            propagate_backwards(call_op->getIn(2), void_ptr);
        return;
    }

    // fread/fwrite(ptr, size, nmemb, FILE*) / fgets(buf, size, FILE*)
    if (func_name == "fread" || func_name == "fwrite" || func_name == "fgets") {
        if (call_op->numInput() >= 2 && void_ptr)
            propagate_backwards(call_op->getIn(1), void_ptr);
        return;
    }

    // memcpy/memmove/memset(void* dst, ...)
    if (func_name == "memcpy" || func_name == "memmove" ||
        func_name == "memset" || func_name == "bcopy") {
        if (call_op->numInput() >= 2 && void_ptr)
            propagate_backwards(call_op->getIn(1), void_ptr);
        if ((func_name == "memcpy" || func_name == "memmove" || func_name == "bcopy") &&
            call_op->numInput() >= 3 && void_ptr)
            propagate_backwards(call_op->getIn(2), void_ptr);
        return;
    }

    // strcmp/strncmp/strcpy/strncpy/strcat(char*, char* [, n])
    if (func_name == "strcmp"  || func_name == "strncmp"  ||
        func_name == "strcpy"  || func_name == "strncpy"  ||
        func_name == "strcat"  || func_name == "strncat"  ||
        func_name == "strchr"  || func_name == "strstr") {
        if (call_op->numInput() >= 2 && char_ptr)
            propagate_backwards(call_op->getIn(1), char_ptr);
        if (call_op->numInput() >= 3 && char_ptr &&
            (func_name == "strncmp" || func_name == "strncpy" || func_name == "strncat"))
            propagate_backwards(call_op->getIn(2), char_ptr);
        return;
    }

    // strlen/strnlen(const char* s [, size_t n])
    if (func_name == "strlen" || func_name == "strnlen") {
        if (call_op->numInput() >= 2 && char_ptr)
            propagate_backwards(call_op->getIn(1), char_ptr);
        return;
    }

    // printf/fprintf/sprintf/snprintf(const char* fmt, ...)
    if (func_name == "printf" || func_name == "vprintf") {
        if (call_op->numInput() >= 2 && char_ptr)
            propagate_backwards(call_op->getIn(1), char_ptr);
        return;
    }
    if (func_name == "fprintf" || func_name == "vfprintf") {
        if (call_op->numInput() >= 3 && char_ptr)
            propagate_backwards(call_op->getIn(2), char_ptr);
        return;
    }
    if (func_name == "sprintf" || func_name == "snprintf" ||
        func_name == "vsprintf" || func_name == "vsnprintf") {
        if (call_op->numInput() >= 2 && void_ptr)
            propagate_backwards(call_op->getIn(1), void_ptr);
        if (func_name == "snprintf" || func_name == "vsnprintf") {
            if (call_op->numInput() >= 4 && char_ptr)
                propagate_backwards(call_op->getIn(3), char_ptr);
        } else {
            if (call_op->numInput() >= 3 && char_ptr)
                propagate_backwards(call_op->getIn(2), char_ptr);
        }
        return;
    }
}

void TypePropagator::propagate_backwards(Varnode* vn, Datatype* type) {
    if (!vn || !type) return;
    
    uint64_t vid = get_varnode_id(vn);
    if (processed.count(vid)) return;
    processed.insert(vid);
    
    // Store inferred type
    auto it = inferred_types.find(vid);
    if (it == inferred_types.end()) {
        inferred_types[vid] = type;
    } else {
        // Keep more specific type (non-void, has known size)
        if (type->getSize() > it->second->getSize()) {
            inferred_types[vid] = type;
        }
    }

    if (type->getMetatype() != TYPE_UNKNOWN &&
        (type->getSize() == 0 || type->getSize() == vn->getSize())) {
        vn->setTempType(type);
    }
    
    // Follow definition backwards
    PcodeOp* def = vn->getDef();
    if (!def) return;
    
    OpCode opc = def->code();
    
    switch (opc) {
        case CPUI_COPY:
            // Direct copy - propagate to input
            if (def->numInput() > 0) {
                propagate_backwards(def->getIn(0), type);
            }
            break;
            
        case CPUI_CAST:
            // Cast - propagate to input (may need adjustment)
            if (def->numInput() > 0) {
                Varnode* input = def->getIn(0);
                // For casts, try to infer input type based on output
                if (input->getSize() == type->getSize()) {
                    propagate_backwards(input, type);
                }
            }
            break;
            
        case CPUI_LOAD:
            // Load from memory - could track pointer type
            if (def->numInput() >= 2) {
                Varnode* ptr = def->getIn(1);
                // Create pointer type to loaded value
                Datatype* ptr_type = arch->types->getTypePointer(
                    arch->getDefaultCodeSpace()->getAddrSize(),
                    type,
                    arch->getDefaultCodeSpace()->getWordSize()
                );
                if (ptr_type) {
                    propagate_backwards(ptr, ptr_type);
                }
            }
            break;
            
        case CPUI_MULTIEQUAL:
            // PHI node - propagate to all inputs
            for (int i = 0; i < def->numInput(); ++i) {
                propagate_backwards(def->getIn(i), type);
            }
            break;
            
        case CPUI_INT_ADD:
        case CPUI_INT_SUB:
            // Arithmetic operations - propagate integer type
            if (type->getMetatype() == TYPE_INT || type->getMetatype() == TYPE_UINT) {
                for (int i = 0; i < def->numInput(); ++i) {
                    Varnode* input = def->getIn(i);
                    if (input->getSize() == type->getSize()) {
                        propagate_backwards(input, type);
                    }
                }
            }
            break;
            
        case CPUI_PTRSUB:
        case CPUI_PTRADD:
            // Pointer arithmetic - first input should be pointer
            if (def->numInput() > 0 && type->getMetatype() == TYPE_PTR) {
                propagate_backwards(def->getIn(0), type);
            }
            break;
            
        case CPUI_INT_ZEXT:
        case CPUI_INT_SEXT:
            // Extension operations - propagate smaller type to input
            if (def->numInput() > 0) {
                Varnode* input = def->getIn(0);
                Datatype* input_type = arch->types->getBase(
                    input->getSize(),
                    (opc == CPUI_INT_SEXT) ? TYPE_INT : TYPE_UINT
                );
                if (input_type) {
                    propagate_backwards(input, input_type);
                }
            }
            break;
            
        default:
            // For other operations, don't propagate backwards
            break;
    }
}

bool TypePropagator::propagate_type_edge(PcodeOp* op, int inslot, int outslot) {
    if (!op) return false;
    
    Varnode* invn = (inslot == -1) ? op->getOut() : op->getIn(inslot);
    if (!invn) return false;

    Datatype* alttype = invn->getTempType();
    if (!alttype) return false;

    if (alttype->needsResolution()) {
        alttype = alttype->resolveInFlow(op, inslot);
    }
    if (inslot == outslot) return false;

    Varnode* outvn = nullptr;
    if (outslot < 0) {
        outvn = op->getOut();
    } else {
        outvn = op->getIn(outslot);
        if (outvn && outvn->isAnnotation()) return false;
    }
    if (!outvn) return false;
    if (outvn->isTypeLock()) return false;
    if (outslot >= 0 && outvn->stopsUpPropagation()) return false;

    if (alttype->getMetatype() == TYPE_BOOL && outvn->getNZMask() > 1) {
        return false;
    }

    Datatype* newtype = op->getOpcode()->propagateType(alttype, op, invn, outvn, inslot, outslot);
    if (!newtype) return false;

    if (0 > newtype->typeOrder(*outvn->getTempType())) {
        outvn->setTempType(newtype);
        return !outvn->isMark();
    }
    return false;
}

void TypePropagator::apply_inferred_types(Funcdata* fd) {
    int applied = 0;
    // Apply types to high-level varnodes
    VarnodeLocSet::const_iterator iter;
    for (iter = fd->beginLoc(); iter != fd->endLoc(); ++iter) {
        Varnode* vn = *iter;
        if (!vn) continue;

        Datatype* temp_type = vn->getTempType();
        if (temp_type && temp_type->getMetatype() != TYPE_UNKNOWN &&
            (temp_type->getSize() == 0 || temp_type->getSize() == vn->getSize())) {
            if (vn->updateType(temp_type)) {
                applied++;
                continue;
            }
        }

        uint64_t vid = get_varnode_id(vn);
        auto it = inferred_types.find(vid);
        if (it != inferred_types.end() && it->second) {
            Datatype* inferred = it->second;
            // typeOrder guard: skip if varnode already carries a more-specific pointer type
            // (mirrors Ghidra's ActionInferTypes::propagateTypeEdge typeOrder check).
            Datatype* existing = vn->getType();
            bool existing_is_more_specific = false;
            if (existing && existing->getMetatype() == TYPE_PTR &&
                inferred->getMetatype() == TYPE_PTR) {
                // TYPE_PTR guarantees TypePointer — safe to static_cast
                TypePointer* ep = static_cast<TypePointer*>(existing);
                TypePointer* ip = static_cast<TypePointer*>(inferred);
                if (ep && ip && ep->getPtrTo() && ip->getPtrTo()) {
                    type_metatype em = ep->getPtrTo()->getMetatype();
                    type_metatype im = ip->getPtrTo()->getMetatype();
                    // Existing is more specific when the inferred pointee is void/unknown
                    // but the existing pointee is a concrete type.
                    existing_is_more_specific = (im == TYPE_VOID || im == TYPE_UNKNOWN) &&
                                                (em != TYPE_VOID && em != TYPE_UNKNOWN);
                }
            }
            if (existing_is_more_specific) continue;
            if (inferred->getMetatype() != TYPE_UNKNOWN &&
                (inferred->getSize() == 0 || inferred->getSize() == vn->getSize())) {
                if (vn->updateType(inferred)) {
                    applied++;
                }
            }
        }
    }

    if (applied > 0) {
        fission::utils::log_stream() << "[TypePropagator] Applied " << applied << " inferred types" << std::endl;
    }
}

void TypePropagator::propagate_one_type(Varnode* vn) {
    if (!vn) return;
    
    // Use a work queue for propagation
    std::vector<Varnode*> work_queue;
    work_queue.push_back(vn);
    vn->setMark();
    
    while (!work_queue.empty()) {
        Varnode* current = work_queue.back();
        work_queue.pop_back();
        
        // Propagate to all descendant operations
        list<PcodeOp*>::const_iterator iter;
        for (iter = current->beginDescend(); iter != current->endDescend(); ++iter) {
            PcodeOp* op = *iter;
            if (op->isDead()) continue;
            
            int inslot = op->getSlot(current);
            
            // Try to propagate to output
            if (propagate_type_edge(op, inslot, -1)) {
                Varnode* out = op->getOut();
                if (out && !out->isMark()) {
                    work_queue.push_back(out);
                    out->setMark();
                }
            }
            
            // Try to propagate to other inputs
            for (int outslot = 0; outslot < op->numInput(); ++outslot) {
                if (outslot == inslot) continue;
                if (propagate_type_edge(op, inslot, outslot)) {
                    Varnode* in = op->getIn(outslot);
                    if (in && !in->isMark() && !in->isAnnotation()) {
                        work_queue.push_back(in);
                        in->setMark();
                    }
                }
            }
        }
        
        // Also check definition
        if (current->isWritten()) {
            PcodeOp* def = current->getDef();
            if (def && !def->isDead()) {
                for (int inslot = 0; inslot < def->numInput(); ++inslot) {
                    if (propagate_type_edge(def, -1, inslot)) {
                        Varnode* in = def->getIn(inslot);
                        if (in && !in->isMark() && !in->isAnnotation()) {
                            work_queue.push_back(in);
                            in->setMark();
                        }
                    }
                }
            }
        }
        
        current->clearMark();
    }
}

void TypePropagator::build_local_types(Funcdata* fd) {
    if (!fd) return;

    VarnodeLocSet::const_iterator iter;
    for (iter = fd->beginLoc(); iter != fd->endLoc(); ++iter) {
        Varnode* vn = *iter;
        if (!vn) continue;
        if (vn->isAnnotation()) continue;
        if (!vn->isWritten() && vn->hasNoDescend()) continue;

        bool needs_block = false;
        Datatype* ct = nullptr;

        try {
            SymbolEntry* entry = vn->getSymbolEntry();
            if (entry != nullptr && !vn->isTypeLock() && entry->getSymbol()->isTypeLocked()) {
                TypeFactory* typegrp = fd->getArch()->types;
                int4 cur_off = (vn->getAddr().getOffset() - entry->getAddr().getOffset()) + entry->getOffset();
                ct = typegrp->getExactPiece(entry->getSymbol()->getType(), cur_off, vn->getSize());
                if (ct == nullptr || ct->getMetatype() == TYPE_UNKNOWN) {
                    ct = vn->getLocalType(needs_block);
                }
            } else {
                ct = vn->getLocalType(needs_block);
            }
        } catch (const LowlevelError&) {
            // Expected: varnode not fully initialised or type not resolvable — skip.
            continue;
        } catch (const std::exception& ex) {
            fission::utils::log_stream() << "[TypePropagator] WARNING: unexpected exception during "
                                            "varnode type inference: " << ex.what() << std::endl;
            continue;
        } catch (...) {
            fission::utils::log_stream() << "[TypePropagator] WARNING: unknown exception during "
                                            "varnode type inference — skipping varnode" << std::endl;
            continue;
        }

        if (!ct) continue;
        if (needs_block) {
            vn->setStopUpPropagation();
        }
        vn->setTempType(ct);

        uint64_t vid = get_varnode_id(vn);
        auto it = inferred_types.find(vid);
        if (it != inferred_types.end() && it->second) {
            if (0 > it->second->typeOrder(*vn->getTempType())) {
                vn->setTempType(it->second);
            }
        }
    }
}

int TypePropagator::propagate(Funcdata* fd) {
    if (!fd) return 0;

    // Prevent infinite loops (Ghidra uses max 7 iterations)
    if (local_count >= MAX_TYPE_ITERATIONS) {
        if (local_count == MAX_TYPE_ITERATIONS) {
            fission::utils::log_stream() << "[TypePropagator] Type propagation not settling after "
                      << MAX_TYPE_ITERATIONS << " iterations" << std::endl;
            local_count++;
        }
        return 0;
    }

    int count = 0;

    // Phase 1: Build local types from PcodeOp context (Ghidra style)
    build_local_types(fd);

    // Phase 2: Propagate call return types using FuncCallSpecs
    propagate_call_return_types(fd);

    // Phase 3: Find all CALL operations and propagate from them
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (op && op->code() == CPUI_CALL) {
            propagate_from_call(fd, op);
            count++;
        }
    }

    // Phase 4: Edge-based propagation for all varnodes (Ghidra style)
    VarnodeLocSet::const_iterator vn_iter;
    for (vn_iter = fd->beginLoc(); vn_iter != fd->endLoc(); ++vn_iter) {
        Varnode* vn = *vn_iter;
        if (vn->isAnnotation()) continue;
        if (!vn->isWritten() && vn->hasNoDescend()) continue;
        propagate_one_type(vn);
    }

    // Phase 4b: Synchronise types across CPUI_RETURN ops (Ghidra propagateAcrossReturns)
    propagate_across_returns(fd);

    // Phase 5: Infer pointer types for stack variables
    infer_stack_pointer_types(fd);

    // Phase 5b: Infer pointer types for function parameters
    infer_parameter_pointer_types(fd);

    // Phase 6: Write back and check for changes (Ghidra style)
    bool changed = write_back(fd);

    // Phase 7: Apply inferred types
    if (!inferred_types.empty()) {
        apply_inferred_types(fd);
    }

    // Recurse if types changed (iterative propagation)
    if (changed) {
        local_count++;
        return propagate(fd);
    }

    return inferred_types.size();
}

Datatype* TypePropagator::get_type(Varnode* vn) {
    if (!vn) return nullptr;
    uint64_t vid = get_varnode_id(vn);
    auto it = inferred_types.find(vid);
    return (it != inferred_types.end()) ? it->second : nullptr;
}

void TypePropagator::clear() {
    inferred_types.clear();
    processed.clear();
    local_count = 0;
}

void TypePropagator::infer_stack_pointer_types(Funcdata* fd) {
    if (!fd) return;
    
    TypeFactory* tf = arch->types;
    if (!tf) return;
    
    int ptr_size = fission::core::ArchPolicy::getPointerSize(arch);
    int pointer_types_applied = 0;
    
    // Iterate through all varnodes looking for pointer-sized stack variables
    VarnodeLocSet::const_iterator iter;
    for (iter = fd->beginLoc(); iter != fd->endLoc(); ++iter) {
        Varnode* vn = *iter;
        if (!vn) continue;
        if (vn->isAnnotation()) continue;
        if (vn->isTypeLock()) continue;  // Skip if type is already locked
        if (vn->isConstant()) continue;  // Skip constants
        
        // Only check pointer-sized varnodes
        if ((int)vn->getSize() != ptr_size) continue;
        
        // Conservative pointer detection: only check LOAD/STORE dereferencing
        bool is_dereferenced = false;
        
        auto desc_iter = vn->beginDescend();
        auto desc_end = vn->endDescend();
        
        for (; desc_iter != desc_end; ++desc_iter) {
            PcodeOp* use_op = *desc_iter;
            if (!use_op) continue;
            
            OpCode code = use_op->code();
            
            // LOAD(space, ptr) - ptr is input(1), value is dereferenced
            if (code == CPUI_LOAD && use_op->getIn(1) == vn) {
                is_dereferenced = true;
                break;
            }
            
            // STORE(space, ptr, val) - ptr is input(1), value is dereferenced
            if (code == CPUI_STORE && use_op->getIn(1) == vn) {
                is_dereferenced = true;
                break;
            }
            
            // PTRSUB/PTRADD operations indicate pointer arithmetic
            if (code == CPUI_PTRSUB || code == CPUI_PTRADD) {
                if (use_op->getIn(0) == vn) {
                    is_dereferenced = true;
                    break;
                }
            }
        }
        
        if (!is_dereferenced) continue;
        
        // Get current type
        Datatype* current_type = vn->getType();
        if (current_type && current_type->getMetatype() == TYPE_PTR) continue;  // Already a pointer
        
        // Check if it's an integer type that should be a pointer
        if (current_type) {
            type_metatype meta = current_type->getMetatype();
            if (meta != TYPE_INT && meta != TYPE_UINT && meta != TYPE_UNKNOWN) continue;
        }
        
        // Create void* type
        Datatype* void_type = tf->getTypeVoid();
        Datatype* void_ptr = tf->getTypePointer(ptr_size, void_type, ptr_size);
        
        if (void_ptr) {
            // Store inferred type
            uint64_t vid = get_varnode_id(vn);
            inferred_types[vid] = void_ptr;
            
            // Also set temp type for propagation
            vn->setTempType(void_ptr);
            pointer_types_applied++;
        }
    }
    
    if (pointer_types_applied > 0) {
        fission::utils::log_stream() << "[TypePropagator] Inferred " << pointer_types_applied 
                  << " pointer types from usage analysis" << std::endl;
    }
}

void TypePropagator::infer_parameter_pointer_types(Funcdata* fd) {
    if (!fd) return;
    
    TypeFactory* tf = arch->types;
    if (!tf) return;
    
    int ptr_size = fission::core::ArchPolicy::getPointerSize(arch);
    int pointer_params_applied = 0;
    
    // Get function prototype
    FuncProto& proto = fd->getFuncProto();
    int num_params = proto.numParams();
    
    fission::utils::log_stream() << "[TypePropagator] Analyzing " << num_params << " parameters for pointer inference" << std::endl;
    
    // Check each parameter
    for (int i = 0; i < num_params; ++i) {
        ProtoParameter* param = proto.getParam(i);
        if (!param) continue;
        
        // Skip if type is already locked
        if (param->isTypeLocked()) continue;
        
        Datatype* current_type = param->getType();
        
        // Skip if already a pointer
        if (current_type && current_type->getMetatype() == TYPE_PTR) continue;
        
        // Get the address of the parameter storage
        Address param_addr = param->getAddress();
        
        // Find all varnodes at this address
        VarnodeDefSet::const_iterator iter;
        bool is_dereferenced = false;
        Datatype* inferred_element_type = nullptr;
        
        for (iter = fd->beginDef(); iter != fd->endDef(); ++iter) {
            Varnode* vn = *iter;
            if (!vn) continue;
            
            // Check if this varnode corresponds to the parameter
            if (vn->getAddr() != param_addr) continue;
            if ((int)vn->getSize() != ptr_size) continue;
            
            // Check descendants (usages)
            auto desc_iter = vn->beginDescend();
            auto desc_end = vn->endDescend();
            
            for (; desc_iter != desc_end; ++desc_iter) {
                PcodeOp* use_op = *desc_iter;
                if (!use_op) continue;
                
                OpCode code = use_op->code();
                
                // LOAD(space, ptr) - ptr is input(1), value is dereferenced
                if (code == CPUI_LOAD && use_op->getIn(1) == vn) {
                    is_dereferenced = true;
                    
                    // Try to infer element type from the LOAD result
                    Varnode* loaded = use_op->getOut();
                    if (loaded) {
                        int element_size = loaded->getSize();
                        Datatype* elem_type = nullptr;
                        
                        // Infer element type based on size
                        if (element_size == 1) {
                            elem_type = tf->getBase(1, TYPE_INT);  // char
                        } else if (element_size == 2) {
                            elem_type = tf->getBase(2, TYPE_INT);  // short
                        } else if (element_size == 4) {
                            elem_type = tf->getBase(4, TYPE_INT);  // int
                        } else if (element_size == 8) {
                            elem_type = tf->getBase(8, TYPE_INT);  // long long
                        }
                        
                        if (elem_type && !inferred_element_type) {
                            inferred_element_type = elem_type;
                        }
                    }
                    break;
                }
                
                // STORE(space, ptr, val) - ptr is input(1), value is dereferenced
                if (code == CPUI_STORE && use_op->getIn(1) == vn) {
                    is_dereferenced = true;
                    
                    // Try to infer element type from the STORE value
                    Varnode* stored = use_op->getIn(2);
                    if (stored) {
                        int element_size = stored->getSize();
                        Datatype* elem_type = nullptr;
                        
                        if (element_size == 1) {
                            elem_type = tf->getBase(1, TYPE_INT);
                        } else if (element_size == 2) {
                            elem_type = tf->getBase(2, TYPE_INT);
                        } else if (element_size == 4) {
                            elem_type = tf->getBase(4, TYPE_INT);
                        } else if (element_size == 8) {
                            elem_type = tf->getBase(8, TYPE_INT);
                        }
                        
                        if (elem_type && !inferred_element_type) {
                            inferred_element_type = elem_type;
                        }
                    }
                    break;
                }
                
                // PTRSUB/PTRADD operations indicate pointer arithmetic
                if (code == CPUI_PTRSUB || code == CPUI_PTRADD) {
                    if (use_op->getIn(0) == vn) {
                        is_dereferenced = true;
                        break;
                    }
                }
                
                // INT_ADD with constant offset (array access pattern: param_1 + i * 4)
                if (code == CPUI_INT_ADD) {
                    is_dereferenced = true;
                    break;
                }
            }
            
            if (is_dereferenced) break;
        }
        
        // If parameter is dereferenced, store the inferred type for later application
        if (is_dereferenced) {
            Datatype* element_type = inferred_element_type;
            
            // If we couldn't infer element type, use void
            if (!element_type) {
                element_type = tf->getTypeVoid();
            }
            
            Datatype* ptr_type = tf->getTypePointer(ptr_size, element_type, ptr_size);
            
            if (ptr_type) {
                // Store the inferred type for this parameter
                // We'll apply it through the normal type propagation mechanism
                // by finding the corresponding varnodes and setting their temp types
                
                for (iter = fd->beginDef(); iter != fd->endDef(); ++iter) {
                    Varnode* vn = *iter;
                    if (!vn) continue;
                    
                    if (vn->getAddr() == param_addr && (int)vn->getSize() == ptr_size) {
                        vn->setTempType(ptr_type);
                        uint64_t vid = get_varnode_id(vn);
                        inferred_types[vid] = ptr_type;
                        pointer_params_applied++;
                        
                        fission::utils::log_stream() << "[TypePropagator] Parameter " << i << " inferred as " 
                                  << ptr_type->getName() << " (was " 
                                  << (current_type ? current_type->getName() : "unknown") << ")" << std::endl;
                        break;
                    }
                }
            }
        }
    }
    
    if (pointer_params_applied > 0) {
        fission::utils::log_stream() << "[TypePropagator] Inferred " << pointer_params_applied 
                  << " parameter pointer types from usage analysis" << std::endl;
    }
}

bool TypePropagator::propagate_struct_types(Funcdata* fd) {
    if (!fd || !arch || !arch->types) return false;

    bool changed = false;
    TypeFactory* tf = arch->types;
    if (!tf) return false;
    int ptr_size = fission::core::ArchPolicy::getPointerSize(arch);

    // Seed call return types early so stack struct updates can see pointer returns.
    propagate_call_return_types(fd);

    if (struct_registry && !struct_registry->empty()) {
        // Scan all CALL operations (struct_registry -> call args)
        list<PcodeOp*>::const_iterator iter;
        for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
            PcodeOp* op = *iter;
            if (!op || op->code() != CPUI_CALL) continue;

            Varnode* target = op->getIn(0);
            if (!target || !target->isConstant()) continue;

            uint64_t callee_addr = target->getOffset();

            // Check if this function has registered struct parameters
            if (struct_registry->count(callee_addr)) {
                const auto& params_map = struct_registry->at(callee_addr);

                // Apply struct types to each argument
                int num_inputs = op->numInput();
                for (int i = 1; i < num_inputs; ++i) {
                    int param_index = i - 1;
                    if (params_map.count(param_index)) {
                        std::string struct_name = params_map.at(param_index);

                        // Find struct type by name
                        Datatype* type = tf->findByName(struct_name);
                        if (type) {
                            // Create pointer to struct
                            Datatype* ptr_type = tf->getTypePointer(ptr_size, type, ptr_size);

                            Varnode* arg = op->getIn(i);
                            if (arg) {
                                // Force update with type lock
                                arg->updateType(ptr_type, true, true);
                                changed = true;

                                fission::utils::log_stream() << "[TypePropagator] Applied " << struct_name
                                          << "* to arg " << i << " of call to 0x"
                                          << std::hex << callee_addr << std::dec << std::endl;
                            }
                        }
                    }
                }
            }
        }
    }

    // Stack struct field updates based on inferred pointer origins.
    AddrSpace* stack_space = arch->getStackSpace();
    if (!stack_space) return changed;

    auto value_is_pointer = [&](Varnode* vn, const PcodeOp* read_op, int depth, auto&& self_ref) -> bool {
        if (!vn || depth <= 0) return false;

        auto type_label = [](Datatype* type) -> std::string {
            return type ? type->getName() : "null";
        };

        Datatype* inferred = get_type(vn);
        Datatype* temp_type = vn->getTempType();
        Datatype* base_type = vn->getType();
        Datatype* high_read = nullptr;
        Datatype* high_def = nullptr;

        // Safely access high variable types with exception handling
        // getHigh() may return non-null but the high variable may not be fully initialized
        try {
            if (vn->getHigh()) {
                if (read_op) {
                    int slot = read_op->getSlot(vn);
                    if (slot >= 0 && slot < read_op->numInput()) {
                        high_read = vn->getHighTypeReadFacing(read_op);
                    }
                }
                if (vn->isWritten()) {
                    high_def = vn->getHighTypeDefFacing();
                }
            }
        } catch (const LowlevelError&) {
            // High variable not fully initialized, skip high type checks
            high_read = nullptr;
            high_def = nullptr;
        } catch (...) {
            high_read = nullptr;
            high_def = nullptr;
        }

        if (read_op && read_op->code() == CPUI_STORE) {
            fission::utils::log_stream() << "[TypePropagator] STORE value types: temp=" << type_label(temp_type)
                      << " base=" << type_label(base_type)
                      << " inferred=" << type_label(inferred)
                      << " high_read=" << type_label(high_read)
                      << " high_def=" << type_label(high_def)
                      << " size=" << vn->getSize() << std::endl;
        }

        if (is_pointer_type(inferred) || is_pointer_type(temp_type) || is_pointer_type(base_type) ||
            is_pointer_type(high_read) || is_pointer_type(high_def)) {
            if (read_op && read_op->code() == CPUI_STORE) {
                fission::utils::log_stream() << "[TypePropagator] STORE value pointer hit (direct)" << std::endl;
            }
            return true;
        }

        if (!vn->isWritten()) return false;
        PcodeOp* def = vn->getDef();
        if (!def) return false;

        OpCode opc = def->code();
        if (opc == CPUI_CALL || opc == CPUI_CALLIND) {
            FuncCallSpecs* fc = fd->getCallSpecs(def);
            Datatype* ret = fc ? fc->getOutputType() : nullptr;
            bool is_ptr = is_pointer_type(ret);
            fission::utils::log_stream() << "[TypePropagator] CALL value types: ret=" << type_label(ret)
                      << " ptr=" << (is_ptr ? "yes" : "no") << std::endl;
            return is_ptr;
        }

        switch (opc) {
            case CPUI_COPY:
            case CPUI_CAST:
            case CPUI_INT_ZEXT:
            case CPUI_INT_SEXT:
            case CPUI_SUBPIECE:
                return self_ref(def->getIn(0), def, depth - 1, self_ref);
            case CPUI_MULTIEQUAL:
            case CPUI_INDIRECT: {
                for (int slot = 0; slot < def->numInput(); ++slot) {
                    if (self_ref(def->getIn(slot), def, depth - 1, self_ref)) {
                        return true;
                    }
                }
                return false;
            }
            default:
                return false;
        }
    };

    std::set<int64_t> pointer_offsets;

    // STORE-based pointer detection (covers casted values)
    for (auto iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op || op->code() != CPUI_STORE) continue;

        Varnode* addr_vn = op->getIn(1);
        int64_t offset = 0;
        if (!resolve_stack_offset(addr_vn, stack_space, offset)) {
            continue;
        }

        Varnode* val = op->getIn(2);
        if (val && value_is_pointer(val, op, 6, value_is_pointer)) {
            pointer_offsets.insert(offset);
        }
    }

    // Varnode-based pointer detection (already mapped to stack space)
    for (auto iter = fd->beginLoc(); iter != fd->endLoc(); ++iter) {
        Varnode* vn = *iter;
        if (!vn || vn->isAnnotation() || vn->isConstant()) continue;
        if (vn->getSpace() != stack_space) continue;

        int64_t offset = normalize_stack_offset(stack_space, vn->getOffset());
        if (value_is_pointer(vn, nullptr, 6, value_is_pointer)) {
            pointer_offsets.insert(offset);
        }
    }

    if (pointer_offsets.empty()) return changed;

    StackFrameAnalyzer stack_analyzer(arch);
    int detected = stack_analyzer.analyze(fd);
    if (detected <= 0) return changed;

    Datatype* void_type = tf->getTypeVoid();
    Datatype* void_ptr = tf->getTypePointer(ptr_size, void_type, ptr_size);
    if (!void_ptr) return changed;

    for (const auto& cluster : stack_analyzer.get_clusters()) {
        Datatype* existing = tf->findByName(cluster.inferred_name);
        if (!existing || existing->getMetatype() != TYPE_STRUCT) {
            continue;
        }

        TypeStruct* ts = static_cast<TypeStruct*>(existing);
        int struct_size = ts->getSize();
        bool struct_changed = false;
        std::vector<TypeField> fields;

        for (auto it = ts->beginField(); it != ts->endField(); ++it) {
            Datatype* field_type = it->type;
            int64_t abs_off = cluster.base_offset + it->offset;
            bool should_pointer = pointer_offsets.count(abs_off) > 0;

            if (should_pointer && (!field_type || !is_pointer_type(field_type))) {
                int field_size = field_type ? field_type->getSize() : 0;
                if (field_size == ptr_size) {
                    field_type = void_ptr;
                    struct_changed = true;
                }
            }

            fields.push_back(TypeField(0, it->offset, it->name, field_type));
        }

        if (struct_changed) {
            // Only try to set fields if the structure supports modification
            // Ghidra's TypeFactory only allows setFields on incomplete structures
            try {
                tf->setFields(fields, ts, struct_size, 0, 0);
                changed = true;
            } catch (const LowlevelError&) {
                // Structure is already complete, cannot modify fields - this is not fatal
            }
        }
    }

    return changed;
}

std::string TypePropagator::apply_struct_types(
    std::string c_code,
    Funcdata* fd,
    const std::map<unsigned long long, TypeStruct*>& structs
) {
    if (!fd || structs.empty()) return c_code;

    const FuncProto& proto = fd->getFuncProto();
    int numParams = proto.numParams();
    
    for (int i = 0; i < numParams; ++i) {
        ProtoParameter* param = proto.getParam(i);
        if (!param) continue;
        
        uint64_t off = param->getAddress().getOffset();
        
        if (structs.count(off)) {
            TypeStruct* st = structs.at(off);
            if (!st) continue;
            
            std::string sname = st->getName();
            std::string pname = param->getName();
            
            // Search for pointer declaration: "*pname" or "* pname"
            std::string target = "*" + pname;
            size_t pos = c_code.find(target);
            
            if (pos == std::string::npos) {
                target = "* " + pname;
                pos = c_code.find(target);
            }
            
            if (pos != std::string::npos) {
                // Backtrack to find type name start
                size_t type_end = pos;
                while (type_end > 0 && (c_code[type_end-1] == ' ' || c_code[type_end-1] == '\t')) {
                    type_end--;
                }
                
                size_t type_start = type_end;
                while (type_start > 0) {
                    char c = c_code[type_start-1];
                    if (c == ' ' || c == '\t' || c == '\n' || c == '(' || c == ',') break;
                    type_start--;
                }
                
                if (type_start < type_end) {
                    std::string old_type = c_code.substr(type_start, type_end - type_start);
                    c_code.replace(type_start, type_end - type_start, sname);
                    
                    fission::utils::log_stream() << "[TypePropagator] Replaced type '" << old_type 
                              << "' for " << pname << " with " << sname << std::endl;
                }
            }
        }
    }
    
    return c_code;
}

std::string TypePropagator::get_fid_filename(bool is_64bit, const std::string& compiler_id) {
    return fission::config::get_fid_filename(is_64bit, compiler_id);
}

bool TypePropagator::write_back(Funcdata* fd) {
    if (!fd) return false;
    
    bool change = false;
    VarnodeLocSet::const_iterator iter;
    
    for (iter = fd->beginLoc(); iter != fd->endLoc(); ++iter) {
        Varnode* vn = *iter;
        if (vn->isAnnotation()) continue;
        if (!vn->isWritten() && vn->hasNoDescend()) continue;
        
        Datatype* ct = vn->getTempType();
        if (!ct) continue;
        
        // Compare with current type and update if temp type is better
        Datatype* current = vn->getType();
        if (current == ct) continue;
        
        // Check type ordering (Ghidra's typeOrder)
        // 0 means same, < 0 means ct is more specific
        if (ct != current) {
            // Update varnode type
            if (vn->updateType(ct)) {
                change = true;
            }
        }
    }
    
    return change;
}

void TypePropagator::propagate_call_return_types(Funcdata* fd) {
    if (!fd) return;
    
    TypeFactory* tf = arch->types;
    if (!tf) return;
    
    int ptr_size = fission::core::ArchPolicy::getPointerSize(arch);
    int types_set = 0;
    
    // Scan all CALL and CALLIND operations
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op) continue;
        
        OpCode opc = op->code();
        if (opc != CPUI_CALL && opc != CPUI_CALLIND) continue;
        
        Varnode* output = op->getOut();
        if (!output) continue;
        
        // Get FuncCallSpecs for this call
        FuncCallSpecs* fc = fd->getCallSpecs(op);
        if (!fc) continue;
        
        // Check if output type is already locked with a valid pointer type
        std::string func_name_dbg = fc->getName();
        bool should_apply_heuristic = true;
        
        if (fc->isOutputLocked()) {
            Datatype* retType = fc->getOutputType();
            fission::utils::log_stream() << "[TypePropagator] " << func_name_dbg << " output locked, type=" 
                      << (retType ? retType->getName() : "null") 
                      << ", meta=" << (retType ? (int)retType->getMetatype() : -1) << std::endl;
            // Only skip heuristic if already a pointer type
            if (retType && retType->getMetatype() == TYPE_PTR) {
                output->setTempType(retType);
                uint64_t vid = get_varnode_id(output);
                inferred_types[vid] = retType;
                types_set++;
                should_apply_heuristic = false;  // Already has pointer type
            }
        }
        
        // Apply heuristic if no valid type was found
        if (!should_apply_heuristic) {
            fission::utils::log_stream() << "Skipping heuristic for " << func_name_dbg << " (already valid)" << std::endl;
            continue;
        }
        
        fission::utils::log_stream() << "Proceeding to heuristic for " << func_name_dbg << std::endl;
        
        if (output->getSize() == ptr_size || output->getSize() >= ptr_size) {
            std::string func_name = fc->getName();
            
            // ── Step 1: Try to resolve the callee's Funcdata and read its prototype ──
            // If the callee has a known return type (e.g., from COFF symbols +
            // prior type-recovery) that's a pointer, use it directly instead of
            // falling back to the name-based heuristic below.
            Datatype* resolved_ret = nullptr;
            Varnode* target_vn = op->getIn(0);
            if (target_vn && target_vn->isConstant()) {
                uint64_t target_addr = target_vn->getOffset();
                try {
                    Funcdata* callee_fd = arch->symboltab->getGlobalScope()->queryFunction(
                        Address(arch->getDefaultCodeSpace(), target_addr));
                    if (callee_fd) {
                        FuncProto& callee_proto = callee_fd->getFuncProto();
                        Datatype* ret = callee_proto.getOutputType();
                        if (ret && ret->getMetatype() == TYPE_PTR) {
                            resolved_ret = ret;
                            fission::utils::log_stream() << "[TypePropagator] Resolved concrete return ptr type "
                                      << ret->getName() << " for callee " << func_name << std::endl;
                        }
                    }
                } catch (const LowlevelError&) {
                    // Callee not yet analysed — normal during incremental analysis, skip.
                } catch (...) {}
            }

            // ── Step 2: Name-based heuristic (allocator pattern) ──
            Datatype* ptr_type = nullptr;
            if (resolved_ret) {
                ptr_type = resolved_ret;
            } else {
                // Normalize to lowercase for matching
                std::string lower_name;
                for (char c : func_name) {
                    lower_name += std::tolower(c);
                }
                
                bool is_allocator = (
                    lower_name.find("malloc") != std::string::npos ||
                    lower_name.find("calloc") != std::string::npos ||
                    lower_name.find("realloc") != std::string::npos ||
                    lower_name.find("alloc") != std::string::npos ||
                    lower_name.find("create") != std::string::npos ||
                    lower_name.find("new") != std::string::npos ||
                    lower_name.find("open") != std::string::npos
                );
                
                if (is_allocator) {
                    // ── Step 2.5: Trace downstream usage to infer concrete struct type ──
                    // Walk the output varnode's descendants to find PTRSUB/INT_ADD/LOAD/STORE
                    // operations that reveal offset access patterns matching a known struct.
                    Datatype* concrete_struct_ptr = nullptr;
                    if (struct_registry) {
                        std::set<int> observed_offsets;
                        // Collect offsets from descendants of the output varnode
                        std::vector<Varnode*> work = {output};
                        std::set<Varnode*> visited;
                        int depth = 0;
                        while (!work.empty() && depth < 8) {
                            std::vector<Varnode*> next_work;
                            for (Varnode* v : work) {
                                if (!v || visited.count(v)) continue;
                                visited.insert(v);
                                // Walk all descendants
                                auto desc_iter = v->beginDescend();
                                auto desc_end = v->endDescend();
                                for (; desc_iter != desc_end; ++desc_iter) {
                                    PcodeOp* desc_op = *desc_iter;
                                    if (!desc_op) continue;
                                    OpCode desc_opc = desc_op->code();
                                    if (desc_opc == CPUI_PTRSUB || desc_opc == CPUI_INT_ADD || desc_opc == CPUI_PTRADD) {
                                        // The offset is typically the second input
                                        Varnode* off_vn = desc_op->getIn(1);
                                        if (off_vn && off_vn->isConstant()) {
                                            int off = (int)off_vn->getOffset();
                                            observed_offsets.insert(off);
                                        }
                                    }
                                    // Follow through COPY/CAST
                                    if (desc_opc == CPUI_COPY || desc_opc == CPUI_CAST ||
                                        desc_opc == CPUI_PTRSUB || desc_opc == CPUI_INT_ADD ||
                                        desc_opc == CPUI_PTRADD) {
                                        Varnode* out = desc_op->getOut();
                                        if (out) next_work.push_back(out);
                                    }
                                }
                            }
                            work = next_work;
                            depth++;
                        }

                        // Match observed offsets against struct_registry entries for current func
                        if (!observed_offsets.empty()) {
                            uint64_t func_addr = fd->getAddress().getOffset();
                            auto func_it = struct_registry->find(func_addr);
                            if (func_it != struct_registry->end()) {
                                // found — try to find a matching struct via TypeFactory
                                for (auto const& [param_idx, struct_name] : func_it->second) {
                                    Datatype* dt = tf->findByName(struct_name);
                                    if (dt && dt->getMetatype() == TYPE_STRUCT) {
                                        concrete_struct_ptr = tf->getTypePointer(
                                            ptr_size, dt, ptr_size);
                                        fission::utils::log_stream()
                                            << "[TypePropagator] Allocator " << func_name
                                            << " → concrete struct " << struct_name
                                            << "* (matched " << observed_offsets.size()
                                            << " offsets)" << std::endl;
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    if (concrete_struct_ptr) {
                        ptr_type = concrete_struct_ptr;
                    } else {
                        Datatype* void_type = tf->getTypeVoid();
                        ptr_type = tf->getTypePointer(ptr_size, void_type, ptr_size);
                    }
                    fission::utils::log_stream() << "[TypePropagator] Matched allocator-like function: "
                              << func_name << " → " << (ptr_type ? ptr_type->getName() : "void *") << std::endl;
                }
            }

            // ── Step 3: Apply the pointer type to the output varnode ──
            if (ptr_type) {
                output->setTempType(ptr_type);
                uint64_t vid = get_varnode_id(output);
                inferred_types[vid] = ptr_type;
                
                // Lock return type in FuncCallSpecs so it persists across
                // Ghidra's rerun_action loop.
                try {
                    ProtoParameter* outparam = fc->getOutput();
                    if (outparam && !outparam->isTypeLocked()) {
                        ParameterPieces piece;
                        piece.type = ptr_type;
                        piece.addr = outparam->getAddress();
                        piece.flags = 0;
                        fc->setOutput(piece);
                        fc->setOutputLock(true);
                        fission::utils::log_stream() << "[TypePropagator] Locked return type to "
                                  << ptr_type->getName() << " for: " << func_name << std::endl;
                    }
                } catch (const LowlevelError&) {
                    // FuncCallSpecs not modifiable (e.g. setOutput on locked proto) — skip.
                } catch (const std::exception& ex) {
                    fission::utils::log_stream() << "[TypePropagator] WARNING: failed to lock return type for "
                              << func_name << ": " << ex.what() << std::endl;
                } catch (...) {}
                
                types_set++;
            }
        }
    }
    
    if (types_set > 0) {
        fission::utils::log_stream() << "[TypePropagator] Set " << types_set 
                  << " return types from FuncCallSpecs" << std::endl;
    }
}

// ============================================================================
// seed_before_action: inject API-derived types into Ghidra's recommendation
// system BEFORE action->perform() so that ActionInferTypes can propagate them
// within its own iterative loop.
// ============================================================================
void TypePropagator::seed_before_action(Funcdata* fd) {
    if (!fd || !arch) return;

    ghidra::ScopeLocal* local = fd->getScopeLocal();
    if (!local) return;

    TypeFactory* tf = arch->types;
    if (!tf) return;

    int ptr_size = arch->getDefaultDataSpace()->getAddrSize();
    int seeded = 0;

    // Walk all live CPUI_CALL ops and apply addTypeRecommendation for each
    // argument whose API table gives a concrete type.
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op) continue;
        if (op->code() != CPUI_CALL) continue;

        Varnode* target_vn = op->getIn(0);
        if (!target_vn || !target_vn->isConstant()) continue;

        uint64_t target_addr = target_vn->getOffset();
        Funcdata* callee = arch->symboltab->getGlobalScope()->queryFunction(
            Address(arch->getDefaultCodeSpace(), target_addr));
        if (!callee) continue;

        // Platform-specific inference: collect (arg_index, Datatype*) hints
        std::string func_name = callee->getName();
        // We reuse infer_*_api_types to mark inferred_types, then mirror
        // those into TypeRecommendations.
        size_t before = inferred_types.size();

        if (compiler_id_.empty() || compiler_id_ == "windows" ||
            compiler_id_ == "msvc"  || compiler_id_ == "mingw") {
            infer_windows_api_types(op, func_name);
        } else {
            infer_posix_api_types(op, func_name);
        }

        // For each newly tracked varnode, add a TypeRecommendation so Ghidra
        // can use it during ActionInferTypes.
        if (inferred_types.size() == before) continue;

        // Scan call arguments (inputs 1..N)
        for (int i = 1; i < op->numInput(); ++i) {
            Varnode* arg = op->getIn(i);
            if (!arg) continue;
            uint64_t vid = get_varnode_id(arg);
            auto it = inferred_types.find(vid);
            if (it == inferred_types.end() || !it->second) continue;

            Datatype* dt = it->second;
            // Only recommend concrete non-unknown types with proper size
            if (dt->getMetatype() == TYPE_UNKNOWN) continue;
            if (arg->isConstant()) continue;

            // Stack and register varnodes have storage addresses
            Address storage(arg->getSpace(), arg->getOffset());
            try {
                local->addTypeRecommendation(storage, dt);
                seeded++;
            } catch (...) {
                // addTypeRecommendation may reject invalid addresses — ignore
            }
        }
    }

    if (seeded > 0) {
        fission::utils::log_stream() << "[TypePropagator] seed_before_action: injected "
                  << seeded << " type recommendations for function 0x"
                  << std::hex << fd->getAddress().getOffset() << std::dec << std::endl;
    }
}

// ---------------------------------------------------------------------------
// propagateAcrossReturns — mirrors Ghidra ActionInferTypes::canonicalReturnOp
// + propagateAcrossReturns (coreaction.cc L5186–L5245)
// ---------------------------------------------------------------------------

PcodeOp* TypePropagator::canonical_return_op(Funcdata* fd) {
    PcodeOp* best = nullptr;
    Datatype* bestdt = nullptr;
    auto iter = fd->beginOp(CPUI_RETURN);
    auto iterend = fd->endOp(CPUI_RETURN);
    for (; iter != iterend; ++iter) {
        PcodeOp* retop = *iter;
        if (retop->isDead()) continue;
        if (retop->getHaltType() != 0) continue;
        if (retop->numInput() <= 1) continue;  // no return value
        Varnode* vn = retop->getIn(1);
        Datatype* ct = vn->getTempType();
        if (!ct) continue;
        if (!bestdt) {
            best = retop;
            bestdt = ct;
        } else if (ct->typeOrder(*bestdt) < 0) {
            // ct is more specific than current best
            best = retop;
            bestdt = ct;
        }
    }
    return best;
}

void TypePropagator::propagate_across_returns(Funcdata* fd) {
    if (!fd) return;
    // Skip if the function prototype already has a locked return type —
    // in that case ActionInferTypes won't touch it either.
    if (fd->getFuncProto().isOutputLocked()) return;

    PcodeOp* op = canonical_return_op(fd);
    if (!op) return;

    Varnode* baseVn = op->getIn(1);
    Datatype* ct = baseVn->getTempType();
    if (!ct) return;

    int baseSize = baseVn->getSize();
    bool isBool = (ct->getMetatype() == TYPE_BOOL);

    auto iter = fd->beginOp(CPUI_RETURN);
    auto iterend = fd->endOp(CPUI_RETURN);
    for (; iter != iterend; ++iter) {
        PcodeOp* retop = *iter;
        if (retop == op) continue;
        if (retop->isDead()) continue;
        if (retop->getHaltType() != 0) continue;
        if (retop->numInput() <= 1) continue;
        Varnode* vn = retop->getIn(1);
        if (vn->getSize() != baseSize) continue;
        // Don't propagate bool to a varnode that can hold values other than 0/1.
        if (isBool && vn->getNZMask() > 1) continue;
        if (vn->getTempType() == ct) continue;  // already consistent
        vn->setTempType(ct);
        propagate_one_type(vn);
    }
}

} // namespace analysis
} // namespace fission
