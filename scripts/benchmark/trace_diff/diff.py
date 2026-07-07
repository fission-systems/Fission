import json
import argparse
import sys

# Common GP registers to compare on x86_64
GP_REGS = [
    "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "rbp", "rsp",
    "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"
]

def load_frida_trace(path):
    with open(path, 'r') as f:
        data = json.load(f)
    # data is a list of { pc: str, pc_hex: str, registers: { rax: str, ... } }
    trace = []
    for entry in data:
        pc = int(entry['pc'])
        regs = {}
        for k, v in entry['registers'].items():
            regs[k.lower()] = int(v)
        trace.append({
            'pc': pc,
            'registers': regs
        })
    return trace

def load_fission_trace(path):
    trace = []
    with open(path, 'r') as f:
        # It could be jsonl or a single json array.
        # But we wrote it using to_writer + \n so it's JSONL.
        for line in f:
            line = line.strip()
            if not line:
                continue
            entry = json.loads(line)
            if entry.get('kind') == 'Instruction':
                pc = entry['pc']
                regs = {}
                for k, v in entry.get('registers', {}).items():
                    regs[k.lower()] = int(v)
                trace.append({
                    'pc': pc,
                    'registers': regs,
                    'pcode_ops': entry.get('pcode_ops', []),
                    'bytes_hex': entry.get('bytes_hex', ''),
                    'mnemonic': entry.get('mnemonic', ''),
                })
    return trace

def compare_traces(frida_trace, fission_trace):
    print(f"[*] Loaded {len(frida_trace)} frida entries, {len(fission_trace)} fission entries.")
    
    min_len = min(len(frida_trace), len(fission_trace))
    
    for i in range(min_len):
        frida_entry = frida_trace[i]
        fission_entry = fission_trace[i]
        
        pc_frida = frida_entry['pc']
        pc_fission = fission_entry['pc']
        
        if pc_frida != pc_fission:
            print(f"\n[!] DIVERGENCE at step {i}")
            print(f"    Frida PC:   0x{pc_frida:X}")
            print(f"    Fission PC: 0x{pc_fission:X}  ({fission_entry.get('mnemonic')})")
            print("    -> Branch logic or previous instruction caused divergent control flow.")
            return False
            
        # Compare GP registers
        mismatches = []
        for reg in GP_REGS:
            f_val = frida_entry['registers'].get(reg, 0)
            fi_val = fission_entry['registers'].get(reg, 0)
            if f_val != fi_val:
                mismatches.append((reg, f_val, fi_val))
                
        if mismatches:
            print(f"\n[!] REGISTER MISMATCH at step {i} (PC: 0x{pc_fission:X})")
            print(f"    Instruction: {fission_entry.get('bytes_hex')} {fission_entry.get('mnemonic')}")
            print(f"    P-Code: {', '.join(fission_entry.get('pcode_ops', []))}")
            for reg, f_val, fi_val in mismatches:
                print(f"      {reg.upper()}: Frida=0x{f_val:X} != Fission=0x{fi_val:X}")
            print("    -> Semantic gap in Sleigh lifting or Emulator execution.")
            return False

    if len(frida_trace) != len(fission_trace):
        print(f"\n[?] Traces diverge in length. Frida={len(frida_trace)}, Fission={len(fission_trace)}")
        print("    This is normal if max_inst was used or program terminates differently.")
        
    print("\n[+] Traces match perfectly up to the compared length!")
    return True

def main():
    parser = argparse.ArgumentParser(description="Trace Diff Tool")
    parser.add_argument("frida_trace", help="Frida trace JSON file")
    parser.add_argument("fission_trace", help="Fission trace JSONL file")
    args = parser.parse_args()
    
    frida_trace = load_frida_trace(args.frida_trace)
    fission_trace = load_fission_trace(args.fission_trace)
    
    if not frida_trace:
        print("[!] Frida trace is empty.")
        sys.exit(1)
    if not fission_trace:
        print("[!] Fission trace is empty.")
        sys.exit(1)
        
    success = compare_traces(frida_trace, fission_trace)
    if not success:
        sys.exit(1)

if __name__ == "__main__":
    main()
