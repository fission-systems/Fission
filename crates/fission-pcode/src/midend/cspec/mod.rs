//! Ghidra-style `.cspec` (Compiler Specification) parser for Fission.
//!
//! # Ghidra Design
//!
//! Ghidra's `CompilerSpec` reads XML from a `.cspec` file and associates it with a
//! `SleighLanguage`. Register names in `<prototype>` entries (e.g. `<register name="RDI"/>`)
//! are resolved against the language's compiled register table backed by `ELEM_VARNODE_SYM`
//! in the `.sla` file.
//!
//! # Fission Design
//!
//! We follow the same two-step pipeline:
//! 1. Parse `.cspec` XML to extract register **names** in prototype input/output/killedbycall/unaffected.
//! 2. Resolve names to `(offset, size)` via the `SlaRegisterMap` (populated from `ELEM_VARNODE_SYM`).
//!
//! Callers who already have the `CompiledSlaTemplateLibrary` can call
//! [`CspecDocument::load_and_resolve`] to get a fully resolved [`ResolvedCspec`].
//!
//! Callers must populate cspec fields from `utils/sleigh-specs/languages` before ABI-sensitive
//! lowering (see [`apply::apply_cspec_for_options`]).

use crate::midend::HashMap;
use std::path::Path;

pub mod apply;
pub mod dwarf_regs;
pub mod ldefs;
pub mod loader;
pub mod pspec;
pub mod register_model;
pub use register_model::{
    RegisterModel, RegisterNamer, register_model_for_abi, register_model_for_language,
    register_namer_for_abi, register_namer_from_options,
};
mod slaspec_parse;
#[cfg(test)]
pub(crate) mod test_maps;

/// A name → (offset_in_register_space, size_in_bytes) lookup table.
/// Populated from `ELEM_VARNODE_SYM` in the compiled `.sla` artifact.
pub type SlaRegisterMap = HashMap<String, (u64, u32)>;

// ── Raw .cspec structures ─────────────────────────────────────────────────────

/// A `<pentry>` within a prototype — either a named register or a stack slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CspecPentry {
    /// `<pentry><register name="RDI"/></pentry>` — a named hardware register.
    Register {
        name: String,
        /// Optional `metatype` from parent `<pentry>` (e.g. `"float"`). None means any.
        metatype: Option<String>,
        /// Optional `storage` from parent `<pentry>` (e.g. `"hiddenret"`).
        storage: Option<String>,
    },
    /// `<pentry><addr space="stack" offset="8"/></pentry>` — a stack slot.
    Stack { offset: i64 },
}

/// A `<prototype>` block within a `<compiler_spec>`.
#[derive(Debug, Clone)]
pub struct CspecPrototype {
    /// Prototype name, e.g. `"__stdcall"`, `"__cdecl"`, `"MSABI"`.
    pub name: String,
    /// Stack shift applied after CALL (size of return address on stack).
    pub extrapop: i64,
    /// Ordered input parameter locations.
    pub input: Vec<CspecPentry>,
    /// Ordered output (return value) locations.
    pub output: Vec<CspecPentry>,
    /// Registers preserved across calls (callee-saved / unaffected).
    pub unaffected: Vec<String>,
    /// Registers clobbered by call (caller-saved / killed-by-call).
    pub killedbycall: Vec<String>,
}

/// Parsed contents of a `.cspec` file.
#[derive(Debug, Clone)]
pub struct CspecDocument {
    /// Register name used as the stack pointer (from `<stackpointer register="..."/>`).
    pub stackpointer: Option<String>,
    /// The default calling convention prototype (from `<default_proto>`).
    pub default_proto: Option<CspecPrototype>,
    /// All named prototypes (including the default).
    pub prototypes: Vec<CspecPrototype>,
}

// ── Resolved structures ───────────────────────────────────────────────────────

/// A resolved prototype — register names have been translated to REGISTER-space offsets.
#[derive(Debug, Clone)]
pub struct ResolvedPrototype {
    pub name: String,
    /// Ordered integer (non-float) parameter register offsets, in REGISTER space.
    pub int_param_offsets: Vec<u64>,
    /// Integer return register offset in REGISTER space (primary slot).
    pub return_offset: Option<u64>,
    /// Float/double return register offset in REGISTER space (e.g. x86's
    /// `ST0`), from the `<output>` prototype's `metatype="float"` pentry.
    /// Distinct from `return_offset` -- a function can return through either
    /// depending on its return type, and both can be live in the same
    /// prototype (x86's own `x86gcc.cspec` lists ST0 *and* EAX as separate
    /// `<output>` pentries for exactly this reason).
    pub float_return_offset: Option<u64>,
    /// Stack pointer offset in REGISTER space.
    pub stack_pointer_offset: Option<u64>,
    /// Byte offset where stack parameters begin (from stack `<pentry>`).
    pub stack_arg_base: Option<i64>,
    /// Return-address size on stack (`extrapop` / `stackshift` from the prototype).
    pub extrapop: i64,
    /// Callee-saved register offsets.
    pub unaffected_offsets: Vec<u64>,
    /// Caller-saved register offsets.
    pub killedbycall_offsets: Vec<u64>,
}

/// A fully resolved `.cspec` document: register names → REGISTER-space offsets.
#[derive(Debug, Clone)]
pub struct ResolvedCspec {
    pub stack_pointer_offset: Option<u64>,
    pub default_proto: Option<ResolvedPrototype>,
}

// ── Parser ────────────────────────────────────────────────────────────────────

impl CspecDocument {
    /// Parse a `.cspec` file from disk.
    pub fn parse_file(path: &Path) -> Option<Self> {
        let contents = std::fs::read_to_string(path).ok()?;
        Self::parse_str(&contents)
    }

    /// Parse a `.cspec` XML string.
    pub fn parse_str(xml: &str) -> Option<Self> {
        Some(CspecParser::new(xml).parse())
    }

    /// Parse the `.cspec` file and resolve register names using `reg_map`.
    pub fn load_and_resolve(path: &Path, reg_map: &SlaRegisterMap) -> Option<ResolvedCspec> {
        let doc = Self::parse_file(path)?;
        Some(doc.resolve(reg_map))
    }

    /// Resolve register names to offsets using the provided `SlaRegisterMap`.
    pub fn resolve(&self, reg_map: &SlaRegisterMap) -> ResolvedCspec {
        let sp_offset = self
            .stackpointer
            .as_deref()
            .and_then(|name| resolve_reg_offset(name, reg_map));

        let default_proto = self
            .default_proto
            .as_ref()
            .map(|proto| resolve_prototype(proto, sp_offset, reg_map));

        ResolvedCspec {
            stack_pointer_offset: sp_offset,
            default_proto,
        }
    }
}

// ── Resolution helpers ────────────────────────────────────────────────────────

fn resolve_reg_offset(name: &str, reg_map: &SlaRegisterMap) -> Option<u64> {
    // Try exact name first, then case variants (.cspec uses uppercase, SLA may vary).
    reg_map
        .get(name)
        .or_else(|| reg_map.get(&name.to_ascii_uppercase()))
        .or_else(|| reg_map.get(&name.to_ascii_lowercase()))
        .map(|(offset, _size)| *offset)
}

fn resolve_prototype(
    proto: &CspecPrototype,
    sp_offset: Option<u64>,
    reg_map: &SlaRegisterMap,
) -> ResolvedPrototype {
    let int_param_offsets = proto
        .input
        .iter()
        .filter_map(|pentry| {
            if let CspecPentry::Register {
                name,
                metatype,
                storage,
            } = pentry
            {
                if metatype.as_deref() == Some("float") || storage.as_deref() == Some("float") {
                    return None;
                }
                if storage.as_deref() == Some("hiddenret") {
                    return None;
                }
                resolve_reg_offset(name, reg_map)
            } else {
                None
            }
        })
        .collect();

    let stack_arg_base = proto.input.iter().find_map(|pentry| {
        if let CspecPentry::Stack { offset } = pentry {
            Some(*offset)
        } else {
            None
        }
    });

    let return_offset = proto.output.iter().find_map(|pentry| {
        if let CspecPentry::Register {
            name,
            metatype,
            storage,
            ..
        } = pentry
        {
            if metatype.as_deref() == Some("float") || storage.as_deref() == Some("float") {
                return None;
            }
            resolve_reg_offset(name, reg_map)
        } else {
            None
        }
    });

    let float_return_offset = proto.output.iter().find_map(|pentry| {
        if let CspecPentry::Register { name, metatype, .. } = pentry {
            if metatype.as_deref() != Some("float") {
                return None;
            }
            resolve_reg_offset(name, reg_map)
        } else {
            None
        }
    });

    let unaffected_offsets = proto
        .unaffected
        .iter()
        .filter_map(|name| resolve_reg_offset(name, reg_map))
        .collect();

    let killedbycall_offsets = proto
        .killedbycall
        .iter()
        .filter_map(|name| resolve_reg_offset(name, reg_map))
        .collect();

    ResolvedPrototype {
        name: proto.name.clone(),
        int_param_offsets,
        return_offset,
        float_return_offset,
        stack_pointer_offset: sp_offset,
        stack_arg_base,
        extrapop: proto.extrapop,
        unaffected_offsets,
        killedbycall_offsets,
    }
}

// ── State-machine XML parser ──────────────────────────────────────────────────
// We use a hand-written state machine because:
// 1. Zero external deps (xml-rs / quick-xml are not in scope).
// 2. .cspec files are small, well-formed XML — no need for a full parser.
// 3. We need to track parent–child context (pentry → register / addr).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseState {
    Root,
    DefaultProto,
    Prototype,
    Input,
    Output,
    Unaffected,
    KilledByCall,
    /// Inside a `<pentry>` — accumulates `metatype` for next register/addr child.
    Pentry,
}

struct CspecParser<'a> {
    xml: &'a str,
    state_stack: Vec<ParseState>,
    state: ParseState,

    // Accumulated document
    stackpointer: Option<String>,
    default_proto: Option<CspecPrototype>,
    prototypes: Vec<CspecPrototype>,
    cur_proto: Option<CspecPrototype>,
    cur_pentry_metatype: Option<String>,
    cur_pentry_storage: Option<String>,
    cur_io: Option<IoKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IoKind {
    Input,
    Output,
}

impl<'a> CspecParser<'a> {
    fn new(xml: &'a str) -> Self {
        Self {
            xml,
            state_stack: Vec::new(),
            state: ParseState::Root,
            stackpointer: None,
            default_proto: None,
            prototypes: Vec::new(),
            cur_proto: None,
            cur_pentry_metatype: None,
            cur_pentry_storage: None,
            cur_io: None,
        }
    }

    fn parse(mut self) -> CspecDocument {
        let tokens = tokenize_xml(self.xml);
        for token in &tokens {
            match token {
                XmlToken::Open { name, attrs } => self.handle_open(name, attrs),
                XmlToken::Close { name } => self.handle_close(name),
                XmlToken::SelfClose { name, attrs } => {
                    self.handle_open(name, attrs);
                    self.handle_close(name);
                }
            }
        }
        CspecDocument {
            stackpointer: self.stackpointer,
            default_proto: self.default_proto,
            prototypes: self.prototypes,
        }
    }

    fn handle_open(&mut self, name: &str, attrs: &HashMap<String, String>) {
        match name {
            "stackpointer" => {
                if let Some(reg) = attrs.get("register") {
                    self.stackpointer = Some(reg.clone());
                }
            }
            "default_proto" => {
                self.push(ParseState::DefaultProto);
            }
            "prototype" if matches!(self.state, ParseState::Root | ParseState::DefaultProto) => {
                let proto_name = attrs.get("name").cloned().unwrap_or_default();
                let extrapop = attrs
                    .get("extrapop")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                self.cur_proto = Some(CspecPrototype {
                    name: proto_name,
                    extrapop,
                    input: Vec::new(),
                    output: Vec::new(),
                    unaffected: Vec::new(),
                    killedbycall: Vec::new(),
                });
                self.push(ParseState::Prototype);
            }
            "input" if self.state == ParseState::Prototype => {
                self.cur_io = Some(IoKind::Input);
                self.push(ParseState::Input);
            }
            "output" if self.state == ParseState::Prototype => {
                self.cur_io = Some(IoKind::Output);
                self.push(ParseState::Output);
            }
            "unaffected" if self.state == ParseState::Prototype => {
                self.push(ParseState::Unaffected);
            }
            "killedbycall" if self.state == ParseState::Prototype => {
                self.push(ParseState::KilledByCall);
            }
            "pentry" if matches!(self.state, ParseState::Input | ParseState::Output) => {
                self.cur_pentry_metatype = attrs.get("metatype").cloned();
                self.cur_pentry_storage = attrs.get("storage").cloned();
                self.push(ParseState::Pentry);
            }
            "register" if self.state == ParseState::Pentry => {
                if let Some(reg_name) = attrs.get("name") {
                    let pentry = CspecPentry::Register {
                        name: reg_name.clone(),
                        metatype: self.cur_pentry_metatype.clone(),
                        storage: self.cur_pentry_storage.clone(),
                    };
                    if let Some(proto) = self.cur_proto.as_mut() {
                        match self.cur_io {
                            Some(IoKind::Input) => proto.input.push(pentry),
                            Some(IoKind::Output) => proto.output.push(pentry),
                            None => {}
                        }
                    }
                }
            }
            "addr" if self.state == ParseState::Pentry => {
                if attrs.get("space").map_or(false, |s| s == "stack") {
                    let offset: i64 = attrs
                        .get("offset")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                    if let Some(proto) = self.cur_proto.as_mut() {
                        if self.cur_io == Some(IoKind::Input) {
                            proto.input.push(CspecPentry::Stack { offset });
                        }
                    }
                }
            }
            "register"
                if matches!(
                    self.state,
                    ParseState::Unaffected | ParseState::KilledByCall
                ) =>
            {
                if let Some(reg_name) = attrs.get("name") {
                    if let Some(proto) = self.cur_proto.as_mut() {
                        match self.state {
                            ParseState::Unaffected => proto.unaffected.push(reg_name.clone()),
                            ParseState::KilledByCall => proto.killedbycall.push(reg_name.clone()),
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_close(&mut self, name: &str) {
        match name {
            "default_proto" if self.state == ParseState::DefaultProto => {
                self.pop();
            }
            "prototype" if self.state == ParseState::Prototype => {
                self.pop();
                if let Some(proto) = self.cur_proto.take() {
                    let is_default = self.state == ParseState::DefaultProto
                        || matches!(
                            self.state_stack.last().copied(),
                            Some(ParseState::DefaultProto)
                        )
                        || self.default_proto.is_none() && self.prototypes.is_empty();
                    if is_default {
                        self.default_proto = Some(proto.clone());
                    }
                    self.prototypes.push(proto);
                }
            }
            "input" if self.state == ParseState::Input => {
                self.cur_io = None;
                self.pop();
            }
            "output" if self.state == ParseState::Output => {
                self.cur_io = None;
                self.pop();
            }
            "unaffected" if self.state == ParseState::Unaffected => {
                self.pop();
            }
            "killedbycall" if self.state == ParseState::KilledByCall => {
                self.pop();
            }
            "pentry" if self.state == ParseState::Pentry => {
                self.cur_pentry_metatype = None;
                self.cur_pentry_storage = None;
                self.pop();
            }
            _ => {}
        }
    }

    fn push(&mut self, new_state: ParseState) {
        self.state_stack.push(self.state);
        self.state = new_state;
    }

    fn pop(&mut self) {
        if let Some(prev) = self.state_stack.pop() {
            self.state = prev;
        }
    }
}

// ── Tokenizer ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
enum XmlToken {
    Open {
        name: String,
        attrs: HashMap<String, String>,
    },
    Close {
        name: String,
    },
    SelfClose {
        name: String,
        attrs: HashMap<String, String>,
    },
}

fn tokenize_xml(xml: &str) -> Vec<XmlToken> {
    let mut tokens = Vec::new();
    let bytes = xml.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        i += 1; // skip '<'
        // Skip comments and processing instructions.
        if i < bytes.len() && (bytes[i] == b'?' || bytes[i] == b'!') {
            // Find matching '>'
            while i < bytes.len() && bytes[i] != b'>' {
                i += 1;
            }
            i += 1;
            continue;
        }
        let Some(end) = xml[i..].find('>') else {
            break;
        };
        let inner = xml[i..i + end].trim();
        i += end + 1;

        if inner.is_empty() {
            continue;
        }

        let closing = inner.starts_with('/');
        if closing {
            let name = inner[1..].trim().to_string();
            tokens.push(XmlToken::Close { name });
            continue;
        }

        let self_closing = inner.ends_with('/');
        let body = if self_closing {
            inner[..inner.len() - 1].trim()
        } else {
            inner
        };

        let (name, attrs) = parse_tag_body(body);
        if name.is_empty() {
            continue;
        }
        if self_closing {
            tokens.push(XmlToken::SelfClose { name, attrs });
        } else {
            tokens.push(XmlToken::Open { name, attrs });
        }
    }
    tokens
}

fn parse_tag_body(body: &str) -> (String, HashMap<String, String>) {
    let name_end = body
        .char_indices()
        .find_map(|(idx, ch)| ch.is_whitespace().then_some(idx))
        .unwrap_or(body.len());
    let name = body[..name_end].to_string();
    let mut attrs = HashMap::default();
    let mut rest = body[name_end..].trim_start();

    while !rest.is_empty() {
        let Some(eq_idx) = rest.find('=') else { break };
        let key = rest[..eq_idx].trim().to_string();
        let after_eq = rest[eq_idx + 1..].trim_start();
        if !after_eq.starts_with('"') {
            break;
        }
        let after_quote = &after_eq[1..];
        let Some(val_end) = after_quote.find('"') else {
            break;
        };
        let value = after_quote[..val_end].to_string();
        if !key.is_empty() {
            attrs.insert(key, value);
        }
        rest = after_quote[val_end + 1..].trim_start();
    }
    (name, attrs)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Approximate x86-64 SLA register offsets (from ELEM_VARNODE_SYM in the .sla file).
    fn make_x64_reg_map() -> SlaRegisterMap {
        let mut m = SlaRegisterMap::default();
        m.insert("RAX".into(), (0x00, 8));
        m.insert("RCX".into(), (0x08, 8));
        m.insert("RDX".into(), (0x10, 8));
        m.insert("RBX".into(), (0x18, 8));
        m.insert("RSP".into(), (0x20, 8));
        m.insert("RBP".into(), (0x28, 8));
        m.insert("RSI".into(), (0x30, 8));
        m.insert("RDI".into(), (0x38, 8));
        m.insert("R8".into(), (0x80, 8));
        m.insert("R9".into(), (0x88, 8));
        m.insert("R10".into(), (0x90, 8));
        m.insert("R11".into(), (0x98, 8));
        m.insert("R12".into(), (0xa0, 8));
        m.insert("R13".into(), (0xa8, 8));
        m.insert("R14".into(), (0xb0, 8));
        m.insert("R15".into(), (0xb8, 8));
        m
    }

    /// Smoke test with a fragment that matches the actual .cspec structure (pentry > register).
    #[test]
    fn parses_sysv_gcc_fragment() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<compiler_spec>
  <stackpointer register="RSP" space="ram"/>
  <default_proto>
    <prototype name="__stdcall" extrapop="8" stackshift="8">
      <input>
        <pentry minsize="4" maxsize="8" metatype="float">
          <register name="XMM0_Qa"/>
        </pentry>
        <pentry minsize="1" maxsize="8">
          <register name="RDI"/>
        </pentry>
        <pentry minsize="1" maxsize="8">
          <register name="RSI"/>
        </pentry>
        <pentry minsize="1" maxsize="8">
          <register name="RDX"/>
        </pentry>
        <pentry minsize="1" maxsize="8">
          <register name="RCX"/>
        </pentry>
        <pentry minsize="1" maxsize="8">
          <register name="R8"/>
        </pentry>
        <pentry minsize="1" maxsize="8">
          <register name="R9"/>
        </pentry>
        <pentry minsize="1" maxsize="500" align="8">
          <addr offset="8" space="stack"/>
        </pentry>
      </input>
      <output>
        <pentry minsize="1" maxsize="8">
          <register name="RAX"/>
        </pentry>
      </output>
      <unaffected>
        <register name="RBX"/>
        <register name="RSP"/>
        <register name="RBP"/>
        <register name="R12"/>
        <register name="R13"/>
        <register name="R14"/>
        <register name="R15"/>
      </unaffected>
      <killedbycall>
        <register name="RAX"/>
        <register name="RDX"/>
      </killedbycall>
    </prototype>
  </default_proto>
</compiler_spec>"#;

        let doc = CspecDocument::parse_str(xml).expect("parse should succeed");
        assert_eq!(doc.stackpointer.as_deref(), Some("RSP"), "stackpointer");
        let proto = doc.default_proto.as_ref().expect("default_proto missing");
        assert_eq!(proto.name, "__stdcall");

        // float pentries are not in integer params
        let int_inputs: Vec<_> = proto
            .input
            .iter()
            .filter(|p| {
                if let CspecPentry::Register { metatype, .. } = p {
                    metatype.as_deref() != Some("float")
                } else {
                    true
                }
            })
            .collect();
        // RDI, RSI, RDX, RCX, R8, R9, stack pentry = 7
        assert_eq!(int_inputs.len(), 7, "expected 7 non-float input pentries");

        // Stack pentry exists
        let has_stack = proto
            .input
            .iter()
            .any(|p| matches!(p, CspecPentry::Stack { .. }));
        assert!(has_stack, "expected stack pentry in input");

        // Resolve against SLA register map
        let reg_map = make_x64_reg_map();
        let resolved = doc.resolve(&reg_map);
        assert_eq!(resolved.stack_pointer_offset, Some(0x20), "RSP offset");

        let rp = resolved.default_proto.expect("resolved proto");
        // Integer params: RDI=0x38, RSI=0x30, RDX=0x10, RCX=0x08, R8=0x80, R9=0x88
        assert_eq!(
            rp.int_param_offsets,
            vec![0x38, 0x30, 0x10, 0x08, 0x80, 0x88],
            "sysv integer param offsets"
        );
        assert_eq!(rp.return_offset, Some(0x00), "RAX return");
        assert_eq!(rp.stack_arg_base, Some(8), "stack arg base");
        assert_eq!(rp.extrapop, 8, "return-address stack size");
        assert!(rp.unaffected_offsets.contains(&0x20), "RSP is callee-saved");
        assert!(
            rp.killedbycall_offsets.contains(&0x00),
            "RAX is caller-saved"
        );
    }

    #[test]
    fn resolve_reg_offset_case_insensitive() {
        let mut reg_map = SlaRegisterMap::default();
        reg_map.insert("RAX".into(), (0x00, 8));
        assert_eq!(resolve_reg_offset("rax", &reg_map), Some(0x00));
        assert_eq!(resolve_reg_offset("RAX", &reg_map), Some(0x00));
    }
}
