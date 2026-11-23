//! Calls out of the slice: what is known about each callee, and how a call is
//! observed.
//!
//! A call is not a refusal but an observable effect. Both sides stub the
//! callee the way `verify --tier ground-truth` does: the slice's P-code
//! summary stops at the call, and the emulator stops at the call instruction
//! (`fission_dir::EmulatorHarness::call_stubbed`). What the call shows the
//! outside world is its target and the argument registers it is given; what
//! it returns is chosen by the environment -- an extra input port of the
//! step, one per call in dynamic order -- so the quotient holds for every
//! value a callee could return.
//!
//! What a callee may do besides is decided from facts, never guessed:
//!
//! - **no-return**: the static no-return analysis proves it never returns
//!   (`fission_dir::call_sites`); the step ends there and the machine halts;
//! - **library**: its name has a prototype in the signature library
//!   (`fission-signatures`, resolved through `PathConfig`); its arity is
//!   known, so exactly its parameters are observed;
//! - **unknown**: neither; every argument register the boundary defines is
//!   observed, and an address of the context or the frame in any argument
//!   register stops the summary, because the callee may write through it.

use std::collections::{BTreeMap, BTreeSet};

use fission_loader::loader::LoadedBinary;
use fission_pcode_ir::{PcodeFunction, PcodeOpcode};
use serde::Serialize;

/// Calls one step may make before the summary gives up: each is an input
/// port of its own.
pub const MAX_CALLS_PER_STEP: usize = 8;

/// What the signature library says about a callee.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prototype {
    /// Declared parameters, in order. A variadic prototype lists only its
    /// fixed prefix.
    pub params: Vec<Param>,
    pub variadic: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub type_name: String,
    /// The width the type names, when the type is informative and an
    /// integer or a pointer of known size.
    pub bits: Option<u32>,
}

/// What is known about the callee at one call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallFact {
    /// Where execution continues once the callee returns; `None` when the
    /// static no-return analysis proves it does not.
    pub returns: bool,
    /// The callee's name, from the symbol table or the PLT.
    pub callee: Option<String>,
    pub prototype: Option<Prototype>,
}

/// The call facts of one function, read from its machine code and the
/// binary's symbols. `error` is set when the call sites could not be listed
/// completely; a slice that makes a call is then unsupported with it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallFacts {
    /// Call instruction address -> fact.
    pub sites: BTreeMap<u64, CallFact>,
    /// Direct jumps out of the function (tail calls), with their callee.
    pub tail_jumps: BTreeMap<u64, CallFact>,
    /// The start of every instruction the machine-code decode reached --
    /// including those without P-code of their own (a `nop`), which a
    /// branch may still target.
    pub instructions: BTreeSet<u64>,
    pub error: Option<String>,
}

impl CallFacts {
    /// Facts for the function at `entry`, whose lifted P-code is `pcode`.
    #[must_use]
    pub fn for_function(binary: &LoadedBinary, entry: u64, pcode: &PcodeFunction) -> Self {
        let flow = match fission_dir::call_sites(binary, entry) {
            Ok(flow) => flow,
            Err(error) => {
                return Self {
                    error: Some(error.to_string()),
                    ..Self::default()
                };
            }
        };
        // Direct targets as the P-code names them: a CALL's destination, and
        // a BRANCH that leaves the function.
        let mut targets = BTreeMap::new();
        for block in &pcode.blocks {
            for op in &block.ops {
                if matches!(op.opcode, PcodeOpcode::Call | PcodeOpcode::Branch)
                    && let Some(target) = op.inputs.first()
                    && !target.is_constant
                    && target.space_id != 0
                {
                    targets.entry(op.address).or_insert(target.offset);
                }
            }
        }
        let fact = |site: u64, returns: bool| {
            let callee = targets
                .get(&site)
                .and_then(|target| callee_name(binary, *target));
            let prototype = callee
                .as_deref()
                .and_then(|name| prototype(name, binary.is_64bit));
            CallFact {
                returns,
                callee,
                prototype,
            }
        };
        Self {
            sites: flow
                .calls
                .iter()
                .map(|(site, continuation)| (*site, fact(*site, continuation.is_some())))
                .collect(),
            tail_jumps: flow
                .tail_jumps
                .iter()
                .map(|site| (*site, fact(*site, true)))
                .collect(),
            instructions: flow.instructions.clone(),
            error: None,
        }
    }
}

/// The name a call target goes by: a function symbol, or the import a PLT
/// stub jumps through. Symbol versions (`@GLIBC_2.2.5`) are dropped.
fn callee_name(binary: &LoadedBinary, target: u64) -> Option<String> {
    let name = binary
        .function_at_exact(target)
        .map(|function| function.name.clone())
        .or_else(|| binary.inner().iat_symbols.get(&target).cloned())?;
    let name = name.split('@').next().unwrap_or("").trim().to_string();
    (!name.is_empty() && !name.starts_with("sub_") && !name.starts_with("FUN_")).then_some(name)
}

/// The prototype the signature library holds for `name`.
fn prototype(name: &str, is_64bit: bool) -> Option<Prototype> {
    let signature = fission_signatures::provider::SIGNATURE_RESOURCES
        .api_signature_for_bitness(name, is_64bit)?;
    let variadic = signature.is_variadic
        || fission_signatures::api_types::is_known_variadic_runtime_symbol(name);
    let pointer_bits = if is_64bit { 64 } else { 32 };
    Some(Prototype {
        params: signature
            .params
            .iter()
            .map(|param| Param {
                type_name: param.type_name.clone(),
                bits: type_bits(&param.type_name, pointer_bits),
            })
            .collect(),
        variadic,
    })
}

/// The width a parameter type names, when the signature library's type
/// string can be trusted to mean it. Its extractor writes `int` for any type
/// it could not resolve (a lost `FILE *` among them), so `int` says nothing.
fn type_bits(type_name: &str, pointer_bits: u32) -> Option<u32> {
    let text = type_name.trim();
    if text.contains('*') {
        return Some(pointer_bits);
    }
    if !fission_signatures::api_types::type_name_is_informative(text) {
        return None;
    }
    let words = text
        .split_whitespace()
        .filter(|word| !matches!(*word, "const" | "volatile" | "signed" | "unsigned"))
        .collect::<Vec<_>>();
    match words.as_slice() {
        ["char"] | ["uchar"] | ["byte"] | ["bool"] | ["_Bool"] => Some(8),
        ["short"] | ["short", "int"] | ["ushort"] | ["word"] => Some(16),
        ["long", "long"] | ["long", "long", "int"] | ["longlong"] | ["ulonglong"] => Some(64),
        ["size_t"] | ["ssize_t"] | ["intptr_t"] | ["uintptr_t"] | ["ptrdiff_t"] => {
            Some(pointer_bits)
        }
        ["uint8_t"] | ["int8_t"] => Some(8),
        ["uint16_t"] | ["int16_t"] => Some(16),
        ["uint32_t"] | ["int32_t"] | ["uint"] | ["dword"] => Some(32),
        ["uint64_t"] | ["int64_t"] | ["qword"] => Some(64),
        _ => None,
    }
}

/// How a callee is treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalleeClass {
    /// Proven not to return: the step ends at the call.
    NoReturn,
    /// A prototype from the signature library.
    Library,
    /// Neither.
    Unknown,
}

impl CallFact {
    #[must_use]
    pub fn class(&self) -> CalleeClass {
        if !self.returns {
            CalleeClass::NoReturn
        } else if self.prototype.is_some() {
            CalleeClass::Library
        } else {
            CalleeClass::Unknown
        }
    }
}

/// How one call site is observed. Both the summary and the emulator oracle
/// read a call through this, so they observe the same thing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallSiteModel {
    #[serde(serialize_with = "crate::report::hex_address")]
    pub site: u64,
    #[serde(serialize_with = "crate::report::hex_address")]
    pub target: u64,
    pub callee: Option<String>,
    pub class: CalleeClass,
    /// A jump out of the slice: the callee's return is the step's return.
    pub tail: bool,
    /// Bits observed of each argument register, in convention order; 0
    /// where the register is not observed.
    pub arg_bits: Vec<u32>,
}

impl CallSiteModel {
    /// The observed arguments, from every argument register's raw value.
    #[must_use]
    pub fn observe(&self, registers: &[u64]) -> Vec<u64> {
        self.arg_bits
            .iter()
            .zip(registers)
            .filter(|(bits, _)| **bits > 0)
            .map(|(bits, value)| value & crate::expr::mask(*bits))
            .collect()
    }
}

/// Every call site a summary observed, by call instruction address.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallModel {
    pub sites: BTreeMap<u64, CallSiteModel>,
}

impl CallModel {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }

    /// Callee names with a no-return or tail role, for the assumptions.
    #[must_use]
    pub fn classes(&self) -> BTreeSet<CalleeClass> {
        self.sites.values().map(|site| site.class).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_the_library_could_not_resolve_has_no_width() {
        assert_eq!(type_bits("int", 64), None);
        assert_eq!(type_bits("const char *", 64), Some(64));
        assert_eq!(type_bits("unsigned char", 64), Some(8));
        assert_eq!(type_bits("size_t", 32), Some(32));
    }
}
