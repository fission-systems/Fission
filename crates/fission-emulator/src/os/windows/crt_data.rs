//! The CRT's process-wide variables, in one page the loader maps.
//!
//! Two different things ask for these, and they have to be the same storage:
//!
//! * `__p__fmode()` and friends hand the program the *address* of a variable,
//!   because that is the whole point of the `__p_*` family.
//! * mingw imports several of them as **data**, not code: the IAT slot for
//!   `_fmode` holds a pointer to msvcrt's variable, and the start-up writes
//!   through it. Patching such a slot with a call trampoline makes the
//!   program write into the trampoline region -- which is how every 32-bit PE
//!   in the dev corpus ended its run.
//!
//! So the cells are allocated once, at import-patch time, in a fixed page.
//! That is early enough for the IAT to point at them and stable enough for
//! `__p_*` to hand out their addresses later.

use anyhow::Result;

use crate::pcode::page_map::prot;
use crate::pcode::state::MachineState;

/// Where the page goes. Below the PEB/TEB pages and above everything an image
/// maps, in both address-space layouts the PE loader builds.
const BASE: u64 = 0x7FFD_0000;
const SIZE: u64 = 0x1000;

/// The CRT variables, by guest address.
#[derive(Debug, Clone, Copy)]
pub struct CrtGlobals {
    pub argc: u64,
    pub argv: u64,
    pub environ: u64,
    pub commode: u64,
    pub fmode: u64,
    pub acmdln: u64,
    pub initenv: u64,
    /// Cells for data imports with no special meaning here: they exist, they
    /// are writable, and nothing reads them back.
    scratch: u64,
    scratch_cells: u64,
    /// The vectors the pointers above point at.
    pub argv_vector: u64,
    pub environ_vector: u64,
    /// `argv[0]`, which is also the command line.
    pub program_name: u64,
    /// The same process, spelled in UTF-16 for a `wmain` program.
    ///
    /// These used to be the narrow cells. A `wmain` program reads `argv[0]`
    /// as UTF-16, so `"program.exe\0"` came out as three CJK characters and
    /// whatever followed -- libdeflate's gzip driver parsed that as a file
    /// operand and reported "unable to stat file".
    pub wargv: u64,
    pub wenviron: u64,
    pub winitenv: u64,
    pub wcmdln: u64,
    pub wargv_vector: u64,
    pub wenviron_vector: u64,
    pub wide_program_name: u64,
    /// The cell `_errno()` hands out the address of.
    pub errno: u64,
    /// The `FILE` objects, one per descriptor. A program never fills one in
    /// itself -- it asks the CRT for the pointer -- but `feof` and `ferror`
    /// are macros in some headers and do read through it, so the storage has
    /// to be real and readable.
    ///
    /// Slot *n* is descriptor *n*, so a `FILE*` and a fd convert into each
    /// other by arithmetic and there is no table to keep in step. The first
    /// three are the standard streams the VFS already reserves.
    pub iob: u64,
    ptr: u64,
}

/// Bytes per `FILE`. Larger than any field this emulator fills in, so a
/// program that reads one gets zeros rather than the next stream's storage.
pub const IOB_STRIDE: u64 = 0x40;
/// How many descriptors can be a `FILE`. One page's worth; a program that
/// opens more gets a failed `fopen`, which is a case it already handles.
pub const IOB_COUNT: u64 = STREAM_SIZE / IOB_STRIDE;

/// The `FILE` array's own page. Its own, because one page holds sixty-four
/// of them and the CRT variables need the room.
const STREAM_BASE: u64 = 0x7FFC_0000;
const STREAM_SIZE: u64 = 0x1000;

impl CrtGlobals {
    /// Build the page. Called once per process, from `patch_imports`.
    pub fn build(state: &mut MachineState, is_64bit: bool) -> Result<Self> {
        state
            .page_map
            .map_region(BASE, SIZE, prot::VALID | prot::READ | prot::WRITE, true);

        let ptr = if is_64bit { 8 } else { 4 };
        let mut next = BASE;
        let mut cell = || {
            let at = next;
            next += ptr;
            at
        };
        let (argc, argv, environ, commode, fmode, acmdln, initenv) =
            (cell(), cell(), cell(), cell(), cell(), cell(), cell());
        let (wargv, wenviron, winitenv, wcmdln) = (cell(), cell(), cell(), cell());
        let scratch_cells = 32;
        let scratch = next;
        next += ptr * scratch_cells;

        let argv_vector = next;
        next += ptr * 2;
        let environ_vector = next;
        next += ptr;
        let program_name = next;
        next += 32;
        let wargv_vector = next;
        next += ptr * 2;
        let wenviron_vector = next;
        next += ptr;
        let wide_program_name = next;
        next += 32;
        let errno = next;
        next += ptr;
        debug_assert!(next - BASE <= SIZE, "the CRT page is full");

        state.page_map.map_region(
            STREAM_BASE,
            STREAM_SIZE,
            prot::VALID | prot::READ | prot::WRITE,
            true,
        );
        let iob = STREAM_BASE;

        let mut globals = Self {
            argc,
            argv,
            environ,
            commode,
            fmode,
            acmdln,
            initenv,
            scratch,
            scratch_cells,
            argv_vector,
            environ_vector,
            program_name,
            wargv,
            wenviron,
            winitenv,
            wcmdln,
            wargv_vector,
            wenviron_vector,
            wide_program_name,
            errno,
            iob,
            ptr,
        };
        globals.write_defaults(state)?;
        Ok(globals)
    }

    fn write_defaults(&mut self, state: &mut MachineState) -> Result<()> {
        let ram = state.ram_space();
        let name = b"program.exe\0";
        state.write_space(ram, self.program_name, name)?;

        // argv = { name, NULL }; environ = { NULL }.
        self.put(state, self.argv_vector, self.program_name)?;
        self.put(state, self.argv_vector + self.ptr, 0)?;
        self.put(state, self.environ_vector, 0)?;

        self.put(state, self.argc, 1)?;
        self.put(state, self.argv, self.argv_vector)?;
        self.put(state, self.environ, self.environ_vector)?;
        self.put(state, self.initenv, self.environ_vector)?;
        self.put(state, self.acmdln, self.program_name)?;

        // The wide copies: same shape, UTF-16 text.
        let wide_name: Vec<u8> = "program.exe\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        state.write_space(ram, self.wide_program_name, &wide_name)?;
        self.put(state, self.wargv_vector, self.wide_program_name)?;
        self.put(state, self.wargv_vector + self.ptr, 0)?;
        self.put(state, self.wenviron_vector, 0)?;
        self.put(state, self.wargv, self.wargv_vector)?;
        self.put(state, self.wenviron, self.wenviron_vector)?;
        self.put(state, self.winitenv, self.wenviron_vector)?;
        self.put(state, self.wcmdln, self.wide_program_name)?;
        // `_commode` and `_fmode` both default to zero: no commit-on-write,
        // text mode. Writing them is what makes them readable at all.
        self.put(state, self.commode, 0)?;
        self.put(state, self.fmode, 0)?;
        for i in 0..self.scratch_cells {
            self.put(state, self.scratch + i * self.ptr, 0)?;
        }
        self.put(state, self.errno, 0)?;
        state.write_space(ram, self.iob, &vec![0u8; STREAM_SIZE as usize])?;
        Ok(())
    }

    fn put(&self, state: &mut MachineState, addr: u64, value: u64) -> Result<()> {
        let ram = state.ram_space();
        let bytes = value.to_le_bytes();
        state.write_space(ram, addr, &bytes[..self.ptr as usize])?;
        Ok(())
    }

    /// The cell an imported *data* symbol names, if this is one.
    ///
    /// Everything not on this list stays a call trampoline, so an
    /// unimplemented **function** is still reported as a miss by name rather
    /// than silently becoming a writable word.
    pub fn data_cell(&self, bare_name: &str, nth_unknown: u64) -> Option<u64> {
        let known = match bare_name {
            "__argc" => self.argc,
            "__argv" => self.argv,
            "__wargv" => self.wargv,
            "_environ" => self.environ,
            "_wenviron" => self.wenviron,
            "__initenv" => self.initenv,
            "__winitenv" => self.winitenv,
            "_commode" => self.commode,
            "_fmode" => self.fmode,
            "_acmdln" => self.acmdln,
            "_wcmdln" => self.wcmdln,
            // Data msvcrt exports that mingw imports and this emulator has no
            // opinion about. They still have to be writable memory.
            "_pgmptr" | "_wpgmptr" | "_pctype" | "_mbctype" | "__mb_cur_max" | "_daylight"
            | "_timezone" | "_tzname" | "_osver" | "_winver" | "_winmajor" | "_winminor"
            | "_osplatform" | "_amblksiz" | "_sys_errlist" | "_sys_nerr" | "_iob"
            | "__lc_codepage" | "_MB_CUR_MAX" => {
                return Some(self.scratch + (nth_unknown % self.scratch_cells) * self.ptr);
            }
            _ => return None,
        };
        Some(known)
    }
}

impl CrtGlobals {
    /// The `FILE*` for a descriptor, if there is a slot for it.
    pub fn stream(&self, fd: u64) -> u64 {
        if fd >= IOB_COUNT {
            return 0;
        }
        self.iob + fd * IOB_STRIDE
    }

    /// Which descriptor a `FILE*` is, if it is one of ours.
    ///
    /// A pointer into the middle of a `FILE` still resolves: a program that
    /// was handed `&iob[1]` and passes `&iob[1].flags` is talking about
    /// stdout either way.
    pub fn stream_index(&self, pointer: u64) -> Option<u64> {
        (pointer >= self.iob && pointer < self.iob + STREAM_SIZE)
            .then(|| (pointer - self.iob) / IOB_STRIDE)
    }
}
