//! Restricted OpenQASM 3.0 frontend. Translates syntax into FSL, then uses the
//! ordinary compiler and canonical FIR validator. No independent semantic IR.
use crate::{
    quantum::{QuantumContract, MAX_QUBITS},
    FslError, FslcPackage,
};
use sha2::{Digest, Sha256};

#[derive(Debug)]
pub struct ImportedQuantum {
    pub package: FslcPackage,
    pub qubits: u8,
}
struct Parser {
    tokens: Vec<String>,
    cursor: usize,
    qubits: usize,
    bits: usize,
    defined: usize,
}
impl Parser {
    fn fail(&self, message: &str) -> FslError {
        FslError::at(
            1,
            1,
            format!("OpenQASM subset token {}: {message}", self.cursor),
        )
    }
    fn take(&mut self) -> Result<String, FslError> {
        let token = self
            .tokens
            .get(self.cursor)
            .cloned()
            .ok_or_else(|| self.fail("unexpected EOF"))?;
        self.cursor += 1;
        Ok(token)
    }
    fn expect(&mut self, wanted: &str) -> Result<(), FslError> {
        if self.take()? != wanted {
            return Err(self.fail(&format!("expected {wanted}")));
        }
        Ok(())
    }
    fn number(&mut self) -> Result<usize, FslError> {
        let token = self.take()?;
        if !token.bytes().all(|b| b.is_ascii_digit()) {
            return Err(self.fail("expected decimal integer"));
        }
        token
            .parse()
            .map_err(|_| self.fail("integer outside host range"))
    }
    fn indexed(&mut self, name: &str, extent: usize) -> Result<usize, FslError> {
        self.expect(name)?;
        self.expect("[")?;
        let index = self.number()?;
        self.expect("]")?;
        if index >= extent {
            return Err(self.fail("register index out of range"));
        }
        Ok(index)
    }
    fn q(&mut self) -> Result<usize, FslError> {
        self.indexed("q", self.qubits)
    }
    fn statement(&mut self) -> Result<String, FslError> {
        let op = self.take()?;
        match op.as_str() {
            "h" | "x" | "z" | "reset" => {
                let q = self.q()?;
                self.expect(";")?;
                Ok(format!("quantum.{op} {q};\n"))
            }
            "cx" => {
                let control = self.q()?;
                self.expect(",")?;
                let target = self.q()?;
                self.expect(";")?;
                Ok(format!("quantum.cx {control}, {target};\n"))
            }
            "m" => {
                self.expect("[")?;
                let id = self.number()?;
                self.expect("]")?;
                self.expect("=")?;
                if id >= self.bits || id != self.defined {
                    return Err(self.fail(
                        "classical destinations must be assigned once in dense index order",
                    ));
                }
                let value = self.take()?;
                let statement = if value == "measure" {
                    let q = self.q()?;
                    format!("%m{id}: u1 = quantum.measure_z {q};\n")
                } else if value == "0" || value == "1" {
                    format!("%m{id}: u1 = int.const {value};\n")
                } else {
                    return Err(self.fail("only measurement and bit constants supported"));
                };
                self.expect(";")?;
                self.defined += 1;
                Ok(statement)
            }
            "if" => {
                self.expect("(")?;
                let id = self.indexed("m", self.bits)?;
                if id >= self.defined {
                    return Err(self.fail("condition read before definition"));
                }
                for token in ["=", "=", "1", ")", "{", "x"] {
                    self.expect(token)?;
                }
                let q = self.q()?;
                self.expect(";")?;
                self.expect("}")?;
                Ok(format!("quantum.x_if {q}, %m{id};\n"))
            }
            _ => Err(self.fail("unsupported instruction; no opaque fallback")),
        }
    }
}
fn tokens(source: &str) -> Result<Vec<String>, FslError> {
    if source.len() > 262144 || !source.is_ascii() {
        return Err(FslError::at(
            1,
            1,
            "OpenQASM subset requires <=256KiB ASCII source",
        ));
    }
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if bytes.get(i..i + 2) == Some(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let start = i;
        if bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                i += 1;
            }
            if i == bytes.len() {
                return Err(FslError::at(1, 1, "unterminated OpenQASM string"));
            }
            i += 1;
        } else if b"[]{}();,=".contains(&bytes[i]) {
            i += 1;
        } else if bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' {
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || b"_.".contains(&bytes[i]))
            {
                i += 1;
            }
        } else {
            return Err(FslError::at(1, 1, "unsupported OpenQASM character"));
        }
        out.push(source[start..i].to_owned());
        if out.len() > 8192 {
            return Err(FslError::at(1, 1, "too many OpenQASM tokens"));
        }
    }
    Ok(out)
}
/// Fixed names q/m, static indexed resources, dense single-assignment bit slots;
/// h/x/z/cx/reset/measure and if(m[i]==1){x q[j];}. Every other construct refuses.
/// Initial quantum state remains an explicit external precondition, not assumed 0.
pub fn import_openqasm(source: &str) -> Result<ImportedQuantum, FslError> {
    let mut p = Parser {
        tokens: tokens(source)?,
        cursor: 0,
        qubits: 0,
        bits: 0,
        defined: 0,
    };
    for token in [
        "OPENQASM",
        "3.0",
        ";",
        "include",
        "\"stdgates.inc\"",
        ";",
        "qubit",
        "[",
    ] {
        p.expect(token)?;
    }
    p.qubits = p.number()?;
    for token in ["]", "q", ";", "bit", "["] {
        p.expect(token)?;
    }
    p.bits = p.number()?;
    for token in ["]", "m", ";"] {
        p.expect(token)?;
    }
    if !(1..=usize::from(MAX_QUBITS)).contains(&p.qubits) || !(1..=64).contains(&p.bits) {
        return Err(p.fail("requires 1..8 qubits and 1..64 classical slots"));
    }
    let mut semantics = String::new();
    while p.cursor < p.tokens.len() {
        semantics += &p.statement()?;
    }
    let sha = format!("{:x}", Sha256::digest(source.as_bytes()));
    let source=format!("language quantum.openqasm.subset {{ byte_order little; address_unit byte; instruction circuit {{ opcode 0; mnemonic \"circuit\"; evidence \"openqasm-subset\" \"https://openqasm.com/versions/3.0/\" \"sha256:{sha}\" \"Restricted logical circuit import, no physical ISA or hardware claim\"; semantics {{ {semantics} }} }} }}");
    let package = crate::compile_source(&source)?;
    QuantumContract::for_instruction(&package.instructions[0])?;
    Ok(ImportedQuantum {
        package,
        qubits: p.qubits as u8,
    })
}
