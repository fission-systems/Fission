//! Common CLI argument parsing utilities

mod decomp;
mod inventory;
mod legacy;
mod oneshot;

pub use decomp::DecompArgs;
pub use inventory::{InventoryArgs, InventoryCommand};
pub use legacy::{LegacyCliArgs, LegacyInvocationKind, ParsedOneShotArgs, normalize_legacy};
pub use oneshot::OneShotArgs;

use clap::{Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum FunctionDiscoveryProfileArg {
    Conservative,
    Balanced,
    Aggressive,
}

/// Parse hexadecimal address string (supports 0x prefix)
pub fn parse_hex_address(s: &str) -> Result<u64, String> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(s, 16).map_err(|e| format!("Invalid hex address: {}", e))
}

/// Parse boolean string for flag arguments
pub fn parse_bool_str(s: &str) -> Result<bool, String> {
    match s.to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(format!("Invalid boolean value: {}", s)),
    }
}

/// Parsed top-level CLI invocation (legacy one-shot pipeline vs Rhai script runner).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedInvocation {
    OneShot(ParsedOneShotArgs),
    Script(ScriptInvocation),
    ResourcesStatus {
        json: bool,
        verbose: bool,
    },
    Debug(DebugArgs),
    /// Sandbox (emulator) execution
    Sandbox(SandboxArgs),
    /// AI chat / authentication subcommand.
    Ai(AiInvocation),
}

/// The AI subcommand action to perform.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiInvocation {
    /// Launch interactive TUI chat session.
    Chat(crate::cli::args::AiChatArgs),
    /// Analyze decompiled pseudocode using an AI provider.
    Analyze(AiAnalyzeArgs),
    /// Run Codex Browser OAuth login.
    Login,
    /// Run GitHub Copilot Device Code OAuth login.
    CopilotLogin,
    /// Show authentication status.
    Status,
    /// Remove stored auth token.
    Logout,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptInvocation {
    pub verbose: bool,
    pub cmd: ScriptCmd,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptCmd {
    Check {
        script: PathBuf,
    },
    Run {
        binary: PathBuf,
        script: PathBuf,
        json: bool,
    },
}

#[derive(Args, Clone, Debug, Default)]
struct CommonBinaryOutputArgs {
    /// Output in JSON format
    #[arg(short, long)]
    json: bool,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Parser, Debug)]
#[command(name = "fission_cli")]
#[command(author = "Fission Dev Team")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Rust-native binary analysis and decompilation")]
#[command(
    long_about = "Fission is a headless-first binary analysis and decompilation tool with explicit one-shot subcommands.\n\nCanonical human-facing entrypoints:\n  fission_cli info binary.exe\n  fission_cli list binary.exe --json\n  fission_cli disasm binary.exe --addr 0x1400\n  fission_cli raw-pcode binary.exe --addr 0x1400\n  fission_cli pcode-stages binary.exe --addr 0x1400 --json\n  fission_cli nir-stats binary.exe --addr 0x1400 --json\n  fission_cli pcode-topology binary.exe --addr 0x1400 --json\n  fission_cli decomp binary.exe --addr 0x1400\n  fission_cli strings binary.exe --min-len 6\n  fission_cli xrefs binary.exe --json\n  fission_cli callgraph binary.exe --json\n\nOperator-oriented inventory lives under:\n  fission_cli inventory <SUBCOMMAND> ...\n"
)]
#[command(arg_required_else_help = true)]
struct CliArgs {
    /// Override resource bundle root (signatures, detectors). Also read from FISSION_RESOURCE_ROOT.
    #[arg(long = "resource-root", global = true, value_name = "DIR")]
    resource_root: Option<PathBuf>,

    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    /// Show binary metadata and inventory views
    Info(InfoArgs),
    /// List discovered functions
    List(ListArgs),
    /// Disassemble instructions or a full function
    Disasm(DisasmArgs),
    /// Emit Rust-Sleigh raw p-code for a function
    RawPcode(RawPcodeArgs),
    /// Emit Rust-Sleigh decode/NIR/render stage diagnostics for a function
    PcodeStages(PcodeStagesArgs),
    /// Emit canonical NIR build telemetry for a function
    NirStats(NirStatsArgs),
    /// Emit raw p-code CFG/topology diagnostics for a function
    PcodeTopology(PcodeTopologyArgs),
    /// Decompile one function or all discovered functions
    Decomp(DecompArgs),
    /// Extract binary strings
    Strings(StringsArgs),
    /// Canonical cross-reference index (loader seeds + optional disassembly layer)
    Xrefs(XrefsArgs),
    /// Call graph (caller/callee relationships from xref analysis)
    Callgraph(CallgraphArgs),
    /// Operator-oriented inventory and batch emitters
    Inventory(InventoryArgs),
    /// Inspect resolved resource paths and bundle-root candidates
    Resources(ResourcesArgs),
    /// Rhai scripts over read-only binary inventory (`binary.*`, `emit`)
    Script(ScriptArgs),
    /// Live process debugger (Windows only)
    Debug(DebugArgs),
    /// Sandbox (emulator) execution
    Sandbox(SandboxArgs),
    /// AI chat session and authentication (Codex OAuth / OpenAI / Ollama)
    Ai(AiArgs),
}

// ── Sandbox subcommand types ──────────────────────────────────────────────────

#[derive(Args, Debug, Clone, PartialEq, Eq)]
pub struct SandboxArgs {
    /// Path to binary to run in sandbox (optional when using --srd-diff only)
    #[arg(value_name = "BINARY")]
    pub binary: Option<PathBuf>,

    /// Take a snapshot when execution reaches this address
    #[arg(long, value_parser = parse_hex_address)]
    pub snapshot_at: Option<u64>,

    /// Restore snapshot from this directory before executing
    #[arg(long)]
    pub restore_snapshot: Option<PathBuf>,

    /// Maximum number of instructions to execute (for infinite loop prevention)
    #[arg(long)]
    pub max_inst: Option<u64>,

    /// Mock standard input string to feed into console reads
    #[arg(long)]
    pub stdin_mock: Option<String>,

    /// Dump execution trace to the specified JSON file
    #[arg(long)]
    pub dump_trace: Option<PathBuf>,

    /// Record TTD snapshot every N instructions (0 = disable)
    #[arg(long)]
    pub ttd_record: Option<u64>,

    /// Seek to a specific instruction step (via TTD) after execution halts
    #[arg(long)]
    pub ttd_seek: Option<u64>,

    /// Enable concolic/symbolic path exploration on TTD (fork on CBranch)
    #[arg(long)]
    pub sym_explore: bool,

    /// Print sandbox metrics report as JSON on stdout
    #[arg(long)]
    pub json: bool,

    /// Write sandbox metrics report JSON to this path
    #[arg(long, value_name = "PATH")]
    pub metrics_out: Option<PathBuf>,

    /// Write Semantic Replay Diff snapshot JSON (owner-native run facts) to this path
    #[arg(long, value_name = "PATH")]
    pub srd_out: Option<PathBuf>,

    /// Label stored inside the SRD snapshot (default: binary file name)
    #[arg(long, value_name = "LABEL")]
    pub srd_label: Option<String>,

    /// Include static musl mallocng BSS probes in the SRD snapshot (fixture layout)
    #[arg(long)]
    pub srd_mallocng: bool,

    /// Diff two existing SRD snapshot JSON files and print the structured delta
    #[arg(long, value_name = "PATH", num_args = 2, value_names = ["LEFT", "RIGHT"])]
    pub srd_diff: Option<Vec<PathBuf>>,

    /// Write SRD delta JSON to this path (use with --srd-diff)
    #[arg(long, value_name = "PATH")]
    pub srd_diff_out: Option<PathBuf>,

    /// Max unimplemented opcode events allowed (budget gate; requires --max-unimpl-kinds or defaults kinds=16)
    #[arg(long)]
    pub max_unimpl_events: Option<u64>,

    /// Max distinct unimplemented opcode kinds allowed
    #[arg(long)]
    pub max_unimpl_kinds: Option<usize>,

    /// Max missing HLE procedures allowed (fake-success returns 0)
    #[arg(long)]
    pub max_hle_misses: Option<u64>,

    /// Max unknown Linux syscalls allowed (fake RAX=0 path)
    #[arg(long)]
    pub max_unknown_syscalls: Option<u64>,

    /// Exit non-zero when quality budget is exceeded (implies opcode defaults 0/0 if unset;
    /// HLE/syscall defaults to unlimited unless --max-hle-misses / --max-unknown-syscalls set)
    #[arg(long)]
    pub fail_on_budget: bool,
}

// ── AI subcommand types ───────────────────────────────────────────────────────

#[derive(Args, Debug)]
#[command(
    about = "Interactive AI chat and authentication",
    long_about = "Launch an interactive AI chat session or manage authentication.\n\nProvider priority: stored OAuth token (Copilot or Codex) > FISSION_AI_API_KEY > OPENAI_API_KEY > Ollama (local)",
    after_help = "Examples:\n  fission_cli ai copilot-login      # GitHub Copilot OAuth ($10/mo, recommended)\n  fission_cli ai login              # Codex/ChatGPT OAuth (Plus required)\n  fission_cli ai chat               # Launch TUI chat\n  fission_cli ai chat --provider copilot --model gpt-4o\n  fission_cli ai chat --provider openai --model gpt-4o\n  fission_cli ai status             # Show auth status\n  fission_cli ai logout             # Remove stored token"
)]
pub struct AiArgs {
    #[command(subcommand)]
    command: Option<AiCommand>,
}

#[derive(Subcommand, Debug)]
enum AiCommand {
    /// Launch interactive TUI chat session
    Chat(AiChatArgs),
    /// Analyze decompiled pseudocode using an AI provider
    Analyze(AiAnalyzeArgs),
    /// Login with Codex/ChatGPT Browser OAuth (ChatGPT Plus required)
    Login,
    /// Login with GitHub Copilot Device Code OAuth (Copilot Individual $10/mo)
    CopilotLogin,
    /// Show current authentication status
    Status,
    /// Remove stored authentication token
    Logout,
}

#[derive(Args, Clone, Debug, PartialEq, Eq)]
pub struct AiChatArgs {
    /// Optional binary to load into the AI context
    pub binary: Option<PathBuf>,

    /// AI provider to use: codex | openai | ollama
    #[arg(long)]
    pub provider: Option<String>,
    /// Model name override (e.g. gpt-4o, llama3)
    #[arg(long)]
    pub model: Option<String>,
}

#[derive(Args, Clone, Debug, PartialEq, Eq)]
pub struct AiAnalyzeArgs {
    /// Custom code string to analyze (optional, positional)
    pub code: Option<String>,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Show binary metadata plus optional section/import/export inventories.\n\nUse this command for quick facts about the loaded binary without entering the decompilation path.",
    after_help = "Examples:\n  fission_cli info app.exe\n  fission_cli info app.exe --sections\n  fission_cli info app.exe --imports --json\n  fission_cli info app.exe --detections --json\n  fission_cli info app.exe --identity --json\n  fission_cli info app.exe --xrefs --json"
)]
struct InfoArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Run integrated detection (section/import/string rules plus Detect It Easy signatures)
    #[arg(long)]
    detections: bool,

    /// Attach structured loader identity report (entropy, overlay, PE hints with evidence)
    #[arg(long)]
    identity: bool,

    /// Show section information
    #[arg(short = 'S', long)]
    sections: bool,

    /// List imported functions
    #[arg(short = 'I', long)]
    imports: bool,

    /// List exported functions
    #[arg(short = 'E', long)]
    exports: bool,

    /// Attach canonical xref index summary (JSON adds `xrefs`; text mode prints a short section)
    #[arg(long)]
    xrefs: bool,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "List discovered functions for a binary.\n\nThis is the canonical function inventory surface for humans and automation.",
    after_help = "Examples:\n  fission_cli list app.exe\n  fission_cli list app.exe --json"
)]
struct ListArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Function discovery profile (conservative|balanced|aggressive)
    #[arg(long = "function-discovery-profile", value_enum)]
    function_discovery_profile: Option<FunctionDiscoveryProfileArg>,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Disassemble instructions at a specific address or decode the full containing function.\n\nUse `--function` when you want function boundaries rather than a fixed instruction window.",
    after_help = "Examples:\n  fission_cli disasm app.exe --addr 0x140001000\n  fission_cli disasm app.exe --addr 0x140001000 --count 64\n  fission_cli disasm app.exe --addr 0x140001000 --function --json"
)]
struct DisasmArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Address to disassemble
    #[arg(long, value_parser = parse_hex_address, required = true)]
    addr: u64,

    /// Disassemble the full function instead of a fixed instruction count
    #[arg(long)]
    function: bool,

    /// Number of instructions to disassemble
    #[arg(short = 'n', long, default_value_t = 20)]
    count: usize,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Emit Rust-Sleigh raw p-code for the function starting at or containing an address.\n\nThis is a diagnostic surface for SLEIGH/runtime parity and NIR admission debugging.",
    after_help = "Examples:\n  fission_cli raw-pcode app.exe --addr 0x140001000\n  fission_cli raw-pcode app.exe --addr 0x140001000 --json\n  fission_cli raw-pcode app.exe --addr 0x140001000 --max-bytes 1024 --instruction-limit 128"
)]
struct RawPcodeArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Function entry address or low-bit encoded code pointer
    #[arg(long, value_parser = parse_hex_address, required = true)]
    addr: u64,

    /// Maximum bytes to read from the containing executable section
    #[arg(long, default_value_t = 4096)]
    max_bytes: usize,

    /// Maximum instructions to decode
    #[arg(long, default_value_t = 512)]
    instruction_limit: usize,

    /// Continue past indirect branches while lifting
    #[arg(long)]
    continue_past_indirect: bool,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Emit Rust-Sleigh decode, raw p-code, NIR, normalization, structuring, and render stage diagnostics for one function.\n\nThis reuses the canonical debug_decomp bundle shape so automation can compare stage status and telemetry without parsing rendered C-like output.",
    after_help = "Examples:\n  fission_cli pcode-stages app.exe --addr 0x140001000\n  fission_cli pcode-stages app.exe --addr 0x140001000 --json\n  fission_cli pcode-stages app.exe --addr 0x140001000 --max-bytes 1024 --instruction-limit 128"
)]
struct PcodeStagesArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Function entry address or an address inside the function
    #[arg(long, value_parser = parse_hex_address, required = true)]
    addr: u64,

    /// Maximum bytes to decode from the function body
    #[arg(long, default_value_t = 0x4000)]
    max_bytes: usize,

    /// Maximum instructions to decode
    #[arg(long, default_value_t = 512)]
    instruction_limit: usize,

    /// Stop at indirect branches instead of continuing through the function body
    #[arg(long)]
    strict_indirect_stop: bool,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Emit the canonical flat NirBuildStats telemetry for one Rust-Sleigh decompilation.\n\nJSON mode preserves the public NirBuildStats field names used by automation and benchmark artifacts.",
    after_help = "Examples:\n  fission_cli nir-stats app.exe --addr 0x140001000\n  fission_cli nir-stats app.exe --addr 0x140001000 --json\n  fission_cli nir-stats app.exe --addr 0x140001000 --max-bytes 1024 --instruction-limit 128"
)]
struct NirStatsArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Function entry address or an address inside the function
    #[arg(long, value_parser = parse_hex_address, required = true)]
    addr: u64,

    /// Maximum bytes to decode from the function body
    #[arg(long, default_value_t = 0x4000)]
    max_bytes: usize,

    /// Maximum instructions to decode
    #[arg(long, default_value_t = 512)]
    instruction_limit: usize,

    /// Stop at indirect branches instead of continuing through the function body
    #[arg(long)]
    strict_indirect_stop: bool,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Emit raw p-code block topology from the Rust-Sleigh pipeline without dumping every p-code op.\n\nUse this to inspect block count, edges, terminal opcodes, and per-block successors before NIR materialization or structuring changes.",
    after_help = "Examples:\n  fission_cli pcode-topology app.exe --addr 0x140001000\n  fission_cli pcode-topology app.exe --addr 0x140001000 --json\n  fission_cli pcode-topology app.exe --addr 0x140001000 --max-bytes 1024 --instruction-limit 128"
)]
struct PcodeTopologyArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Function entry address or an address inside the function
    #[arg(long, value_parser = parse_hex_address, required = true)]
    addr: u64,

    /// Maximum bytes to decode from the function body
    #[arg(long, default_value_t = 0x4000)]
    max_bytes: usize,

    /// Maximum instructions to decode
    #[arg(long, default_value_t = 512)]
    instruction_limit: usize,

    /// Stop at indirect branches instead of continuing through the function body
    #[arg(long)]
    strict_indirect_stop: bool,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Extract printable strings from the binary image.",
    after_help = "Examples:\n  fission_cli strings app.exe\n  fission_cli strings app.exe --min-len 8 --json"
)]
struct StringsArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Minimum string length
    #[arg(long = "min-len", default_value_t = 4)]
    min_len: usize,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Emit canonical cross-reference records (`fission-static::xref_index`): loader-derived seeds plus optional Sleigh disassembly layer.\n\nDisassembly requires a usable load spec on `LoadedBinary`. Use `--no-disassembly` for import/export/string/global anchors only.",
    after_help = "Examples:\n  fission_cli xrefs app.exe --json\n  fission_cli xrefs app.exe --no-disassembly --json\n  fission_cli xrefs app.exe --function 0x140001000 --json"
)]
struct XrefsArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    /// Skip Sleigh xref extraction (loader seeds only)
    #[arg(long)]
    no_disassembly: bool,

    /// Include per-function xref slice for this function entry VA in JSON output
    #[arg(long, value_parser = parse_hex_address)]
    function: Option<u64>,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Build a call graph from cross-reference analysis.\n\nEdges are aggregated from call-type xrefs discovered in the binary. Output includes callers and callees per function.",
    after_help = "Examples:\n  fission_cli callgraph app.exe\n  fission_cli callgraph app.exe --json"
)]
struct CallgraphArgs {
    /// Path to the binary file to analyze
    binary: PathBuf,

    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Inspect bundled runtime resources (signatures, type corpora, detectors).\n\nUse `resources status` to see candidate bundle roots and paths resolved via `PATHS`.",
    after_help = "Examples:\n  fission_cli resources status\n  fission_cli resources status --json\n  fission_cli --resource-root /opt/fission-data resources status --json"
)]
struct ResourcesArgs {
    #[command(subcommand)]
    command: ResourcesCommand,
}

#[derive(Subcommand, Debug)]
enum ResourcesCommand {
    /// Show candidate bundle roots and resolved resource paths
    Status(ResourcesStatusArgs),
}

#[derive(Args, Debug)]
struct ResourcesStatusArgs {
    #[command(flatten)]
    common: CommonBinaryOutputArgs,
}

#[derive(Args, Debug)]
#[command(
    long_about = "Compile-check or run embedded Rhai scripts with read-only access to loaded binary inventory.\n\nScripts receive `binary` (path, format, functions, imports, …) and may call `emit` to record structured findings.",
    after_help = "Examples:\n  fission_cli script check --script scan.rhai\n  fission_cli script run app.exe --script scan.rhai --json"
)]
struct ScriptArgs {
    #[command(subcommand)]
    command: ScriptCommand,
}

#[derive(Subcommand, Debug)]
enum ScriptCommand {
    /// Syntax-check a Rhai script (no binary load)
    Check(ScriptCheckArgs),
    /// Load a binary, run Balanced discovery, then evaluate the script
    Run(ScriptRunArgs),
}

#[derive(Args, Debug)]
struct ScriptCheckArgs {
    /// Path to the `.rhai` script
    #[arg(long, value_name = "FILE")]
    script: PathBuf,

    #[arg(short, long)]
    verbose: bool,
}

#[derive(Args, Debug, Clone, PartialEq, Eq)]
struct ScriptRunArgs {
    /// Binary to analyze
    binary: PathBuf,

    /// Path to the `.rhai` script
    #[arg(long, value_name = "FILE")]
    script: PathBuf,

    #[arg(short, long)]
    json: bool,

    #[arg(short, long)]
    verbose: bool,
}

const CANONICAL_SUBCOMMANDS: &[&str] = &[
    "info",
    "list",
    "disasm",
    "raw-pcode",
    "pcode-stages",
    "nir-stats",
    "pcode-topology",
    "decomp",
    "strings",
    "xrefs",
    "callgraph",
    "inventory",
    "resources",
    "script",
    "sandbox",
    "debug",
    "ai",
];

fn should_use_canonical_parser(argv: &[OsString]) -> bool {
    if argv.len() <= 1 {
        return true;
    }

    // Global flags may precede the subcommand (`fission_cli --resource-root DIR resources status`).
    if argv.iter().any(|arg| {
        arg.to_str()
            .is_some_and(|s| CANONICAL_SUBCOMMANDS.contains(&s))
    }) {
        return true;
    }

    match argv[1].to_str() {
        Some("help" | "--help" | "-h" | "--version" | "-V") => true,
        _ => false,
    }
}

pub fn parse_oneshot_args() -> ParsedInvocation {
    parse_oneshot_args_from(std::env::args_os())
}

pub fn parse_oneshot_args_from<I, T>(iter: I) -> ParsedInvocation
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let argv: Vec<OsString> = iter.into_iter().map(Into::into).collect();
    if should_use_canonical_parser(&argv) {
        let cli = CliArgs::parse_from(argv);
        fission_core::resource_roots::set_cli_resource_bundle_root(cli.resource_root.clone());
        normalize_canonical(cli)
    } else {
        ParsedInvocation::OneShot(normalize_legacy(LegacyCliArgs::parse_from(argv)))
    }
}

fn normalize_canonical(cli: CliArgs) -> ParsedInvocation {
    match cli.command {
        CliCommand::Script(script) => {
            let invocation = match script.command {
                ScriptCommand::Check(c) => ScriptInvocation {
                    verbose: c.verbose,
                    cmd: ScriptCmd::Check { script: c.script },
                },
                ScriptCommand::Run(r) => ScriptInvocation {
                    verbose: r.verbose,
                    cmd: ScriptCmd::Run {
                        binary: r.binary,
                        script: r.script,
                        json: r.json,
                    },
                },
            };
            ParsedInvocation::Script(invocation)
        }
        CliCommand::Resources(resources) => match resources.command {
            ResourcesCommand::Status(s) => ParsedInvocation::ResourcesStatus {
                json: s.common.json,
                verbose: s.common.verbose,
            },
        },
        cmd => {
            let args = match cmd {
                CliCommand::Info(info) => {
                    let mut args = OneShotArgs::with_binary(info.binary);
                    args.json = info.common.json;
                    args.verbose = info.common.verbose;
                    args.sections = info.sections;
                    args.imports = info.imports;
                    args.exports = info.exports;
                    args.info_detections = info.detections;
                    args.info_identity = info.identity;
                    args.info_xrefs = info.xrefs;
                    args.info = !args.sections && !args.imports && !args.exports;
                    args
                }
                CliCommand::List(list) => {
                    let mut args = OneShotArgs::with_binary(list.binary);
                    args.list = true;
                    args.json = list.common.json;
                    args.verbose = list.common.verbose;
                    args.function_discovery_profile = list.function_discovery_profile;
                    args
                }
                CliCommand::Disasm(disasm) => {
                    let mut args = OneShotArgs::with_binary(disasm.binary);
                    args.count = disasm.count;
                    args.json = disasm.common.json;
                    args.verbose = disasm.common.verbose;
                    if disasm.function {
                        args.disasm_function = Some(disasm.addr);
                    } else {
                        args.disasm = Some(disasm.addr);
                    }
                    args
                }
                CliCommand::RawPcode(raw_pcode) => {
                    let mut args = OneShotArgs::with_binary(raw_pcode.binary);
                    args.raw_pcode = Some(raw_pcode.addr);
                    args.raw_pcode_max_bytes = raw_pcode.max_bytes;
                    args.raw_pcode_instruction_limit = raw_pcode.instruction_limit;
                    args.raw_pcode_continue_past_indirect = raw_pcode.continue_past_indirect;
                    args.json = raw_pcode.common.json;
                    args.verbose = raw_pcode.common.verbose;
                    args
                }
                CliCommand::PcodeStages(stages) => {
                    let mut args = OneShotArgs::with_binary(stages.binary);
                    args.pcode_stages = Some(stages.addr);
                    args.pcode_stages_max_bytes = stages.max_bytes;
                    args.pcode_stages_instruction_limit = stages.instruction_limit;
                    args.pcode_stages_strict_indirect_stop = stages.strict_indirect_stop;
                    args.json = stages.common.json;
                    args.verbose = stages.common.verbose;
                    args
                }
                CliCommand::NirStats(stats) => {
                    let mut args = OneShotArgs::with_binary(stats.binary);
                    args.nir_stats = Some(stats.addr);
                    args.nir_stats_max_bytes = stats.max_bytes;
                    args.nir_stats_instruction_limit = stats.instruction_limit;
                    args.nir_stats_strict_indirect_stop = stats.strict_indirect_stop;
                    args.json = stats.common.json;
                    args.verbose = stats.common.verbose;
                    args
                }
                CliCommand::PcodeTopology(topology) => {
                    let mut args = OneShotArgs::with_binary(topology.binary);
                    args.pcode_topology = Some(topology.addr);
                    args.pcode_topology_max_bytes = topology.max_bytes;
                    args.pcode_topology_instruction_limit = topology.instruction_limit;
                    args.pcode_topology_strict_indirect_stop = topology.strict_indirect_stop;
                    args.json = topology.common.json;
                    args.verbose = topology.common.verbose;
                    args
                }
                CliCommand::Decomp(decomp) => {
                    let mut args = OneShotArgs::with_binary(decomp.binary);
                    args.address = decomp.addr;
                    args.decomp_all = decomp.all || decomp.addresses_file.is_some();
                    args.addresses_file = decomp.addresses_file;
                    args.decomp_limit = decomp.limit;
                    args.include_nonuser_functions = decomp.include_nonuser_functions;
                    args.profile = decomp.profile;
                    args.layer = decomp.layer;
                    args.engine = decomp.engine;
                    args.compiler_id = decomp.compiler_id;
                    args.timeout_ms = decomp.timeout_ms;
                    args.function_discovery_profile = decomp.function_discovery_profile;
                    args.json = decomp.json;
                    args.output = decomp.output;
                    args.verbose = decomp.verbose;
                    args.no_header = decomp.no_header;
                    args.no_warnings = decomp.no_warnings;
                    args.ghidra_compat = decomp.ghidra_compat;
                    args.benchmark = decomp.benchmark;
                    args.debug_decomp = decomp.debug_decomp;
                    args.debug_decomp_bundle = decomp.debug_decomp_bundle;
                    args.format = decomp.format;
                    args
                }
                CliCommand::Strings(strings) => {
                    let mut args = OneShotArgs::with_binary(strings.binary);
                    args.strings = Some(strings.min_len);
                    args.json = strings.common.json;
                    args.verbose = strings.common.verbose;
                    args
                }
                CliCommand::Xrefs(xrefs) => {
                    let mut args = OneShotArgs::with_binary(xrefs.binary);
                    args.xrefs_cmd = true;
                    args.xref_no_disassembly = xrefs.no_disassembly;
                    args.xref_function = xrefs.function;
                    args.json = xrefs.common.json;
                    args.verbose = xrefs.common.verbose;
                    args
                }
                CliCommand::Callgraph(callgraph) => {
                    let mut args = OneShotArgs::with_binary(callgraph.binary);
                    args.callgraph_cmd = true;
                    args.json = callgraph.common.json;
                    args.verbose = callgraph.common.verbose;
                    args
                }
                CliCommand::Inventory(inventory) => match inventory.command {
                    InventoryCommand::FunctionFacts(facts) => {
                        let mut args = OneShotArgs::with_binary(facts.binary);
                        args.emit_function_facts_inventory = true;
                        args.address = facts.addr;
                        args.addresses_file = facts.addresses_file;
                        args.functions_limit = facts.functions_limit;
                        args.include_nonuser_functions = facts.include_nonuser_functions;
                        args.chunk_size = facts.chunk_size;
                        args.output_jsonl = facts.output_jsonl;
                        args.summary_json = facts.summary_json;
                        args.resume_from = facts.resume_from;
                        args.quiet_batch_errors = facts.quiet_batch_errors;
                        args.compiler_id = facts.compiler_id;
                        args.profile = facts.profile;
                        args.timeout_ms = facts.timeout_ms;
                        args.function_discovery_profile = facts.function_discovery_profile;
                        args.verbose = facts.verbose;
                        args
                    }
                    InventoryCommand::PreviewCandidates(preview) => {
                        let mut args = OneShotArgs::with_binary(preview.binary);
                        args.preview_candidate_inventory = preview.inventory || !preview.batch;
                        args.preview_candidate_scan_batch = preview.batch;
                        args.address = preview.addr;
                        args.preview_candidate_limit = preview.preview_candidate_limit;
                        args.include_nonuser_functions = preview.include_nonuser_functions;
                        args.addresses_file = preview.addresses_file;
                        args.functions_limit = preview.functions_limit;
                        args.chunk_size = preview.chunk_size;
                        args.output_jsonl = preview.output_jsonl;
                        args.summary_json = preview.summary_json;
                        args.resume_from = preview.resume_from;
                        args.quiet_batch_errors = preview.quiet_batch_errors;
                        args.compiler_id = preview.compiler_id;
                        args.profile = preview.profile;
                        args.timeout_ms = preview.timeout_ms;
                        args.function_discovery_profile = preview.function_discovery_profile;
                        args.verbose = preview.verbose;
                        args
                    }
                },
                CliCommand::Debug(debug) => {
                    return ParsedInvocation::Debug(debug);
                }
                CliCommand::Ai(ai_args) => {
                    let inv = match ai_args.command {
                        None => AiInvocation::Chat(crate::cli::args::AiChatArgs {
                            binary: None,
                            provider: None,
                            model: None,
                        }),
                        Some(AiCommand::Chat(args)) => AiInvocation::Chat(args),
                        Some(AiCommand::Analyze(args)) => AiInvocation::Analyze(args),
                        Some(AiCommand::Login) => AiInvocation::Login,
                        Some(AiCommand::CopilotLogin) => AiInvocation::CopilotLogin,
                        Some(AiCommand::Status) => AiInvocation::Status,
                        Some(AiCommand::Logout) => AiInvocation::Logout,
                    };
                    return ParsedInvocation::Ai(inv);
                }
                CliCommand::Sandbox(sandbox) => return ParsedInvocation::Sandbox(sandbox),
                CliCommand::Script(_) => unreachable!("script branch handled above"),
                CliCommand::Resources(_) => unreachable!("resources branch handled above"),
            };
            ParsedInvocation::OneShot(ParsedOneShotArgs {
                args,
                legacy_warning: None,
            })
        }
    }
}

// Re-export debug CLI types so existing consumers don't change paths
mod debug;
#[allow(unused_imports)]
pub use debug::{
    DebugAllocArgs, DebugArgs, DebugAttachArgs, DebugBpArgs, DebugBpListArgs, DebugCommand,
    DebugDllBpArgs, DebugExBpArgs, DebugFindArgs, DebugFlagArgs, DebugFreeArgs, DebugHwBpArgs,
    DebugInitArgs, DebugMemBpArgs, DebugModuleArgs, DebugProtectArgs, DebugReadArgs,
    DebugSetRegArgs, DebugStackPeekArgs, DebugStackPopArgs, DebugStackPushArgs,
    DebugSwitchThreadArgs, DebugWriteArgs, HwBpKindArg, MemoryBpKindArg,
};

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    fn parse_canonical(args: &[&str]) -> ParsedOneShotArgs {
        match parse_oneshot_args_from(args.iter().copied()) {
            ParsedInvocation::OneShot(p) => p,
            ParsedInvocation::Script(_) => panic!("expected one-shot canonical parse"),
            ParsedInvocation::ResourcesStatus { .. } => {
                panic!("expected one-shot canonical parse")
            }
            ParsedInvocation::Debug(_) => panic!("expected one-shot canonical parse"),
            ParsedInvocation::Ai(_) => panic!("expected one-shot canonical parse"),
            ParsedInvocation::Sandbox(_) => panic!("expected one-shot canonical parse"),
        }
    }

    fn parse_legacy(args: &[&str]) -> ParsedOneShotArgs {
        match parse_oneshot_args_from(args.iter().copied()) {
            ParsedInvocation::OneShot(p) => p,
            ParsedInvocation::Script(_) => panic!("legacy parser cannot emit script"),
            ParsedInvocation::ResourcesStatus { .. } => {
                panic!("legacy parser cannot emit resources status")
            }
            ParsedInvocation::Debug(_) => panic!("legacy parser cannot emit debug"),
            ParsedInvocation::Ai(_) => panic!("legacy parser cannot emit AI"),
            ParsedInvocation::Sandbox(_) => panic!("legacy parser cannot emit sandbox"),
        }
    }

    #[test]
    fn canonical_script_check_parsing() {
        let inv =
            parse_oneshot_args_from(["fission_cli", "script", "check", "--script", "rules.rhai"]);
        match inv {
            ParsedInvocation::Script(s) => {
                assert!(!s.verbose);
                match s.cmd {
                    ScriptCmd::Check { script } => {
                        assert_eq!(script, PathBuf::from("rules.rhai"));
                    }
                    _ => panic!("expected check"),
                }
            }
            _ => panic!("expected script invocation"),
        }
    }

    #[test]
    fn canonical_script_run_parsing() {
        let inv = parse_oneshot_args_from([
            "fission_cli",
            "script",
            "run",
            "app.exe",
            "--script",
            "scan.rhai",
            "--json",
            "--verbose",
        ]);
        match inv {
            ParsedInvocation::Script(s) => {
                assert!(s.verbose);
                match s.cmd {
                    ScriptCmd::Run {
                        binary,
                        script,
                        json,
                    } => {
                        assert_eq!(binary, PathBuf::from("app.exe"));
                        assert_eq!(script, PathBuf::from("scan.rhai"));
                        assert!(json);
                    }
                    _ => panic!("expected run"),
                }
            }
            _ => panic!("expected script invocation"),
        }
    }

    #[test]
    fn canonical_resources_status_parsing() {
        let inv = parse_oneshot_args_from(["fission_cli", "resources", "status", "--json"]);
        match inv {
            ParsedInvocation::ResourcesStatus { json, verbose } => {
                assert!(json);
                assert!(!verbose);
            }
            _ => panic!("expected ResourcesStatus"),
        }
    }

    #[test]
    fn global_resource_root_sets_override() {
        let tmp = "/tmp/fission-res-root-test";
        let inv =
            parse_oneshot_args_from(["fission_cli", "--resource-root", tmp, "resources", "status"]);
        match &inv {
            ParsedInvocation::ResourcesStatus { json, verbose } => {
                assert!(!*json);
                assert!(!*verbose);
            }
            _ => panic!("expected ResourcesStatus, got {inv:?}"),
        }
        assert_eq!(
            fission_core::resource_roots::cli_resource_bundle_root(),
            Some(PathBuf::from(tmp))
        );

        let _cleanup = parse_oneshot_args_from(["fission_cli", "info", "app.exe"]);
        assert_eq!(
            fission_core::resource_roots::cli_resource_bundle_root(),
            None
        );
    }

    #[test]
    fn canonical_info_parsing_maps_to_info_command() {
        let parsed = parse_canonical(&["fission_cli", "info", "bin.exe"]);
        assert_eq!(parsed.legacy_warning, None);
        assert!(parsed.args.info);
        assert_eq!(parsed.args.binary, PathBuf::from("bin.exe"));
        assert!(!parsed.args.info_detections);
        assert!(!parsed.args.info_identity);
    }

    #[test]
    fn canonical_info_identity_flag_sets_info_identity() {
        let parsed = parse_canonical(&["fission_cli", "info", "bin.exe", "--identity", "--json"]);
        assert!(parsed.args.info);
        assert!(parsed.args.json);
        assert!(parsed.args.info_identity);
    }

    #[test]
    fn canonical_info_detections_flag_sets_info_detections() {
        let parsed = parse_canonical(&["fission_cli", "info", "bin.exe", "--detections", "--json"]);
        assert!(parsed.args.info);
        assert!(parsed.args.json);
        assert!(parsed.args.info_detections);
    }

    #[test]
    fn canonical_info_xrefs_sets_flag() {
        let parsed = parse_canonical(&["fission_cli", "info", "bin.exe", "--xrefs", "--json"]);
        assert!(parsed.args.info);
        assert!(parsed.args.info_xrefs);
        assert!(parsed.args.json);
    }

    #[test]
    fn canonical_xrefs_subcommand_maps_fields() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "xrefs",
            "bin.exe",
            "--no-disassembly",
            "--function",
            "0x140001000",
            "--json",
        ]);
        assert!(parsed.args.xrefs_cmd);
        assert!(parsed.args.xref_no_disassembly);
        assert_eq!(parsed.args.xref_function, Some(0x140001000));
        assert!(parsed.args.json);
    }

    #[test]
    fn canonical_list_parsing_maps_to_list_command() {
        let parsed = parse_canonical(&["fission_cli", "list", "bin.exe", "--json"]);
        assert!(parsed.args.list);
        assert!(parsed.args.json);
    }

    #[test]
    fn canonical_disasm_parsing_maps_to_instruction_mode() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "disasm",
            "bin.exe",
            "--addr",
            "0x1400",
            "--count",
            "32",
        ]);
        assert_eq!(parsed.args.disasm, Some(0x1400));
        assert_eq!(parsed.args.disasm_function, None);
        assert_eq!(parsed.args.count, 32);
    }

    #[test]
    fn canonical_raw_pcode_parsing_maps_to_raw_pcode_command() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "raw-pcode",
            "bin.exe",
            "--addr",
            "0x1400",
            "--max-bytes",
            "1024",
            "--instruction-limit",
            "64",
            "--continue-past-indirect",
            "--json",
        ]);
        assert_eq!(parsed.args.raw_pcode, Some(0x1400));
        assert_eq!(parsed.args.raw_pcode_max_bytes, 1024);
        assert_eq!(parsed.args.raw_pcode_instruction_limit, 64);
        assert!(parsed.args.raw_pcode_continue_past_indirect);
        assert!(parsed.args.json);
    }

    #[test]
    fn canonical_pcode_stages_parsing_maps_to_pcode_stages_command() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "pcode-stages",
            "bin.exe",
            "--addr",
            "0x1400",
            "--max-bytes",
            "1024",
            "--instruction-limit",
            "64",
            "--strict-indirect-stop",
            "--json",
        ]);
        assert_eq!(parsed.args.pcode_stages, Some(0x1400));
        assert_eq!(parsed.args.pcode_stages_max_bytes, 1024);
        assert_eq!(parsed.args.pcode_stages_instruction_limit, 64);
        assert!(parsed.args.pcode_stages_strict_indirect_stop);
        assert!(parsed.args.json);
    }

    #[test]
    fn canonical_nir_stats_parsing_maps_to_nir_stats_command() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "nir-stats",
            "bin.exe",
            "--addr",
            "0x1400",
            "--max-bytes",
            "1024",
            "--instruction-limit",
            "64",
            "--strict-indirect-stop",
            "--json",
        ]);
        assert_eq!(parsed.args.nir_stats, Some(0x1400));
        assert_eq!(parsed.args.nir_stats_max_bytes, 1024);
        assert_eq!(parsed.args.nir_stats_instruction_limit, 64);
        assert!(parsed.args.nir_stats_strict_indirect_stop);
        assert!(parsed.args.json);
    }

    #[test]
    fn canonical_pcode_topology_parsing_maps_to_pcode_topology_command() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "pcode-topology",
            "bin.exe",
            "--addr",
            "0x1400",
            "--max-bytes",
            "2048",
            "--instruction-limit",
            "96",
            "--strict-indirect-stop",
            "--json",
        ]);
        assert_eq!(parsed.args.pcode_topology, Some(0x1400));
        assert_eq!(parsed.args.pcode_topology_max_bytes, 2048);
        assert_eq!(parsed.args.pcode_topology_instruction_limit, 96);
        assert!(parsed.args.pcode_topology_strict_indirect_stop);
        assert!(parsed.args.json);
    }

    #[test]
    fn canonical_decomp_parsing_maps_to_decomp_command() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "decomp",
            "bin.exe",
            "--addr",
            "0x1400",
            "--ghidra-compat",
            "--benchmark",
            "--json",
        ]);
        assert_eq!(parsed.args.address, Some(0x1400));
        assert!(parsed.args.ghidra_compat);
        assert!(parsed.args.benchmark);
        assert!(parsed.args.json);
    }

    #[test]
    fn canonical_decomp_debug_flags_parse() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "decomp",
            "bin.exe",
            "--addr",
            "0x100",
            "--json",
            "--debug-decomp",
            "--debug-decomp-bundle",
            "/tmp/dbg.json",
        ]);
        assert!(parsed.args.debug_decomp);
        assert!(parsed.args.json);
        assert_eq!(
            parsed.args.debug_decomp_bundle,
            Some(PathBuf::from("/tmp/dbg.json"))
        );
    }

    #[test]
    fn canonical_decomp_all_limit_maps_to_batch_decomp() {
        let parsed =
            parse_canonical(&["fission_cli", "decomp", "bin.exe", "--all", "--limit", "10"]);
        assert!(parsed.args.decomp_all);
        assert_eq!(parsed.args.decomp_limit, Some(10));
    }

    #[test]
    fn canonical_decomp_addresses_file_maps_to_batch_decomp() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "decomp",
            "bin.exe",
            "--addresses-file",
            "/tmp/addrs.txt",
            "--json",
        ]);
        assert!(parsed.args.decomp_all);
        assert_eq!(
            parsed.args.addresses_file,
            Some(PathBuf::from("/tmp/addrs.txt"))
        );
        assert!(parsed.args.json);
    }

    #[test]
    fn canonical_decomp_all_can_include_nonuser_functions() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "decomp",
            "bin.exe",
            "--all",
            "--include-nonuser-functions",
        ]);
        assert!(parsed.args.decomp_all);
        assert!(parsed.args.include_nonuser_functions);
    }

    #[test]
    fn canonical_inventory_function_facts_maps_to_inventory_surface() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "inventory",
            "function-facts",
            "bin.exe",
            "--output-jsonl",
            "rows.jsonl",
            "--summary-json",
            "summary.json",
        ]);
        assert!(parsed.args.emit_function_facts_inventory);
        assert_eq!(parsed.args.output_jsonl, Some(PathBuf::from("rows.jsonl")));
        assert_eq!(
            parsed.args.summary_json,
            Some(PathBuf::from("summary.json"))
        );
        assert!(!parsed.args.include_nonuser_functions);
    }

    #[test]
    fn canonical_inventory_preview_candidates_maps_to_inventory_surface() {
        let parsed = parse_canonical(&[
            "fission_cli",
            "inventory",
            "preview-candidates",
            "bin.exe",
            "--batch",
            "--output-jsonl",
            "rows.jsonl",
        ]);
        assert!(parsed.args.preview_candidate_scan_batch);
        assert!(!parsed.args.preview_candidate_inventory);
        assert_eq!(parsed.args.output_jsonl, Some(PathBuf::from("rows.jsonl")));
    }

    #[test]
    fn canonical_inventory_surfaces_accept_include_nonuser_functions() {
        let facts = parse_canonical(&[
            "fission_cli",
            "inventory",
            "function-facts",
            "bin.exe",
            "--include-nonuser-functions",
            "--output-jsonl",
            "rows.jsonl",
            "--summary-json",
            "summary.json",
        ]);
        assert!(facts.args.emit_function_facts_inventory);
        assert!(facts.args.include_nonuser_functions);

        let preview = parse_canonical(&[
            "fission_cli",
            "inventory",
            "preview-candidates",
            "bin.exe",
            "--batch",
            "--include-nonuser-functions",
            "--output-jsonl",
            "rows.jsonl",
        ]);
        assert!(preview.args.preview_candidate_scan_batch);
        assert!(preview.args.include_nonuser_functions);
    }

    #[test]
    fn legacy_info_invocation_maps_to_same_internal_shape() {
        let parsed = parse_legacy(&["fission_cli", "bin.exe", "--info"]);
        assert_eq!(parsed.legacy_warning, Some(LegacyInvocationKind::Info));
        assert!(parsed.args.info);
    }

    #[test]
    fn legacy_list_invocation_maps_to_same_internal_shape() {
        let parsed = parse_legacy(&["fission_cli", "bin.exe", "--funcs"]);
        assert_eq!(parsed.legacy_warning, Some(LegacyInvocationKind::List));
        assert!(parsed.args.list);
    }

    #[test]
    fn legacy_disasm_invocation_maps_to_same_internal_shape() {
        let parsed = parse_legacy(&["fission_cli", "bin.exe", "--asm", "0x1400"]);
        assert_eq!(parsed.legacy_warning, Some(LegacyInvocationKind::Disasm));
        assert_eq!(parsed.args.disasm, Some(0x1400));
    }

    #[test]
    fn legacy_decomp_invocation_maps_to_same_internal_shape() {
        let parsed = parse_legacy(&["fission_cli", "bin.exe", "--decomp", "0x1400"]);
        assert_eq!(parsed.legacy_warning, Some(LegacyInvocationKind::Decomp));
        assert_eq!(parsed.args.address, Some(0x1400));
    }

    #[test]
    fn legacy_batch_invocation_can_include_nonuser_functions() {
        let parsed = parse_legacy(&[
            "fission_cli",
            "bin.exe",
            "--decomp-all",
            "--include-nonuser-functions",
        ]);
        assert!(parsed.args.decomp_all);
        assert!(parsed.args.include_nonuser_functions);
    }

    #[test]
    fn canonical_non_decomp_subcommands_reject_decomp_only_options() {
        let err = CliArgs::try_parse_from(["fission_cli", "info", "bin.exe", "--benchmark"])
            .expect_err("expected unknown flag on info subcommand");
        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }

    // ============================================================================
    // Tests
    // ============================================================================

    #[test]
    fn canonical_non_inventory_subcommands_reject_inventory_only_options() {
        let err = CliArgs::try_parse_from([
            "fission_cli",
            "decomp",
            "bin.exe",
            "--addr",
            "0x1400",
            "--output-jsonl",
            "rows.jsonl",
        ])
        .expect_err("expected inventory-only flag rejection");
        assert_eq!(err.kind(), ErrorKind::UnknownArgument);
    }
}
