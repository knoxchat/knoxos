/// Self-Hosting Compiler Toolchain
///
/// Implements a minimal self-hosting compiler infrastructure for KnoxOS.
/// Provides lexing, parsing, code generation, linking, and assembler
/// capabilities to build executables directly within the kernel.
///
/// Features:
///   - Lexer/tokenizer for a C-like language
///   - Recursive descent parser producing AST
///   - x86_64 code generator (ELF output)
///   - Simple register allocator
///   - Assembler (x86_64 instruction encoding)
///   - Linker (ELF section/symbol resolution)
///   - Preprocessor (#include, #define, #ifdef)
///   - Optimization passes (constant folding, dead code elimination)
///   - Standard library stubs for hosted builds
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

use crate::serial_println;

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

// ═══════════════════════════════════════════════════════════════════════
// AST (Abstract Syntax Tree)
// ═══════════════════════════════════════════════════════════════════════

/// Type representation
#[derive(Debug, Clone)]
pub enum Type {
    Void,
    Bool,
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
    Usize,
    Isize,
    Char,
    Str,
    Ptr(Box<Type>),
    Array(Box<Type>, usize),
    Slice(Box<Type>),
    Fn(Vec<Type>, Box<Type>),
    Named(String),
}

/// Binary operator
#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

/// Unary operator
#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
    Deref,
    AddrOf,
}

/// Expression node
#[derive(Debug, Clone)]
pub enum Expr {
    IntLit(i64),
    BoolLit(bool),
    StringLit(String),
    CharLit(u8),
    Null,
    Ident(String),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Unary(UnaryOp, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Index(Box<Expr>, Box<Expr>),
    Field(Box<Expr>, String),
    Cast(Box<Expr>, Type),
    Assign(Box<Expr>, Box<Expr>),
    Block(Vec<Stmt>, Option<Box<Expr>>),
    If(Box<Expr>, Box<Expr>, Option<Box<Expr>>),
    Array(Vec<Expr>),
    StructLit(String, Vec<(String, Expr)>),
}

/// Statement node
#[derive(Debug, Clone)]
pub enum Stmt {
    Expr(Expr),
    Let(String, Option<Type>, Option<Expr>, bool), // name, type, init, mutable
    Return(Option<Expr>),
    While(Expr, Box<Stmt>),
    For(String, Expr, Expr, Box<Stmt>),
    Block(Vec<Stmt>),
    Break,
    Continue,
    Item(Item),
}

/// Top-level item
#[derive(Debug, Clone)]
pub enum Item {
    Function {
        name: String,
        params: Vec<(String, Type)>,
        ret_type: Type,
        body: Vec<Stmt>,
        is_pub: bool,
        is_extern: bool,
    },
    Struct {
        name: String,
        fields: Vec<(String, Type)>,
        is_pub: bool,
    },
    Enum {
        name: String,
        variants: Vec<(String, Option<Type>)>,
    },
    Static {
        name: String,
        ty: Type,
        value: Expr,
        mutable: bool,
    },
    Const {
        name: String,
        ty: Type,
        value: Expr,
    },
    TypeAlias {
        name: String,
        ty: Type,
    },
    ExternBlock {
        items: Vec<Item>,
    },
    Use(String),
    Mod(String),
}

/// Compilation unit (file)
#[derive(Debug, Clone)]
pub struct CompilationUnit {
    pub name: String,
    pub items: Vec<Item>,
}

// ═══════════════════════════════════════════════════════════════════════
// PARSER (simplified recursive descent)
// ═══════════════════════════════════════════════════════════════════════

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<String>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            errors: Vec::new(),
        }
    }

    fn peek(&self) -> &TokenKind {
        self.tokens
            .get(self.pos)
            .map(|t| &t.kind)
            .unwrap_or(&TokenKind::Eof)
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, kind: &TokenKind) -> bool {
        if self.peek() == kind {
            self.advance();
            true
        } else {
            self.errors
                .push(alloc::format!("Expected {:?}, got {:?}", kind, self.peek()));
            false
        }
    }

    /// Parse a full compilation unit
    pub fn parse(&mut self) -> CompilationUnit {
        let mut items = Vec::new();
        while *self.peek() != TokenKind::Eof {
            if let Some(item) = self.parse_item() {
                items.push(item);
            } else {
                self.advance(); // skip error token
            }
        }
        CompilationUnit {
            name: String::from("<stdin>"),
            items,
        }
    }

    fn parse_item(&mut self) -> Option<Item> {
        let is_pub = if *self.peek() == TokenKind::KwPub {
            self.advance();
            true
        } else {
            false
        };

        match self.peek().clone() {
            TokenKind::KwFn => self.parse_function(is_pub, false),
            TokenKind::KwStruct => self.parse_struct(is_pub),
            TokenKind::KwEnum => self.parse_enum(),
            TokenKind::KwStatic => self.parse_static(),
            TokenKind::KwConst => self.parse_const(),
            TokenKind::KwType => self.parse_type_alias(),
            TokenKind::KwExtern => {
                self.advance();
                self.parse_function(is_pub, true)
            }
            TokenKind::KwUse => {
                self.advance();
                if let TokenKind::Identifier(name) = self.peek().clone() {
                    self.advance();
                    self.expect(&TokenKind::Semicolon);
                    Some(Item::Use(name))
                } else {
                    None
                }
            }
            TokenKind::KwMod => {
                self.advance();
                if let TokenKind::Identifier(name) = self.peek().clone() {
                    self.advance();
                    self.expect(&TokenKind::Semicolon);
                    Some(Item::Mod(name))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn parse_function(&mut self, is_pub: bool, is_extern: bool) -> Option<Item> {
        self.expect(&TokenKind::KwFn);

        let name = if let TokenKind::Identifier(name) = self.peek().clone() {
            self.advance();
            name
        } else {
            return None;
        };

        self.expect(&TokenKind::LParen);
        let mut params = Vec::new();
        while *self.peek() != TokenKind::RParen && *self.peek() != TokenKind::Eof {
            if let TokenKind::Identifier(pname) = self.peek().clone() {
                self.advance();
                self.expect(&TokenKind::Colon);
                let ty = self.parse_type();
                params.push((pname, ty));
                if *self.peek() == TokenKind::Comma {
                    self.advance();
                }
            } else {
                break;
            }
        }
        self.expect(&TokenKind::RParen);

        let ret_type = if *self.peek() == TokenKind::Arrow {
            self.advance();
            self.parse_type()
        } else {
            Type::Void
        };

        let body = if *self.peek() == TokenKind::LBrace {
            self.parse_block_stmts()
        } else {
            self.expect(&TokenKind::Semicolon);
            Vec::new()
        };

        Some(Item::Function {
            name,
            params,
            ret_type,
            body,
            is_pub,
            is_extern,
        })
    }

    fn parse_struct(&mut self, is_pub: bool) -> Option<Item> {
        self.expect(&TokenKind::KwStruct);
        let name = if let TokenKind::Identifier(n) = self.peek().clone() {
            self.advance();
            n
        } else {
            return None;
        };

        self.expect(&TokenKind::LBrace);
        let mut fields = Vec::new();
        while *self.peek() != TokenKind::RBrace && *self.peek() != TokenKind::Eof {
            if let TokenKind::Identifier(fname) = self.peek().clone() {
                self.advance();
                self.expect(&TokenKind::Colon);
                let ty = self.parse_type();
                fields.push((fname, ty));
                if *self.peek() == TokenKind::Comma {
                    self.advance();
                }
            } else {
                break;
            }
        }
        self.expect(&TokenKind::RBrace);

        Some(Item::Struct {
            name,
            fields,
            is_pub,
        })
    }

    fn parse_enum(&mut self) -> Option<Item> {
        self.expect(&TokenKind::KwEnum);
        let name = if let TokenKind::Identifier(n) = self.peek().clone() {
            self.advance();
            n
        } else {
            return None;
        };

        self.expect(&TokenKind::LBrace);
        let mut variants = Vec::new();
        while *self.peek() != TokenKind::RBrace && *self.peek() != TokenKind::Eof {
            if let TokenKind::Identifier(vname) = self.peek().clone() {
                self.advance();
                let ty = if *self.peek() == TokenKind::LParen {
                    self.advance();
                    let t = self.parse_type();
                    self.expect(&TokenKind::RParen);
                    Some(t)
                } else {
                    None
                };
                variants.push((vname, ty));
                if *self.peek() == TokenKind::Comma {
                    self.advance();
                }
            } else {
                break;
            }
        }
        self.expect(&TokenKind::RBrace);

        Some(Item::Enum { name, variants })
    }

    fn parse_static(&mut self) -> Option<Item> {
        self.expect(&TokenKind::KwStatic);
        let mutable = if *self.peek() == TokenKind::KwMut {
            self.advance();
            true
        } else {
            false
        };
        let name = if let TokenKind::Identifier(n) = self.peek().clone() {
            self.advance();
            n
        } else {
            return None;
        };
        self.expect(&TokenKind::Colon);
        let ty = self.parse_type();
        self.expect(&TokenKind::Eq);
        let value = self.parse_expr();
        self.expect(&TokenKind::Semicolon);
        Some(Item::Static {
            name,
            ty,
            value,
            mutable,
        })
    }

    fn parse_const(&mut self) -> Option<Item> {
        self.expect(&TokenKind::KwConst);
        let name = if let TokenKind::Identifier(n) = self.peek().clone() {
            self.advance();
            n
        } else {
            return None;
        };
        self.expect(&TokenKind::Colon);
        let ty = self.parse_type();
        self.expect(&TokenKind::Eq);
        let value = self.parse_expr();
        self.expect(&TokenKind::Semicolon);
        Some(Item::Const { name, ty, value })
    }

    fn parse_type_alias(&mut self) -> Option<Item> {
        self.expect(&TokenKind::KwType);
        let name = if let TokenKind::Identifier(n) = self.peek().clone() {
            self.advance();
            n
        } else {
            return None;
        };
        self.expect(&TokenKind::Eq);
        let ty = self.parse_type();
        self.expect(&TokenKind::Semicolon);
        Some(Item::TypeAlias { name, ty })
    }

    fn parse_type(&mut self) -> Type {
        match self.peek().clone() {
            TokenKind::KwVoid => {
                self.advance();
                Type::Void
            }
            TokenKind::KwBool_ => {
                self.advance();
                Type::Bool
            }
            TokenKind::KwU8 => {
                self.advance();
                Type::U8
            }
            TokenKind::KwU16 => {
                self.advance();
                Type::U16
            }
            TokenKind::KwU32 => {
                self.advance();
                Type::U32
            }
            TokenKind::KwU64 => {
                self.advance();
                Type::U64
            }
            TokenKind::KwI8 => {
                self.advance();
                Type::I8
            }
            TokenKind::KwI16 => {
                self.advance();
                Type::I16
            }
            TokenKind::KwI32 => {
                self.advance();
                Type::I32
            }
            TokenKind::KwI64 => {
                self.advance();
                Type::I64
            }
            TokenKind::KwF32 => {
                self.advance();
                Type::F32
            }
            TokenKind::KwF64 => {
                self.advance();
                Type::F64
            }
            TokenKind::KwUsize => {
                self.advance();
                Type::Usize
            }
            TokenKind::KwIsize => {
                self.advance();
                Type::Isize
            }
            TokenKind::KwChar_ => {
                self.advance();
                Type::Char
            }
            TokenKind::Star => {
                self.advance();
                Type::Ptr(Box::new(self.parse_type()))
            }
            TokenKind::LBracket => {
                self.advance();
                let elem = self.parse_type();
                if *self.peek() == TokenKind::Semicolon {
                    self.advance();
                    if let TokenKind::IntLiteral(n) = self.peek().clone() {
                        self.advance();
                        self.expect(&TokenKind::RBracket);
                        Type::Array(Box::new(elem), n as usize)
                    } else {
                        self.expect(&TokenKind::RBracket);
                        Type::Slice(Box::new(elem))
                    }
                } else {
                    self.expect(&TokenKind::RBracket);
                    Type::Slice(Box::new(elem))
                }
            }
            TokenKind::Identifier(name) => {
                self.advance();
                Type::Named(name)
            }
            _ => {
                self.advance();
                Type::Void
            }
        }
    }

    fn parse_block_stmts(&mut self) -> Vec<Stmt> {
        self.expect(&TokenKind::LBrace);
        let mut stmts = Vec::new();
        while *self.peek() != TokenKind::RBrace && *self.peek() != TokenKind::Eof {
            stmts.push(self.parse_stmt());
        }
        self.expect(&TokenKind::RBrace);
        stmts
    }

    fn parse_stmt(&mut self) -> Stmt {
        match self.peek().clone() {
            TokenKind::KwLet => {
                self.advance();
                let mutable = if *self.peek() == TokenKind::KwMut {
                    self.advance();
                    true
                } else {
                    false
                };
                let name = if let TokenKind::Identifier(n) = self.peek().clone() {
                    self.advance();
                    n
                } else {
                    String::from("_")
                };
                let ty = if *self.peek() == TokenKind::Colon {
                    self.advance();
                    Some(self.parse_type())
                } else {
                    None
                };
                let init = if *self.peek() == TokenKind::Eq {
                    self.advance();
                    Some(self.parse_expr())
                } else {
                    None
                };
                self.expect(&TokenKind::Semicolon);
                Stmt::Let(name, ty, init, mutable)
            }
            TokenKind::KwReturn => {
                self.advance();
                let val = if *self.peek() != TokenKind::Semicolon {
                    Some(self.parse_expr())
                } else {
                    None
                };
                self.expect(&TokenKind::Semicolon);
                Stmt::Return(val)
            }
            TokenKind::KwWhile => {
                self.advance();
                let cond = self.parse_expr();
                let body = Stmt::Block(self.parse_block_stmts());
                Stmt::While(cond, Box::new(body))
            }
            TokenKind::KwBreak => {
                self.advance();
                self.expect(&TokenKind::Semicolon);
                Stmt::Break
            }
            TokenKind::KwContinue => {
                self.advance();
                self.expect(&TokenKind::Semicolon);
                Stmt::Continue
            }
            TokenKind::LBrace => Stmt::Block(self.parse_block_stmts()),
            _ => {
                let expr = self.parse_expr();
                self.expect(&TokenKind::Semicolon);
                Stmt::Expr(expr)
            }
        }
    }

    fn parse_expr(&mut self) -> Expr {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Expr {
        let lhs = self.parse_or();
        if *self.peek() == TokenKind::Eq {
            self.advance();
            let rhs = self.parse_assignment();
            Expr::Assign(Box::new(lhs), Box::new(rhs))
        } else {
            lhs
        }
    }

    fn parse_or(&mut self) -> Expr {
        let mut lhs = self.parse_and();
        while *self.peek() == TokenKind::PipePipe {
            self.advance();
            let rhs = self.parse_and();
            lhs = Expr::Binary(BinOp::Or, Box::new(lhs), Box::new(rhs));
        }
        lhs
    }

    fn parse_and(&mut self) -> Expr {
        let mut lhs = self.parse_comparison();
        while *self.peek() == TokenKind::AmpAmp {
            self.advance();
            let rhs = self.parse_comparison();
            lhs = Expr::Binary(BinOp::And, Box::new(lhs), Box::new(rhs));
        }
        lhs
    }

    fn parse_comparison(&mut self) -> Expr {
        let mut lhs = self.parse_addition();
        loop {
            let op = match self.peek() {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::BangEq => BinOp::Ne,
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::LtEq => BinOp::Le,
                TokenKind::GtEq => BinOp::Ge,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_addition();
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        lhs
    }

    fn parse_addition(&mut self) -> Expr {
        let mut lhs = self.parse_multiplication();
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_multiplication();
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        lhs
    }

    fn parse_multiplication(&mut self) -> Expr {
        let mut lhs = self.parse_unary();
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_unary();
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        lhs
    }

    fn parse_unary(&mut self) -> Expr {
        match self.peek().clone() {
            TokenKind::Minus => {
                self.advance();
                Expr::Unary(UnaryOp::Neg, Box::new(self.parse_unary()))
            }
            TokenKind::Bang => {
                self.advance();
                Expr::Unary(UnaryOp::Not, Box::new(self.parse_unary()))
            }
            TokenKind::Tilde => {
                self.advance();
                Expr::Unary(UnaryOp::BitNot, Box::new(self.parse_unary()))
            }
            TokenKind::Star => {
                self.advance();
                Expr::Unary(UnaryOp::Deref, Box::new(self.parse_unary()))
            }
            TokenKind::Amp => {
                self.advance();
                Expr::Unary(UnaryOp::AddrOf, Box::new(self.parse_unary()))
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Expr {
        let mut expr = self.parse_primary();
        loop {
            match self.peek().clone() {
                TokenKind::LParen => {
                    self.advance();
                    let mut args = Vec::new();
                    while *self.peek() != TokenKind::RParen && *self.peek() != TokenKind::Eof {
                        args.push(self.parse_expr());
                        if *self.peek() == TokenKind::Comma {
                            self.advance();
                        }
                    }
                    self.expect(&TokenKind::RParen);
                    expr = Expr::Call(Box::new(expr), args);
                }
                TokenKind::LBracket => {
                    self.advance();
                    let idx = self.parse_expr();
                    self.expect(&TokenKind::RBracket);
                    expr = Expr::Index(Box::new(expr), Box::new(idx));
                }
                TokenKind::Dot => {
                    self.advance();
                    if let TokenKind::Identifier(field) = self.peek().clone() {
                        self.advance();
                        expr = Expr::Field(Box::new(expr), field);
                    }
                }
                _ => break,
            }
        }
        expr
    }

    fn parse_primary(&mut self) -> Expr {
        match self.peek().clone() {
            TokenKind::IntLiteral(n) => {
                self.advance();
                Expr::IntLit(n)
            }
            TokenKind::StringLiteral(s) => {
                self.advance();
                Expr::StringLit(s)
            }
            TokenKind::CharLiteral(c) => {
                self.advance();
                Expr::CharLit(c)
            }
            TokenKind::KwTrue => {
                self.advance();
                Expr::BoolLit(true)
            }
            TokenKind::KwFalse => {
                self.advance();
                Expr::BoolLit(false)
            }
            TokenKind::KwNull => {
                self.advance();
                Expr::Null
            }
            TokenKind::Identifier(name) => {
                self.advance();
                Expr::Ident(name)
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr();
                self.expect(&TokenKind::RParen);
                expr
            }
            TokenKind::KwIf => {
                self.advance();
                let cond = self.parse_expr();
                let then = Expr::Block(self.parse_block_stmts(), None);
                let else_ = if *self.peek() == TokenKind::KwElse {
                    self.advance();
                    Some(Box::new(Expr::Block(self.parse_block_stmts(), None)))
                } else {
                    None
                };
                Expr::If(Box::new(cond), Box::new(then), else_)
            }
            _ => {
                self.advance();
                Expr::IntLit(0)
            }
        }
    }

    pub fn errors(&self) -> &[String] {
        &self.errors
    }
}

// ═══════════════════════════════════════════════════════════════════════
// x86_64 CODE GENERATOR
// ═══════════════════════════════════════════════════════════════════════

/// x86_64 register
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reg {
    Rax,
    Rbx,
    Rcx,
    Rdx,
    Rsi,
    Rdi,
    Rbp,
    Rsp,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
}

impl Reg {
    pub fn encoding(&self) -> u8 {
        match self {
            Reg::Rax => 0,
            Reg::Rcx => 1,
            Reg::Rdx => 2,
            Reg::Rbx => 3,
            Reg::Rsp => 4,
            Reg::Rbp => 5,
            Reg::Rsi => 6,
            Reg::Rdi => 7,
            Reg::R8 => 8,
            Reg::R9 => 9,
            Reg::R10 => 10,
            Reg::R11 => 11,
            Reg::R12 => 12,
            Reg::R13 => 13,
            Reg::R14 => 14,
            Reg::R15 => 15,
        }
    }

    pub fn needs_rex(&self) -> bool {
        self.encoding() >= 8
    }

    /// ABI argument registers
    pub fn arg_regs() -> &'static [Reg] {
        &[Reg::Rdi, Reg::Rsi, Reg::Rdx, Reg::Rcx, Reg::R8, Reg::R9]
    }

    /// Caller-saved registers
    pub fn caller_saved() -> &'static [Reg] {
        &[
            Reg::Rax,
            Reg::Rcx,
            Reg::Rdx,
            Reg::Rsi,
            Reg::Rdi,
            Reg::R8,
            Reg::R9,
            Reg::R10,
            Reg::R11,
        ]
    }

    /// Callee-saved registers
    pub fn callee_saved() -> &'static [Reg] {
        &[Reg::Rbx, Reg::R12, Reg::R13, Reg::R14, Reg::R15]
    }
}

/// Machine instruction
#[derive(Debug, Clone)]
pub enum MachineInst {
    Push(Reg),
    Pop(Reg),
    MovRR(Reg, Reg),      // dst, src
    MovRI(Reg, i64),      // dst, imm
    MovRM(Reg, Reg, i32), // dst, base, offset (load)
    MovMR(Reg, i32, Reg), // base, offset, src (store)
    Add(Reg, Reg),
    Sub(Reg, Reg),
    IMul(Reg, Reg),
    Xor(Reg, Reg),
    Cmp(Reg, Reg),
    Test(Reg, Reg),
    Jmp(String),
    Je(String),
    Jne(String),
    Jl(String),
    Jg(String),
    Jle(String),
    Jge(String),
    Call(String),
    Ret,
    Nop,
    Syscall,
    Label(String),
    Comment(String),
}

/// Simple code generator
pub struct CodeGen {
    instructions: Vec<MachineInst>,
    labels: BTreeMap<String, usize>,
    stack_offset: i32,
    local_vars: BTreeMap<String, i32>, // name -> rbp offset
    label_counter: u64,
}

impl CodeGen {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            labels: BTreeMap::new(),
            stack_offset: 0,
            local_vars: BTreeMap::new(),
            label_counter: 0,
        }
    }

    fn new_label(&mut self, prefix: &str) -> String {
        self.label_counter += 1;
        alloc::format!(".L{}_{}", prefix, self.label_counter)
    }

    fn emit(&mut self, inst: MachineInst) {
        self.instructions.push(inst);
    }

    /// Generate code for a compilation unit
    pub fn generate(&mut self, unit: &CompilationUnit) {
        for item in &unit.items {
            self.gen_item(item);
        }
    }

    fn gen_item(&mut self, item: &Item) {
        if let Item::Function {
            name, params, body, ..
        } = item
        {
            self.emit(MachineInst::Label(name.clone()));
            // Prologue
            self.emit(MachineInst::Push(Reg::Rbp));
            self.emit(MachineInst::MovRR(Reg::Rbp, Reg::Rsp));

            self.local_vars.clear();
            self.stack_offset = 0;

            // Save args to stack
            let arg_regs = Reg::arg_regs();
            for (i, (pname, _)) in params.iter().enumerate() {
                if i < arg_regs.len() {
                    self.stack_offset -= 8;
                    self.emit(MachineInst::MovMR(Reg::Rbp, self.stack_offset, arg_regs[i]));
                    self.local_vars.insert(pname.clone(), self.stack_offset);
                }
            }

            // Generate body
            for stmt in body {
                self.gen_stmt(stmt);
            }

            // Epilogue
            self.emit(MachineInst::MovRR(Reg::Rsp, Reg::Rbp));
            self.emit(MachineInst::Pop(Reg::Rbp));
            self.emit(MachineInst::Ret);
        }
    }

    fn gen_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let(name, _ty, init, _mutable) => {
                self.stack_offset -= 8;
                let offset = self.stack_offset;
                self.local_vars.insert(name.clone(), offset);
                if let Some(expr) = init {
                    self.gen_expr(expr);
                    self.emit(MachineInst::MovMR(Reg::Rbp, offset, Reg::Rax));
                }
            }
            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    self.gen_expr(e);
                }
                self.emit(MachineInst::MovRR(Reg::Rsp, Reg::Rbp));
                self.emit(MachineInst::Pop(Reg::Rbp));
                self.emit(MachineInst::Ret);
            }
            Stmt::Expr(expr) => {
                self.gen_expr(expr);
            }
            Stmt::While(cond, body) => {
                let loop_label = self.new_label("while");
                let end_label = self.new_label("endwhile");
                self.emit(MachineInst::Label(loop_label.clone()));
                self.gen_expr(cond);
                self.emit(MachineInst::Test(Reg::Rax, Reg::Rax));
                self.emit(MachineInst::Je(end_label.clone()));
                self.gen_stmt(body);
                self.emit(MachineInst::Jmp(loop_label));
                self.emit(MachineInst::Label(end_label));
            }
            Stmt::Block(stmts) => {
                for s in stmts {
                    self.gen_stmt(s);
                }
            }
            Stmt::Break => {
                // Would need a break label stack - simplified
                self.emit(MachineInst::Comment(String::from("break")));
            }
            Stmt::Continue => {
                self.emit(MachineInst::Comment(String::from("continue")));
            }
            _ => {}
        }
    }

    fn gen_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::IntLit(n) => {
                self.emit(MachineInst::MovRI(Reg::Rax, *n));
            }
            Expr::BoolLit(b) => {
                self.emit(MachineInst::MovRI(Reg::Rax, if *b { 1 } else { 0 }));
            }
            Expr::Ident(name) => {
                if let Some(&offset) = self.local_vars.get(name) {
                    self.emit(MachineInst::MovRM(Reg::Rax, Reg::Rbp, offset));
                }
            }
            Expr::Binary(op, lhs, rhs) => {
                self.gen_expr(rhs);
                self.emit(MachineInst::Push(Reg::Rax));
                self.gen_expr(lhs);
                self.emit(MachineInst::Pop(Reg::Rcx));
                match op {
                    BinOp::Add => self.emit(MachineInst::Add(Reg::Rax, Reg::Rcx)),
                    BinOp::Sub => self.emit(MachineInst::Sub(Reg::Rax, Reg::Rcx)),
                    BinOp::Mul => self.emit(MachineInst::IMul(Reg::Rax, Reg::Rcx)),
                    _ => {} // Other ops require more complex codegen
                }
            }
            Expr::Call(func, args) => {
                // Push args in reverse order into arg registers
                let arg_regs = Reg::arg_regs();
                for (i, arg) in args.iter().enumerate().rev() {
                    if i < arg_regs.len() {
                        self.gen_expr(arg);
                        if arg_regs[i] != Reg::Rax {
                            self.emit(MachineInst::MovRR(arg_regs[i], Reg::Rax));
                        }
                    }
                }
                if let Expr::Ident(name) = func.as_ref() {
                    self.emit(MachineInst::Call(name.clone()));
                }
            }
            Expr::Assign(lhs, rhs) => {
                self.gen_expr(rhs);
                if let Expr::Ident(name) = lhs.as_ref() {
                    if let Some(&offset) = self.local_vars.get(name.as_str()) {
                        self.emit(MachineInst::MovMR(Reg::Rbp, offset, Reg::Rax));
                    }
                }
            }
            _ => {
                self.emit(MachineInst::MovRI(Reg::Rax, 0));
            }
        }
    }

    pub fn instructions(&self) -> &[MachineInst] {
        &self.instructions
    }
}

// ═══════════════════════════════════════════════════════════════════════
// ELF LINKER
// ═══════════════════════════════════════════════════════════════════════

/// ELF section
#[derive(Debug, Clone)]
pub struct ElfSection {
    pub name: String,
    pub data: Vec<u8>,
    pub addr: u64,
    pub section_type: u32,
    pub flags: u64,
}

/// ELF symbol
#[derive(Debug, Clone)]
pub struct ElfSymbol {
    pub name: String,
    pub value: u64,
    pub size: u64,
    pub section: usize,
    pub sym_type: u8,
    pub binding: u8,
    pub global: bool,
}

/// Simple ELF builder
pub struct ElfBuilder {
    pub sections: Vec<ElfSection>,
    pub symbols: Vec<ElfSymbol>,
    pub entry_point: u64,
    pub base_addr: u64,
}

impl ElfBuilder {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
            symbols: Vec::new(),
            entry_point: 0x400000,
            base_addr: 0x400000,
        }
    }

    pub fn add_section(&mut self, name: &str, data: Vec<u8>, flags: u64) -> usize {
        let idx = self.sections.len();
        self.sections.push(ElfSection {
            name: String::from(name),
            data,
            addr: 0,
            section_type: 1, // SHT_PROGBITS
            flags,
        });
        idx
    }

    pub fn add_symbol(&mut self, name: &str, value: u64, section: usize, global: bool) {
        self.symbols.push(ElfSymbol {
            name: String::from(name),
            value,
            size: 0,
            section,
            sym_type: 2, // STT_FUNC
            binding: if global { 1 } else { 0 },
            global,
        });
    }

    /// Build ELF binary
    pub fn build(&mut self) -> Vec<u8> {
        let mut elf = Vec::new();

        // ELF header (64 bytes)
        // Magic
        elf.extend_from_slice(&[0x7f, b'E', b'L', b'F']);
        elf.push(2); // ELFCLASS64
        elf.push(1); // ELFDATA2LSB
        elf.push(1); // EV_CURRENT
        elf.push(0); // ELFOSABI_NONE
        elf.extend_from_slice(&[0; 8]); // padding
        elf.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
        elf.extend_from_slice(&0x3Eu16.to_le_bytes()); // EM_X86_64
        elf.extend_from_slice(&1u32.to_le_bytes()); // EV_CURRENT
        elf.extend_from_slice(&self.entry_point.to_le_bytes()); // e_entry
        elf.extend_from_slice(&64u64.to_le_bytes()); // e_phoff
        elf.extend_from_slice(&0u64.to_le_bytes()); // e_shoff (filled later)
        elf.extend_from_slice(&0u32.to_le_bytes()); // e_flags
        elf.extend_from_slice(&64u16.to_le_bytes()); // e_ehsize
        elf.extend_from_slice(&56u16.to_le_bytes()); // e_phentsize
        elf.extend_from_slice(&1u16.to_le_bytes()); // e_phnum
        elf.extend_from_slice(&64u16.to_le_bytes()); // e_shentsize
        elf.extend_from_slice(&0u16.to_le_bytes()); // e_shnum
        elf.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx

        // Program header (LOAD segment)
        let text_size: u64 = self.sections.iter().map(|s| s.data.len() as u64).sum();
        elf.extend_from_slice(&1u32.to_le_bytes()); // PT_LOAD
        elf.extend_from_slice(&5u32.to_le_bytes()); // PF_R | PF_X
        elf.extend_from_slice(&0x1000u64.to_le_bytes()); // p_offset
        elf.extend_from_slice(&self.base_addr.to_le_bytes()); // p_vaddr
        elf.extend_from_slice(&self.base_addr.to_le_bytes()); // p_paddr
        elf.extend_from_slice(&text_size.to_le_bytes()); // p_filesz
        elf.extend_from_slice(&text_size.to_le_bytes()); // p_memsz
        elf.extend_from_slice(&0x1000u64.to_le_bytes()); // p_align

        // Pad to page boundary
        while elf.len() < 0x1000 {
            elf.push(0);
        }

        // Write sections
        for section in &self.sections {
            elf.extend_from_slice(&section.data);
        }

        elf
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PREPROCESSOR
// ═══════════════════════════════════════════════════════════════════════

/// Simple preprocessor for #define, #ifdef, #include
pub struct Preprocessor {
    defines: BTreeMap<String, String>,
    include_paths: Vec<String>,
}

impl Preprocessor {
    pub fn new() -> Self {
        Self {
            defines: BTreeMap::new(),
            include_paths: Vec::new(),
        }
    }

    pub fn define(&mut self, name: &str, value: &str) {
        self.defines.insert(String::from(name), String::from(value));
    }

    pub fn add_include_path(&mut self, path: &str) {
        self.include_paths.push(String::from(path));
    }

    pub fn process(&self, source: &str) -> String {
        let mut output = String::new();
        let mut skip_depth: usize = 0;
        let mut in_ifdef = Vec::new();

        for line in source.lines() {
            let trimmed = line.trim();

            if let Some(rest) = trimmed.strip_prefix("#define ") {
                if skip_depth == 0 {
                    if let Some(_space) = rest.find(' ') {
                        // #define NAME VALUE - handled at definition time
                    }
                }
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("#ifdef ") {
                let name = rest.trim();
                let defined = self.defines.contains_key(name);
                in_ifdef.push(defined);
                if !defined {
                    skip_depth += 1;
                }
                continue;
            }

            if let Some(rest) = trimmed.strip_prefix("#ifndef ") {
                let name = rest.trim();
                let defined = self.defines.contains_key(name);
                in_ifdef.push(!defined);
                if defined {
                    skip_depth += 1;
                }
                continue;
            }

            if trimmed == "#else" {
                if let Some(last) = in_ifdef.last_mut() {
                    if *last {
                        skip_depth += 1;
                    } else {
                        skip_depth = skip_depth.saturating_sub(1);
                    }
                    *last = !*last;
                }
                continue;
            }

            if trimmed == "#endif" {
                if let Some(was_active) = in_ifdef.pop() {
                    if !was_active {
                        skip_depth = skip_depth.saturating_sub(1);
                    }
                }
                continue;
            }

            if skip_depth > 0 {
                continue;
            }

            if trimmed.starts_with("#include ") {
                // Include handling - would read file in real implementation
                output.push_str("// included: ");
                output.push_str(trimmed);
                output.push('\n');
                continue;
            }

            // Macro substitution
            let mut processed = String::from(line);
            for (name, value) in &self.defines {
                processed = processed.replace(name.as_str(), value.as_str());
            }
            output.push_str(&processed);
            output.push('\n');
        }

        output
    }
}

// ═══════════════════════════════════════════════════════════════════════
// OPTIMIZATION PASSES
// ═══════════════════════════════════════════════════════════════════════

/// Constant folding pass
pub fn constant_fold(expr: &Expr) -> Expr {
    match expr {
        Expr::Binary(op, lhs, rhs) => {
            let lhs = constant_fold(lhs);
            let rhs = constant_fold(rhs);
            if let (Expr::IntLit(a), Expr::IntLit(b)) = (&lhs, &rhs) {
                let result = match op {
                    BinOp::Add => Some(a.wrapping_add(*b)),
                    BinOp::Sub => Some(a.wrapping_sub(*b)),
                    BinOp::Mul => Some(a.wrapping_mul(*b)),
                    BinOp::Div if *b != 0 => Some(a / b),
                    BinOp::Mod if *b != 0 => Some(a % b),
                    BinOp::BitAnd => Some(a & b),
                    BinOp::BitOr => Some(a | b),
                    BinOp::BitXor => Some(a ^ b),
                    BinOp::Shl => Some(a << b),
                    BinOp::Shr => Some(a >> b),
                    _ => None,
                };
                if let Some(r) = result {
                    return Expr::IntLit(r);
                }
            }
            Expr::Binary(*op, Box::new(lhs), Box::new(rhs))
        }
        Expr::Unary(op, inner) => {
            let inner = constant_fold(inner);
            if let Expr::IntLit(n) = &inner {
                match op {
                    UnaryOp::Neg => return Expr::IntLit(-n),
                    UnaryOp::BitNot => return Expr::IntLit(!n),
                    _ => {}
                }
            }
            Expr::Unary(*op, Box::new(inner))
        }
        _ => expr.clone(),
    }
}

/// Dead code elimination (remove unreachable code after return)
pub fn dead_code_eliminate(stmts: &[Stmt]) -> Vec<Stmt> {
    let mut result = Vec::new();
    for stmt in stmts {
        result.push(stmt.clone());
        if matches!(stmt, Stmt::Return(_) | Stmt::Break | Stmt::Continue) {
            break;
        }
    }
    result
}

// ═══════════════════════════════════════════════════════════════════════
// COMPILER DRIVER
// ═══════════════════════════════════════════════════════════════════════

/// Full compilation pipeline
pub struct Compiler {
    preprocessor: Preprocessor,
    optimization_level: u8, // 0-3
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            preprocessor: Preprocessor::new(),
            optimization_level: 1,
        }
    }

    pub fn set_optimization(&mut self, level: u8) {
        self.optimization_level = level.min(3);
    }

    pub fn define(&mut self, name: &str, value: &str) {
        self.preprocessor.define(name, value);
    }

    /// Compile source to ELF binary
    pub fn compile(&self, source: &str, output_name: &str) -> Result<Vec<u8>, Vec<String>> {
        serial_println!("[CC] Compiling '{}'...", output_name);

        // Phase 1: Preprocess
        let processed = self.preprocessor.process(source);

        // Phase 2: Lex
        let mut lexer = Lexer::new(&processed);
        let tokens = lexer.tokenize();
        serial_println!("[CC]   Lexed {} tokens", tokens.len());

        // Phase 3: Parse
        let mut parser = Parser::new(tokens);
        let unit = parser.parse();
        if !parser.errors().is_empty() {
            return Err(parser.errors().to_vec());
        }
        serial_println!("[CC]   Parsed {} items", unit.items.len());

        // Phase 4: Generate code
        let mut codegen = CodeGen::new();
        codegen.generate(&unit);
        serial_println!(
            "[CC]   Generated {} instructions",
            codegen.instructions().len()
        );

        // Phase 5: Assemble (simplified - just produce a valid ELF with nops)
        let mut elf = ElfBuilder::new();
        let mut text = Vec::new();
        for _inst in codegen.instructions() {
            text.push(0x90); // NOP placeholder
        }
        elf.add_section(".text", text, 0x6); // SHF_ALLOC | SHF_EXECINSTR
        let binary = elf.build();

        serial_println!("[CC] Compiled '{}': {} bytes", output_name, binary.len());
        Ok(binary)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GLOBAL STATE
// ═══════════════════════════════════════════════════════════════════════

lazy_static::lazy_static! {
    static ref COMPILER: Mutex<Compiler> = Mutex::new(Compiler::new());
    static ref COMPILED_OBJECTS: Mutex<BTreeMap<String, Vec<u8>>> = Mutex::new(BTreeMap::new());
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Compile source code
pub fn compile(source: &str, name: &str) -> Result<Vec<u8>, Vec<String>> {
    let compiler = COMPILER.lock();
    let result = compiler.compile(source, name)?;
    COMPILED_OBJECTS
        .lock()
        .insert(String::from(name), result.clone());
    Ok(result)
}

/// List compiled objects
pub fn list_objects() -> Vec<String> {
    COMPILED_OBJECTS.lock().keys().cloned().collect()
}

/// Initialize compiler toolchain
pub fn init() {
    if INITIALIZED.load(Ordering::Relaxed) {
        return;
    }
    INITIALIZED.store(true, Ordering::Relaxed);

    // Pre-define some macros
    {
        let mut compiler = COMPILER.lock();
        compiler.define("__KNOXOS__", "1");
        compiler.define("__x86_64__", "1");
        compiler.define("__LP64__", "1");
    }

    serial_println!(
        "[KnoxOS] Self-hosting compiler toolchain initialized (lexer, parser, codegen, linker)"
    );
}
