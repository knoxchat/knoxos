use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

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
