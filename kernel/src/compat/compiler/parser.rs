use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use super::ast::{BinOp, CompilationUnit, Expr, Item, Stmt, Type, UnaryOp};
use super::lexer::{Token, TokenKind};

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
