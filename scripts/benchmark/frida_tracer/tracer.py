import frida
import sys
import json
import argparse
import os

agent_code = """
let mainModule = Process.enumerateModules()[0];
let base = mainModule.base;
let size = mainModule.size;

let traceLog = [];

Stalker.follow(Process.getCurrentThreadId(), {
  events: {
    call: false,
    ret: false,
    exec: false,
    block: false,
    compile: false
  },
  transform: function (iterator) {
    let instruction = iterator.next();
    if (!instruction) return;
    
    let is_target = instruction.address.compare(base) >= 0 && instruction.address.compare(base.add(size)) < 0;
    
    do {
      if (is_target) {
        iterator.putCallout(onInstruction);
      }
      iterator.keep();
    } while ((instruction = iterator.next()) !== null);
  }
});

function onInstruction(context) {
    let pc = context.pc;
    let payload = {
        pc: pc.toString(10), // Send as base 10 string for easy u64 parsing or hex matching later
        pc_hex: pc.toString(16),
        registers: {
            rax: context.rax ? context.rax.toString(10) : "0",
            rbx: context.rbx ? context.rbx.toString(10) : "0",
            rcx: context.rcx ? context.rcx.toString(10) : "0",
            rdx: context.rdx ? context.rdx.toString(10) : "0",
            rsi: context.rsi ? context.rsi.toString(10) : "0",
            rdi: context.rdi ? context.rdi.toString(10) : "0",
            rbp: context.rbp ? context.rbp.toString(10) : "0",
            rsp: context.rsp ? context.rsp.toString(10) : "0",
            r8: context.r8 ? context.r8.toString(10) : "0",
            r9: context.r9 ? context.r9.toString(10) : "0",
            r10: context.r10 ? context.r10.toString(10) : "0",
            r11: context.r11 ? context.r11.toString(10) : "0",
            r12: context.r12 ? context.r12.toString(10) : "0",
            r13: context.r13 ? context.r13.toString(10) : "0",
            r14: context.r14 ? context.r14.toString(10) : "0",
            r15: context.r15 ? context.r15.toString(10) : "0",
        }
    };
    send(payload);
}
"""

def main():
    parser = argparse.ArgumentParser(description="Frida Tracer for Fission Differential Testing")
    parser.add_argument("--target", required=True, help="Path to the executable to trace")
    parser.add_argument("--out", required=True, help="Output JSON file path")
    args = parser.parse_args()

    target_path = os.path.abspath(args.target)
    
    print(f"[*] Spawning {target_path}...")
    pid = frida.spawn([target_path])
    session = frida.attach(pid)
    
    script = session.create_script(agent_code)
    
    trace_entries = []

    def on_message(message, data):
        if message['type'] == 'send':
            trace_entries.append(message['payload'])
        elif message['type'] == 'error':
            print(f"[!] Error: {message['stack']}")

    script.on('message', on_message)
    script.load()
    
    print("[*] Resuming execution...")
    frida.resume(pid)
    
    try:
        sys.stdin.read()
    except KeyboardInterrupt:
        print("\n[*] Interrupted. Saving trace...")
    
    # Dump the entries
    with open(args.out, "w") as f:
        json.dump(trace_entries, f, indent=2)
        
    print(f"[*] Trace saved to {args.out} with {len(trace_entries)} entries.")
    
    try:
        session.detach()
    except:
        pass

if __name__ == "__main__":
    main()
