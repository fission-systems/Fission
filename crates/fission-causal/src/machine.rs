//! A reproducible program run: one binary, one input stream, one
//! environment, and fresh emulators built from the same prototype state.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::observe::{Observer, SyscallArg};
use fission_emulator::os::LinuxEnv;
use fission_emulator::os::linux::image_info::{ImageInfo, ProcessArgs};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

use crate::contract::{CONTRACT_VERSION, EnvSource, Location, ReplayFingerprint};

/// Which execution engine a replay uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    /// Cranelift blocks; the production path.
    Jit,
    /// The p-code interpreter (single-stepping always uses it).
    Interpreter,
}

/// One guest syscall as the OS layer named it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyscallEvent {
    pub pc: u64,
    pub number: u64,
    pub name: Option<&'static str>,
    pub args: [u64; 6],
}

/// Observer that copies syscall events out of the emulator.
struct SyscallTap {
    log: Arc<Mutex<Vec<SyscallEvent>>>,
}

impl Observer for SyscallTap {
    fn on_syscall_detailed(
        &mut self,
        pc: u64,
        number: u64,
        name: Option<&'static str>,
        args: &[u64; 6],
        _detail: Vec<SyscallArg>,
    ) {
        if let Ok(mut log) = self.log.lock() {
            log.push(SyscallEvent {
                pc,
                number,
                name,
                args: *args,
            });
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A live emulator plus the syscalls it has made.
pub struct Run {
    pub emu: Emulator,
    syscalls: Arc<Mutex<Vec<SyscallEvent>>>,
}

impl Run {
    pub fn syscall_count(&self) -> usize {
        self.syscalls.lock().map(|l| l.len()).unwrap_or(0)
    }

    pub fn syscalls_since(&self, start: usize) -> Vec<SyscallEvent> {
        self.syscalls
            .lock()
            .map(|l| l.get(start..).map(<[_]>::to_vec).unwrap_or_default())
            .unwrap_or_default()
    }

    /// Run until exactly `step` instructions have retired, or the program
    /// stops first. Returns whether the step was reached.
    pub fn run_to_step(&mut self, step: u64) -> Result<bool> {
        if self.emu.inst_count > step {
            bail!(
                "cannot run backwards: at step {} asked for {step}",
                self.emu.inst_count
            );
        }
        if self.emu.inst_count == step {
            return Ok(true);
        }
        let saved = self.emu.max_inst;
        self.emu.max_inst = Some(step);
        let outcome = self.emu.resume();
        self.emu.max_inst = saved;
        outcome?;
        Ok(self.emu.inst_count == step)
    }

    /// Guest standard output so far.
    pub fn stdout(&self) -> Vec<u8> {
        self.emu
            .vfs
            .files
            .get(&1)
            .map(|f| f.content.clone())
            .unwrap_or_default()
    }
}

/// The resolved storage of a location: `(space, offset, size)`.
pub type Storage = (u64, u64, u32);

/// Everything a replay is pinned to, and the prototype it starts from.
pub struct Program {
    pub path: String,
    pub binary: LoadedBinary,
    prototype: MachineState,
    image: ImageInfo,
    sleigh: RuntimeSleighFrontend,
    pub language: String,
    pub input: Vec<u8>,
    pub args: ProcessArgs,
    binary_blake3: String,
    initial_state_blake3: String,
    /// Register names sorted by `(offset, size, name)` for deterministic
    /// reverse lookup.
    registers: Vec<(u64, u32, String)>,
    pub register_space: u64,
    pub ram_space: u64,
    pub pc_register: String,
    pub sp_register: String,
}

impl Program {
    /// Load `path` and pin `input` as its standard input.
    pub fn load(path: &str, input: &[u8]) -> Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("read {path}"))?;
        let binary = LoadedBinary::from_file(path).map_err(|e| anyhow!("load {path}: {e}"))?;
        let args = ProcessArgs::default();
        let mut prototype = MachineState::new();
        let image = fission_emulator::os::linux::loader::load_elf_with_args(
            &mut prototype,
            &binary,
            &args,
        )?;
        let load_spec = binary
            .load_spec()
            .ok_or_else(|| anyhow!("{path}: no load spec"))?
            .clone();
        let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
            .map_err(|e| anyhow!("sleigh for {path}: {e}"))?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("{path}: no sleigh frontend"))?;
        let language = load_spec.pair.language_id.as_str().to_string();
        let mut program = Self {
            path: path.to_string(),
            binary,
            prototype,
            image,
            sleigh,
            language,
            input: input.to_vec(),
            args,
            binary_blake3: blake3::hash(&bytes).to_hex().to_string(),
            initial_state_blake3: String::new(),
            registers: Vec::new(),
            register_space: 0,
            ram_space: 0,
            pc_register: String::new(),
            sp_register: String::new(),
        };
        let run = program.fresh(Engine::Jit)?;
        let mut registers: Vec<(u64, u32, String)> = run
            .emu
            .register_map
            .iter()
            .map(|(name, &(_, offset, size))| (offset, size, name.clone()))
            .collect();
        registers.sort();
        program.registers = registers;
        program.register_space = run.emu.state.register_space();
        program.ram_space = run.emu.state.ram_space();
        program.pc_register = run.emu.arch.pc_reg.to_string();
        program.sp_register = run.emu.arch.sp_reg.to_string();
        program.initial_state_blake3 = state_digest(&run.emu);
        Ok(program)
    }

    /// A new emulator at the initial state, with the input stream seeded.
    pub fn fresh(&self, engine: Engine) -> Result<Run> {
        let arch = ArchInfo::from_language_id(&self.language, Some(&self.binary))
            .map_err(|e| anyhow!("no arch info for {}: {e}", self.language))?;
        let mut emu = Emulator::new(
            self.prototype.clone(),
            self.binary.clone(),
            self.sleigh.clone(),
            arch,
            Box::new(LinuxEnv::new()),
        )?;
        emu.force_interpreter = engine == Engine::Interpreter;
        emu.apply_linux_image(self.image.clone())?;
        emu.seed_stdin(&self.input);
        emu.set_host_stdout_echo(false);
        let syscalls = Arc::new(Mutex::new(Vec::new()));
        emu.add_observer(Box::new(SyscallTap {
            log: Arc::clone(&syscalls),
        }));
        Ok(Run { emu, syscalls })
    }

    pub fn fingerprint(&self, environment_sources: Vec<EnvSource>) -> ReplayFingerprint {
        ReplayFingerprint {
            contract: CONTRACT_VERSION.to_string(),
            binary_blake3: self.binary_blake3.clone(),
            language: self.language.clone(),
            input_blake3: blake3::hash(&self.input).to_hex().to_string(),
            input_len: self.input.len(),
            argv: self.args.argv.clone(),
            envp: self.args.envp.clone(),
            initial_state_blake3: self.initial_state_blake3.clone(),
            environment_sources,
            engine: "fission-emulator: JIT for replays, interpreter for the recorded trace"
                .to_string(),
        }
    }

    /// `(space, offset, size)` of a register.
    pub fn register(&self, name: &str) -> Option<Storage> {
        self.registers
            .iter()
            .find(|(_, _, known)| known.eq_ignore_ascii_case(name))
            .map(|&(offset, size, _)| (self.register_space, offset, size))
    }

    /// The register exactly occupying `offset..offset+size`, if one does.
    pub fn register_named(&self, offset: u64, size: u32) -> Option<&str> {
        self.registers
            .iter()
            .find(|(o, s, _)| *o == offset && *s == size)
            .map(|(_, _, name)| name.as_str())
    }

    /// The smallest register that contains `offset..offset+size`.
    pub fn register_containing(&self, offset: u64, size: u32) -> Option<(&str, u64, u32)> {
        self.registers
            .iter()
            .filter(|(o, s, _)| *o <= offset && offset + u64::from(size) <= *o + u64::from(*s))
            .min_by_key(|(o, s, name)| (*s, *o, name.clone()))
            .map(|(o, s, name)| (name.as_str(), *o, *s))
    }

    pub fn storage(&self, location: &Location) -> Option<Storage> {
        match location {
            Location::Register { name } => self.register(name),
            Location::Memory { addr, len } => Some((self.ram_space, *addr, *len)),
        }
    }

    /// Lift one instruction with this program's Sleigh frontend.
    pub fn sleigh_decode(
        &self,
        bytes: &[u8],
        pc: u64,
    ) -> Option<(Vec<fission_pcode_ir::ir::PcodeOp>, u64)> {
        self.sleigh
            .decode_and_lift_with_context_override(bytes, pc, None)
            .ok()
            .map(|(ops, len, _)| (ops, len))
    }

    pub fn find_function(&self, name: &str) -> Option<u64> {
        self.binary.find_function(name).map(|f| f.address)
    }

    pub fn find_global(&self, name: &str) -> Option<u64> {
        let mut hits: Vec<u64> = self
            .binary
            .global_symbols
            .iter()
            .filter(|(_, n)| n.as_str() == name)
            .map(|(a, _)| *a)
            .collect();
        hits.sort_unstable();
        hits.first().copied().or_else(|| {
            self.binary
                .loader_symbols
                .iter()
                .find(|s| s.name == name)
                .map(|s| s.address)
        })
    }
}

/// BLAKE3 over the register file and every mapped RAM page, in address
/// order. Used to pin the initial state.
pub fn state_digest(emu: &Emulator) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&emu.pc.to_le_bytes());
    let mut spaces: Vec<u64> = emu.state.spaces.keys().copied().collect();
    spaces.sort_unstable();
    let unique = emu.state.unique_space();
    for space in spaces {
        if space == unique {
            continue;
        }
        let Some(s) = emu.state.spaces.get(&space) else {
            continue;
        };
        let mut pages: Vec<u64> = s.pages.keys().copied().collect();
        pages.sort_unstable();
        hasher.update(&space.to_le_bytes());
        for page in pages {
            let Ok(bytes) = emu
                .state
                .read_space_readonly(space, page, s.page_size as usize)
            else {
                continue;
            };
            hasher.update(&page.to_le_bytes());
            hasher.update(&bytes);
        }
    }
    hasher.finalize().to_hex().to_string()
}

/// BLAKE3 over what a run left behind: exit status, standard output and the
/// retired-instruction count.
pub fn end_digest(run: &Run) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&run.emu.inst_count.to_le_bytes());
    hasher.update(&run.emu.exit_code.unwrap_or(u32::MAX).to_le_bytes());
    hasher.update(&run.stdout());
    hasher.finalize().to_hex().to_string()
}
