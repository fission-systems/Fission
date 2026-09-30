use crate::{AddressUnit, ByteOrder, Evidence, FslError, ValueType};

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
    pub opcode: u8,
    pub evidence: Vec<Evidence>,
    pub statements: Vec<Statement>,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug)]
pub(super) enum Statement {
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
    StackPush {
        value: String,
        line: usize,
        column: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Ident(String),
    Integer(u64),
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
            } else if "{}:;=,%".contains(ch) {
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

    fn integer(&mut self) -> Result<u64, FslError> {
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
            u64::from_str_radix(&digits, 16)
                .map_err(|_| FslError::at(line, column, "hex integer does not fit in u64"))
        } else {
            let digits = self.consume_while(|ch| ch.is_ascii_digit() || ch == '_');
            digits
                .replace('_', "")
                .parse::<u64>()
                .map_err(|_| FslError::at(line, column, "integer does not fit in u64"))
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
    fn parse_profile(mut self) -> Result<ParsedProfile, FslError> {
        self.expect_ident("language")?;
        let language = self.expect_name()?;
        self.expect_symbol('{')?;

        let mut byte_order = None;
        let mut address_unit = None;
        let mut instructions = Vec::new();
        while !self.at_symbol('}') {
            if self.at_ident("byte_order") {
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
        let mut opcode = None;
        let mut mnemonic = None;
        let mut evidence = Vec::new();
        let mut statements = None;
        while !self.at_symbol('}') {
            if self.at_ident("opcode") {
                self.advance();
                let token = self.current().clone();
                let value = self.expect_integer()?;
                opcode = Some(u8::try_from(value).map_err(|_| {
                    FslError::at(token.line, token.column, "opcode must fit in one byte")
                })?);
                self.expect_symbol(';')?;
            } else if self.at_ident("mnemonic") {
                self.advance();
                mnemonic = Some(self.expect_string()?);
                self.expect_symbol(';')?;
            } else if self.at_ident("evidence") {
                evidence.push(self.parse_evidence()?);
            } else if self.at_ident("semantics") {
                self.advance();
                self.expect_symbol('{')?;
                let mut parsed = Vec::new();
                while !self.at_symbol('}') {
                    parsed.push(self.parse_statement()?);
                }
                self.expect_symbol('}')?;
                if statements.replace(parsed).is_some() {
                    return Err(self.error_here("instruction has more than one semantics block"));
                }
            } else {
                return Err(self.error_here("expected opcode, mnemonic, evidence, or semantics"));
            }
        }
        self.expect_symbol('}')?;
        let opcode = opcode.ok_or_else(|| {
            FslError::at(
                name_token.line,
                name_token.column,
                "instruction has no opcode",
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
            opcode,
            evidence,
            statements,
            line: name_token.line,
            column: name_token.column,
        })
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
            let statement = if operation == "stack.pop" {
                Statement::StackPop {
                    name,
                    ty,
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
            } else {
                return Err(FslError::at(
                    token.line,
                    token.column,
                    format!("unsupported FIR operation {operation:?}"),
                ));
            };
            self.expect_symbol(';')?;
            Ok(statement)
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
