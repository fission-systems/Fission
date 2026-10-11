use crate::{AddressUnit, BitField, ByteOrder, Encoding, Evidence, FslError, ValueType};

#[derive(Debug)]
pub(super) struct ParsedProfile {
    pub language: String,
    pub byte_order: ByteOrder,
    pub address_unit: AddressUnit,
    pub instructions: Vec<ParsedInstruction>,
}

#[derive(Debug)]
pub(super) struct ParsedInstruction {
    pub name: String,
    pub mnemonic: String,
    pub encoding: Encoding,
    pub evidence: Vec<Evidence>,
    pub statements: Vec<Statement>,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug)]
pub(super) enum Statement {
    MemoryLoadLittle {
        name: String,
        ty: ValueType,
        address: String,
        line: usize,
        column: usize,
    },
    BlockStart {
        name: String,
        parameters: Vec<(String, ValueType)>,
    },
    BlockEnd(ParsedTerminator),
    Convert {
        name: String,
        ty: ValueType,
        input: String,
        kind: crate::IntConversion,
        line: usize,
        column: usize,
    },
    Constant {
        name: String,
        ty: ValueType,
        value: u64,
        line: usize,
        column: usize,
    },
    Compare {
        name: String,
        ty: ValueType,
        left: String,
        right: String,
        predicate: crate::IntPredicate,
        line: usize,
        column: usize,
    },
    LaneMaskRead {
        name: String,
        ty: ValueType,
        lanes: u16,
        line: usize,
        column: usize,
    },
    LaneRead {
        name: String,
        ty: ValueType,
        field: String,
        bias: u64,
        mask: String,
        line: usize,
        column: usize,
    },
    LaneWrite {
        field: String,
        value: String,
        mask: String,
        line: usize,
        column: usize,
    },
    RegisterRead {
        name: String,
        ty: ValueType,
        field: String,
        line: usize,
        column: usize,
    },
    FlagRead {
        name: String,
        ty: ValueType,
        slot: u16,
        line: usize,
        column: usize,
    },
    FieldRead {
        name: String,
        ty: ValueType,
        field: String,
        line: usize,
        column: usize,
    },
    GuestPcRead {
        name: String,
        ty: ValueType,
        line: usize,
        column: usize,
    },
    GuestNextPcWrite {
        value: String,
        line: usize,
        column: usize,
    },
    RegisterWrite {
        field: String,
        value: String,
        line: usize,
        column: usize,
    },
    FlagWrite {
        slot: u16,
        value: String,
        line: usize,
        column: usize,
    },
    AddCarry {
        name: String,
        ty: ValueType,
        left: String,
        right: String,
        line: usize,
        column: usize,
    },
    AddCarryIn {
        name: String,
        ty: ValueType,
        left: String,
        right: String,
        carry: String,
        line: usize,
        column: usize,
    },
    Unsupported,
    StackPop {
        name: String,
        ty: ValueType,
        line: usize,
        column: usize,
    },
    AddWrap {
        name: String,
        ty: ValueType,
        left: String,
        right: String,
        line: usize,
        column: usize,
    },
    IntBinary {
        name: String,
        ty: ValueType,
        left: String,
        right: String,
        op: crate::IntBinaryOp,
        line: usize,
        column: usize,
    },
    AddWrapCarry {
        name: String,
        ty: ValueType,
        left: String,
        right: String,
        carry: String,
        line: usize,
        column: usize,
    },
    StackPush {
        value: String,
        line: usize,
        column: usize,
    },
}

#[derive(Debug)]
pub(super) struct ParsedEdge {
    pub target: String,
    pub arguments: Vec<String>,
}

#[derive(Debug)]
pub(super) enum ParsedTerminator {
    Return,
    Branch(ParsedEdge),
    CondBranch {
        condition: String,
        on_true: ParsedEdge,
        on_false: ParsedEdge,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Ident(String),
    Integer(u128),
    String(String),
    Symbol(char),
    End,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    line: usize,
    column: usize,
}

pub(super) fn parse(source: &str) -> Result<ParsedProfile, FslError> {
    let tokens = Lexer::new(source).tokenize()?;
    Parser { tokens, cursor: 0 }.parse_profile()
}

pub(super) fn parse_abi(source: &str) -> Result<crate::abi::AbiProfile, FslError> {
    let tokens = Lexer::new(source).tokenize()?;
    Parser { tokens, cursor: 0 }.parse_abi_profile()
}

pub(super) fn parse_layout(source: &str) -> Result<crate::registers::RegisterLayout, FslError> {
    let tokens = Lexer::new(source).tokenize()?;
    Parser { tokens, cursor: 0 }.parse_register_layout()
}

struct Lexer<'a> {
    chars: Vec<char>,
    _source: &'a str,
    cursor: usize,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            chars: source.chars().collect(),
            _source: source,
            cursor: 0,
            line: 1,
            column: 1,
        }
    }

    fn tokenize(mut self) -> Result<Vec<Token>, FslError> {
        let mut tokens = Vec::new();
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
                continue;
            }
            if ch == '#' || (ch == '/' && self.peek_n(1) == Some('/')) {
                self.skip_comment();
                continue;
            }
            let line = self.line;
            let column = self.column;
            if is_ident_start(ch) {
                tokens.push(Token {
                    kind: TokenKind::Ident(self.identifier()),
                    line,
                    column,
                });
            } else if ch.is_ascii_digit() {
                tokens.push(Token {
                    kind: TokenKind::Integer(self.integer()?),
                    line,
                    column,
                });
            } else if ch == '"' {
                tokens.push(Token {
                    kind: TokenKind::String(self.string()?),
                    line,
                    column,
                });
            } else if "{}():;=,%".contains(ch) {
                self.advance();
                tokens.push(Token {
                    kind: TokenKind::Symbol(ch),
                    line,
                    column,
                });
            } else {
                return Err(FslError::at(
                    line,
                    column,
                    format!("unexpected character {ch:?}"),
                ));
            }
        }
        tokens.push(Token {
            kind: TokenKind::End,
            line: self.line,
            column: self.column,
        });
        Ok(tokens)
    }

    fn identifier(&mut self) -> String {
        let mut output = String::new();
        while let Some(ch) = self.peek() {
            if is_ident_continue(ch) {
                output.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        output
    }

    fn integer(&mut self) -> Result<u128, FslError> {
        let line = self.line;
        let column = self.column;
        if self.peek() == Some('0') && matches!(self.peek_n(1), Some('x' | 'X')) {
            self.advance();
            self.advance();
            let digits = self.consume_while(|ch| ch.is_ascii_hexdigit() || ch == '_');
            let digits = digits.replace('_', "");
            if digits.is_empty() {
                return Err(FslError::at(line, column, "hex integer has no digits"));
            }
            u128::from_str_radix(&digits, 16)
                .map_err(|_| FslError::at(line, column, "hex integer does not fit in u128"))
        } else {
            let digits = self.consume_while(|ch| ch.is_ascii_digit() || ch == '_');
            digits
                .replace('_', "")
                .parse::<u128>()
                .map_err(|_| FslError::at(line, column, "integer does not fit in u128"))
        }
    }

    fn string(&mut self) -> Result<String, FslError> {
        let line = self.line;
        let column = self.column;
        self.advance();
        let mut output = String::new();
        loop {
            match self.peek() {
                Some('"') => {
                    self.advance();
                    return Ok(output);
                }
                Some('\\') => {
                    self.advance();
                    let escaped = self.peek().ok_or_else(|| {
                        FslError::at(line, column, "unterminated escape in string")
                    })?;
                    self.advance();
                    output.push(match escaped {
                        '"' => '"',
                        '\\' => '\\',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        _ => {
                            return Err(FslError::at(
                                self.line,
                                self.column.saturating_sub(1),
                                format!("unsupported string escape \\{escaped}"),
                            ));
                        }
                    });
                }
                Some('\n') | None => {
                    return Err(FslError::at(line, column, "unterminated string literal"));
                }
                Some(ch) => {
                    output.push(ch);
                    self.advance();
                }
            }
        }
    }

    fn skip_comment(&mut self) {
        if self.peek() == Some('/') {
            self.advance();
            self.advance();
        } else {
            self.advance();
        }
        while let Some(ch) = self.peek() {
            if ch == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn consume_while(&mut self, predicate: impl Fn(char) -> bool) -> String {
        let mut output = String::new();
        while let Some(ch) = self.peek() {
            if predicate(ch) {
                output.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        output
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.cursor).copied()
    }

    fn peek_n(&self, n: usize) -> Option<char> {
        self.chars.get(self.cursor.saturating_add(n)).copied()
    }

    fn advance(&mut self) {
        if let Some(ch) = self.peek() {
            self.cursor += 1;
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | '-')
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    fn abi_integer(&mut self) -> Result<u64, FslError> {
        self.expect_integer()
    }

    fn abi_boolean(&mut self) -> Result<bool, FslError> {
        match self.expect_name()?.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(self.error_here("expected true or false")),
        }
    }

    fn parse_register_layout(mut self) -> Result<crate::registers::RegisterLayout, FslError> {
        use crate::registers::{RegisterLayout, RegisterView, Space, SpaceKind};
        self.expect_ident("layout")?;
        let name = self.expect_name()?;
        self.expect_symbol('{')?;
        let mut layout = RegisterLayout {
            name,
            default_space: String::new(),
            evidence: Vec::new(),
            spaces: Vec::new(),
            registers: Vec::new(),
        };
        let mut default_space_seen = false;
        while !self.at_symbol('}') {
            if self.at_ident("evidence") {
                layout.evidence.push(self.parse_evidence()?);
            } else if self.at_ident("default_space") {
                self.advance();
                if default_space_seen {
                    return Err(self.error_here("duplicate default space"));
                }
                default_space_seen = true;
                layout.default_space = self.expect_string()?;
                self.expect_symbol(';')?;
            } else if self.at_ident("space") {
                self.advance();
                let name = self.expect_string()?;
                let kind = match self.expect_name()?.as_str() {
                    "register" => SpaceKind::Register,
                    "memory" => SpaceKind::Memory,
                    _ => return Err(self.error_here("expected register or memory space")),
                };
                let address_bytes = self.expect_integer()?;
                self.expect_ident("byte")?;
                let byte_order = match self.expect_name()?.as_str() {
                    "little" => ByteOrder::Little,
                    "big" => ByteOrder::Big,
                    _ => return Err(self.error_here("expected little or big byte order")),
                };
                self.expect_symbol(';')?;
                layout.spaces.push(Space {
                    name,
                    kind,
                    address_bytes,
                    byte_order,
                });
            } else if self.at_ident("register") {
                self.advance();
                let name = self.expect_string()?;
                let space = self.expect_string()?;
                let offset = self.expect_integer()?;
                let size_bytes = self.expect_integer()?;
                self.expect_symbol(';')?;
                layout.registers.push(RegisterView {
                    name,
                    space,
                    offset,
                    size_bytes,
                });
            } else {
                return Err(self.error_here("expected layout evidence, space or register"));
            }
        }
        self.expect_symbol('}')?;
        if !matches!(self.current().kind, TokenKind::End) {
            return Err(self.error_here("unexpected trailing layout tokens"));
        }
        Ok(layout)
    }

    fn parse_abi_profile(mut self) -> Result<crate::abi::AbiProfile, FslError> {
        use crate::abi::{AbiConvention, AbiMemoryEffect, AbiProfile, AbiRegisterEntry};
        use std::collections::BTreeMap;
        self.expect_ident("abi")?;
        let name = self.expect_name()?;
        self.expect_symbol('{')?;
        let mut evidence = Vec::new();
        let mut data = BTreeMap::new();
        let mut size_alignments = BTreeMap::new();
        let mut global_spaces = Vec::new();
        let mut stack = None;
        let mut default_convention = None;
        let mut conventions = Vec::new();
        while !self.at_symbol('}') {
            if self.at_ident("evidence") {
                evidence.push(self.parse_evidence()?);
                continue;
            }
            let keyword = self.expect_name()?;
            match keyword.as_str() {
                "data" => {
                    let key = self.expect_name()?;
                    // No opaque properties: these are the admitted data organization primitives.
                    if !matches!(
                        key.as_str(),
                        "absolute_max_alignment"
                            | "machine_alignment"
                            | "default_alignment"
                            | "default_pointer_alignment"
                            | "pointer_size"
                            | "wchar_size"
                            | "short_size"
                            | "integer_size"
                            | "long_size"
                            | "long_long_size"
                            | "float_size"
                            | "double_size"
                            | "long_double_size"
                    ) {
                        return Err(self.error_here("unsupported ABI data property"));
                    }
                    if data.insert(key, self.abi_integer()?).is_some() {
                        return Err(self.error_here("duplicate ABI data property"));
                    }
                }
                "alignment" => {
                    let size = self.abi_integer()?;
                    let alignment = self.abi_integer()?;
                    if size_alignments.insert(size, alignment).is_some() {
                        return Err(self.error_here("duplicate ABI size alignment"));
                    }
                }
                "global_space" => global_spaces.push(self.expect_string()?),
                "stack_pointer" => {
                    let register = self.expect_string()?;
                    let space = self.expect_string()?;
                    if stack.replace((register, space)).is_some() {
                        return Err(self.error_here("duplicate ABI stack pointer"));
                    }
                }
                "default_convention" => {
                    if default_convention.replace(self.expect_string()?).is_some() {
                        return Err(self.error_here("duplicate ABI default convention"));
                    }
                }
                "convention" => {
                    let name = self.expect_string()?;
                    self.expect_symbol('{')?;
                    let mut extrapop = None;
                    let mut stackshift = None;
                    let mut convention = AbiConvention {
                        name,
                        extrapop: None,
                        stackshift: 0,
                        inputs: Vec::new(),
                        outputs: Vec::new(),
                        output_killed_by_call: false,
                        preserved_registers: Vec::new(),
                        clobbered_registers: Vec::new(),
                        preserved_memory: Vec::new(),
                    };
                    let mut output_killed = None;
                    while !self.at_symbol('}') {
                        match self.expect_name()?.as_str() {
                            "extrapop" => {
                                let value = if self.at_ident("unknown") {
                                    self.advance();
                                    None
                                } else {
                                    Some(self.abi_integer()?)
                                };
                                if extrapop.replace(value).is_some() {
                                    return Err(self.error_here("duplicate extrapop"));
                                }
                            }
                            "stackshift" => {
                                if stackshift.replace(self.abi_integer()?).is_some() {
                                    return Err(self.error_here("duplicate stackshift"));
                                }
                            }
                            "input_register" => convention.inputs.push(AbiRegisterEntry {
                                register: self.expect_string()?,
                                min_bytes: self.abi_integer()?,
                                max_bytes: self.abi_integer()?,
                            }),
                            "output_register" => convention.outputs.push(AbiRegisterEntry {
                                register: self.expect_string()?,
                                min_bytes: self.abi_integer()?,
                                max_bytes: self.abi_integer()?,
                            }),
                            "output_killed_by_call" => {
                                if output_killed.replace(self.abi_boolean()?).is_some() {
                                    return Err(self.error_here("duplicate output_killed_by_call"));
                                }
                            }
                            "preserved_register" => {
                                convention.preserved_registers.push(self.expect_string()?)
                            }
                            "clobbered_register" => {
                                convention.clobbered_registers.push(self.expect_string()?)
                            }
                            "preserved_memory" => {
                                convention.preserved_memory.push(AbiMemoryEffect {
                                    space: self.expect_string()?,
                                    offset: self.abi_integer()?,
                                    size_bytes: self.abi_integer()?,
                                })
                            }
                            _ => {
                                return Err(self.error_here("unsupported ABI convention operation"))
                            }
                        }
                        self.expect_symbol(';')?;
                    }
                    self.expect_symbol('}')?;
                    convention.extrapop =
                        extrapop.ok_or_else(|| self.error_here("missing extrapop"))?;
                    convention.stackshift =
                        stackshift.ok_or_else(|| self.error_here("missing stackshift"))?;
                    convention.output_killed_by_call = output_killed.unwrap_or(false);
                    conventions.push(convention);
                    continue;
                }
                _ => return Err(self.error_here("unsupported ABI declaration")),
            }
            self.expect_symbol(';')?;
        }
        self.expect_symbol('}')?;
        if self.current().kind != TokenKind::End {
            return Err(self.error_here("trailing ABI source"));
        }
        let (stack_register, stack_space) =
            stack.ok_or_else(|| self.error_here("missing stack_pointer"))?;
        Ok(AbiProfile {
            name,
            evidence,
            data,
            size_alignments,
            global_spaces,
            stack_register,
            stack_space,
            default_convention: default_convention
                .ok_or_else(|| self.error_here("missing default_convention"))?,
            conventions,
        })
    }

    fn parse_profile(mut self) -> Result<ParsedProfile, FslError> {
        self.expect_ident("language")?;
        let language = self.expect_name()?;
        self.expect_symbol('{')?;

        let mut byte_order = None;
        let mut address_unit = None;
        let mut instructions = Vec::new();
        while !self.at_symbol('}') {
            if self.at_ident("byte_order") {
                if byte_order.is_some() {
                    return Err(self.error_here("duplicate byte_order declaration"));
                }
                self.advance();
                let token = self.current().clone();
                byte_order = Some(match self.expect_name()?.as_str() {
                    "little" => ByteOrder::Little,
                    "big" => ByteOrder::Big,
                    other => {
                        return Err(FslError::at(
                            token.line,
                            token.column,
                            format!("unsupported byte order {other:?}"),
                        ));
                    }
                });
                self.expect_symbol(';')?;
            } else if self.at_ident("address_unit") {
                if address_unit.is_some() {
                    return Err(self.error_here("duplicate address_unit declaration"));
                }
                self.advance();
                let token = self.current().clone();
                let unit = self.expect_name()?;
                address_unit = Some(match unit.as_str() {
                    "byte" => AddressUnit::Byte,
                    other => {
                        return Err(FslError::at(
                            token.line,
                            token.column,
                            format!("unsupported address unit {other:?}"),
                        ));
                    }
                });
                self.expect_symbol(';')?;
            } else if self.at_ident("instruction") {
                instructions.push(self.parse_instruction()?);
            } else {
                return Err(self.error_here("expected byte_order, address_unit, or instruction"));
            }
        }
        self.expect_symbol('}')?;
        if self.current().kind != TokenKind::End {
            return Err(self.error_here("unexpected content after language declaration"));
        }
        Ok(ParsedProfile {
            language,
            byte_order: byte_order
                .ok_or_else(|| self.error_here("language must declare byte_order"))?,
            address_unit: address_unit
                .ok_or_else(|| self.error_here("language must declare address_unit"))?,
            instructions,
        })
    }

    fn parse_instruction(&mut self) -> Result<ParsedInstruction, FslError> {
        self.expect_ident("instruction")?;
        let name_token = self.current().clone();
        let name = self.expect_name()?;
        self.expect_symbol('{')?;
        let mut encoding = None;
        let mut mnemonic = None;
        let mut evidence = Vec::new();
        let mut statements = None;
        while !self.at_symbol('}') {
            if self.at_ident("opcode") {
                self.advance();
                let token = self.current().clone();
                let value = self.expect_integer()?;
                let opcode = u8::try_from(value).map_err(|_| {
                    FslError::at(token.line, token.column, "opcode must fit in one byte")
                })?;
                if encoding.replace(Encoding::byte_opcode(opcode)).is_some() {
                    return Err(self.error_here("duplicate encoding declaration"));
                }
                self.expect_symbol(';')?;
            } else if self.at_ident("encoding") {
                let parsed = self.parse_encoding()?;
                if encoding.replace(parsed).is_some() {
                    return Err(self.error_here("duplicate encoding declaration"));
                }
            } else if self.at_ident("mnemonic") {
                if mnemonic.is_some() {
                    return Err(self.error_here("duplicate mnemonic declaration"));
                }
                self.advance();
                mnemonic = Some(self.expect_string()?);
                self.expect_symbol(';')?;
            } else if self.at_ident("evidence") {
                evidence.push(self.parse_evidence()?);
            } else if self.at_ident("semantics") {
                self.advance();
                if self.at_ident("unsupported") {
                    self.advance();
                    self.expect_symbol(';')?;
                    if statements.replace(vec![Statement::Unsupported]).is_some() {
                        return Err(self.error_here("duplicate semantics declaration"));
                    }
                    continue;
                }
                self.expect_symbol('{')?;
                let mut parsed = Vec::new();
                while !self.at_symbol('}') {
                    if self.at_ident("block") {
                        self.advance();
                        let name = self.expect_name()?;
                        self.expect_symbol('(')?;
                        let mut parameters = Vec::new();
                        while !self.at_symbol(')') {
                            self.expect_symbol('%')?;
                            let name = self.expect_name()?;
                            self.expect_symbol(':')?;
                            let ty_name = self.expect_name()?;
                            let ty = parse_value_type(&ty_name)
                                .ok_or_else(|| self.error_here("invalid block parameter type"))?;
                            parameters.push((name, ty));
                            if !self.at_symbol(',') {
                                break;
                            }
                            self.advance();
                        }
                        self.expect_symbol(')')?;
                        self.expect_symbol('{')?;
                        parsed.push(Statement::BlockStart { name, parameters });
                        while !self.at_ident("return")
                            && !self.at_ident("branch")
                            && !self.at_ident("branch.if")
                        {
                            parsed.push(self.parse_statement()?);
                        }
                        let terminator = if self.at_ident("return") {
                            self.advance();
                            ParsedTerminator::Return
                        } else if self.at_ident("branch") {
                            self.advance();
                            ParsedTerminator::Branch(self.parse_edge()?)
                        } else {
                            self.advance();
                            self.expect_symbol('%')?;
                            let condition = self.expect_name()?;
                            self.expect_symbol(',')?;
                            let on_true = self.parse_edge()?;
                            self.expect_symbol(',')?;
                            let on_false = self.parse_edge()?;
                            ParsedTerminator::CondBranch {
                                condition,
                                on_true,
                                on_false,
                            }
                        };
                        self.expect_symbol(';')?;
                        self.expect_symbol('}')?;
                        parsed.push(Statement::BlockEnd(terminator));
                    } else {
                        parsed.push(self.parse_statement()?);
                    }
                }
                self.expect_symbol('}')?;
                if statements.replace(parsed).is_some() {
                    return Err(self.error_here("instruction has more than one semantics block"));
                }
            } else {
                return Err(
                    self.error_here("expected opcode, encoding, mnemonic, evidence, or semantics")
                );
            }
        }
        self.expect_symbol('}')?;
        let encoding = encoding.ok_or_else(|| {
            FslError::at(
                name_token.line,
                name_token.column,
                "instruction has no encoding",
            )
        })?;
        let mnemonic = mnemonic.ok_or_else(|| {
            FslError::at(
                name_token.line,
                name_token.column,
                "instruction has no mnemonic",
            )
        })?;
        if evidence.is_empty() {
            return Err(FslError::at(
                name_token.line,
                name_token.column,
                "instruction must include at least one evidence declaration",
            ));
        }
        let statements = statements.ok_or_else(|| {
            FslError::at(
                name_token.line,
                name_token.column,
                "instruction has no semantics block",
            )
        })?;
        Ok(ParsedInstruction {
            name,
            mnemonic,
            encoding,
            evidence,
            statements,
            line: name_token.line,
            column: name_token.column,
        })
    }

    fn parse_encoding(&mut self) -> Result<Encoding, FslError> {
        self.expect_ident("encoding")?;
        let bits = u16::try_from(self.expect_integer()?)
            .map_err(|_| self.error_here("encoding width exceeds u16"))?;
        self.expect_ident("mask")?;
        let mask = self.expect_wide_integer()?;
        self.expect_ident("value")?;
        let value = self.expect_wide_integer()?;
        self.expect_symbol('{')?;
        let mut fields = Vec::new();
        while !self.at_symbol('}') {
            self.expect_ident("field")?;
            let name = self.expect_name()?;
            self.expect_ident("offset")?;
            let offset = u16::try_from(self.expect_integer()?)
                .map_err(|_| self.error_here("field offset exceeds u16"))?;
            self.expect_ident("bits")?;
            let bits = u16::try_from(self.expect_integer()?)
                .map_err(|_| self.error_here("field width exceeds u16"))?;
            let mut excluded = Vec::new();
            if self.at_ident("exclude") {
                self.advance();
                excluded.push(self.expect_integer()?);
                while self.at_symbol(',') {
                    self.advance();
                    excluded.push(self.expect_integer()?);
                }
            }
            self.expect_symbol(';')?;
            fields.push(BitField {
                name,
                offset,
                bits,
                excluded,
            });
        }
        self.expect_symbol('}')?;
        let encoding = Encoding {
            bits,
            mask,
            value,
            fields,
        };
        encoding.validate()?;
        Ok(encoding)
    }

    fn parse_evidence(&mut self) -> Result<Evidence, FslError> {
        self.expect_ident("evidence")?;
        let source_id = self.expect_string()?;
        let url = self.expect_string()?;
        let revision = self.expect_string()?;
        let claim = self.expect_string()?;
        self.expect_symbol(';')?;
        if source_id.is_empty() || url.is_empty() || revision.is_empty() || claim.is_empty() {
            return Err(self.error_here("evidence fields cannot be empty"));
        }
        Ok(Evidence {
            source_id,
            url,
            revision,
            claim,
        })
    }

    fn parse_edge(&mut self) -> Result<ParsedEdge, FslError> {
        let target = self.expect_name()?;
        self.expect_symbol('(')?;
        let mut arguments = Vec::new();
        while !self.at_symbol(')') {
            self.expect_symbol('%')?;
            arguments.push(self.expect_name()?);
            if !self.at_symbol(',') {
                break;
            }
            self.advance();
        }
        self.expect_symbol(')')?;
        Ok(ParsedEdge { target, arguments })
    }

    fn parse_statement(&mut self) -> Result<Statement, FslError> {
        let token = self.current().clone();
        if self.at_symbol('%') {
            self.advance();
            let name = self.expect_name()?;
            self.expect_symbol(':')?;
            let ty_name = self.expect_name()?;
            let ty = parse_value_type(&ty_name).ok_or_else(|| {
                FslError::at(
                    token.line,
                    token.column,
                    format!("unsupported FIR type {ty_name:?}"),
                )
            })?;
            self.expect_symbol('=')?;
            let operation = self.expect_name()?;
            let statement = if matches!(operation.as_str(), "int.zext" | "int.sext" | "int.trunc") {
                self.expect_symbol('%')?;
                let input = self.expect_name()?;
                let kind = match operation.as_str() {
                    "int.zext" => crate::IntConversion::ZeroExtend,
                    "int.sext" => crate::IntConversion::SignExtend,
                    _ => crate::IntConversion::Truncate,
                };
                Statement::Convert {
                    name,
                    ty,
                    input,
                    kind,
                    line: token.line,
                    column: token.column,
                }
            } else if operation == "memory.load.le" {
                self.expect_symbol('%')?;
                let address = self.expect_name()?;
                Statement::MemoryLoadLittle {
                    name,
                    ty,
                    address,
                    line: token.line,
                    column: token.column,
                }
            } else if operation == "int.const" {
                let value = self.expect_integer()?;
                Statement::Constant {
                    name,
                    ty,
                    value,
                    line: token.line,
                    column: token.column,
                }
            } else if matches!(operation.as_str(), "int.eq" | "int.ult" | "int.slt") {
                self.expect_symbol('%')?;
                let left = self.expect_name()?;
                self.expect_symbol(',')?;
                self.expect_symbol('%')?;
                let right = self.expect_name()?;
                let predicate = match operation.as_str() {
                    "int.eq" => crate::IntPredicate::Equal,
                    "int.ult" => crate::IntPredicate::UnsignedLess,
                    _ => crate::IntPredicate::SignedLess,
                };
                Statement::Compare {
                    name,
                    ty,
                    left,
                    right,
                    predicate,
                    line: token.line,
                    column: token.column,
                }
            } else if operation == "lane.mask.read" {
                let lanes = u16::try_from(self.expect_integer()?)
                    .map_err(|_| self.error_here("lane extent exceeds u16"))?;
                Statement::LaneMaskRead {
                    name,
                    ty,
                    lanes,
                    line: token.line,
                    column: token.column,
                }
            } else if operation == "lane.register.read" {
                let field = self.expect_name()?;
                self.expect_symbol(',')?;
                let bias = self.expect_integer()?;
                self.expect_symbol(',')?;
                self.expect_symbol('%')?;
                let mask = self.expect_name()?;
                Statement::LaneRead {
                    name,
                    ty,
                    field,
                    bias,
                    mask,
                    line: token.line,
                    column: token.column,
                }
            } else if operation == "stack.pop" {
                Statement::StackPop {
                    name,
                    ty,
                    line: token.line,
                    column: token.column,
                }
            } else if operation == "register.read" {
                Statement::RegisterRead {
                    name,
                    ty,
                    field: self.expect_name()?,
                    line: token.line,
                    column: token.column,
                }
            } else if operation == "field.read" {
                Statement::FieldRead {
                    name,
                    ty,
                    field: self.expect_name()?,
                    line: token.line,
                    column: token.column,
                }
            } else if operation == "guest.pc.read" {
                Statement::GuestPcRead {
                    name,
                    ty,
                    line: token.line,
                    column: token.column,
                }
            } else if operation == "flag.read" {
                let slot = u16::try_from(self.expect_integer()?)
                    .map_err(|_| self.error_here("flag slot exceeds u16"))?;
                Statement::FlagRead {
                    name,
                    ty,
                    slot,
                    line: token.line,
                    column: token.column,
                }
            } else if operation == "int.add.carry" {
                self.expect_symbol('%')?;
                let left = self.expect_name()?;
                self.expect_symbol(',')?;
                self.expect_symbol('%')?;
                let right = self.expect_name()?;
                Statement::AddCarry {
                    name,
                    ty,
                    left,
                    right,
                    line: token.line,
                    column: token.column,
                }
            } else if operation == "int.add.carry.in" {
                self.expect_symbol('%')?;
                let left = self.expect_name()?;
                self.expect_symbol(',')?;
                self.expect_symbol('%')?;
                let right = self.expect_name()?;
                self.expect_symbol(',')?;
                self.expect_symbol('%')?;
                let carry = self.expect_name()?;
                Statement::AddCarryIn {
                    name,
                    ty,
                    left,
                    right,
                    carry,
                    line: token.line,
                    column: token.column,
                }
            } else if let Some(op) = [
                crate::IntBinaryOp::Sub,
                crate::IntBinaryOp::And,
                crate::IntBinaryOp::Or,
                crate::IntBinaryOp::Xor,
            ]
            .into_iter()
            .find(|op| operation == format!("{ty_name}.{}", op.name()))
            {
                self.expect_symbol('%')?;
                let left = self.expect_name()?;
                self.expect_symbol(',')?;
                self.expect_symbol('%')?;
                let right = self.expect_name()?;
                Statement::IntBinary {
                    name,
                    ty,
                    left,
                    right,
                    op,
                    line: token.line,
                    column: token.column,
                }
            } else if operation == format!("{ty_name}.add.wrap") {
                self.expect_symbol('%')?;
                let left = self.expect_name()?;
                self.expect_symbol(',')?;
                self.expect_symbol('%')?;
                let right = self.expect_name()?;
                Statement::AddWrap {
                    name,
                    ty,
                    left,
                    right,
                    line: token.line,
                    column: token.column,
                }
            } else if operation == format!("{ty_name}.add.carry") {
                self.expect_symbol('%')?;
                let left = self.expect_name()?;
                self.expect_symbol(',')?;
                self.expect_symbol('%')?;
                let right = self.expect_name()?;
                self.expect_symbol(',')?;
                self.expect_symbol('%')?;
                let carry = self.expect_name()?;
                Statement::AddWrapCarry {
                    name,
                    ty,
                    left,
                    right,
                    carry,
                    line: token.line,
                    column: token.column,
                }
            } else {
                return Err(FslError::at(
                    token.line,
                    token.column,
                    format!("unsupported FIR operation {operation:?}"),
                ));
            };
            self.expect_symbol(';')?;
            Ok(statement)
        } else if self.at_ident("lane.register.write") {
            self.advance();
            let field = self.expect_name()?;
            self.expect_symbol(',')?;
            self.expect_symbol('%')?;
            let value = self.expect_name()?;
            self.expect_symbol(',')?;
            self.expect_symbol('%')?;
            let mask = self.expect_name()?;
            self.expect_symbol(';')?;
            Ok(Statement::LaneWrite {
                field,
                value,
                mask,
                line: token.line,
                column: token.column,
            })
        } else if self.at_ident("register.write") {
            self.advance();
            let field = self.expect_name()?;
            self.expect_symbol(',')?;
            self.expect_symbol('%')?;
            let value = self.expect_name()?;
            self.expect_symbol(';')?;
            Ok(Statement::RegisterWrite {
                field,
                value,
                line: token.line,
                column: token.column,
            })
        } else if self.at_ident("guest.next_pc.write") {
            self.advance();
            self.expect_symbol('%')?;
            let value = self.expect_name()?;
            self.expect_symbol(';')?;
            Ok(Statement::GuestNextPcWrite {
                value,
                line: token.line,
                column: token.column,
            })
        } else if self.at_ident("flag.write") {
            self.advance();
            let slot = u16::try_from(self.expect_integer()?)
                .map_err(|_| self.error_here("flag slot exceeds u16"))?;
            self.expect_symbol(',')?;
            self.expect_symbol('%')?;
            let value = self.expect_name()?;
            self.expect_symbol(';')?;
            Ok(Statement::FlagWrite {
                slot,
                value,
                line: token.line,
                column: token.column,
            })
        } else if self.at_ident("stack.push") {
            self.advance();
            self.expect_symbol('%')?;
            let value = self.expect_name()?;
            self.expect_symbol(';')?;
            Ok(Statement::StackPush {
                value,
                line: token.line,
                column: token.column,
            })
        } else {
            Err(self.error_here("expected FIR value definition or stack.push"))
        }
    }

    fn expect_ident(&mut self, expected: &str) -> Result<(), FslError> {
        match &self.current().kind {
            TokenKind::Ident(value) if value == expected => {
                self.advance();
                Ok(())
            }
            _ => Err(self.error_here(&format!("expected {expected:?}"))),
        }
    }

    fn expect_name(&mut self) -> Result<String, FslError> {
        match self.current().kind.clone() {
            TokenKind::Ident(value) => {
                self.advance();
                Ok(value)
            }
            _ => Err(self.error_here("expected identifier")),
        }
    }

    fn expect_integer(&mut self) -> Result<u64, FslError> {
        let value = self.expect_wide_integer()?;
        u64::try_from(value).map_err(|_| self.error_here("integer exceeds u64"))
    }

    fn expect_wide_integer(&mut self) -> Result<u128, FslError> {
        match self.current().kind {
            TokenKind::Integer(value) => {
                self.advance();
                Ok(value)
            }
            _ => Err(self.error_here("expected integer literal")),
        }
    }

    fn expect_string(&mut self) -> Result<String, FslError> {
        match self.current().kind.clone() {
            TokenKind::String(value) => {
                self.advance();
                Ok(value)
            }
            _ => Err(self.error_here("expected quoted string")),
        }
    }

    fn expect_symbol(&mut self, expected: char) -> Result<(), FslError> {
        match self.current().kind {
            TokenKind::Symbol(value) if value == expected => {
                self.advance();
                Ok(())
            }
            _ => Err(self.error_here(&format!("expected {expected:?}"))),
        }
    }

    fn at_ident(&self, expected: &str) -> bool {
        matches!(&self.current().kind, TokenKind::Ident(value) if value == expected)
    }

    fn at_symbol(&self, expected: char) -> bool {
        matches!(self.current().kind, TokenKind::Symbol(value) if value == expected)
    }

    fn error_here(&self, message: &str) -> FslError {
        let token = self.current();
        FslError::at(token.line, token.column, message)
    }

    fn current(&self) -> &Token {
        &self.tokens[self.cursor]
    }

    fn advance(&mut self) {
        if !matches!(self.current().kind, TokenKind::End) {
            self.cursor += 1;
        }
    }
}

fn parse_value_type(name: &str) -> Option<ValueType> {
    let (sign, width) = match name.as_bytes().first().copied()? {
        b'i' => (crate::IntegerSign::Signed, &name[1..]),
        b'u' => (crate::IntegerSign::Unsigned, &name[1..]),
        _ => return None,
    };
    let bits = width.parse::<u16>().ok()?;
    (bits > 0 && bits <= 4096).then_some(ValueType { bits, sign })
}
