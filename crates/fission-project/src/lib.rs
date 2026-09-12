//! What an analyst decided about a binary, kept between runs.
//!
//! Every other artefact this tool produces is *derived*: run it again and you
//! get it again. A name someone chose for `FUN_00401230`, a note about why a
//! loop matters, the breakpoint that took an hour to find -- none of that can
//! be re-derived, and until now there was nowhere to put it. Each invocation
//! started from the file and ended with the terminal, so work did not
//! accumulate. That is the difference between a set of tools and a platform,
//! and it is the one thing Ghidra, radare2 and x64dbg all have.
//!
//! A project is deliberately small and deliberately plain JSON. It holds
//! decisions, not analysis: no function bodies, no control-flow graphs, no
//! caches. Everything in it is something a person or an agent chose, so it
//! stays readable, diffable, and small enough to commit next to the sample.
//!
//! # Binding
//!
//! A project names the binary by content hash. Applying one to a different
//! file would silently move every decision onto the wrong addresses, which is
//! worse than having no project at all, so [`Project::apply`] refuses rather
//! than guesses. The path is recorded too, but only as a hint for a human --
//! files get moved and renamed, and the hash is what decides.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use fission_loader::loader::LoadedBinary;
use serde::{Deserialize, Serialize};

/// The schema this crate writes. Bumped when a change would make an older
/// reader misunderstand a file rather than merely miss a field.
pub const SCHEMA_VERSION: u32 = 1;

/// The extension appended to a binary's path to find its project.
pub const PROJECT_SUFFIX: &str = ".fission.json";

/// Address-keyed maps, written as `"0x…"`.
mod hex_keys {
    use super::BTreeMap;
    use serde::de::{Deserialize, Deserializer, Error};
    use serde::ser::{Serialize, SerializeMap, Serializer};

    pub fn serialize<S, V>(map: &BTreeMap<u64, V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        V: Serialize,
    {
        let mut out = serializer.serialize_map(Some(map.len()))?;
        for (address, value) in map {
            out.serialize_entry(&format!("{address:#x}"), value)?;
        }
        out.end()
    }

    pub fn deserialize<'de, D, V>(deserializer: D) -> Result<BTreeMap<u64, V>, D::Error>
    where
        D: Deserializer<'de>,
        V: Deserialize<'de>,
    {
        // Decimal is accepted too, because an earlier version of this file
        // wrote it and a reader that rejects its own history is a bad reader.
        let raw = BTreeMap::<String, V>::deserialize(deserializer)?;
        raw.into_iter()
            .map(|(key, value)| {
                let address = super::parse_address(&key)
                    .ok_or_else(|| D::Error::custom(format!("{key:?} is not an address")))?;
                Ok((address, value))
            })
            .collect()
    }
}

/// Address lists, written as `["0x…"]` for the same reason.
mod hex_list {
    use serde::de::{Deserialize, Deserializer, Error};
    use serde::ser::{SerializeSeq, Serializer};

    pub fn serialize<S: Serializer>(list: &[u64], serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_seq(Some(list.len()))?;
        for address in list {
            out.serialize_element(&format!("{address:#x}"))?;
        }
        out.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u64>, D::Error> {
        // A number is accepted too: that is what an earlier version wrote.
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Address {
            Text(String),
            Number(u64),
        }
        Vec::<Address>::deserialize(deserializer)?
            .into_iter()
            .map(|address| match address {
                Address::Number(n) => Ok(n),
                Address::Text(text) => super::parse_address(&text)
                    .ok_or_else(|| D::Error::custom(format!("{text:?} is not an address"))),
            })
            .collect()
    }
}

/// `0x…`, `0X…` or decimal.
fn parse_address(text: &str) -> Option<u64> {
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok(),
        None => text.parse::<u64>().ok(),
    }
}

/// One address, written as `"0x…"`.
mod hex_scalar {
    use serde::Serializer;
    use serde::de::{Deserialize, Deserializer, Error};

    pub fn serialize<S: Serializer>(address: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{address:#x}"))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Address {
            Text(String),
            Number(u64),
        }
        match Address::deserialize(deserializer)? {
            Address::Number(n) => Ok(n),
            Address::Text(text) => super::parse_address(&text)
                .ok_or_else(|| D::Error::custom(format!("{text:?} is not an address"))),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("could not read project at {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not write project at {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not a Fission project: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error(
        "this project was made for a different binary (project {expected}, this file {actual}); \
         applying it would move every decision onto the wrong addresses"
    )]
    WrongBinary { expected: String, actual: String },
    #[error(
        "this project was written by a newer Fission (schema {found}, this one understands {known})"
    )]
    NewerSchema { found: u32, known: u32 },
}

/// A function signature somebody chose.
///
/// The half of a decision that a name cannot carry. A name says *what* a
/// function is; a signature says what goes in and what comes out, and that
/// propagates -- a parameter typed `char *` makes its uses inside the
/// function read as a string, and its arguments at every call site read as
/// one too. It is the lever Ghidra's type editor is, and the reason renaming
/// alone plateaus.
///
/// Types are the strings a person writes (`char *`, `struct stat *`). The
/// layers below take type *names* -- `NirFunctionHints` is a map of index to
/// `String` -- so there is nothing to parse them into.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// Return type. Absent leaves whatever was inferred.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub returns: Option<String>,
    /// Parameters, in order, as `(type, name)`. A name may be empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
}

/// One parameter of a [`Signature`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Param {
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
}

/// A note attached to an address.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub text: String,
}

/// A memory range worth stopping on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Watch {
    #[serde(with = "hex_scalar")]
    pub address: u64,
    pub size: u64,
    #[serde(default)]
    pub on_read: bool,
    #[serde(default = "yes")]
    pub on_write: bool,
}

fn yes() -> bool {
    true
}

/// Everything decided about one binary.
///
/// Addresses key the maps, and JSON object keys are strings whatever the
/// type says, so they are written the way every other part of this tool
/// writes an address: `"0x140001000"`. Serde's own rendering of a `u64` key
/// is decimal -- `"5368713216"` -- which is correct, unreadable, and useless
/// in a diff, and this file exists to be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub schema_version: u32,
    /// Content hash of the binary this describes -- the only thing that
    /// decides whether a project belongs to a file.
    pub binary_hash: String,
    /// Where the binary was when this was written. A hint for a person
    /// reading the file; nothing trusts it.
    #[serde(default)]
    pub binary_path: String,
    /// Function entry point -> the name someone chose for it.
    #[serde(default, with = "hex_keys")]
    pub names: BTreeMap<u64, String>,
    /// Address -> a note about it.
    #[serde(default, with = "hex_keys")]
    pub comments: BTreeMap<u64, Comment>,
    /// Function entry point -> the signature someone chose for it.
    #[serde(default, with = "hex_keys", skip_serializing_if = "BTreeMap::is_empty")]
    pub signatures: BTreeMap<u64, Signature>,
    /// Addresses a debug session should break on.
    #[serde(default, with = "hex_list")]
    pub breakpoints: Vec<u64>,
    /// Memory a debug session should watch.
    #[serde(default)]
    pub watchpoints: Vec<Watch>,
}

impl Project {
    /// An empty project bound to `binary`.
    pub fn for_binary(binary: &LoadedBinary) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            binary_hash: binary.hash.clone(),
            binary_path: binary.path.clone(),
            names: BTreeMap::new(),
            comments: BTreeMap::new(),
            signatures: BTreeMap::new(),
            breakpoints: Vec::new(),
            watchpoints: Vec::new(),
        }
    }

    /// Where a binary's project lives by default: beside it, with
    /// [`PROJECT_SUFFIX`] appended to the whole file name.
    ///
    /// Appended rather than substituted, so `cat` and `cat.exe` do not share
    /// one project and neither shadows the other.
    pub fn default_path(binary_path: &Path) -> PathBuf {
        let mut name = binary_path.as_os_str().to_os_string();
        name.push(PROJECT_SUFFIX);
        PathBuf::from(name)
    }

    /// Read a project, or `Ok(None)` if there is no file there.
    ///
    /// A missing project is the ordinary case -- most binaries have never
    /// been looked at -- so it is not an error; an unreadable or malformed
    /// one is.
    pub fn read(path: &Path) -> Result<Option<Self>, ProjectError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => {
                return Err(ProjectError::Read {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        let project: Self = serde_json::from_str(&text).map_err(|source| ProjectError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        if project.schema_version > SCHEMA_VERSION {
            return Err(ProjectError::NewerSchema {
                found: project.schema_version,
                known: SCHEMA_VERSION,
            });
        }
        Ok(Some(project))
    }

    /// Write the project, creating the parent directory if needed.
    ///
    /// Pretty-printed on purpose: this file is meant to be read by people and
    /// to produce a legible diff when a name changes.
    pub fn write(&self, path: &Path) -> Result<(), ProjectError> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(|source| ProjectError::Write {
                path: path.to_path_buf(),
                source,
            })?;
        }
        let mut text =
            serde_json::to_string_pretty(self).map_err(|source| ProjectError::Parse {
                path: path.to_path_buf(),
                source,
            })?;
        text.push('\n');
        std::fs::write(path, text).map_err(|source| ProjectError::Write {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Whether this project describes `binary`.
    pub fn matches(&self, binary: &LoadedBinary) -> bool {
        self.binary_hash == binary.hash
    }

    /// Put the project's decisions onto a freshly loaded binary.
    ///
    /// This is the whole point: after it, every command downstream --
    /// listing, decompiling, disassembling -- sees the chosen names rather
    /// than `FUN_00401230`, because they all read the same `LoadedBinary`.
    ///
    /// Refuses a binary it was not made for. A project applied to the wrong
    /// file would rename whatever happened to live at those addresses, and
    /// the result would look like analysis.
    pub fn apply(&self, binary: &mut LoadedBinary) -> Result<usize, ProjectError> {
        if !self.matches(binary) {
            return Err(ProjectError::WrongBinary {
                expected: self.binary_hash.clone(),
                actual: binary.hash.clone(),
            });
        }
        if self.names.is_empty() && self.signatures.is_empty() {
            return Ok(0);
        }

        let inner = binary.inner_mut();
        let mut applied = 0usize;
        for function in inner.functions.iter_mut() {
            if let Some(name) = self.names.get(&function.address) {
                function.name = name.clone();
                applied += 1;
            }
        }

        // Signatures ride along the same way, and are read by the decompiler
        // ahead of anything it inferred.
        for (address, signature) in &self.signatures {
            inner.user_signatures.insert(
                *address,
                fission_loader::loader::types::UserSignature {
                    return_type: signature.returns.clone(),
                    param_types: signature
                        .params
                        .iter()
                        .map(|p| p.type_name.clone())
                        .collect(),
                    param_names: signature.params.iter().map(|p| p.name.clone()).collect(),
                },
            );
            applied += 1;
        }
        // The name index maps names to positions, so it is stale the moment a
        // name changes. Rebuilt rather than patched: a half-updated index is
        // harder to notice than a rebuilt one is to pay for.
        inner.function_name_index = inner
            .functions
            .iter()
            .enumerate()
            .map(|(i, f)| (f.name.clone(), i))
            .collect();
        Ok(applied)
    }

    /// Name the function entered at `address`. Returns the previous name.
    pub fn set_name(&mut self, address: u64, name: impl Into<String>) -> Option<String> {
        self.names.insert(address, name.into())
    }

    pub fn clear_name(&mut self, address: u64) -> Option<String> {
        self.names.remove(&address)
    }

    pub fn set_comment(&mut self, address: u64, text: impl Into<String>) -> Option<Comment> {
        self.comments.insert(address, Comment { text: text.into() })
    }

    pub fn clear_comment(&mut self, address: u64) -> Option<Comment> {
        self.comments.remove(&address)
    }

    pub fn set_signature(&mut self, address: u64, signature: Signature) -> Option<Signature> {
        self.signatures.insert(address, signature)
    }

    pub fn clear_signature(&mut self, address: u64) -> Option<Signature> {
        self.signatures.remove(&address)
    }

    /// Remember a breakpoint. Returns whether it was new.
    pub fn add_breakpoint(&mut self, address: u64) -> bool {
        if self.breakpoints.contains(&address) {
            return false;
        }
        self.breakpoints.push(address);
        self.breakpoints.sort_unstable();
        true
    }

    pub fn remove_breakpoint(&mut self, address: u64) -> bool {
        let before = self.breakpoints.len();
        self.breakpoints.retain(|a| *a != address);
        before != self.breakpoints.len()
    }

    /// Remember a watchpoint, replacing any at the same address.
    pub fn add_watchpoint(&mut self, watch: Watch) {
        self.watchpoints.retain(|w| w.address != watch.address);
        self.watchpoints.push(watch);
        self.watchpoints.sort_unstable_by_key(|w| w.address);
    }

    pub fn remove_watchpoint(&mut self, address: u64) -> bool {
        let before = self.watchpoints.len();
        self.watchpoints.retain(|w| w.address != address);
        before != self.watchpoints.len()
    }

    /// Whether there is anything in here worth writing.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
            && self.signatures.is_empty()
            && self.comments.is_empty()
            && self.breakpoints.is_empty()
            && self.watchpoints.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_loader::FunctionInfo;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};

    fn binary(hash: &str) -> LoadedBinary {
        let mut loaded =
            LoadedBinaryBuilder::new("sample.bin".into(), DataBuffer::Heap(vec![0x90; 16]))
                .format("TEST")
                .image_base(0x400000)
                .is_64bit(true)
                .add_function(FunctionInfo {
                    name: "FUN_00401230".into(),
                    address: 0x401230,
                    size: 16,
                    ..Default::default()
                })
                .add_function(FunctionInfo {
                    name: "FUN_00401300".into(),
                    address: 0x401300,
                    size: 16,
                    ..Default::default()
                })
                .build()
                .expect("fixture");
        loaded.inner_mut().hash = hash.to_string();
        loaded
    }

    #[test]
    fn a_name_survives_a_round_trip_and_reaches_the_binary() {
        let mut b = binary("abc123");
        let mut project = Project::for_binary(&b);
        project.set_name(0x401230, "parse_header");

        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("sample.bin.fission.json");
        project.write(&path).expect("write");

        let read = Project::read(&path).expect("read").expect("a project");
        assert_eq!(read, project);

        assert_eq!(read.apply(&mut b).expect("apply"), 1);
        assert_eq!(b.functions[0].name, "parse_header");
        // And the untouched one is untouched.
        assert_eq!(b.functions[1].name, "FUN_00401300");
    }

    /// The failure that would look like analysis.
    #[test]
    fn a_project_refuses_a_binary_it_was_not_made_for() {
        let mine = binary("abc123");
        let mut other = binary("def456");
        let mut project = Project::for_binary(&mine);
        project.set_name(0x401230, "parse_header");

        let error = project.apply(&mut other).expect_err("must refuse");
        assert!(
            matches!(error, ProjectError::WrongBinary { .. }),
            "wrong error: {error}"
        );
        assert_eq!(
            other.functions[0].name, "FUN_00401230",
            "it renamed something anyway"
        );
    }

    /// A binary nobody has looked at is the ordinary case, not an error.
    #[test]
    fn a_missing_project_is_not_a_failure() {
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = dir.path().join("never-analysed.fission.json");
        assert!(Project::read(&missing).expect("not an error").is_none());
    }

    #[test]
    fn a_project_from_a_newer_fission_is_refused_rather_than_misread() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("future.fission.json");
        std::fs::write(
            &path,
            r#"{"schema_version": 9999, "binary_hash": "abc", "names": {}}"#,
        )
        .expect("write");
        let error = Project::read(&path).expect_err("must refuse");
        assert!(
            matches!(error, ProjectError::NewerSchema { found: 9999, .. }),
            "wrong error: {error}"
        );
    }

    /// The default path is appended, not substituted, so `cat` and `cat.exe`
    /// do not share one project.
    #[test]
    fn two_binaries_that_differ_only_by_extension_get_two_projects() {
        let a = Project::default_path(Path::new("/tmp/cat"));
        let b = Project::default_path(Path::new("/tmp/cat.exe"));
        assert_ne!(a, b);
        assert_eq!(a, PathBuf::from("/tmp/cat.fission.json"));
    }

    /// The file is meant to be read and diffed, so an address in it looks
    /// like an address.
    #[test]
    fn addresses_are_written_in_hex_and_read_back_either_way() {
        let b = binary("abc123");
        let mut project = Project::for_binary(&b);
        project.set_name(0x401230, "parse_header");
        project.set_comment(0x401244, "off by one");

        let text = serde_json::to_string_pretty(&project).expect("serialise");
        assert!(
            text.contains("\"0x401230\"") && text.contains("\"0x401244\""),
            "addresses are not hex:\n{text}"
        );
        assert!(
            !text.contains("4198960"),
            "an address leaked out in decimal:\n{text}"
        );
        let back: Project = serde_json::from_str(&text).expect("round trip");
        assert_eq!(back, project);

        // Breakpoints and watch addresses too, so the whole file reads the
        // same way.
        let mut with_stops = project.clone();
        with_stops.add_breakpoint(0x401230);
        with_stops.add_watchpoint(Watch {
            address: 0x500010,
            size: 8,
            on_read: false,
            on_write: true,
        });
        let text = serde_json::to_string(&with_stops).expect("serialise");
        assert!(
            text.contains("\"0x401230\"") && text.contains("\"0x500010\""),
            "{text}"
        );
        assert_eq!(
            serde_json::from_str::<Project>(&text).expect("round trip"),
            with_stops
        );
        // And the shapes an earlier version wrote.
        let old = r#"{"schema_version":1,"binary_hash":"abc123","breakpoints":[4198960],
                      "watchpoints":[{"address":5242896,"size":8,"on_write":true}]}"#;
        let old: Project = serde_json::from_str(old).expect("numeric addresses still read");
        assert_eq!(old.breakpoints, vec![0x401230]);
        assert_eq!(old.watchpoints[0].address, 0x500010);

        // And a file written before the hex change still reads.
        let decimal = r#"{"schema_version":1,"binary_hash":"abc123","names":{"4198960":"old"}}"#;
        let old: Project = serde_json::from_str(decimal).expect("decimal keys still read");
        assert_eq!(old.names.get(&0x401230).map(String::as_str), Some("old"));
    }

    #[test]
    fn breakpoints_and_watchpoints_do_not_accumulate_duplicates() {
        let b = binary("abc123");
        let mut project = Project::for_binary(&b);
        assert!(project.add_breakpoint(0x401230));
        assert!(!project.add_breakpoint(0x401230));
        assert_eq!(project.breakpoints, vec![0x401230]);

        project.add_watchpoint(Watch {
            address: 0x500000,
            size: 8,
            on_read: false,
            on_write: true,
        });
        project.add_watchpoint(Watch {
            address: 0x500000,
            size: 64,
            on_read: true,
            on_write: true,
        });
        assert_eq!(project.watchpoints.len(), 1);
        assert_eq!(project.watchpoints[0].size, 64);
    }
}

/// Parsing what a person writes into a [`Signature`].
///
/// A C declarator is a small grammar and this reads the part of it people
/// actually type at a prompt: `int (char *buf, int len)`, `void (void)`,
/// `(int)` to set the parameters and leave the return alone. It is
/// deliberately not a C parser -- a wrong parse here would attach a type
/// nobody chose and the output would look like analysis, so anything it
/// cannot read confidently is an error rather than a guess.
pub mod signature_syntax {
    use super::{Param, Signature};

    /// Split `text` at the parenthesis that opens the parameter list.
    fn split_at_params(text: &str) -> Result<(&str, &str), String> {
        let open = text
            .find('(')
            .ok_or_else(|| format!("{text:?} has no parameter list; write e.g. `int (char *)`"))?;
        if !text.trim_end().ends_with(')') {
            return Err(format!("{text:?} is missing its closing parenthesis"));
        }
        let close = text.rfind(')').expect("checked above");
        Ok((text[..open].trim(), text[open + 1..close].trim()))
    }

    /// Split a parameter list on commas that are not inside brackets.
    ///
    /// `void (*)(int, int)` is one parameter, not two.
    fn split_params(text: &str) -> Result<Vec<&str>, String> {
        let mut out = Vec::new();
        let mut depth = 0i32;
        let mut start = 0usize;
        for (i, c) in text.char_indices() {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => {
                    depth -= 1;
                    if depth < 0 {
                        return Err(format!("unbalanced brackets in {text:?}"));
                    }
                }
                ',' if depth == 0 => {
                    out.push(text[start..i].trim());
                    start = i + 1;
                }
                _ => {}
            }
        }
        if depth != 0 {
            return Err(format!("unbalanced brackets in {text:?}"));
        }
        out.push(text[start..].trim());
        Ok(out)
    }

    /// Separate a declaration into its type and the name it declares.
    ///
    /// The name, if there is one, is the trailing identifier: in
    /// `char *buf` it is `buf` and the type is `char *`; in `char *` there is
    /// none. `unsigned int` has no name even though it ends in a word, which
    /// is why a trailing word only counts when what precedes it is not empty
    /// *and* the word is not a type keyword.
    fn split_declaration(text: &str) -> (String, String) {
        const KEYWORDS: [&str; 14] = [
            "void", "char", "short", "int", "long", "float", "double", "signed", "unsigned",
            "const", "volatile", "struct", "union", "enum",
        ];
        let text = text.trim();
        let Some(last_space) = text.rfind(|c: char| c.is_whitespace() || c == '*') else {
            return (text.to_string(), String::new());
        };
        let (head, tail) = text.split_at(last_space + 1);
        let tail = tail.trim();
        let is_identifier = !tail.is_empty()
            && tail.chars().all(|c| c.is_alphanumeric() || c == '_')
            && tail
                .chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
            && !KEYWORDS.contains(&tail);
        if is_identifier && !head.trim().is_empty() {
            (head.trim().to_string(), tail.to_string())
        } else {
            (text.to_string(), String::new())
        }
    }

    /// Read `<return> (<params>)`.
    pub fn parse(text: &str) -> Result<Signature, String> {
        let text = text.trim();
        if text.is_empty() {
            return Err("empty signature".to_string());
        }
        let (returns, params_text) = split_at_params(text)?;

        let returns = (!returns.is_empty()).then(|| returns.to_string());

        let mut params = Vec::new();
        if !params_text.is_empty() && params_text != "void" {
            for piece in split_params(params_text)? {
                if piece.is_empty() {
                    return Err(format!("empty parameter in {text:?}"));
                }
                if piece == "..." {
                    // A variadic tail is not a parameter with a type; the
                    // layers below have no way to say "and then some", so
                    // saying so is better than dropping it silently.
                    return Err(
                        "variadic parameters (`...`) are not supported; give the fixed ones"
                            .to_string(),
                    );
                }
                let (type_name, name) = split_declaration(piece);
                params.push(Param { type_name, name });
            }
        }
        Ok(Signature { returns, params })
    }

    /// Render a signature the way it would be written.
    pub fn render(signature: &Signature) -> String {
        let params = if signature.params.is_empty() {
            "void".to_string()
        } else {
            signature
                .params
                .iter()
                .map(|p| {
                    if p.name.is_empty() {
                        p.type_name.clone()
                    } else if p.type_name.ends_with('*') {
                        format!("{}{}", p.type_name, p.name)
                    } else {
                        format!("{} {}", p.type_name, p.name)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        match &signature.returns {
            Some(r) => format!("{r} ({params})"),
            None => format!("({params})"),
        }
    }
}

#[cfg(test)]
mod signature_tests {
    use super::signature_syntax::{parse, render};

    #[test]
    fn a_signature_reads_the_way_it_is_written() {
        let s = parse("int (char *buf, int len)").expect("parse");
        assert_eq!(s.returns.as_deref(), Some("int"));
        assert_eq!(s.params.len(), 2);
        assert_eq!(s.params[0].type_name, "char *");
        assert_eq!(s.params[0].name, "buf");
        assert_eq!(s.params[1].type_name, "int");
        assert_eq!(s.params[1].name, "len");
    }

    /// A type made of several words is not a type plus a name.
    #[test]
    fn a_multi_word_type_without_a_name_stays_a_type() {
        let s = parse("(unsigned int, const char *, struct stat *st)").expect("parse");
        assert_eq!(s.returns, None, "no return type was given");
        assert_eq!(s.params[0].type_name, "unsigned int");
        assert_eq!(s.params[0].name, "");
        assert_eq!(s.params[1].type_name, "const char *");
        assert_eq!(s.params[1].name, "");
        assert_eq!(s.params[2].type_name, "struct stat *");
        assert_eq!(s.params[2].name, "st");
    }

    /// A function pointer parameter contains a comma and is still one
    /// parameter.
    #[test]
    fn a_comma_inside_brackets_does_not_split_a_parameter() {
        let s = parse("int (void (*cmp)(int, int), int n)").expect("parse");
        assert_eq!(s.params.len(), 2, "{:?}", s.params);
        assert_eq!(s.params[1].type_name, "int");
        assert_eq!(s.params[1].name, "n");
    }

    #[test]
    fn void_and_nothing_both_mean_no_parameters() {
        assert!(parse("void (void)").expect("parse").params.is_empty());
        assert!(parse("void ()").expect("parse").params.is_empty());
    }

    /// Anything it cannot read confidently is an error: a wrong parse would
    /// attach a type nobody chose.
    #[test]
    fn what_it_cannot_read_is_refused() {
        for bad in ["int", "int (char *", "int (a,, b)", "int (int, ...)"] {
            assert!(parse(bad).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn rendering_round_trips() {
        for text in [
            "int (char *buf, int len)",
            "void (void)",
            "char * (const char *, unsigned int n)",
        ] {
            let once = parse(text).expect("parse");
            let again = parse(&render(&once)).expect("re-parse");
            assert_eq!(once, again, "{text:?} did not survive a round trip");
        }
    }
}
