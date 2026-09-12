//! A live emulated machine, exposed to a script.
//!
//! The read-only half of this crate hands a script the binary's inventory --
//! functions, imports, sections, strings -- all of which the command line can
//! print just as well, so a script that only reads them is a longer way to
//! spell `fission functions --json | jq`.
//!
//! This half is the part that has no outside equivalent. An emulated process
//! exists only inside the command that launched it, so anything built out of
//! more than one step -- set a breakpoint on every import and see which ones
//! are reached, run until a buffer is written and read the stack, step until
//! the program counter leaves a range -- cannot be assembled from separate
//! invocations at any price. It needs a loop inside one process, and that is
//! what a script is.

use std::sync::{Arc, Mutex};

use fission_dynamic::debug::emulator_backend::EmulatorBackend;
use fission_dynamic::debug::traits::ExecutionBackend;
use fission_dynamic::debug::types::{DebugEvent, MemoryBpKind};
use rhai::{Array, Dynamic, EvalAltResult, Map};

/// The machine, shared with the Rhai engine.
///
/// `Arc<Mutex<..>>` because Rhai's registered methods take `&mut self` on a
/// value the engine owns and clones, and the machine has to be the same one
/// across every call.
#[derive(Clone)]
pub struct MachineHost(Arc<Mutex<Session>>);

/// The machine plus whatever it has reported and nobody has asked for yet.
///
/// The backend's queue is drained by whoever polls it first, so `output()`
/// reading until it saw a non-output event both stopped early *and* threw
/// that event away. Everything is pumped into here instead, and each accessor
/// takes only what it is about and leaves the rest.
struct Session {
    backend: EmulatorBackend,
    pending: Vec<DebugEvent>,
}

impl Session {
    fn pump(&mut self) {
        while let Ok(Some(event)) = self.backend.poll_event(0) {
            self.pending.push(event);
        }
    }
}

/// The one thread an emulated process has.
const THREAD: u32 = 1;

type ScriptResult<T> = Result<T, Box<EvalAltResult>>;

fn fail(message: impl std::fmt::Display) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        Dynamic::from(message.to_string()),
        rhai::Position::NONE,
    ))
}

impl MachineHost {
    /// Launch `path` under the emulator.
    pub fn launch(path: &str) -> Result<Self, String> {
        let mut backend = EmulatorBackend::new();
        backend
            .launch(path, &[])
            .map_err(|e| format!("could not launch {path}: {e}"))?;
        Ok(Self(Arc::new(Mutex::new(Session {
            backend,
            pending: Vec::new(),
        }))))
    }

    fn with<T>(
        &self,
        what: &str,
        f: impl FnOnce(&mut EmulatorBackend) -> ScriptResult<T>,
    ) -> ScriptResult<T> {
        let mut guard = self
            .0
            .lock()
            .map_err(|_| fail(format!("the machine is poisoned; cannot {what}")))?;
        let out = f(&mut guard.backend);
        // Whatever running produced, kept for whoever asks.
        guard.pump();
        out
    }

    fn with_session<T>(&self, what: &str, f: impl FnOnce(&mut Session) -> T) -> ScriptResult<T> {
        let mut guard = self
            .0
            .lock()
            .map_err(|_| fail(format!("the machine is poisoned; cannot {what}")))?;
        guard.pump();
        Ok(f(&mut guard))
    }

    // ── Stopping ────────────────────────────────────────────────────────────

    pub fn breakpoint(&mut self, address: i64) -> ScriptResult<()> {
        self.with("set a breakpoint", |b| {
            b.set_sw_breakpoint(address as u64).map_err(fail)
        })
    }

    pub fn remove_breakpoint(&mut self, address: i64) -> ScriptResult<()> {
        self.with("remove a breakpoint", |b| {
            b.remove_sw_breakpoint(address as u64).map_err(fail)
        })
    }

    /// Stop when `[address, address + size)` is written.
    pub fn watch_write(&mut self, address: i64, size: i64) -> ScriptResult<()> {
        self.watch(address, size, "write".into())
    }

    /// `kind` is `read`, `write` or `access`.
    pub fn watch(&mut self, address: i64, size: i64, kind: String) -> ScriptResult<()> {
        let kind = match kind.to_ascii_lowercase().as_str() {
            "read" => MemoryBpKind::Read,
            "write" => MemoryBpKind::Write,
            "access" => MemoryBpKind::Access,
            other => {
                return Err(fail(format!(
                    "watch kind must be read, write or access, not {other:?}"
                )));
            }
        };
        self.with("set a watchpoint", |b| {
            b.set_memory_breakpoint(address as u64, size.max(1) as usize, kind)
                .map_err(fail)
        })
    }

    pub fn unwatch(&mut self, address: i64) -> ScriptResult<()> {
        self.with("remove a watchpoint", |b| {
            b.remove_memory_breakpoint(address as u64).map_err(fail)
        })
    }

    // ── Running ─────────────────────────────────────────────────────────────

    /// Run until something stops the machine. Returns the reason, so a loop
    /// can decide what to do next without a second call.
    pub fn resume(&mut self) -> ScriptResult<String> {
        self.with("resume", |b| {
            b.continue_execution().map_err(fail)?;
            Ok(b.stop_reason().unwrap_or_else(|| "stopped".into()))
        })
    }

    pub fn step(&mut self) -> ScriptResult<String> {
        self.with("step", |b| {
            b.single_step().map_err(fail)?;
            Ok(b.stop_reason().unwrap_or_else(|| "stepped".into()))
        })
    }

    pub fn step_over(&mut self) -> ScriptResult<String> {
        self.with("step over", |b| {
            b.step_over().map_err(fail)?;
            Ok(b.stop_reason().unwrap_or_else(|| "stepped".into()))
        })
    }

    pub fn step_out(&mut self) -> ScriptResult<String> {
        self.with("step out", |b| {
            b.step_out().map_err(fail)?;
            Ok(b.stop_reason().unwrap_or_else(|| "stepped".into()))
        })
    }

    /// Whether the program has stopped for good, so a `while` loop has a
    /// condition that terminates.
    pub fn finished(&mut self) -> ScriptResult<bool> {
        self.with("ask whether the program finished", |b| {
            Ok(matches!(
                b.get_state().status,
                fission_dynamic::debug::types::DebugStatus::Terminated
            ))
        })
    }

    pub fn stop_reason(&mut self) -> ScriptResult<String> {
        self.with("read the stop reason", |b| {
            Ok(b.stop_reason().unwrap_or_else(|| "not-started".into()))
        })
    }

    // ── Looking ─────────────────────────────────────────────────────────────

    pub fn pc(&mut self) -> ScriptResult<i64> {
        self.with("read the program counter", |b| {
            Ok(b.fetch_registers(THREAD).map_err(fail)?.pc as i64)
        })
    }

    pub fn reg(&mut self, name: String) -> ScriptResult<i64> {
        self.with("read a register", |b| {
            let regs = b.fetch_registers(THREAD).map_err(fail)?;
            regs.get(&name)
                .map(|v| v as i64)
                .ok_or_else(|| fail(format!("this machine has no register {name}")))
        })
    }

    pub fn regs(&mut self) -> ScriptResult<Map> {
        self.with("read the registers", |b| {
            let regs = b.fetch_registers(THREAD).map_err(fail)?;
            let mut map = Map::new();
            map.insert("pc".into(), Dynamic::from(regs.pc as i64));
            for (name, value) in regs.iter() {
                map.insert(
                    name.to_ascii_lowercase().into(),
                    Dynamic::from(value as i64),
                );
            }
            Ok(map)
        })
    }

    pub fn set_reg(&mut self, name: String, value: i64) -> ScriptResult<()> {
        self.with("write a register", |b| {
            let mut regs = b.fetch_registers(THREAD).map_err(fail)?;
            if regs.get(&name).is_none() {
                return Err(fail(format!("this machine has no register {name}")));
            }
            regs.set(&name, value as u64);
            b.set_registers(THREAD, &regs).map_err(fail)
        })
    }

    /// Memory as an array of byte values, so a script can index it.
    pub fn read(&mut self, address: i64, size: i64) -> ScriptResult<Array> {
        self.with("read memory", |b| {
            let bytes = b
                .read_memory(address as u64, size.max(0) as usize)
                .map_err(fail)?;
            Ok(bytes.into_iter().map(|v| Dynamic::from(v as i64)).collect())
        })
    }

    /// Memory as a string, for the common case of looking at what a program
    /// wrote into a buffer.
    pub fn read_string(&mut self, address: i64, size: i64) -> ScriptResult<String> {
        self.with("read memory", |b| {
            let bytes = b
                .read_memory(address as u64, size.max(0) as usize)
                .map_err(fail)?;
            let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
            Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
        })
    }

    pub fn write(&mut self, address: i64, bytes: Array) -> ScriptResult<()> {
        let mut data = Vec::with_capacity(bytes.len());
        for value in bytes {
            let byte = value
                .as_int()
                .map_err(|_| fail("write takes an array of byte values"))?;
            data.push(u8::try_from(byte).map_err(|_| fail(format!("{byte} is not a byte")))?);
        }
        self.with("write memory", |b| {
            b.write_memory(address as u64, &data).map_err(fail)
        })
    }

    /// Everything that has happened since this was last called.
    pub fn events(&mut self) -> ScriptResult<Array> {
        self.with_session("read events", |s| {
            s.pending
                .drain(..)
                .map(|event| Dynamic::from_map(event_map(&event)))
                .collect()
        })
    }

    /// What the program has written to its own output since this was last
    /// called.
    ///
    /// Takes only the output, so a later `events()` still sees everything
    /// else that happened.
    pub fn output(&mut self) -> ScriptResult<String> {
        self.with_session("read the program's output", |s| {
            let mut text = String::new();
            s.pending.retain(|event| match event {
                DebugEvent::OutputString { message } => {
                    text.push_str(message);
                    false
                }
                _ => true,
            });
            text
        })
    }
}

fn event_map(event: &DebugEvent) -> Map {
    let mut m = Map::new();
    let mut set = |k: &str, v: Dynamic| {
        m.insert(k.into(), v);
    };
    match event {
        DebugEvent::ProcessCreated { pid, .. } => {
            set("event", Dynamic::from("process_created".to_string()));
            set("pid", Dynamic::from(*pid as i64));
        }
        DebugEvent::ProcessExited { exit_code } => {
            set("event", Dynamic::from("process_exited".to_string()));
            set("exit_code", Dynamic::from(*exit_code as i64));
        }
        DebugEvent::BreakpointHit { address, .. } => {
            set("event", Dynamic::from("breakpoint_hit".to_string()));
            set("address", Dynamic::from(*address as i64));
        }
        DebugEvent::WatchpointHit {
            address,
            size,
            write,
            pc,
            ..
        } => {
            set("event", Dynamic::from("watchpoint_hit".to_string()));
            set("address", Dynamic::from(*address as i64));
            set("size", Dynamic::from(*size as i64));
            set("write", Dynamic::from(*write));
            set("pc", Dynamic::from(*pc as i64));
        }
        DebugEvent::SingleStep { .. } => {
            set("event", Dynamic::from("single_step".to_string()));
        }
        DebugEvent::OutputString { message } => {
            set("event", Dynamic::from("output".to_string()));
            set("message", Dynamic::from(message.clone()));
        }
        DebugEvent::Exception { code, address, .. } => {
            set("event", Dynamic::from("exception".to_string()));
            set("code", Dynamic::from(*code as i64));
            set("address", Dynamic::from(*address as i64));
        }
        DebugEvent::ThreadCreated { thread_id } | DebugEvent::ThreadExited { thread_id } => {
            set("event", Dynamic::from("thread".to_string()));
            set("thread_id", Dynamic::from(*thread_id as i64));
        }
        DebugEvent::DllLoaded { base_address, name } => {
            set("event", Dynamic::from("dll_loaded".to_string()));
            set("base", Dynamic::from(*base_address as i64));
            set("name", Dynamic::from(name.clone()));
        }
        DebugEvent::DllUnloaded { base_address } => {
            set("event", Dynamic::from("dll_unloaded".to_string()));
            set("base", Dynamic::from(*base_address as i64));
        }
    }
    m
}

pub fn register(engine: &mut rhai::Engine) {
    engine
        .register_type_with_name::<MachineHost>("Machine")
        .register_fn("bp", MachineHost::breakpoint)
        .register_fn("rm_bp", MachineHost::remove_breakpoint)
        .register_fn("watch", MachineHost::watch_write)
        .register_fn("watch", MachineHost::watch)
        .register_fn("unwatch", MachineHost::unwatch)
        .register_fn("resume", MachineHost::resume)
        .register_fn("step", MachineHost::step)
        .register_fn("step_over", MachineHost::step_over)
        .register_fn("step_out", MachineHost::step_out)
        .register_fn("finished", MachineHost::finished)
        .register_fn("stop_reason", MachineHost::stop_reason)
        .register_fn("pc", MachineHost::pc)
        .register_fn("reg", MachineHost::reg)
        .register_fn("regs", MachineHost::regs)
        .register_fn("set_reg", MachineHost::set_reg)
        .register_fn("read", MachineHost::read)
        .register_fn("read_string", MachineHost::read_string)
        .register_fn("write", MachineHost::write)
        .register_fn("events", MachineHost::events)
        .register_fn("output", MachineHost::output);
}
