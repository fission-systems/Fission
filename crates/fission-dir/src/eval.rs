//! A tree-walking interpreter for `PreHirStmt`/`PreHirExpr` (from
//! `fission-midend-prehir`) and `HirStmt`/`HirExpr` (from `fission-midend-core`).
//!
//! This is the concrete-tier backend used by both [`crate::diff`] (PreHIR-vs-HIR
//! diffing, no ground truth) and [`crate::ground_truth`] (PreHIR/HIR-vs-real-
//! machine-code comparison). Two interpreters, [`prehir::interpret`] and
//! [`hir::interpret`], generated from one [`define_interp!`] macro
//! invocation each, so the interpretation *logic* can never silently drift
//! between the two even though `PreHirStmt`/`HirStmt` are genuinely separate
//! types (see `fission_midend_core::ir::hir`'s module doc). Divergence
//! between PreHIR and HIR results is exactly what this crate exists to check
//! for -- if this file's own two copies disagreed on what "the same"
//! p-code-derived logic means, that would corrupt the very comparison this
//! crate makes.
//!
//! # Scope (concrete tier)
//!
//! Supported: `Var`/`Const`/`Cast`/`Unary`/`Binary`/`Select` expressions,
//! memory (`Load`/`PtrOffset`/`Index`/`FieldAccess` through a pointer,
//! `AddressOfGlobal`, `AddressOfLocal`), calls through the world's stubs;
//! `Assign` (to a variable or through `Deref`/`Index`/`FieldAccess`),
//! `Block`, `Switch`, `If`, `While`, `DoWhile`, `For`, `Label`, `Goto`,
//! `Return`, `Break`, `Continue` statements.
//!
//! Memory is the world's: bytes the body stores, over the backing the caller
//! supplies (the program image and the run's initial state). A local whose
//! address the body takes lives there, at the address the world gives it,
//! and reading or assigning it by name reads or writes those bytes.
//!
//! Not supported -- `eval_expr`/`exec_stmt` return `Err` rather than
//! silently producing a wrong or default value: `AggregateCopy`, a member
//! access on a struct value, a call the world has no stub for, and the
//! address of a local the world did not place. A function that touches any
//! of these bails out of verification with a clear reason rather than being
//! silently skipped or falsely reported equivalent.
//!
//! `LogicalAnd`/`LogicalOr` short-circuit as in C: the right operand of
//! `flag && f()` must not run, or count as a call, when `flag` is zero.

use fission_midend_core::ir::NirType;
use std::collections::HashMap;

/// A single scalar value: the raw two's-complement bit pattern, meaningful
/// in its low declared-type-width bits -- every value entering or leaving
/// an interpreter's environment is normalized to its declared type
/// immediately, so callers never need to re-mask.
pub type Value = i64;

/// `ty`'s declared bit width, clamped to `[1, 64]` (this interpreter's
/// values are always plain `i64`s). Shared by both `prehir::`/`hir::` and by
/// [`crate::ground_truth`], which needs the same width to mask a real
/// emulator register value for a fair comparison against an interpreted
/// result.
pub fn width_of(ty: &NirType) -> u32 {
    match ty {
        NirType::Bool => 1,
        NirType::Int { bits, .. } => *bits,
        NirType::Ptr(_) => 64,
        // Not reachable for any supported expression (Aggregate/Float
        // carriers are all rejected before evaluation reaches here); fall
        // back to full width rather than panicking if one slips through.
        NirType::Aggregate { .. } | NirType::Float { .. } | NirType::Unknown => 64,
    }
}

pub fn is_signed(ty: &NirType) -> bool {
    matches!(ty, NirType::Int { signed: true, .. })
}

/// Mask `raw` to `ty`'s declared width, sign-extending if `ty` is signed.
pub fn normalize(raw: i64, ty: &NirType) -> i64 {
    let bits = width_of(ty).clamp(1, 64);
    if bits >= 64 {
        return raw;
    }
    let mask = (1i64 << bits) - 1;
    let v = raw & mask;
    if is_signed(ty) && (v & (1i64 << (bits - 1))) != 0 {
        v | !mask
    } else {
        v
    }
}

/// One call the interpreted body made: where it was printed from, what it
/// called, and the arguments as the body computed them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallEvent {
    /// The call instruction's address, when the printed call carries it.
    pub site: Option<u64>,
    pub target: String,
    pub args: Vec<Value>,
    /// Each argument's declared width in bits, for comparing it with a
    /// machine register that may carry garbage above it.
    pub arg_bits: Vec<u32>,
}

/// What the interpreted body can see outside its own variables.
///
/// Every field is something the emulator running the same function is told
/// as well; the two only agree because both were given the same world.
#[derive(Default)]
pub struct World<'a> {
    /// Bytes placed in memory before the body runs (scratch buffers).
    pub image: &'a [(u64, u8)],
    /// Values for names the body reads and nothing declares (registers).
    pub bound: &'a [(String, Value)],
    /// Memory nobody wrote during this run: the program image as the
    /// machine holds it. Without it every global read is a refusal.
    pub backing: Option<&'a dyn Fn(u64) -> Option<u8>>,
    /// Names that are globals, and their addresses. A read of one is a load
    /// and an assignment to one is a store, as in the printed C.
    pub globals: Option<&'a HashMap<String, u64>>,
    /// The declared type of each global in `globals`: the width of its
    /// loads and stores, as the printed declaration says.
    pub global_types: Option<&'a HashMap<String, NirType>>,
    /// The value the `n`th call (counting from zero) returns. `None`
    /// refuses every call that is not a modeled intrinsic.
    pub call_return: Option<&'a dyn Fn(usize) -> u64>,
    /// The bytes the `n`th call writes, given its arguments: the same stub
    /// the machine side runs, so a value cached across a call is caught.
    pub call_writes: Option<&'a dyn Fn(usize, &[u64]) -> Vec<(u64, u8)>>,
    /// Call sites that do not return. A call made from one ends the run, as
    /// it ends the machine's.
    pub no_return_sites: Option<&'a std::collections::BTreeSet<u64>>,
    /// Where each local whose address the body takes lives. Such a local is
    /// memory, not a variable: `&x` passed to a call, or `*(&x + 4)`, reads
    /// the bytes `x` is stored in. A body that takes the address of a local
    /// missing here is refused.
    pub local_addresses: Option<&'a HashMap<String, u64>>,
    /// The function's own frame below its entry stack pointer. A read of a
    /// byte there that the body (or a call it made) did not write is an
    /// [`UndefinedRead`], as a read of a variable nothing assigned is.
    pub undefined_frame: Option<&'a std::ops::Range<u64>>,
}

/// The result of one interpreted run.
#[derive(Debug, Clone, Default)]
pub struct Run {
    pub ret: Option<Value>,
    /// The run ended at a call that does not return, the last of `calls`.
    pub ended_in_no_return: bool,
    pub calls: Vec<CallEvent>,
    /// Every byte the body stored to memory, with its final value.
    pub writes: std::collections::BTreeMap<u64, u8>,
}

/// Unwinds a run that reached a call which does not return.
#[derive(Debug)]
struct NoReturnStop;

impl std::fmt::Display for NoReturnStop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("interp: reached a call that does not return")
    }
}

impl std::error::Error for NoReturnStop {}

/// A run that read something C leaves undefined: a variable before anything
/// assigned it, or a byte of the function's own frame below its entry stack
/// pointer that nothing in this run wrote. Not a refusal -- the caller weighs
/// it against whether the machine read undefined frame bytes too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndefinedRead {
    /// What was read, in words.
    pub what: String,
}

impl std::fmt::Display for UndefinedRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "interp: read of {}", self.what)
    }
}

impl std::error::Error for UndefinedRead {}

/// Generates one interpreter module for a `Stmt`/`Expr`/`LValue`/`BinaryOp`/
/// `UnaryOp`/`Binding`/`Function` family (either all `Dir*`, imported from
/// `fission_midend_prehir`, or all `Hir*` plus the shared `NirBinding`,
/// imported from `fission_midend_core::ir`). See this file's module doc for
/// why this is a macro instantiated twice rather than one generic/
/// trait-based implementation or two independently hand-written copies.
macro_rules! define_interp {
    (
        $modname:ident, $ir_crate:path,
        $Stmt:ident, $Expr:ident, $LValue:ident, $BinaryOp:ident, $UnaryOp:ident,
        $Binding:ident, $Function:ident
    ) => {
        pub mod $modname {
            use anyhow::{Result, bail};
            use $ir_crate::{$BinaryOp, $Binding, $Expr, $Function, $LValue, $Stmt, $UnaryOp};
            use fission_midend_core::ir::NirType;
            use rustc_hash::FxHashMap as HashMap;
            use std::cell::RefCell;

            use super::{CallEvent, Run, Value, World};

            #[derive(Debug, Clone)]
            enum Flow {
                Normal,
                Break,
                Continue,
                Return(Option<Value>),
                Goto(String),
            }

            /// An address's own type. Pointer arithmetic produces an address
            /// whatever it points at, so `infer_ty` needs one to hand back.
            const PTR_TY: NirType = NirType::Int {
                bits: 64,
                signed: false,
            };

            /// Sparse byte-addressable memory.
            ///
            /// Only bytes this body actually stored exist. A read that
            /// reaches an unwritten byte is an error, not a zero -- the same
            /// contract the variable environment keeps, and for the same
            /// reason: against a real emulator, an invented value is a
            /// confident wrong answer, and it can as easily make a wrong body
            /// agree as make a right one diverge.
            ///
            /// So this models memory a function *uses internally* -- stores to
            /// a stack slot or a buffer, then loads them back -- and still
            /// refuses globals and caller-owned pointers, whose contents this
            /// tier has no way to know.
            /// Bytes per page of the interpreter's memory.
            const PAGE: u64 = 4096;

            /// One page: the bytes, which of them hold a value, and which
            /// of those this run stored.
            struct Page {
                data: [u8; PAGE as usize],
                present: [u64; (PAGE / 64) as usize],
                written: [u64; (PAGE / 64) as usize],
            }

            impl Page {
                fn new() -> Box<Self> {
                    Box::new(Self {
                        data: [0; PAGE as usize],
                        present: [0; (PAGE / 64) as usize],
                        written: [0; (PAGE / 64) as usize],
                    })
                }

                fn bit(offset: u64) -> (usize, u64) {
                    ((offset / 64) as usize, 1u64 << (offset % 64))
                }
            }

            /// Sparse byte-addressable memory, in pages.
            ///
            /// Byte-keyed maps made memory the slowest thing in a batch:
            /// a scratch image is two thousand bytes placed for every
            /// sample, and every store hashed and inserted once per byte.
            #[derive(Default)]
            struct Memory<'w> {
                pages: HashMap<u64, Box<Page>>,
                /// What the machine holds where nothing here has been placed.
                backing: Option<&'w dyn Fn(u64) -> Option<u8>>,
                /// The function's own frame below its entry stack pointer: a
                /// byte there nothing here placed is undefined, not backed.
                undefined: Option<std::ops::Range<u64>>,
            }

            impl Memory<'_> {
                fn byte(&self, at: u64) -> Option<u8> {
                    let page = self.pages.get(&(at / PAGE))?;
                    let offset = at % PAGE;
                    let (word, bit) = Page::bit(offset);
                    (page.present[word] & bit != 0).then(|| page.data[offset as usize])
                }

                fn put(&mut self, at: u64, value: u8, written: bool) {
                    let page = self.pages.entry(at / PAGE).or_insert_with(Page::new);
                    let offset = at % PAGE;
                    let (word, bit) = Page::bit(offset);
                    page.data[offset as usize] = value;
                    page.present[word] |= bit;
                    if written {
                        page.written[word] |= bit;
                    }
                }

                /// Little-endian, matching every target in the corpus this
                /// verifies against.
                fn read(&self, addr: u64, bits: u32) -> Result<Value> {
                    let n = bytes_for(bits);
                    let mut raw: u64 = 0;
                    for i in 0..n {
                        let at = addr.wrapping_add(i as u64);
                        if self.byte(at).is_none()
                            && self.undefined.as_ref().is_some_and(|r| r.contains(&at))
                        {
                            return Err(anyhow::Error::new(super::UndefinedRead {
                                what: format!("frame byte {at:#x}, which nothing in this run wrote"),
                            }));
                        }
                        let b = self
                            .byte(at)
                            .or_else(|| self.backing.and_then(|read| read(at)))
                            .ok_or_else(|| {
                                anyhow::anyhow!(
                                    "interp: load from unmapped address {at:#x} -- neither \
                                     written by the body nor present in the program image"
                                )
                            })?;
                        raw |= (b as u64) << (8 * i);
                    }
                    Ok(raw as i64)
                }

                fn write(&mut self, addr: u64, bits: u32, value: Value) {
                    for i in 0..bytes_for(bits) {
                        let b = ((value as u64) >> (8 * i)) as u8;
                        self.put(addr.wrapping_add(i as u64), b, true);
                    }
                }

                /// Every byte this run stored, with its final value.
                fn stores(&self) -> std::collections::BTreeMap<u64, u8> {
                    let mut out = std::collections::BTreeMap::new();
                    for (number, page) in &self.pages {
                        for (word, bits) in page.written.iter().enumerate() {
                            let mut rest = *bits;
                            while rest != 0 {
                                let offset = word as u64 * 64 + u64::from(rest.trailing_zeros());
                                rest &= rest - 1;
                                out.insert(number * PAGE + offset, page.data[offset as usize]);
                            }
                        }
                    }
                    out
                }
            }

            /// Bytes a value of `bits` occupies, rounded up and capped at 8.
            fn bytes_for(bits: u32) -> u32 {
                bits.div_ceil(8).clamp(1, 8)
            }

            struct Env<'w> {
                values: HashMap<String, Value>,
                types: HashMap<String, NirType>,
                /// Behind a cell because a call, evaluated inside an
                /// expression, writes what its stub says it writes.
                mem: RefCell<Memory<'w>>,
                call_writes: Option<&'w dyn Fn(usize, &[u64]) -> Vec<(u64, u8)>>,
                globals: Option<&'w std::collections::HashMap<String, u64>>,
                global_types: Option<&'w std::collections::HashMap<String, NirType>>,
                call_return: Option<&'w dyn Fn(usize) -> u64>,
                no_return_sites: Option<&'w std::collections::BTreeSet<u64>>,
                local_addresses: Option<&'w std::collections::HashMap<String, u64>>,
                /// Calls made so far, in order. Behind a cell because
                /// evaluating an expression is otherwise read-only.
                calls: RefCell<Vec<CallEvent>>,
                /// Statements left to execute before this call gives up.
                ///
                /// A decompiled body is not a program anyone has checked
                /// terminates, and grounding runs it on invented arguments: a
                /// list walk handed a buffer that is not a well-formed list
                /// never reaches its null. The emulator side already bounds
                /// itself (`STEP_BUDGET`); without the same bound here one
                /// sample of one function can hang the caller indefinitely,
                /// which is exactly what `advanced_patterns`' `list_sum` did.
                ///
                /// Running out is an error, not a verdict: the caller counts
                /// it as "this tier said nothing", never as agreement.
                fuel: u64,
            }

            /// Statements one grounding call may execute.
            ///
            /// Well above anything the corpus's own functions need -- the
            /// bodies that ground here run in the low thousands -- and low
            /// enough that a non-terminating one is noticed in milliseconds.
            const STEP_FUEL: u64 = 2_000_000;

            impl Env<'_> {
                /// The address of `name` when it is a global rather than a
                /// variable of this body.
                fn global(&self, name: &str) -> Option<u64> {
                    self.globals.and_then(|globals| globals.get(name).copied())
                }

                /// The address of `name` when it is a local living in memory.
                fn local_address(&self, name: &str) -> Option<u64> {
                    self.local_addresses
                        .and_then(|addresses| addresses.get(name).copied())
                }

                fn get(&self, name: &str) -> Result<Value> {
                    if let Some(addr) = self.local_address(name) {
                        let ty = self.ty(name)?;
                        return Ok(normalize(self.mem.borrow().read(addr, width_of(ty))?, ty));
                    }
                    if let Some(v) = self.values.get(name) {
                        return Ok(*v);
                    }
                    if let Some(addr) = self.global(name) {
                        // Its declaration's width: guessing eight bytes for a
                        // 32-bit global read its neighbour into the value.
                        let ty = self.ty(name)?;
                        return Ok(normalize(self.mem.borrow().read(addr, width_of(ty))?, ty));
                    }
                    Err(anyhow::Error::new(super::UndefinedRead {
                        what: format!("undeclared variable '{name}'"),
                    }))
                }

                fn ty(&self, name: &str) -> Result<&NirType> {
                    self.types
                        .get(name)
                        .or_else(|| self.global_types.and_then(|types| types.get(name)))
                        .ok_or_else(|| {
                            anyhow::anyhow!("interp: no declared type for variable '{name}'")
                        })
                }

                fn set(&mut self, name: &str, v: Value) {
                    if let Some(addr) = self.local_address(name) {
                        let bits = self.types.get(name).map_or(64, width_of);
                        self.mem.borrow_mut().write(addr, bits, v);
                        return;
                    }
                    self.values.insert(name.to_string(), v);
                }
            }

            use super::{is_signed, normalize, width_of};

            /// `raw`'s low `bits` bits, as an unsigned value -- used for
            /// unsigned comparisons/shifts/division, where a signed `i64`
            /// reinterpretation would be wrong (e.g. comparing a `u32`
            /// `0xFFFFFFFF` should read as 4294967295, not -1).
            fn unsigned_masked(raw: i64, bits: u32) -> u64 {
                if bits >= 64 {
                    raw as u64
                } else {
                    (raw as u64) & ((1u64 << bits) - 1)
                }
            }

            /// Infer the declared `NirType` of `expr` without evaluating it
            /// -- every supported expr variant carries its own result type
            /// explicitly (`Const(_, ty)`, `Cast{ty,..}`, `Unary{ty,..}`,
            /// `Binary{ty,..}`, `Select{ty,..}`) except `Var`, which is
            /// looked up in the environment's declared param/local types.
            fn infer_ty<'a>(expr: &'a $Expr, env: &'a Env) -> Result<&'a NirType> {
                match expr {
                    $Expr::Var(name) => env.ty(name),
                    $Expr::Const(_, ty)
                    | $Expr::Cast { ty, .. }
                    | $Expr::Unary { ty, .. }
                    | $Expr::Binary { ty, .. }
                    | $Expr::Select { ty, .. }
                    | $Expr::Load { ty, .. }
                    | $Expr::FieldAccess { ty, .. } => Ok(ty),
                    // An address is an address, whatever it points at.
                    $Expr::PtrOffset { .. } => Ok(&PTR_TY),
                    // `p[i]` is the element, as the printer writes it.
                    $Expr::Index { elem_ty, .. } => Ok(elem_ty),
                    $Expr::Call { ty, .. } => Ok(ty),
                    $Expr::AddressOfGlobal(_) | $Expr::AddressOfLocal(_) => Ok(&PTR_TY),
                    other => bail!("interp: cannot infer type of unsupported expr {other:?}"),
                }
            }

            /// The address and width a store targets, or `None` for a plain variable.
            fn eval_store_target(lhs: &$LValue, env: &mut Env) -> Result<Option<(u64, u32)>> {
                match lhs {
                    $LValue::Var(_) => Ok(None),
                    $LValue::Deref { ptr, ty } => {
                        let addr = eval_expr(ptr, env)? as u64;
                        Ok(Some((addr, width_of(ty))))
                    }
                    $LValue::Index {
                        base,
                        index,
                        elem_ty,
                    } => {
                        let stride = bytes_for(width_of(elem_ty)) as i64;
                        let i = eval_expr(index, env)?;
                        let base_addr = eval_expr(base, env)?;
                        let addr = base_addr.wrapping_add(i.wrapping_mul(stride)) as u64;
                        Ok(Some((addr, width_of(elem_ty))))
                    }
                    $LValue::FieldAccess { base, offset, ty, .. } => {
                        let base_addr = eval_expr(base, env)?;
                        let addr = base_addr.wrapping_add(*offset as i64) as u64;
                        Ok(Some((addr, width_of(ty))))
                    }
                }
            }

            fn eval_expr(expr: &$Expr, env: &Env) -> Result<Value> {
                match expr {
                    $Expr::Var(name) => env.get(name),
                    $Expr::Const(v, ty) => Ok(normalize(*v, ty)),
                    $Expr::Cast { ty, expr } => Ok(normalize(eval_expr(expr, env)?, ty)),
                    $Expr::Unary { op, expr, ty } => {
                        let v = eval_expr(expr, env)?;
                        let raw = match op {
                            $UnaryOp::Neg => v.wrapping_neg(),
                            $UnaryOp::Not => {
                                if v == 0 {
                                    1
                                } else {
                                    0
                                }
                            }
                            $UnaryOp::BitNot => !v,
                        };
                        Ok(normalize(raw, ty))
                    }
                    $Expr::Binary { op, lhs, rhs, ty } => eval_binary(op, lhs, rhs, ty, env),
                    $Expr::Select {
                        cond,
                        then_expr,
                        else_expr,
                        ty,
                    } => {
                        let c = eval_expr(cond, env)?;
                        let v = if c != 0 {
                            eval_expr(then_expr, env)?
                        } else {
                            eval_expr(else_expr, env)?
                        };
                        Ok(normalize(v, ty))
                    }
                    $Expr::Load { ptr, ty } => {
                        let addr = eval_expr(ptr, env)? as u64;
                        Ok(normalize(env.mem.borrow().read(addr, width_of(ty))?, ty))
                    }
                    $Expr::PtrOffset { base, offset } => {
                        Ok((eval_expr(base, env)?).wrapping_add(*offset))
                    }
                    // `p[i]` and `p->f` read memory: the printer writes them
                    // as C subscript and member access, and that is what the
                    // printed code computes. Taking them as the element's
                    // address made every field read return the pointer.
                    $Expr::Index {
                        base,
                        index,
                        elem_ty,
                    } => {
                        let stride = bytes_for(width_of(elem_ty)) as i64;
                        let i = eval_expr(index, env)?;
                        let addr = (eval_expr(base, env)?).wrapping_add(i.wrapping_mul(stride));
                        Ok(normalize(env.mem.borrow().read(addr as u64, width_of(elem_ty))?, elem_ty))
                    }
                    $Expr::FieldAccess {
                        base, offset, ty, ..
                    } => {
                        if !matches!(infer_ty(base, env)?, NirType::Ptr(_)) {
                            bail!("interp: member access on a non-pointer base (a struct value)");
                        }
                        let addr = (eval_expr(base, env)?).wrapping_add(*offset as i64);
                        Ok(normalize(env.mem.borrow().read(addr as u64, width_of(ty))?, ty))
                    }
                    $Expr::AddressOfGlobal(name) => env.global(name).map(|a| a as i64).ok_or_else(|| {
                        anyhow::anyhow!("interp: no address for global '{name}'")
                    }),
                    $Expr::AddressOfLocal(name) => {
                        env.local_address(name).map(|a| a as i64).ok_or_else(|| {
                            anyhow::anyhow!("interp: no frame address for local '{name}'")
                        })
                    }
                    $Expr::Call {
                        origin,
                        target,
                        args,
                        ty,
                    } => {
                        if is_modeled_intrinsic(target) {
                            return eval_builtin_call(target, args, ty, env);
                        }
                        let Some(call_return) = env.call_return else {
                            return eval_builtin_call(target, args, ty, env);
                        };
                        // A `__fission_*` name stands for something the
                        // decompiler could not express, not a call the
                        // machine makes.
                        if target.starts_with("__fission_") {
                            bail!("interp: pseudo-call '{target}' has no machine counterpart");
                        }
                        // No call instruction behind it (a user p-code op, an
                        // unmodeled intrinsic): the machine side never stops
                        // there, so stubbing it would only shift every later
                        // call's return value by one.
                        let Some(site) = origin.as_ref().map(|o| o.instruction_address) else {
                            bail!("interp: call to '{target}' has no machine call instruction");
                        };
                        let arg_bits = args
                            .iter()
                            .map(|arg| infer_ty(arg, env).map(width_of).unwrap_or(64))
                            .collect();
                        let args = args
                            .iter()
                            .map(|arg| eval_expr(arg, env))
                            .collect::<Result<Vec<_>>>()?;
                        let mut calls = env.calls.borrow_mut();
                        let n = calls.len();
                        if n >= crate::emu_driver::MAX_STUBBED_CALLS {
                            bail!("interp: more than {n} calls -- the same budget the machine side stops at");
                        }
                        calls.push(CallEvent {
                            site: Some(site),
                            target: target.clone(),
                            args,
                            arg_bits,
                        });
                        if env.no_return_sites.is_some_and(|sites| sites.contains(&site)) {
                            return Err(anyhow::Error::new(super::NoReturnStop));
                        }
                        if let Some(call_writes) = env.call_writes {
                            let raw: Vec<u64> = calls[n].args.iter().map(|a| *a as u64).collect();
                            let mut mem = env.mem.borrow_mut();
                            // The callee's bytes, not the function's stores.
                            for (addr, byte) in call_writes(n, &raw) {
                                mem.put(addr, byte, false);
                            }
                        }
                        Ok(normalize(call_return(n) as i64, ty))
                    }
                    other => bail!(
                        "interp: unsupported expr {other:?} -- no memory/call model at the \
                         concrete tier, see module docs"
                    ),
                }
            }

            /// x86-flag-recovery pseudo-calls (`fission_pcode`'s own
            /// `is_pure_intrinsic_call` list: `__carry`/`__scarry`/
            /// `__sborrow`/`__popcount`) are the *only* `Call` targets
            /// evaluated -- real x86 comparisons almost never survive as a
            /// plain binary-op comparison; they go through this
            /// flag-decomposition machinery instead (`of = __sborrow(a,b);
            /// ... if (zf || xVar) ...`), so without recognizing these
            /// specific four, this tier couldn't verify almost any real
            /// x86 function with a comparison in it. Any other `Call`
            /// target still bails -- a small, fixed, well-known whitelist
            /// of *pure* intrinsics, not general interprocedural support.
            fn is_modeled_intrinsic(target: &str) -> bool {
                matches!(target, "__carry" | "__scarry" | "__sborrow" | "__popcount")
            }

            fn eval_builtin_call(
                target: &str,
                args: &[$Expr],
                ty: &NirType,
                env: &Env,
            ) -> Result<Value> {
                match target {
                    "__carry" | "__scarry" | "__sborrow" => {
                        anyhow::ensure!(args.len() == 2, "interp: {target} needs 2 args");
                        let l = eval_expr(&args[0], env)?;
                        let r = eval_expr(&args[1], env)?;
                        let bits = width_of(infer_ty(&args[0], env)?).clamp(1, 64);
                        let flag = match target {
                            "__carry" => unsigned_add_carries(l, r, bits),
                            "__scarry" => signed_add_overflows(l, r, bits),
                            "__sborrow" => signed_sub_overflows(l, r, bits),
                            _ => unreachable!(),
                        };
                        Ok(normalize(bool_to_raw(flag), ty))
                    }
                    "__popcount" => {
                        anyhow::ensure!(args.len() == 1, "interp: __popcount needs 1 arg");
                        let v = eval_expr(&args[0], env)?;
                        let bits = width_of(infer_ty(&args[0], env)?).clamp(1, 64);
                        let count = unsigned_masked(v, bits).count_ones() as i64;
                        Ok(normalize(count, ty))
                    }
                    other => bail!(
                        "interp: unsupported call target '{other}' -- only the pure x86-flag \
                         intrinsics (__carry/__scarry/__sborrow/__popcount) are modeled, see \
                         module docs"
                    ),
                }
            }

            /// Unsigned carry-out of `l + r` at `bits` width (`INT_CARRY`'s
            /// definition: CF after an unsigned add) -- computed in `u128`
            /// to sidestep width-64 wraparound edge cases entirely rather
            /// than reasoning about it bitwise.
            fn unsigned_add_carries(l: i64, r: i64, bits: u32) -> bool {
                let mask = if bits >= 64 {
                    u64::MAX
                } else {
                    (1u64 << bits) - 1
                };
                let sum = unsigned_masked(l, bits) as u128 + unsigned_masked(r, bits) as u128;
                sum > mask as u128
            }

            /// Signed overflow of `l + r` at `bits` width (`INT_SCARRY`'s
            /// definition: OF after a signed add), via `i128` arithmetic
            /// against the true signed range for `bits`.
            fn signed_add_overflows(l: i64, r: i64, bits: u32) -> bool {
                let (min, max) = signed_range(bits);
                let sum = sign_extend_i128(l, bits) + sign_extend_i128(r, bits);
                sum < min || sum > max
            }

            /// Signed overflow of `l - r` at `bits` width (`INT_SBORROW`'s
            /// definition: OF after a signed subtract / `cmp`).
            fn signed_sub_overflows(l: i64, r: i64, bits: u32) -> bool {
                let (min, max) = signed_range(bits);
                let diff = sign_extend_i128(l, bits) - sign_extend_i128(r, bits);
                diff < min || diff > max
            }

            fn signed_range(bits: u32) -> (i128, i128) {
                if bits >= 64 {
                    (i64::MIN as i128, i64::MAX as i128)
                } else {
                    (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
                }
            }

            fn sign_extend_i128(raw: i64, bits: u32) -> i128 {
                if bits >= 64 {
                    raw as i128
                } else {
                    let masked = (raw as u64) & ((1u64 << bits) - 1);
                    if masked & (1u64 << (bits - 1)) != 0 {
                        (masked as i128) - (1i128 << bits)
                    } else {
                        masked as i128
                    }
                }
            }

            fn eval_binary(
                op: &$BinaryOp,
                lhs: &$Expr,
                rhs: &$Expr,
                ty: &NirType,
                env: &Env,
            ) -> Result<Value> {
                let l = eval_expr(lhs, env)?;
                // C's `&&` and `||` do not evaluate the right operand once
                // the left decides: it may be a call or a guarded load.
                match op {
                    $BinaryOp::LogicalAnd if l == 0 => return Ok(normalize(0, ty)),
                    $BinaryOp::LogicalOr if l != 0 => return Ok(normalize(1, ty)),
                    _ => {}
                }
                let r = eval_expr(rhs, env)?;
                // Comparisons need the *operands'* width (to mask correctly
                // for unsigned comparisons); arithmetic/logical ops are
                // normalized to the *result* type `ty` after computing.
                let operand_bits = width_of(infer_ty(lhs, env)?).clamp(1, 64);
                // Where the two operands meet: C converts both to the wider
                // type before comparing or dividing. Narrowing to the left
                // operand's width instead made `(int)x == 0xFFFFFFFFLL` true
                // at x == -1, a comparison C evaluates as false. Each value is
                // already held extended by its own type's signedness, so
                // masking at the wider width compares what C compares.
                let common_bits = infer_ty(rhs, env)
                    .ok()
                    .map(|ty| width_of(ty).clamp(1, 64))
                    .map_or(operand_bits, |rhs_bits| rhs_bits.max(operand_bits));

                let raw = match op {
                    $BinaryOp::Add => l.wrapping_add(r),
                    $BinaryOp::Sub => l.wrapping_sub(r),
                    $BinaryOp::Mul => l.wrapping_mul(r),
                    $BinaryOp::Div => {
                        // Zero at the operand width, not only as a whole
                        // value: `x % (uint)0x100000000` divides by zero.
                        if r == 0 || unsigned_masked(r, common_bits) == 0 {
                            bail!("interp: division by zero");
                        }
                        if is_signed(ty) {
                            (sign_extend_i128(l, common_bits)
                                / sign_extend_i128(r, common_bits)) as i64
                        } else {
                            (unsigned_masked(l, common_bits) / unsigned_masked(r, common_bits))
                                as i64
                        }
                    }
                    $BinaryOp::Mod => {
                        // Zero at the operand width, not only as a whole
                        // value: `x % (uint)0x100000000` divides by zero.
                        if r == 0 || unsigned_masked(r, common_bits) == 0 {
                            bail!("interp: modulo by zero");
                        }
                        if is_signed(ty) {
                            (sign_extend_i128(l, common_bits)
                                % sign_extend_i128(r, common_bits)) as i64
                        } else {
                            (unsigned_masked(l, common_bits) % unsigned_masked(r, common_bits))
                                as i64
                        }
                    }
                    $BinaryOp::LogicalAnd => bool_to_raw(l != 0 && r != 0),
                    $BinaryOp::LogicalOr => bool_to_raw(l != 0 || r != 0),
                    $BinaryOp::And => l & r,
                    $BinaryOp::Or => l | r,
                    $BinaryOp::Xor => l ^ r,
                    $BinaryOp::Shl => l.wrapping_shl((r as u32) & 63),
                    $BinaryOp::Shr => {
                        (unsigned_masked(l, operand_bits) >> ((r as u32) & 63)) as i64
                    }
                    // The sign of an operand is bit `operand_bits - 1`, not bit
                    // 63 of the stored `i64`: an unsigned-typed 32-bit value is
                    // held zero-extended, so `0x8000_0000` is +2^31 there and
                    // only negative once read at its own width.
                    $BinaryOp::Sar => {
                        (sign_extend_i128(l, operand_bits) >> ((r as u32) & 63)) as i64
                    }
                    $BinaryOp::Eq => bool_to_raw(
                        unsigned_masked(l, common_bits) == unsigned_masked(r, common_bits),
                    ),
                    $BinaryOp::Ne => bool_to_raw(
                        unsigned_masked(l, common_bits) != unsigned_masked(r, common_bits),
                    ),
                    $BinaryOp::Lt => bool_to_raw(
                        unsigned_masked(l, common_bits) < unsigned_masked(r, common_bits),
                    ),
                    $BinaryOp::Le => bool_to_raw(
                        unsigned_masked(l, common_bits) <= unsigned_masked(r, common_bits),
                    ),
                    $BinaryOp::Gt => bool_to_raw(
                        unsigned_masked(l, common_bits) > unsigned_masked(r, common_bits),
                    ),
                    $BinaryOp::Ge => bool_to_raw(
                        unsigned_masked(l, common_bits) >= unsigned_masked(r, common_bits),
                    ),
                    $BinaryOp::SLt => bool_to_raw(
                        sign_extend_i128(l, common_bits) < sign_extend_i128(r, common_bits),
                    ),
                    $BinaryOp::SLe => bool_to_raw(
                        sign_extend_i128(l, common_bits) <= sign_extend_i128(r, common_bits),
                    ),
                    $BinaryOp::SGt => bool_to_raw(
                        sign_extend_i128(l, common_bits) > sign_extend_i128(r, common_bits),
                    ),
                    $BinaryOp::SGe => bool_to_raw(
                        sign_extend_i128(l, common_bits) >= sign_extend_i128(r, common_bits),
                    ),
                };
                Ok(normalize(raw, ty))
            }

            fn bool_to_raw(b: bool) -> i64 {
                if b { 1 } else { 0 }
            }

            /// One unit of work. Charged per statement and per loop
            /// iteration: a loop whose body is empty (or only `continue`)
            /// executes no statement, so charging statements alone let
            /// `while (c) {}` run forever.
            fn charge(env: &mut Env) -> Result<()> {
                env.fuel = env.fuel.checked_sub(1).ok_or_else(|| {
                    anyhow::anyhow!(
                        "interp: body did not finish within {STEP_FUEL} statements"
                    )
                })?;
                Ok(())
            }

            fn exec_stmt(stmt: &$Stmt, env: &mut Env) -> Result<Flow> {
                charge(env)?;
                match stmt {
                    $Stmt::Assign { lhs, rhs } => {
                        // Stores go to memory; only a plain variable falls
                        // through to the environment below.
                        if let Some((addr, bits)) = eval_store_target(lhs, env)? {
                            let v = eval_expr(rhs, env)?;
                            env.mem.borrow_mut().write(addr, bits, v);
                            return Ok(Flow::Normal);
                        }
                        let name = match lhs {
                            $LValue::Var(name) => name,
                            other => bail!(
                                "interp: unsupported assignment target {other:?}"
                            ),
                        };
                        // A real function's declared locals don't
                        // necessarily list every name its body assigns --
                        // e.g. a return-value scaffold slot can be
                        // assigned without a matching binding entry.
                        // Rather than bailing on every such case, infer
                        // the type from the RHS and register it as a
                        // fresh variable the first time it's assigned.
                        let ty = match env.ty(name) {
                            Ok(ty) => ty.clone(),
                            Err(_) => infer_ty(rhs, env)?.clone(),
                        };
                        let v = normalize(eval_expr(rhs, env)?, &ty);
                        if let Some(addr) = env.global(name) {
                            env.mem.borrow_mut().write(addr, width_of(&ty), v);
                            env.types.insert(name.clone(), ty);
                            return Ok(Flow::Normal);
                        }
                        env.types.insert(name.clone(), ty);
                        env.set(name, v);
                        Ok(Flow::Normal)
                    }
                    $Stmt::Expr(e) => {
                        eval_expr(e, env)?;
                        Ok(Flow::Normal)
                    }
                    $Stmt::VaStart { .. } => {
                        bail!("interp: variadic functions are not supported at the concrete tier")
                    }
                    $Stmt::Block(stmts) => exec_block(stmts, env),
                    $Stmt::Switch {
                        expr,
                        cases,
                        default,
                    } => {
                        let v = eval_expr(expr, env)?;
                        let body = cases
                            .iter()
                            .find(|c| c.values.contains(&v))
                            .map(|c| c.body.as_slice())
                            .unwrap_or(default.as_slice());
                        match exec_block(body, env)? {
                            // `Break` exits the nearest enclosing loop
                            // *or* switch -- consumed here, same as a
                            // loop consumes its own `Break`.
                            Flow::Break => Ok(Flow::Normal),
                            other => Ok(other),
                        }
                    }
                    $Stmt::If {
                        cond,
                        then_body,
                        else_body,
                    } => {
                        if eval_expr(cond, env)? != 0 {
                            exec_block(then_body, env)
                        } else {
                            exec_block(else_body, env)
                        }
                    }
                    $Stmt::While { cond, body } => loop {
                        charge(env)?;
                        if eval_expr(cond, env)? == 0 {
                            return Ok(Flow::Normal);
                        }
                        match exec_block(body, env)? {
                            Flow::Normal | Flow::Continue => continue,
                            Flow::Break => return Ok(Flow::Normal),
                            other => return Ok(other),
                        }
                    },
                    $Stmt::DoWhile { body, cond } => loop {
                        charge(env)?;
                        match exec_block(body, env)? {
                            Flow::Normal | Flow::Continue => {}
                            Flow::Break => return Ok(Flow::Normal),
                            other => return Ok(other),
                        }
                        if eval_expr(cond, env)? == 0 {
                            return Ok(Flow::Normal);
                        }
                    },
                    $Stmt::For {
                        init,
                        cond,
                        update,
                        body,
                    } => {
                        if let Some(init) = init {
                            match exec_stmt(init, env)? {
                                Flow::Normal => {}
                                other => return Ok(other),
                            }
                        }
                        loop {
                            charge(env)?;
                            if let Some(cond) = cond {
                                if eval_expr(cond, env)? == 0 {
                                    return Ok(Flow::Normal);
                                }
                            }
                            match exec_block(body, env)? {
                                Flow::Normal | Flow::Continue => {}
                                Flow::Break => return Ok(Flow::Normal),
                                other => return Ok(other),
                            }
                            if let Some(update) = update {
                                match exec_stmt(update, env)? {
                                    Flow::Normal => {}
                                    other => return Ok(other),
                                }
                            }
                        }
                    }
                    $Stmt::Label(_) => Ok(Flow::Normal),
                    $Stmt::Goto(label) => Ok(Flow::Goto(label.clone())),
                    $Stmt::Return(expr) => match expr {
                        Some(e) => Ok(Flow::Return(Some(eval_expr(e, env)?))),
                        None => Ok(Flow::Return(None)),
                    },
                    $Stmt::Break => Ok(Flow::Break),
                    $Stmt::Continue => Ok(Flow::Continue),
                }
            }

            /// Execute a statement list, resolving any `Goto` whose target
            /// `Label` is in `stmts` itself (at any position -- forward or
            /// backward) by moving the cursor there; a `Goto` whose label
            /// isn't found here is propagated to the caller, which tries
            /// its own (enclosing) statement list next. This is what lets
            /// a label at function-body scope catch a `goto` issued from
            /// inside a deeply nested `If`/`Block`.
            fn exec_block(stmts: &[$Stmt], env: &mut Env) -> Result<Flow> {
                let mut idx = 0usize;
                while idx < stmts.len() {
                    match exec_stmt(&stmts[idx], env)? {
                        Flow::Normal => idx += 1,
                        Flow::Goto(label) => match find_label(stmts, &label) {
                            Some(target) => idx = target + 1,
                            None => return Ok(Flow::Goto(label)),
                        },
                        other => return Ok(other),
                    }
                }
                Ok(Flow::Normal)
            }

            fn find_label(stmts: &[$Stmt], label: &str) -> Option<usize> {
                stmts
                    .iter()
                    .position(|s| matches!(s, $Stmt::Label(l) if l == label))
            }

            /// Interpret with a preloaded memory image: `(address, bytes)`
            /// placed before execution starts.
            ///
            /// For grounding a function that takes a pointer. The emulator is
            /// given the same bytes at the same address
            /// (`EmulatorHarness::install_scratch`); the two memories agree
            /// only because both were told the same thing, so these must be
            /// kept in step.
            pub fn interpret_with_memory(
                body: &[$Stmt],
                params: &[$Binding],
                locals: &[$Binding],
                args: &[i64],
                image: &[(u64, u8)],
            ) -> Result<Option<Value>> {
                interpret_inner(body, params, locals, args, image, &[])
            }

            /// As [`interpret_with_memory`], plus named values pre-bound in the
            /// environment -- machine registers the body reads without ever
            /// writing, taken from the emulator that is about to run the same
            /// call.
            pub fn interpret_with_state(
                body: &[$Stmt],
                params: &[$Binding],
                locals: &[$Binding],
                args: &[i64],
                image: &[(u64, u8)],
                bound: &[(String, Value)],
            ) -> Result<Option<Value>> {
                interpret_inner(body, params, locals, args, image, bound)
            }

            fn collect_expr_reads(
                expr: &$Expr,
                reads: &mut Vec<String>,
                addressed: &mut Vec<String>,
            ) {
                match expr {
                    $Expr::AddressOfLocal(name) => {
                        if !addressed.contains(name) {
                            addressed.push(name.clone());
                        }
                    }
                    $Expr::Var(name) => {
                        if !name.is_empty() && !reads.contains(name) {
                            reads.push(name.clone());
                        }
                    }
                    $Expr::Unary { expr, .. } | $Expr::Cast { expr, .. } => {
                        collect_expr_reads(expr, reads, addressed);
                    }
                    $Expr::Binary { lhs, rhs, .. } => {
                        collect_expr_reads(lhs, reads, addressed);
                        collect_expr_reads(rhs, reads, addressed);
                    }
                    $Expr::Select {
                        cond,
                        then_expr,
                        else_expr,
                        ..
                    } => {
                        collect_expr_reads(cond, reads, addressed);
                        collect_expr_reads(then_expr, reads, addressed);
                        collect_expr_reads(else_expr, reads, addressed);
                    }
                    $Expr::Call { args, .. } => {
                        for arg in args {
                            collect_expr_reads(arg, reads, addressed);
                        }
                    }
                    $Expr::Load { ptr, .. } => {
                        collect_expr_reads(ptr, reads, addressed);
                    }
                    $Expr::PtrOffset { base, .. } => {
                        collect_expr_reads(base, reads, addressed);
                    }
                    $Expr::Index { base, index, .. } => {
                        collect_expr_reads(base, reads, addressed);
                        collect_expr_reads(index, reads, addressed);
                    }
                    $Expr::FieldAccess { base, .. } => {
                        collect_expr_reads(base, reads, addressed);
                    }
                    $Expr::AggregateCopy { src, .. } => {
                        collect_expr_reads(src, reads, addressed);
                    }
                    _ => {}
                }
            }

            fn collect_stmt_names(
                stmt: &$Stmt,
                reads: &mut Vec<String>,
                assigned: &mut Vec<String>,
                addressed: &mut Vec<String>,
            ) {
                match stmt {
                    $Stmt::Assign { lhs, rhs } => {
                        match lhs {
                            $LValue::Var(name) => {
                                if !name.is_empty() && !assigned.contains(name) {
                                    assigned.push(name.clone());
                                }
                            }
                            $LValue::Deref { ptr, .. } => {
                                collect_expr_reads(ptr, reads, addressed);
                            }
                            $LValue::Index { base, index, .. } => {
                                collect_expr_reads(base, reads, addressed);
                                collect_expr_reads(index, reads, addressed);
                            }
                            $LValue::FieldAccess { base, .. } => {
                                collect_expr_reads(base, reads, addressed);
                            }
                        }
                        collect_expr_reads(rhs, reads, addressed);
                    }
                    $Stmt::Expr(e) => {
                        collect_expr_reads(e, reads, addressed);
                    }
                    $Stmt::Block(stmts) => {
                        for s in stmts.iter() {
                            collect_stmt_names(s, reads, assigned, addressed);
                        }
                    }
                    $Stmt::Switch { expr, cases, default } => {
                        collect_expr_reads(expr, reads, addressed);
                        for c in cases {
                            for s in c.body.iter() {
                                collect_stmt_names(s, reads, assigned, addressed);
                            }
                        }
                        for s in default.iter() {
                            collect_stmt_names(s, reads, assigned, addressed);
                        }
                    }
                    $Stmt::If { cond, then_body, else_body } => {
                        collect_expr_reads(cond, reads, addressed);
                        for s in then_body.iter() {
                            collect_stmt_names(s, reads, assigned, addressed);
                        }
                        for s in else_body.iter() {
                            collect_stmt_names(s, reads, assigned, addressed);
                        }
                    }
                    $Stmt::While { cond, body } => {
                        collect_expr_reads(cond, reads, addressed);
                        for s in body.iter() {
                            collect_stmt_names(s, reads, assigned, addressed);
                        }
                    }
                    $Stmt::DoWhile { body, cond } => {
                        for s in body.iter() {
                            collect_stmt_names(s, reads, assigned, addressed);
                        }
                        collect_expr_reads(cond, reads, addressed);
                    }
                    $Stmt::For { init, cond, update, body } => {
                        if let Some(init) = init {
                            collect_stmt_names(init, reads, assigned, addressed);
                        }
                        if let Some(cond) = cond {
                            collect_expr_reads(cond, reads, addressed);
                        }
                        if let Some(update) = update {
                            collect_stmt_names(update, reads, assigned, addressed);
                        }
                        for s in body.iter() {
                            collect_stmt_names(s, reads, assigned, addressed);
                        }
                    }
                    $Stmt::Return(Some(e)) => {
                        collect_expr_reads(e, reads, addressed);
                    }
                    _ => {}
                }
            }

            /// Every name the body reads that nothing declares or assigns --
            /// the set worth asking the machine about.
            ///
            /// Deliberately over-inclusive: a name already bound is simply
            /// overwritten by the declaration, and one the machine does not
            /// recognise is dropped.
            pub fn free_names(func: &$Function) -> Vec<String> {
                let mut reads: Vec<String> = Vec::new();
                let mut assigned: Vec<String> = Vec::new();
                let mut addressed: Vec<String> = Vec::new();
                for stmt in &func.body {
                    collect_stmt_names(stmt, &mut reads, &mut assigned, &mut addressed);
                }
                reads.retain(|n| {
                    !assigned.contains(n)
                        && !func.params.iter().any(|p| &p.name == n)
                        && !func.locals.iter().any(|l| &l.name == n)
                });
                reads
            }

            /// Every local whose address the body or a local's initializer
            /// takes (`&x`): such a local lives in memory, where a pointer to
            /// it reads and writes the same bytes the name does.
            pub fn addressed_locals(body: &[$Stmt], locals: &[$Binding]) -> Vec<String> {
                let mut reads: Vec<String> = Vec::new();
                let mut assigned: Vec<String> = Vec::new();
                let mut addressed: Vec<String> = Vec::new();
                for stmt in body {
                    collect_stmt_names(stmt, &mut reads, &mut assigned, &mut addressed);
                }
                for init in locals.iter().filter_map(|l| l.initializer.as_ref()) {
                    collect_expr_reads(init, &mut reads, &mut addressed);
                }
                addressed
            }

            /// Every name the body reads or assigns.
            pub fn mentioned_names(func: &$Function) -> Vec<String> {
                let mut reads: Vec<String> = Vec::new();
                let mut assigned: Vec<String> = Vec::new();
                let mut addressed: Vec<String> = Vec::new();
                for stmt in &func.body {
                    collect_stmt_names(stmt, &mut reads, &mut assigned, &mut addressed);
                }
                reads.extend(assigned);
                reads
            }

            /// Interpret `body` with `args` bound to `params` in order and
            /// `locals` seeded from their initializer expression when they
            /// have one -- an uninitialized local stays unbound, so reading it
            /// before assignment is an error rather than a zero -- and return
            /// the function's return value (`None` for
            /// a bare `return;`, or if control fell off the end of the
            /// body without an explicit `Return`).
            pub fn interpret(
                body: &[$Stmt],
                params: &[$Binding],
                locals: &[$Binding],
                args: &[i64],
            ) -> Result<Option<Value>> {
                interpret_inner(body, params, locals, args, &[], &[])
            }

            fn interpret_inner(
                body: &[$Stmt],
                params: &[$Binding],
                locals: &[$Binding],
                args: &[i64],
                image: &[(u64, u8)],
                bound: &[(String, Value)],
            ) -> Result<Option<Value>> {
                let world = World {
                    image,
                    bound,
                    ..World::default()
                };
                Ok(run(body, params, locals, args, &world)?.ret)
            }

            /// Interpret `body` in `world`: what it returns, every call it
            /// makes and every byte it stores.
            pub fn run(
                body: &[$Stmt],
                params: &[$Binding],
                locals: &[$Binding],
                args: &[i64],
                world: &World<'_>,
            ) -> Result<Run> {
                let image = world.image;
                let bound = world.bound;
                anyhow::ensure!(
                    args.len() == params.len(),
                    "interp: {} args given, function has {} params",
                    args.len(),
                    params.len()
                );
                let mut mem = Memory {
                    backing: world.backing,
                    undefined: world.undefined_frame.cloned(),
                    ..Memory::default()
                };
                for (addr, byte) in image {
                    mem.put(*addr, *byte, false);
                }
                let mut env = Env {
                    values: HashMap::default(),
                    types: HashMap::default(),
                    mem: RefCell::new(mem),
                    call_writes: world.call_writes,
                    globals: world.globals,
                    global_types: world.global_types,
                    call_return: world.call_return,
                    no_return_sites: world.no_return_sites,
                    local_addresses: world.local_addresses,
                    calls: RefCell::new(Vec::new()),
                    fuel: STEP_FUEL,
                };
                // Machine state first, so a real declaration below wins.
                for (name, v) in bound {
                    env.types.insert(name.clone(), PTR_TY);
                    env.values.insert(name.clone(), *v);
                }
                for (p, a) in params.iter().zip(args) {
                    env.types.insert(p.name.clone(), p.ty.clone());
                    env.values.insert(p.name.clone(), normalize(*a, &p.ty));
                }
                for l in locals {
                    env.types.insert(l.name.clone(), l.ty.clone());
                    // Only a local with an initializer gets a value. An
                    // uninitialized one stays unbound, so `Env::get` refuses a
                    // read that reaches it before any assignment -- which is
                    // this tier's whole contract: bail rather than invent.
                    //
                    // Seeding those to 0 made the interpreter answer for
                    // programs it cannot model. `mingw_get_invalid_parameter_handler`
                    // decompiles to `return tmp_140007190;` -- a global read
                    // recovered as an uninitialized temp -- and the zero made
                    // it look like a confident answer that disagreed with the
                    // machine, rather than the "cannot check this" it is. The
                    // same default could equally have made a wrong body agree.
                    if let Some(init) = &l.initializer {
                        let v = normalize(eval_expr(init, &env)?, &l.ty);
                        env.set(&l.name, v);
                    }
                }
                // A global the body names keeps its declared type for the
                // width of its loads, but never a value of its own.
                if let Some(globals) = world.globals {
                    for name in globals.keys() {
                        env.values.remove(name);
                    }
                }
                let (ret, ended_in_no_return) = match exec_block(body, &mut env) {
                    Ok(Flow::Return(v)) => (v, false),
                    Ok(Flow::Normal) => (None, false),
                    Ok(other) => {
                        bail!("interp: body ended in unexpected control state {other:?}")
                    }
                    Err(err) if err.is::<super::NoReturnStop>() => (None, true),
                    Err(err) => return Err(err),
                };
                let writes = env.mem.borrow().stores();
                Ok(Run {
                    ret,
                    ended_in_no_return,
                    calls: env.calls.into_inner(),
                    writes,
                })
            }

            /// Interpret a whole `$Function`, using its own `params`/
            /// `locals`/`body` -- the usual entry point (see [`interpret`]
            /// for the lower-level, explicit-parts version
            /// `crate::diff::diff_prehir_hir` uses to let PreHIR and HIR share
            /// one differential check even though their `params`/`locals`
            /// come from independently-typed structs).
            #[allow(dead_code)]
            pub fn interpret_function(
                func: &$Function,
                args: &[i64],
            ) -> Result<Option<Value>> {
                interpret(&func.body, &func.params, &func.locals, args)
            }
        }
    };
}

define_interp!(
    prehir,
    fission_midend_prehir,
    PreHirStmt,
    PreHirExpr,
    PreHirLValue,
    PreHirBinaryOp,
    PreHirUnaryOp,
    PreHirBinding,
    PreHirFunction
);
define_interp!(
    hir,
    fission_midend_core::ir,
    HirStmt,
    HirExpr,
    HirLValue,
    HirBinaryOp,
    HirUnaryOp,
    NirBinding,
    HirFunction
);

pub use hir::addressed_locals as hir_addressed_locals;
pub use hir::free_names as hir_free_names;
pub use hir::interpret as interpret_hir;
pub use hir::interpret_with_memory as interpret_hir_with_memory;
pub use hir::interpret_with_state as interpret_hir_with_state;
pub use hir::mentioned_names as hir_mentioned_names;
pub use hir::run as run_hir;
pub use prehir::addressed_locals as prehir_addressed_locals;
pub use prehir::free_names as prehir_free_names;
pub use prehir::interpret as interpret_prehir;
pub use prehir::interpret_with_memory as interpret_prehir_with_memory;
pub use prehir::interpret_with_state as interpret_prehir_with_state;
pub use prehir::run as run_prehir;

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_core::ir::{HirBinaryOp, HirExpr, HirLValue, HirStmt, NirBinding, NirType};

    fn u32_const(v: i64) -> HirExpr {
        HirExpr::Const(
            v,
            NirType::Int {
                bits: 32,
                signed: false,
            },
        )
    }

    fn binary(op: HirBinaryOp, l: HirExpr, r: HirExpr, ty: NirType) -> HirExpr {
        HirExpr::Binary {
            op,
            lhs: Box::new(l),
            rhs: Box::new(r),
            ty,
        }
    }

    /// A value is stored zero-extended when its type is unsigned, so a signed
    /// operation on a 32-bit operand must read the sign from bit 31 rather than
    /// from bit 63 of the stored `i64`.
    #[test]
    fn signed_comparisons_use_the_operand_width_not_sixty_four_bits() {
        let boolean = NirType::Bool;
        let int_min = u32_const(0x8000_0000);
        let zero = u32_const(0);
        let eval = |op| {
            interpret_hir(
                &[HirStmt::Return(Some(binary(
                    op,
                    int_min.clone(),
                    zero.clone(),
                    boolean.clone(),
                )))],
                &[],
                &[],
                &[],
            )
            .expect("interpret")
        };
        // 0x80000000 as a signed 32-bit value is negative.
        assert_eq!(eval(HirBinaryOp::SLt), Some(1));
        assert_eq!(eval(HirBinaryOp::SLe), Some(1));
        assert_eq!(eval(HirBinaryOp::SGt), Some(0));
        assert_eq!(eval(HirBinaryOp::SGe), Some(0));
        // Positive control: the unsigned comparison of the same bits is the
        // opposite, and a small positive pair is unaffected.
        assert_eq!(eval(HirBinaryOp::Gt), Some(1));
        let small = interpret_hir(
            &[HirStmt::Return(Some(binary(
                HirBinaryOp::SLt,
                u32_const(1),
                u32_const(2),
                boolean,
            )))],
            &[],
            &[],
            &[],
        )
        .expect("interpret");
        assert_eq!(small, Some(1));
    }

    #[test]
    fn arithmetic_shift_right_replicates_the_operand_sign_bit() {
        let result_ty = NirType::Int {
            bits: 32,
            signed: false,
        };
        // 0x80000000 >> 4 arithmetically, as a 32-bit value, is 0xF8000000.
        let value = interpret_hir(
            &[HirStmt::Return(Some(binary(
                HirBinaryOp::Sar,
                u32_const(0x8000_0000),
                u32_const(4),
                result_ty,
            )))],
            &[],
            &[],
            &[],
        )
        .expect("interpret");
        assert_eq!(value, Some(0xF800_0000));
    }

    #[test]
    fn local_initializer_is_normalized_to_declared_type() {
        // A local initialized with 0xff into a signed 8-bit integer must normalize to -1.
        let locals = vec![NirBinding {
            name: "x".to_string(),
            ty: NirType::Int {
                bits: 8,
                signed: true,
            },
            surface_type_name: None,
            origin: None,
            initializer: Some(HirExpr::Const(
                0xff,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            )),
        }];
        let body = vec![HirStmt::Return(Some(HirExpr::Var("x".to_string())))];
        let val = interpret_hir(&body, &[], &locals, &[]).expect("interpret");
        assert_eq!(val, Some(-1));
    }

    #[test]
    fn store_target_handles_index_lvalue() {
        // Allocate an address (e.g. 0x1000) into a local pointer `base`.
        // Perform `base[2] = 42` (where elem_ty is 32-bit int, stride = 4, target addr = 0x1008).
        // Then read back `base[2]` -- the element, not its address.
        let locals = vec![NirBinding {
            name: "base".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Int {
                bits: 32,
                signed: true,
            })),
            surface_type_name: None,
            origin: None,
            initializer: Some(HirExpr::Const(
                0x1000,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
        }];
        let elem_ty = NirType::Int {
            bits: 32,
            signed: true,
        };
        let body = vec![
            HirStmt::Assign {
                lhs: HirLValue::Index {
                    base: Box::new(HirExpr::Var("base".to_string())),
                    index: Box::new(HirExpr::Const(
                        2,
                        NirType::Int {
                            bits: 32,
                            signed: true,
                        },
                    )),
                    elem_ty: elem_ty.clone(),
                },
                rhs: HirExpr::Const(42, elem_ty.clone()),
            },
            HirStmt::Return(Some(HirExpr::Index {
                base: Box::new(HirExpr::Var("base".to_string())),
                index: Box::new(HirExpr::Const(
                    2,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                )),
                elem_ty,
            })),
        ];
        let val = interpret_hir(&body, &[], &locals, &[]).expect("interpret");
        assert_eq!(val, Some(42));
    }

    #[test]
    fn store_target_handles_field_access_lvalue() {
        // Allocate address 0x2000 into a local pointer `st`.
        // Perform `st.field = 99` at offset 16 (where ty is 64-bit int, target addr = 0x2010).
        // Then read back `st.field`.
        let locals = vec![NirBinding {
            name: "st".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: None,
            initializer: Some(HirExpr::Const(
                0x2000,
                NirType::Int {
                    bits: 64,
                    signed: false,
                },
            )),
        }];
        let ty = NirType::Int {
            bits: 64,
            signed: false,
        };
        let body = vec![
            HirStmt::Assign {
                lhs: HirLValue::FieldAccess {
                    base: Box::new(HirExpr::Var("st".to_string())),
                    field_name: "field".to_string(),
                    offset: 16,
                    ty: ty.clone(),
                },
                rhs: HirExpr::Const(99, ty.clone()),
            },
            HirStmt::Return(Some(HirExpr::FieldAccess {
                base: Box::new(HirExpr::Var("st".to_string())),
                field_name: "field".to_string(),
                offset: 16,
                ty,
            })),
        ];
        let val = interpret_hir(&body, &[], &locals, &[]).expect("interpret");
        assert_eq!(val, Some(99));
    }
}

#[cfg(test)]
mod world_tests {
    use super::*;
    use fission_midend_core::ir::{CallOrigin, HirExpr, HirLValue, HirStmt, NirBinding, NirType};

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn local(name: &str) -> NirBinding {
        NirBinding {
            name: name.to_string(),
            ty: u32_ty(),
            surface_type_name: None,
            origin: None,
            initializer: None,
        }
    }

    /// `g += 1; return g;` where `g` is the global at 0x5000 holding 41 in
    /// the program image: the read is a load from the image, the assignment
    /// a store this run reports.
    #[test]
    fn a_global_is_memory_read_from_the_image_and_stored_back() {
        let body = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("g".to_string()),
                rhs: HirExpr::Binary {
                    op: fission_midend_core::ir::HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("g".to_string())),
                    rhs: Box::new(HirExpr::Const(1, u32_ty())),
                    ty: u32_ty(),
                },
            },
            HirStmt::Return(Some(HirExpr::Var("g".to_string()))),
        ];
        let image_byte = |addr: u64| (addr == 0x5000).then_some(41u8).or(Some(0));
        let globals: HashMap<String, u64> = [("g".to_string(), 0x5000)].into_iter().collect();
        let world = World {
            backing: Some(&image_byte),
            globals: Some(&globals),
            ..World::default()
        };
        let run = run_hir(&body, &[], &[local("g")], &[], &world).expect("run");
        assert_eq!(run.ret, Some(42));
        assert_eq!(run.writes.get(&0x5000), Some(&42));
        // Control: the same body without the globals map reads an
        // unassigned variable and is refused rather than guessed.
        assert!(run_hir(&body, &[], &[local("g")], &[], &World::default()).is_err());
    }

    /// `while (1) {}`, `do {} while (1);` and `for (;;) {}` execute no
    /// statement per iteration; each must still run out of fuel and be
    /// refused instead of spinning. Run on a thread so a regression fails
    /// the test instead of hanging the suite.
    #[test]
    fn a_loop_with_an_empty_body_runs_out_of_fuel() {
        let one = || HirExpr::Const(1, u32_ty());
        let loops = vec![
            HirStmt::While {
                cond: one(),
                body: vec![],
            },
            HirStmt::DoWhile {
                body: vec![HirStmt::Continue],
                cond: one(),
            },
            HirStmt::For {
                init: None,
                cond: None,
                update: None,
                body: vec![],
            },
        ];
        for stmt in loops {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let body = vec![stmt, HirStmt::Return(Some(HirExpr::Const(0, u32_ty())))];
                let _ = tx.send(run_hir(&body, &[], &[], &[], &World::default()).is_err());
            });
            let refused = rx
                .recv_timeout(std::time::Duration::from_secs(20))
                .expect("an empty-bodied infinite loop must not spin forever");
            assert!(refused, "an infinite loop has no result to report");
        }
    }

    /// `x = f(7); return x + g(x);`: both calls are recorded with their
    /// sites and arguments, and each returns what the world says the nth
    /// call returns.
    #[test]
    fn calls_are_recorded_in_order_and_return_the_stubbed_values() {
        let call = |site: u64, target: &str, arg: HirExpr| HirExpr::Call {
            origin: Some(CallOrigin {
                instruction_address: site,
                pcode_sequence: 0,
            }),
            target: target.to_string(),
            args: vec![arg],
            ty: u32_ty(),
        };
        let body = vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("x".to_string()),
                rhs: call(0x100, "f", HirExpr::Const(7, u32_ty())),
            },
            HirStmt::Return(Some(HirExpr::Binary {
                op: fission_midend_core::ir::HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("x".to_string())),
                rhs: Box::new(call(0x108, "g", HirExpr::Var("x".to_string()))),
                ty: u32_ty(),
            })),
        ];
        let returns = |n: usize| [10u64, 20][n];
        let world = World {
            call_return: Some(&returns),
            ..World::default()
        };
        let run = run_hir(&body, &[], &[local("x")], &[], &world).expect("run");
        assert_eq!(run.ret, Some(30));
        assert_eq!(
            run.calls,
            vec![
                CallEvent {
                    site: Some(0x100),
                    target: "f".to_string(),
                    args: vec![7],
                    arg_bits: vec![32],
                },
                CallEvent {
                    site: Some(0x108),
                    target: "g".to_string(),
                    args: vec![10],
                    arg_bits: vec![32],
                },
            ]
        );
        // Control: without a call model the same body is refused.
        assert!(run_hir(&body, &[], &[local("x")], &[], &World::default()).is_err());
    }

    fn call_at(site: u64, target: &str) -> HirExpr {
        HirExpr::Call {
            origin: Some(CallOrigin {
                instruction_address: site,
                pcode_sequence: 0,
            }),
            target: target.to_string(),
            args: vec![],
            ty: u32_ty(),
        }
    }

    fn binary(op: fission_midend_core::ir::HirBinaryOp, lhs: HirExpr, rhs: HirExpr) -> HirExpr {
        HirExpr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            ty: u32_ty(),
        }
    }

    /// `return 0 && f();` and `return 1 || f();`: the right operand never
    /// runs in C, so the call is not made and does not take a stub result.
    #[test]
    fn logical_operators_short_circuit_before_a_call() {
        use fission_midend_core::ir::HirBinaryOp::{LogicalAnd, LogicalOr};
        let returns = |n: usize| 7 + n as u64;
        let world = World {
            call_return: Some(&returns),
            ..World::default()
        };
        for (op, left, value) in [(LogicalAnd, 0, 0), (LogicalOr, 1, 1)] {
            let body = vec![HirStmt::Return(Some(binary(
                op,
                HirExpr::Const(left, u32_ty()),
                call_at(0x100, "f"),
            )))];
            let run = run_hir(&body, &[], &[], &[], &world).expect("run");
            assert_eq!(run.ret, Some(value));
            assert!(run.calls.is_empty(), "{op:?}: {:?}", run.calls);
        }
        // Control: when the left operand does not decide, the call runs.
        let body = vec![HirStmt::Return(Some(binary(
            LogicalAnd,
            HirExpr::Const(1, u32_ty()),
            call_at(0x100, "f"),
        )))];
        assert_eq!(
            run_hir(&body, &[], &[], &[], &world)
                .expect("run")
                .calls
                .len(),
            1
        );
    }

    /// `x % d` where `d` is not zero but is zero at the width the two
    /// operands meet (`0x1_0000_0000` held in a 32-bit `unsigned`): refused,
    /// where it panicked.
    #[test]
    fn a_divisor_that_is_zero_at_the_operand_width_is_refused() {
        for op in [
            fission_midend_core::ir::HirBinaryOp::Mod,
            fission_midend_core::ir::HirBinaryOp::Div,
        ] {
            let body = vec![HirStmt::Return(Some(binary(
                op,
                HirExpr::Const(5, u32_ty()),
                HirExpr::Const(0x1_0000_0000, u32_ty()),
            )))];
            let outcome = std::panic::catch_unwind(|| {
                run_hir(&body, &[], &[], &[], &World::default()).is_err()
            });
            assert_eq!(outcome.ok(), Some(true), "{op:?}");
        }
    }

    /// A wider divisor is not narrowed to the dividend: C converts both to
    /// the wider type, so `5u % 0x100000000ull` is 5 and `5u / ...` is 0.
    /// Narrowing made it a division by zero; a wider divisor that is not a
    /// multiple of 2^32 would have divided by the wrong number.
    #[test]
    fn a_wider_divisor_divides_at_its_own_width() {
        let wide = NirType::Int {
            bits: 64,
            signed: false,
        };
        for (op, want) in [
            (fission_midend_core::ir::HirBinaryOp::Mod, 5),
            (fission_midend_core::ir::HirBinaryOp::Div, 0),
        ] {
            let body = vec![HirStmt::Return(Some(binary(
                op,
                HirExpr::Const(5, u32_ty()),
                HirExpr::Const(0x1_0000_0003, wide.clone()),
            )))];
            let got = run_hir(&body, &[], &[], &[], &World::default())
                .expect("run")
                .ret;
            assert_eq!(got, Some(want), "{op:?}");
        }
    }

    /// `return f() == 7;` needs the call's type to compare at; it used to be
    /// refused as an expression whose type cannot be inferred.
    #[test]
    fn a_call_result_has_the_type_the_call_declares() {
        let returns = |_| 7;
        let world = World {
            call_return: Some(&returns),
            ..World::default()
        };
        let body = vec![HirStmt::Return(Some(binary(
            fission_midend_core::ir::HirBinaryOp::Eq,
            call_at(0x100, "f"),
            HirExpr::Const(7, u32_ty()),
        )))];
        assert_eq!(
            run_hir(&body, &[], &[], &[], &world).expect("run").ret,
            Some(1)
        );
    }

    /// A call with no machine call instruction behind it (a user p-code op)
    /// is refused: the machine never stops there, so stubbing it would shift
    /// every later call's stub result.
    #[test]
    fn a_call_without_a_machine_call_instruction_is_refused() {
        let returns = |_| 0;
        let world = World {
            call_return: Some(&returns),
            ..World::default()
        };
        let pseudo = HirExpr::Call {
            origin: None,
            target: "user_op".to_string(),
            args: vec![],
            ty: u32_ty(),
        };
        assert!(run_hir(&[HirStmt::Expr(pseudo)], &[], &[], &[], &world).is_err());
        // Control: the same call with an instruction behind it is stubbed.
        assert!(run_hir(&[HirStmt::Expr(call_at(0x100, "f"))], &[], &[], &[], &world).is_ok());
    }

    /// `if (g) return 1; return 0;` with `g` a 32-bit global holding 0 next to
    /// one holding 1: the read is four bytes, as declared. Reading a global
    /// with no declared type as eight bytes made this branch taken.
    #[test]
    fn a_global_is_read_at_its_declared_width() {
        let image_byte = |addr: u64| Some(if addr == 0x5004 { 1 } else { 0 });
        let globals: HashMap<String, u64> = [("g".to_string(), 0x5000)].into_iter().collect();
        let types: HashMap<String, NirType> = [("g".to_string(), u32_ty())].into_iter().collect();
        let body = vec![
            HirStmt::If {
                cond: HirExpr::Var("g".to_string()),
                then_body: vec![HirStmt::Return(Some(HirExpr::Const(1, u32_ty())))],
                else_body: vec![],
            },
            HirStmt::Return(Some(HirExpr::Const(0, u32_ty()))),
        ];
        let world = World {
            backing: Some(&image_byte),
            globals: Some(&globals),
            global_types: Some(&types),
            ..World::default()
        };
        assert_eq!(
            run_hir(&body, &[], &[], &[], &world).expect("run").ret,
            Some(0)
        );
        // Control: with no declared type the read is refused, not guessed.
        let untyped = World {
            global_types: None,
            ..world
        };
        assert!(run_hir(&body, &[], &[], &[], &untyped).is_err());
    }

    /// `*p = 1; exit(0); return 5;`: the run stops at the call that does not
    /// return, as the machine's does, and keeps the store made before it.
    #[test]
    fn a_call_that_does_not_return_ends_the_run_and_keeps_earlier_stores() {
        let returns = |_| 0;
        let sites: std::collections::BTreeSet<u64> = [0x200].into_iter().collect();
        let body = vec![
            HirStmt::Assign {
                lhs: HirLValue::Deref {
                    ptr: Box::new(HirExpr::Const(0x6000, u32_ty())),
                    ty: u32_ty(),
                },
                rhs: HirExpr::Const(1, u32_ty()),
            },
            HirStmt::Expr(call_at(0x200, "exit")),
            HirStmt::Return(Some(HirExpr::Const(5, u32_ty()))),
        ];
        let world = World {
            call_return: Some(&returns),
            no_return_sites: Some(&sites),
            ..World::default()
        };
        let run = run_hir(&body, &[], &[], &[], &world).expect("run");
        assert!(run.ended_in_no_return);
        assert_eq!(run.ret, None);
        assert_eq!(run.writes.get(&0x6000), Some(&1));
        // Control: a call that returns lets the body go on to its return.
        let returning = World {
            no_return_sites: None,
            ..world
        };
        let run = run_hir(&body, &[], &[], &[], &returning).expect("run");
        assert!(!run.ended_in_no_return);
        assert_eq!(run.ret, Some(5));
    }

    #[test]
    fn signed_sub_64_div_mod_and_mixed_sign_eq_ne_use_operand_width() {
        let i32_ty = || NirType::Int {
            bits: 32,
            signed: true,
        };
        let i64_ty = || NirType::Int {
            bits: 64,
            signed: true,
        };
        let eval_one =
            |op: fission_midend_core::ir::HirBinaryOp, l: HirExpr, r: HirExpr, ty: NirType| {
                let body = vec![HirStmt::Return(Some(HirExpr::Binary {
                    op,
                    lhs: Box::new(l),
                    rhs: Box::new(r),
                    ty,
                }))];
                run_hir(&body, &[], &[], &[], &World::default())
                    .expect("run")
                    .ret
            };

        // Signed 32-bit div/mod when operands come from unsigned-stored u32 bit patterns
        // (`0xFFFF_FFFF` -> `-1` as i32) or trigger 32-bit `i32::MIN / -1` without panicking.
        assert_eq!(
            eval_one(
                fission_midend_core::ir::HirBinaryOp::Div,
                HirExpr::Const(-6, u32_ty()),
                HirExpr::Const(2, u32_ty()),
                i32_ty(),
            ),
            Some(-3)
        );
        assert_eq!(
            eval_one(
                fission_midend_core::ir::HirBinaryOp::Mod,
                HirExpr::Const(-7, u32_ty()),
                HirExpr::Const(4, u32_ty()),
                i32_ty(),
            ),
            Some(-3)
        );
        assert_eq!(
            eval_one(
                fission_midend_core::ir::HirBinaryOp::Div,
                HirExpr::Const(i64::MIN, i64_ty()),
                HirExpr::Const(-1, i64_ty()),
                i64_ty(),
            ),
            Some(i64::MIN)
        );

        // A wider right operand: C converts both to it, so `(int)-1` is
        // `-1LL`, not `0xFFFFFFFFLL`, and the comparison is false. Masking
        // to the left operand's width called it true.
        assert_eq!(
            eval_one(
                fission_midend_core::ir::HirBinaryOp::Eq,
                HirExpr::Const(-1, i32_ty()),
                HirExpr::Const(0xFFFF_FFFF, i64_ty()),
                NirType::Bool,
            ),
            Some(0)
        );
        assert_eq!(
            eval_one(
                fission_midend_core::ir::HirBinaryOp::Lt,
                HirExpr::Const(0xFFFF_FFFFu32 as i64, u32_ty()),
                HirExpr::Const(
                    0x1_0000_0000,
                    NirType::Int {
                        bits: 64,
                        signed: false
                    }
                ),
                NirType::Bool,
            ),
            Some(1),
            "0xFFFFFFFFu < 0x100000000ull"
        );
        assert_eq!(
            eval_one(
                fission_midend_core::ir::HirBinaryOp::SLt,
                HirExpr::Const(5, i32_ty()),
                HirExpr::Const(0x1_0000_0000, i64_ty()),
                NirType::Bool,
            ),
            Some(1),
            "5 < 0x100000000LL"
        );

        // Mixed signed/unsigned 32-bit equality compares masked 32-bit values (`-1_i32 == 0xFFFF_FFFF_u32`).
        assert_eq!(
            eval_one(
                fission_midend_core::ir::HirBinaryOp::Eq,
                HirExpr::Const(-1, i32_ty()),
                HirExpr::Const(0xFFFF_FFFFu32 as i64, u32_ty()),
                NirType::Bool,
            ),
            Some(1)
        );
        assert_eq!(
            eval_one(
                fission_midend_core::ir::HirBinaryOp::Ne,
                HirExpr::Const(-1, i32_ty()),
                HirExpr::Const(0xFFFF_FFFFu32 as i64, u32_ty()),
                NirType::Bool,
            ),
            Some(0)
        );
    }
}

#[cfg(test)]
mod free_names_tests {
    use super::*;
    use fission_midend_core::ir::{HirExpr, HirFunction, HirLValue, HirStmt, NirType};

    #[test]
    fn test_free_names_typed_ast_visitor() {
        let func = HirFunction {
            body: vec![
                // Assigned local temporary: should NOT be in free_names
                HirStmt::Assign {
                    lhs: HirLValue::Var("uVar1".to_string()),
                    rhs: HirExpr::Const(
                        10,
                        NirType::Int {
                            bits: 32,
                            signed: true,
                        },
                    ),
                },
                // Read of free variable "RBX" (e.g. caller saved register)
                HirStmt::Assign {
                    lhs: HirLValue::Var("uVar2".to_string()),
                    rhs: HirExpr::Var("RBX".to_string()),
                },
                HirStmt::Return(Some(HirExpr::Var("uVar1".to_string()))),
            ],
            params: Vec::new(),
            locals: Vec::new(),
            ..Default::default()
        };

        let free = hir_free_names(&func);
        assert_eq!(free, vec!["RBX".to_string()]);
    }
}
