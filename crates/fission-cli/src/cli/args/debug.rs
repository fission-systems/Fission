//! Debugger CLI argument definitions

use super::{parse_bool_str, parse_hex_address};
use clap::{Args, Subcommand};

// ============================================================================
// Debugger CLI surface
// ============================================================================

#[derive(Clone, Args, Debug, PartialEq, Eq)]
#[command(
    long_about = "Live process debugger (Windows only).\n\nAttach to a running process and inspect or control execution.\n",
    after_help = "Examples:\n  fission_cli debug attach 1234\n  fission_cli debug regs\n  fission_cli debug step\n  fission_cli debug bp 0x401000\n  fission_cli debug read 0x401000 --size 32\n  fission_cli debug modules\n  fission_cli debug continue\n  fission_cli debug detach"
)]
pub struct DebugArgs {
    /// Use emulator backend instead of native OS debugger
    #[arg(long, global = true)]
    pub emulator: bool,

    #[command(subcommand)]
    pub command: DebugCommand,
}

#[derive(Clone, Subcommand, Debug, PartialEq, Eq)]
pub enum DebugCommand {
    /// Launch a new process under the debugger
    Init(DebugInitArgs),
    /// Attach to a running process by PID
    Attach(DebugAttachArgs),
    /// Detach from the current process
    Detach,
    /// Continue execution
    Continue,
    /// Pause execution
    Pause,
    /// Stop / terminate the debugged process
    Stop,
    /// Single-step one instruction (step into)
    Step,
    /// Step over a single instruction
    StepOver,
    /// Step out of the current function
    StepOut,
    /// Skip the current instruction (increment RIP)
    Skip,
    /// Set a software breakpoint at an address
    Bp(DebugBpArgs),
    /// Remove a software breakpoint at an address
    RmBp(DebugBpArgs),
    /// Enable a breakpoint by address
    BpEnable(DebugBpArgs),
    /// Disable a breakpoint by address
    BpDisable(DebugBpArgs),
    /// List all breakpoints
    BpList(DebugBpListArgs),
    /// Set a hardware breakpoint at an address (Windows only)
    HwBp(DebugHwBpArgs),
    /// Set a memory breakpoint at an address (Windows only)
    MemBp(DebugMemBpArgs),
    /// Remove a memory breakpoint at an address (Windows only)
    RmMemBp(DebugBpArgs),
    /// Set a DLL load breakpoint (Windows only)
    DllBp(DebugDllBpArgs),
    /// Remove a DLL load breakpoint (Windows only)
    RmDllBp(DebugDllBpArgs),
    /// Set an exception breakpoint (Windows only)
    ExBp(DebugExBpArgs),
    /// Remove an exception breakpoint (Windows only)
    RmExBp(DebugExBpArgs),
    /// Show CPU registers for the active thread
    Regs,
    /// Set a single register value (e.g. rax, rip)
    SetReg(DebugSetRegArgs),
    /// Get a CPU flag (e.g. zf, cf)
    GetFlag(DebugFlagArgs),
    /// Set a CPU flag (e.g. zf true)
    SetFlag(DebugFlagArgs),
    /// Read memory at an address
    Read(DebugReadArgs),
    /// Write hex bytes to memory at an address
    Write(DebugWriteArgs),
    /// Allocate memory in the target process
    Alloc(DebugAllocArgs),
    /// Free memory in the target process
    Free(DebugFreeArgs),
    /// Get page protection rights at an address
    GetProtect(DebugProtectArgs),
    /// Set page protection rights for a region
    SetProtect(DebugProtectArgs),
    /// Peek a value from the stack
    StackPeek(DebugStackPeekArgs),
    /// Pop a value from the stack
    StackPop(DebugStackPopArgs),
    /// Push a value onto the stack
    StackPush(DebugStackPushArgs),
    /// Find a byte pattern in target memory
    Find(DebugFindArgs),
    /// List exports from a module
    Exports(DebugModuleArgs),
    /// List imports from a module
    Imports(DebugModuleArgs),
    /// List loaded modules
    Modules,
    /// List active threads
    Threads,
    /// Switch active thread
    SwitchThread(DebugSwitchThreadArgs),
    /// Poll for the next debug event
    Event,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugInitArgs {
    /// Executable path to launch
    pub path: String,
    /// Arguments to pass to the executable
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugAttachArgs {
    /// Target process ID
    pub pid: u32,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugSwitchThreadArgs {
    /// Thread ID to switch to
    pub tid: u32,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugBpArgs {
    /// Breakpoint address (hex)
    #[arg(value_parser = parse_hex_address)]
    pub addr: u64,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugHwBpArgs {
    /// Breakpoint address (hex)
    #[arg(value_parser = parse_hex_address)]
    pub addr: u64,
    /// Breakpoint kind: execute, write, read-write
    #[arg(short, long, value_enum, default_value_t = HwBpKindArg::Execute)]
    pub kind: HwBpKindArg,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum HwBpKindArg {
    Execute,
    Write,
    ReadWrite,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugBpListArgs {
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugMemBpArgs {
    /// Breakpoint address (hex)
    #[arg(value_parser = parse_hex_address)]
    pub addr: u64,
    /// Size of the memory region to guard
    #[arg(short, long, default_value_t = 1)]
    pub size: usize,
    /// Access kind: read, write, execute, access
    #[arg(short, long, value_enum, default_value_t = MemoryBpKindArg::Access)]
    pub kind: MemoryBpKindArg,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum MemoryBpKindArg {
    Read,
    Write,
    Execute,
    Access,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugDllBpArgs {
    /// DLL name to break on (e.g. kernel32.dll)
    pub name: String,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugExBpArgs {
    /// Exception code (hex, e.g. 0xC0000005)
    #[arg(value_parser = parse_hex_address)]
    pub code: u64,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugSetRegArgs {
    /// Register name (e.g. rax, rbx, rip, rsp)
    pub name: String,
    /// Value (hex)
    #[arg(value_parser = parse_hex_address)]
    pub value: u64,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugFlagArgs {
    /// Flag name (e.g. zf, cf, of, sf, df, if, pf, af, tf)
    pub name: String,
    /// Value for set-flag (true/false)
    #[arg(value_parser = parse_bool_str)]
    pub value: Option<bool>,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugAllocArgs {
    /// Number of bytes to allocate
    pub size: usize,
    /// Preferred address (hex, 0 for any)
    #[arg(short, long, default_value_t = 0, value_parser = parse_hex_address)]
    pub addr: u64,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugFreeArgs {
    /// Address to free (hex)
    #[arg(value_parser = parse_hex_address)]
    pub addr: u64,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugProtectArgs {
    /// Address (hex)
    #[arg(value_parser = parse_hex_address)]
    pub addr: u64,
    /// Size of the region
    pub size: usize,
    /// Protection flags as raw u32 (e.g. 64 for PAGE_EXECUTE_READWRITE)
    #[arg(required = false)]
    pub protect: Option<u32>,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugStackPeekArgs {
    /// Stack offset in slots (0 = top, 1 = next, -1 = previous)
    #[arg(default_value_t = 0)]
    pub offset: isize,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugStackPopArgs {
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugStackPushArgs {
    /// Value to push (hex)
    #[arg(value_parser = parse_hex_address)]
    pub value: u64,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugFindArgs {
    /// Start address (hex)
    #[arg(value_parser = parse_hex_address)]
    pub start: u64,
    /// Search size in bytes
    pub size: usize,
    /// Hex byte pattern to search (e.g. 4889 or 48,89)
    pub pattern: String,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugModuleArgs {
    /// Module base address (hex)
    #[arg(value_parser = parse_hex_address)]
    pub base: u64,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugReadArgs {
    /// Memory address (hex)
    #[arg(value_parser = parse_hex_address)]
    pub addr: u64,
    /// Number of bytes to read
    #[arg(short, long, default_value_t = 32)]
    pub size: usize,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Clone, Args, Debug, PartialEq, Eq)]
pub struct DebugWriteArgs {
    /// Memory address (hex)
    #[arg(value_parser = parse_hex_address)]
    pub addr: u64,
    /// Hex bytes to write (e.g. CC90 or CC,90)
    pub data: String,
    /// Output in JSON format
    #[arg(short, long)]
    pub json: bool,
}
