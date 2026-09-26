use alloc::string::String;
use alloc::vec::Vec;

// ═══════════════════════════════════════════════════════════════════════
// LEXER / TOKENIZER
// ═══════════════════════════════════════════════════════════════════════

/// Token types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // Literals
    IntLiteral(i64),
    FloatLiteral, // stored as string since no_std
    StringLiteral(String),
    CharLiteral(u8),
    Identifier(String),

    // Keywords
    KwFn,
    KwLet,
    KwMut,
    KwConst,
    KwIf,
    KwElse,
    KwWhile,
    KwFor,
    KwReturn,
    KwStruct,
    KwEnum,
    KwMatch,
    KwBreak,
    KwContinue,
    KwTrue,
    KwFalse,
    KwNull,
    KwAs,
    KwType,
    KwImpl,
    KwPub,
    KwExtern,
    KwUse,
    KwMod,
    KwSelf_,
    KwStatic,
    KwUnsafe,
    KwVoid,
    KwInt,
    KwChar_,
    KwBool_,
    KwU8,
    KwU16,
    KwU32,
    KwU64,
    KwI8,
    KwI16,
    KwI32,
    KwI64,
    KwF32,
    KwF64,
    KwUsize,
    KwIsize,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Bang,
    Lt,
    Gt,
    Eq,
    EqEq,
    BangEq,
    LtEq,
    GtEq,
    AmpAmp,
    PipePipe,
    LtLt,
    GtGt,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    Arrow,    // ->
    FatArrow, // =>
    DotDot,   // ..
    Dot,
    Comma,
    Semicolon,
    Colon,
    ColonColon, // ::

    // Delimiters
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,

    // Special
    Hash, // #
    At,   // @
    Eof,
}

/// Source location
#[derive(Debug, Clone, Copy)]
pub struct SourceLoc {
    pub line: u32,
    pub col: u32,
    pub offset: usize,
}

/// Token with location
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub loc: SourceLoc,
}

/// Lexer
pub struct Lexer {
    source: Vec<u8>,
    pos: usize,
    line: u32,
    col: u32,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.as_bytes().to_vec(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.source.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let ch = self.source.get(self.pos).copied()?;
        self.pos += 1;
        if ch == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn loc(&self) -> SourceLoc {
        SourceLoc {
            line: self.line,
            col: self.col,
            offset: self.pos,
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == b' ' || ch == b'\t' || ch == b'\n' || ch == b'\r' {
                self.advance();
            } else if ch == b'/' {
                if self.source.get(self.pos + 1) == Some(&b'/') {
                    // Line comment
                    while let Some(ch) = self.advance() {
                        if ch == b'\n' {
                            break;
                        }
                    }
                } else if self.source.get(self.pos + 1) == Some(&b'*') {
                    // Block comment
                    self.advance(); // /
                    self.advance(); // *
                    let mut depth = 1;
                    while depth > 0 {
                        match self.advance() {
                            Some(b'*') if self.peek() == Some(b'/') => {
                                self.advance();
                                depth -= 1;
                            }
                            Some(b'/') if self.peek() == Some(b'*') => {
                                self.advance();
                                depth += 1;
                            }
                            None => break,
                            _ => {}
                        }
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
    }

    fn lex_number(&mut self) -> Token {
        let loc = self.loc();
        let mut num: i64 = 0;
        let mut is_hex = false;

        if self.peek() == Some(b'0') {
            self.advance();
            match self.peek() {
                Some(b'x') | Some(b'X') => {
                    self.advance();
                    is_hex = true;
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_hexdigit() {
                            self.advance();
                            let digit = if ch >= b'a' {
                                (ch - b'a' + 10) as i64
                            } else if ch >= b'A' {
                                (ch - b'A' + 10) as i64
                            } else {
                                (ch - b'0') as i64
                            };
                            num = num * 16 + digit;
                        } else {
                            break;
                        }
                    }
                }
                Some(b'b') | Some(b'B') => {
                    self.advance();
                    while let Some(ch) = self.peek() {
                        if ch == b'0' || ch == b'1' {
                            self.advance();
                            num = num * 2 + (ch - b'0') as i64;
                        } else {
                            break;
                        }
                    }
                }
                _ => {
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_digit() {
                            self.advance();
                            num = num * 10 + (ch - b'0') as i64;
                        } else {
                            break;
                        }
                    }
                }
            }
        } else {
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit() {
                    self.advance();
                    num = num * 10 + (ch - b'0') as i64;
                } else {
                    break;
                }
            }
        }

        Token {
            kind: TokenKind::IntLiteral(num),
            loc,
        }
    }

    fn lex_string(&mut self) -> Token {
        let loc = self.loc();
        self.advance(); // opening "
        let mut s = String::new();
        loop {
            match self.advance() {
                Some(b'"') => break,
                Some(b'\\') => match self.advance() {
                    Some(b'n') => s.push('\n'),
                    Some(b't') => s.push('\t'),
                    Some(b'r') => s.push('\r'),
                    Some(b'\\') => s.push('\\'),
                    Some(b'"') => s.push('"'),
                    Some(b'0') => s.push('\0'),
                    Some(ch) => {
                        s.push('\\');
                        s.push(ch as char);
                    }
                    None => break,
                },
                Some(ch) => s.push(ch as char),
                None => break,
            }
        }
        Token {
            kind: TokenKind::StringLiteral(s),
            loc,
        }
    }

    fn lex_identifier(&mut self) -> Token {
        let loc = self.loc();
        let mut name = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == b'_' {
                name.push(ch as char);
                self.advance();
            } else {
                break;
            }
        }

        let kind = match name.as_str() {
            "fn" => TokenKind::KwFn,
            "let" => TokenKind::KwLet,
            "mut" => TokenKind::KwMut,
            "const" => TokenKind::KwConst,
            "if" => TokenKind::KwIf,
            "else" => TokenKind::KwElse,
            "while" => TokenKind::KwWhile,
            "for" => TokenKind::KwFor,
            "return" => TokenKind::KwReturn,
            "struct" => TokenKind::KwStruct,
            "enum" => TokenKind::KwEnum,
            "match" => TokenKind::KwMatch,
            "break" => TokenKind::KwBreak,
            "continue" => TokenKind::KwContinue,
            "true" => TokenKind::KwTrue,
            "false" => TokenKind::KwFalse,
            "null" | "None" => TokenKind::KwNull,
            "as" => TokenKind::KwAs,
            "type" => TokenKind::KwType,
            "impl" => TokenKind::KwImpl,
            "pub" => TokenKind::KwPub,
            "extern" => TokenKind::KwExtern,
            "use" => TokenKind::KwUse,
            "mod" => TokenKind::KwMod,
            "self" => TokenKind::KwSelf_,
            "static" => TokenKind::KwStatic,
            "unsafe" => TokenKind::KwUnsafe,
            "void" => TokenKind::KwVoid,
            "int" => TokenKind::KwInt,
            "char" => TokenKind::KwChar_,
            "bool" => TokenKind::KwBool_,
            "u8" => TokenKind::KwU8,
            "u16" => TokenKind::KwU16,
            "u32" => TokenKind::KwU32,
            "u64" => TokenKind::KwU64,
            "i8" => TokenKind::KwI8,
            "i16" => TokenKind::KwI16,
            "i32" => TokenKind::KwI32,
            "i64" => TokenKind::KwI64,
            "f32" => TokenKind::KwF32,
            "f64" => TokenKind::KwF64,
            "usize" => TokenKind::KwUsize,
            "isize" => TokenKind::KwIsize,
            _ => TokenKind::Identifier(name),
        };

        Token { kind, loc }
    }

    /// Tokenize entire source
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();

        loop {
            self.skip_whitespace();
            let loc = self.loc();

            let ch = match self.peek() {
                Some(ch) => ch,
                None => {
                    tokens.push(Token {
                        kind: TokenKind::Eof,
                        loc,
                    });
                    break;
                }
            };

            if ch.is_ascii_digit() {
                tokens.push(self.lex_number());
                continue;
            }

            if ch == b'"' {
                tokens.push(self.lex_string());
                continue;
            }

            if ch.is_ascii_alphabetic() || ch == b'_' {
                tokens.push(self.lex_identifier());
                continue;
            }

            // Operators and punctuation
            self.advance();
            let kind = match ch {
                b'+' => {
                    if self.peek() == Some(b'=') {
                        self.advance();
                        TokenKind::PlusEq
                    } else {
                        TokenKind::Plus
                    }
                }
                b'-' => {
                    if self.peek() == Some(b'>') {
                        self.advance();
                        TokenKind::Arrow
                    } else if self.peek() == Some(b'=') {
                        self.advance();
                        TokenKind::MinusEq
                    } else {
                        TokenKind::Minus
                    }
                }
                b'*' => {
                    if self.peek() == Some(b'=') {
                        self.advance();
                        TokenKind::StarEq
                    } else {
                        TokenKind::Star
                    }
                }
                b'/' => {
                    if self.peek() == Some(b'=') {
                        self.advance();
                        TokenKind::SlashEq
                    } else {
                        TokenKind::Slash
                    }
                }
                b'%' => {
                    if self.peek() == Some(b'=') {
                        self.advance();
                        TokenKind::PercentEq
                    } else {
                        TokenKind::Percent
                    }
                }
                b'&' => {
                    if self.peek() == Some(b'&') {
                        self.advance();
                        TokenKind::AmpAmp
                    } else if self.peek() == Some(b'=') {
                        self.advance();
                        TokenKind::AmpEq
                    } else {
                        TokenKind::Amp
                    }
                }
                b'|' => {
                    if self.peek() == Some(b'|') {
                        self.advance();
                        TokenKind::PipePipe
                    } else if self.peek() == Some(b'=') {
                        self.advance();
                        TokenKind::PipeEq
                    } else {
                        TokenKind::Pipe
                    }
                }
                b'^' => {
                    if self.peek() == Some(b'=') {
                        self.advance();
                        TokenKind::CaretEq
                    } else {
                        TokenKind::Caret
                    }
                }
                b'~' => TokenKind::Tilde,
                b'!' => {
                    if self.peek() == Some(b'=') {
                        self.advance();
                        TokenKind::BangEq
                    } else {
                        TokenKind::Bang
                    }
                }
                b'<' => {
                    if self.peek() == Some(b'=') {
                        self.advance();
                        TokenKind::LtEq
                    } else if self.peek() == Some(b'<') {
                        self.advance();
                        TokenKind::LtLt
                    } else {
                        TokenKind::Lt
                    }
                }
                b'>' => {
                    if self.peek() == Some(b'=') {
                        self.advance();
                        TokenKind::GtEq
                    } else if self.peek() == Some(b'>') {
                        self.advance();
                        TokenKind::GtGt
                    } else {
                        TokenKind::Gt
                    }
                }
                b'=' => {
                    if self.peek() == Some(b'=') {
                        self.advance();
                        TokenKind::EqEq
                    } else if self.peek() == Some(b'>') {
                        self.advance();
                        TokenKind::FatArrow
                    } else {
                        TokenKind::Eq
                    }
                }
                b'.' => {
                    if self.peek() == Some(b'.') {
                        self.advance();
                        TokenKind::DotDot
                    } else {
                        TokenKind::Dot
                    }
                }
                b',' => TokenKind::Comma,
                b';' => TokenKind::Semicolon,
                b':' => {
                    if self.peek() == Some(b':') {
                        self.advance();
                        TokenKind::ColonColon
                    } else {
                        TokenKind::Colon
                    }
                }
                b'(' => TokenKind::LParen,
                b')' => TokenKind::RParen,
                b'{' => TokenKind::LBrace,
                b'}' => TokenKind::RBrace,
                b'[' => TokenKind::LBracket,
                b']' => TokenKind::RBracket,
                b'#' => TokenKind::Hash,
                b'@' => TokenKind::At,
                b'\'' => {
                    let c = self.advance().unwrap_or(0);
                    if self.peek() == Some(b'\'') {
                        self.advance();
                    }
                    TokenKind::CharLiteral(c)
                }
                _ => continue, // skip unknown
            };

            tokens.push(Token { kind, loc });
        }

        tokens
    }
}
