//! Owned prototype-candidate corpus, independent of FPK and legacy parsers.
//! Type spellings remain unresolved; a row is not proof of a callable function.
use crate::FslError;

const MAX_BYTES: usize = 128 * 1024 * 1024;
const MAX_TEXT: usize = 16384;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibrarySource {
    pub path: String,
    pub sha256: String,
    pub commit: String,
    pub grammar: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterCandidate {
    pub name: String,
    pub type_spelling: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterForm {
    DeclaredEmpty,
    Listed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariadicEvidence {
    Unknown,
    Explicit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrototypeCandidate {
    pub symbol: String,
    pub return_spelling: String,
    pub parameter_form: ParameterForm,
    pub variadic: VariadicEvidence,
    pub parameters: Vec<ParameterCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryCatalog {
    pub source: LibrarySource,
    pub candidates: Vec<PrototypeCandidate>,
}

fn error(message: &str) -> FslError {
    FslError::at(1, 1, message)
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], FslError> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| error("library length overflow"))?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| error("truncated library"))?;
        self.position = end;
        Ok(value)
    }

    fn word(&mut self) -> Result<usize, FslError> {
        let bytes: [u8; 4] = self.take(4)?.try_into().unwrap();
        Ok(u32::from_le_bytes(bytes) as usize)
    }

    fn string(&mut self) -> Result<String, FslError> {
        let size = self.word()?;
        if size == 0 || size > MAX_TEXT {
            return Err(error("invalid library text length"));
        }
        let value =
            std::str::from_utf8(self.take(size)?).map_err(|_| error("invalid library UTF-8"))?;
        if value.chars().any(|c| (c as u32) < 32 || c == '\u{7f}') {
            return Err(error("control character in library text"));
        }
        Ok(value.to_owned())
    }
}

fn is_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl LibraryCatalog {
    /// Read FSLD v1: strict bounds, provenance, schema, ordering and EOF checks.
    /// Whole-file integrity/authenticity is the owning manifest's responsibility.
    pub fn decode_binary(bytes: &[u8]) -> Result<Self, FslError> {
        if bytes.len() > MAX_BYTES {
            return Err(error("library package too large"));
        }
        let mut reader = Reader { bytes, position: 0 };
        if reader.take(8)? != b"FSLD\x01\x00\x00\x00" {
            return Err(error("unsupported library magic/version/flags"));
        }
        let source = LibrarySource {
            path: reader.string()?,
            sha256: reader.string()?,
            commit: reader.string()?,
            grammar: reader.string()?,
        };
        if !is_hex(&source.sha256, 64)
            || !is_hex(&source.commit, 40)
            || source.grammar != "pipe-signatures-v1"
        {
            return Err(error("invalid or unsupported library provenance"));
        }
        let count = reader.word()?;
        if count > 1_000_000 || count > (bytes.len() - reader.position) / 16 {
            return Err(error("invalid library candidate count"));
        }
        let mut candidates: Vec<PrototypeCandidate> = Vec::with_capacity(count);
        for _ in 0..count {
            let symbol = reader.string()?;
            if candidates
                .last()
                .is_some_and(|previous| previous.symbol >= symbol)
            {
                return Err(error("unordered or duplicate library candidates"));
            }
            let return_spelling = reader.string()?;
            let parameter_form = match reader.take(1)?[0] {
                0 => ParameterForm::DeclaredEmpty,
                1 => ParameterForm::Listed,
                _ => return Err(error("unknown parameter form")),
            };
            let variadic = match reader.take(1)?[0] {
                0 => VariadicEvidence::Unknown,
                1 => VariadicEvidence::Explicit,
                _ => return Err(error("unknown variadic evidence")),
            };
            let parameters_count = reader.word()?;
            if parameters_count > 1024
                || parameters_count > (bytes.len() - reader.position) / 10
                || (parameter_form == ParameterForm::DeclaredEmpty
                    && (parameters_count != 0 || variadic != VariadicEvidence::Unknown))
                || (parameter_form == ParameterForm::Listed
                    && parameters_count == 0
                    && variadic != VariadicEvidence::Explicit)
            {
                return Err(error("invalid library parameter list"));
            }
            let mut parameters = Vec::with_capacity(parameters_count);
            for _ in 0..parameters_count {
                parameters.push(ParameterCandidate {
                    name: reader.string()?,
                    type_spelling: reader.string()?,
                });
            }
            candidates.push(PrototypeCandidate {
                symbol,
                return_spelling,
                parameter_form,
                variadic,
                parameters,
            });
        }
        if reader.position != bytes.len() {
            return Err(error("trailing library bytes"));
        }
        Ok(Self { source, candidates })
    }

    /// Exact symbol lookup; no fuzzy match, implicit ABI or type inference.
    pub fn lookup(&self, symbol: &str) -> Option<&PrototypeCandidate> {
        self.candidates
            .binary_search_by(|row| row.symbol.as_str().cmp(symbol))
            .ok()
            .map(|index| &self.candidates[index])
    }
}
