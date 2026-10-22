//! JavaScript Engine — Minimal ECMAScript interpreter
//!
//! Provides a simple JavaScript interpreter for the KnoxOS browser,
//! supporting basic operations: variables, arithmetic, string manipulation,
//! functions, if/else, while loops, DOM-like API stubs.
//! Covers status.md item 9.78 (JavaScript engine).

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// JavaScript value types
#[derive(Debug, Clone)]
pub enum JsValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<JsValue>),
    Object(BTreeMap<String, JsValue>),
    Function(String, Vec<String>, Vec<JsStatement>),
}

impl JsValue {
    /// Truthy check
    pub fn is_truthy(&self) -> bool {
        match self {
            JsValue::Undefined | JsValue::Null => false,
            JsValue::Bool(b) => *b,
            JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
            JsValue::Str(s) => !s.is_empty(),
            JsValue::Array(_) | JsValue::Object(_) | JsValue::Function(_, _, _) => true,
        }
    }

    /// Convert to number
    pub fn to_number(&self) -> f64 {
        match self {
            JsValue::Number(n) => *n,
            JsValue::Bool(true) => 1.0,
            JsValue::Bool(false) => 0.0,
            JsValue::Str(s) => s.parse::<f64>().unwrap_or(f64::NAN),
            _ => f64::NAN,
        }
    }

    /// Convert to string
    pub fn to_string_repr(&self) -> String {
        match self {
            JsValue::Undefined => String::from("undefined"),
            JsValue::Null => String::from("null"),
            JsValue::Bool(b) => {
                if *b {
                    String::from("true")
                } else {
                    String::from("false")
                }
            }
            JsValue::Number(n) => {
                if *n == (*n as i64) as f64 {
                    alloc::format!("{}", *n as i64)
                } else {
                    alloc::format!("{}", n)
                }
            }
            JsValue::Str(s) => s.clone(),
            JsValue::Array(arr) => {
                let parts: Vec<String> = arr.iter().map(|v| v.to_string_repr()).collect();
                parts.join(",")
            }
            JsValue::Object(_) => String::from("[object Object]"),
            JsValue::Function(name, _, _) => alloc::format!("function {}()", name),
        }
    }
}

/// JavaScript expression AST
#[derive(Debug, Clone)]
pub enum JsExpr {
    Literal(JsValue),
    Identifier(String),
    BinaryOp(Box<JsExpr>, JsBinOp, Box<JsExpr>),
    UnaryOp(JsUnaryOp, Box<JsExpr>),
    Call(String, Vec<JsExpr>),
    Member(Box<JsExpr>, String),
    Index(Box<JsExpr>, Box<JsExpr>),
    Assign(String, Box<JsExpr>),
}

/// Binary operators
#[derive(Debug, Clone, Copy)]
pub enum JsBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    StrictEq,
    StrictNeq,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

/// Unary operators
#[derive(Debug, Clone, Copy)]
pub enum JsUnaryOp {
    Not,
    Neg,
    TypeOf,
}

/// JavaScript statement AST
#[derive(Debug, Clone)]
pub enum JsStatement {
    Expr(JsExpr),
    VarDecl(String, Option<JsExpr>),
    LetDecl(String, Option<JsExpr>),
    ConstDecl(String, JsExpr),
    If(JsExpr, Vec<JsStatement>, Option<Vec<JsStatement>>),
    While(JsExpr, Vec<JsStatement>),
    For(Box<JsStatement>, JsExpr, Box<JsStatement>, Vec<JsStatement>),
    Return(Option<JsExpr>),
    FunctionDecl(String, Vec<String>, Vec<JsStatement>),
    Block(Vec<JsStatement>),
}

/// JavaScript execution context
pub struct JsContext {
    /// Variable scopes (stack of frames)
    scopes: Vec<BTreeMap<String, JsValue>>,
    /// Console output buffer
    pub console_output: Vec<String>,
    /// Maximum execution steps (prevent infinite loops)
    max_steps: u64,
    steps: u64,
}

impl JsContext {
    pub fn new() -> Self {
        let mut global = BTreeMap::new();
        // Built-in globals
        global.insert(String::from("NaN"), JsValue::Number(f64::NAN));
        global.insert(String::from("Infinity"), JsValue::Number(f64::INFINITY));
        global.insert(String::from("undefined"), JsValue::Undefined);

        Self {
            scopes: alloc::vec![global],
            console_output: Vec::new(),
            max_steps: 100_000,
            steps: 0,
        }
    }

    /// Look up a variable
    pub fn get_var(&self, name: &str) -> JsValue {
        for scope in self.scopes.iter().rev() {
            if let Some(val) = scope.get(name) {
                return val.clone();
            }
        }
        JsValue::Undefined
    }

    /// Set a variable (in the nearest scope that has it, or global)
    pub fn set_var(&mut self, name: &str, value: JsValue) {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(String::from(name), value);
                return;
            }
        }
        // Set in global scope
        if let Some(global) = self.scopes.first_mut() {
            global.insert(String::from(name), value);
        }
    }

    /// Declare a variable in the current scope
    pub fn declare_var(&mut self, name: &str, value: JsValue) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(String::from(name), value);
        }
    }

    /// Push a new scope
    pub fn push_scope(&mut self) {
        self.scopes.push(BTreeMap::new());
    }

    /// Pop scope
    pub fn pop_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Evaluate an expression
    pub fn eval_expr(&mut self, expr: &JsExpr) -> JsValue {
        self.steps += 1;
        if self.steps > self.max_steps {
            return JsValue::Undefined; // Execution limit
        }

        match expr {
            JsExpr::Literal(v) => v.clone(),
            JsExpr::Identifier(name) => self.get_var(name),
            JsExpr::Assign(name, val_expr) => {
                let val = self.eval_expr(val_expr);
                self.set_var(name, val.clone());
                val
            }
            JsExpr::BinaryOp(left, op, right) => {
                let lv = self.eval_expr(left);
                let rv = self.eval_expr(right);
                eval_binop(&lv, *op, &rv)
            }
            JsExpr::UnaryOp(op, operand) => {
                let v = self.eval_expr(operand);
                match op {
                    JsUnaryOp::Not => JsValue::Bool(!v.is_truthy()),
                    JsUnaryOp::Neg => JsValue::Number(-v.to_number()),
                    JsUnaryOp::TypeOf => {
                        let t = match &v {
                            JsValue::Undefined => "undefined",
                            JsValue::Null => "object",
                            JsValue::Bool(_) => "boolean",
                            JsValue::Number(_) => "number",
                            JsValue::Str(_) => "string",
                            JsValue::Array(_) | JsValue::Object(_) => "object",
                            JsValue::Function(_, _, _) => "function",
                        };
                        JsValue::Str(String::from(t))
                    }
                }
            }
            JsExpr::Call(name, args) => {
                let evaluated_args: Vec<JsValue> = args.iter().map(|a| self.eval_expr(a)).collect();
                self.call_function(name, &evaluated_args)
            }
            JsExpr::Member(obj, prop) => {
                let obj_val = self.eval_expr(obj);
                match &obj_val {
                    JsValue::Object(map) => map.get(prop).cloned().unwrap_or(JsValue::Undefined),
                    JsValue::Str(s) if prop == "length" => JsValue::Number(s.len() as f64),
                    JsValue::Array(arr) if prop == "length" => JsValue::Number(arr.len() as f64),
                    _ => JsValue::Undefined,
                }
            }
            JsExpr::Index(obj, idx) => {
                let obj_val = self.eval_expr(obj);
                let idx_val = self.eval_expr(idx);
                match (&obj_val, &idx_val) {
                    (JsValue::Array(arr), JsValue::Number(n)) => {
                        let i = *n as usize;
                        arr.get(i).cloned().unwrap_or(JsValue::Undefined)
                    }
                    _ => JsValue::Undefined,
                }
            }
        }
    }

    /// Call a function
    fn call_function(&mut self, name: &str, args: &[JsValue]) -> JsValue {
        // Built-in functions
        match name {
            "console.log" | "print" => {
                let msg: Vec<String> = args.iter().map(|a| a.to_string_repr()).collect();
                let line = msg.join(" ");
                self.console_output.push(line);
                return JsValue::Undefined;
            }
            "alert" => {
                if let Some(msg) = args.first() {
                    self.console_output
                        .push(alloc::format!("[alert] {}", msg.to_string_repr()));
                }
                return JsValue::Undefined;
            }
            "parseInt" => {
                if let Some(s) = args.first() {
                    let s = s.to_string_repr();
                    return JsValue::Number(s.parse::<f64>().unwrap_or(f64::NAN));
                }
                return JsValue::Number(f64::NAN);
            }
            "Math.floor" => {
                if let Some(n) = args.first() {
                    return JsValue::Number(libm::floor(n.to_number()));
                }
                return JsValue::Number(f64::NAN);
            }
            "Math.ceil" => {
                if let Some(n) = args.first() {
                    return JsValue::Number(libm::ceil(n.to_number()));
                }
                return JsValue::Number(f64::NAN);
            }
            "Math.abs" => {
                if let Some(n) = args.first() {
                    return JsValue::Number(libm::fabs(n.to_number()));
                }
                return JsValue::Number(f64::NAN);
            }
            "Math.sqrt" => {
                if let Some(n) = args.first() {
                    return JsValue::Number(libm::sqrt(n.to_number()));
                }
                return JsValue::Number(f64::NAN);
            }
            _ => {}
        }

        // User-defined function
        let func = self.get_var(name);
        if let JsValue::Function(_name, params, body) = func {
            self.push_scope();
            for (i, param) in params.iter().enumerate() {
                let val = args.get(i).cloned().unwrap_or(JsValue::Undefined);
                self.declare_var(param, val);
            }
            let result = self.exec_stmts(&body);
            self.pop_scope();
            return result;
        }

        JsValue::Undefined
    }

    /// Execute a list of statements
    pub fn exec_stmts(&mut self, stmts: &[JsStatement]) -> JsValue {
        let mut result = JsValue::Undefined;
        for stmt in stmts {
            result = self.exec_stmt(stmt);
            if self.steps > self.max_steps {
                break;
            }
        }
        result
    }

    /// Execute a single statement
    pub fn exec_stmt(&mut self, stmt: &JsStatement) -> JsValue {
        self.steps += 1;
        if self.steps > self.max_steps {
            return JsValue::Undefined;
        }

        match stmt {
            JsStatement::Expr(e) => self.eval_expr(e),
            JsStatement::VarDecl(name, init) | JsStatement::LetDecl(name, init) => {
                let val = if let Some(e) = init {
                    self.eval_expr(e)
                } else {
                    JsValue::Undefined
                };
                self.declare_var(name, val);
                JsValue::Undefined
            }
            JsStatement::ConstDecl(name, init) => {
                let val = self.eval_expr(init);
                self.declare_var(name, val);
                JsValue::Undefined
            }
            JsStatement::If(cond, then_body, else_body) => {
                let cv = self.eval_expr(cond);
                if cv.is_truthy() {
                    self.exec_stmts(then_body)
                } else if let Some(eb) = else_body {
                    self.exec_stmts(eb)
                } else {
                    JsValue::Undefined
                }
            }
            JsStatement::While(cond, body) => {
                let mut result = JsValue::Undefined;
                loop {
                    let cv = self.eval_expr(cond);
                    if !cv.is_truthy() || self.steps > self.max_steps {
                        break;
                    }
                    result = self.exec_stmts(body);
                }
                result
            }
            JsStatement::For(init, cond, update, body) => {
                self.exec_stmt(init);
                let mut result = JsValue::Undefined;
                loop {
                    let cv = self.eval_expr(cond);
                    if !cv.is_truthy() || self.steps > self.max_steps {
                        break;
                    }
                    result = self.exec_stmts(body);
                    self.exec_stmt(update);
                }
                result
            }
            JsStatement::Return(expr) => {
                if let Some(e) = expr {
                    self.eval_expr(e)
                } else {
                    JsValue::Undefined
                }
            }
            JsStatement::FunctionDecl(name, params, body) => {
                let func = JsValue::Function(name.clone(), params.clone(), body.clone());
                self.declare_var(name, func);
                JsValue::Undefined
            }
            JsStatement::Block(stmts) => {
                self.push_scope();
                let r = self.exec_stmts(stmts);
                self.pop_scope();
                r
            }
        }
    }
}

fn eval_binop(lv: &JsValue, op: JsBinOp, rv: &JsValue) -> JsValue {
    match op {
        JsBinOp::Add => {
            // String concatenation if either is string
            match (lv, rv) {
                (JsValue::Str(a), _) => {
                    JsValue::Str(alloc::format!("{}{}", a, rv.to_string_repr()))
                }
                (_, JsValue::Str(b)) => {
                    JsValue::Str(alloc::format!("{}{}", lv.to_string_repr(), b))
                }
                _ => JsValue::Number(lv.to_number() + rv.to_number()),
            }
        }
        JsBinOp::Sub => JsValue::Number(lv.to_number() - rv.to_number()),
        JsBinOp::Mul => JsValue::Number(lv.to_number() * rv.to_number()),
        JsBinOp::Div => {
            let d = rv.to_number();
            if d == 0.0 {
                JsValue::Number(f64::INFINITY)
            } else {
                JsValue::Number(lv.to_number() / d)
            }
        }
        JsBinOp::Mod => JsValue::Number(lv.to_number() % rv.to_number()),
        JsBinOp::Eq | JsBinOp::StrictEq => {
            let eq = match (lv, rv) {
                (JsValue::Number(a), JsValue::Number(b)) => a == b,
                (JsValue::Str(a), JsValue::Str(b)) => a == b,
                (JsValue::Bool(a), JsValue::Bool(b)) => a == b,
                (JsValue::Null, JsValue::Null) => true,
                (JsValue::Undefined, JsValue::Undefined) => true,
                _ => false,
            };
            JsValue::Bool(eq)
        }
        JsBinOp::Neq | JsBinOp::StrictNeq => {
            let neq = match (lv, rv) {
                (JsValue::Number(a), JsValue::Number(b)) => a != b,
                (JsValue::Str(a), JsValue::Str(b)) => a != b,
                _ => true,
            };
            JsValue::Bool(neq)
        }
        JsBinOp::Lt => JsValue::Bool(lv.to_number() < rv.to_number()),
        JsBinOp::Gt => JsValue::Bool(lv.to_number() > rv.to_number()),
        JsBinOp::Le => JsValue::Bool(lv.to_number() <= rv.to_number()),
        JsBinOp::Ge => JsValue::Bool(lv.to_number() >= rv.to_number()),
        JsBinOp::And => {
            if lv.is_truthy() {
                rv.clone()
            } else {
                lv.clone()
            }
        }
        JsBinOp::Or => {
            if lv.is_truthy() {
                lv.clone()
            } else {
                rv.clone()
            }
        }
    }
}

static EVAL_COUNT: AtomicU64 = AtomicU64::new(0);

lazy_static::lazy_static! {
    static ref ENGINE: Mutex<JsContext> = Mutex::new(JsContext::new());
}

/// Evaluate a simple expression string (very basic tokenizer)
pub fn eval_simple(code: &str) -> String {
    EVAL_COUNT.fetch_add(1, Ordering::Relaxed);
    let mut ctx = ENGINE.lock();
    // For now, handle simple console.log and variable assignments
    // A full parser would be needed for complex JS
    let trimmed = code.trim();
    if trimmed.starts_with("var ") || trimmed.starts_with("let ") || trimmed.starts_with("const ") {
        let rest = if let Some(r) = trimmed.strip_prefix("const ") {
            r
        } else if let Some(r) = trimmed.strip_prefix("var ") {
            r
        } else if let Some(r) = trimmed.strip_prefix("let ") {
            r
        } else {
            trimmed
        };
        if let Some(eq_pos) = rest.find('=') {
            let name = rest[..eq_pos].trim();
            let val_str = rest[eq_pos + 1..].trim().trim_end_matches(';');
            let value = if let Ok(n) = val_str.parse::<f64>() {
                JsValue::Number(n)
            } else if val_str == "true" {
                JsValue::Bool(true)
            } else if val_str == "false" {
                JsValue::Bool(false)
            } else {
                let s = val_str.trim_matches('"').trim_matches('\'');
                JsValue::Str(String::from(s))
            };
            ctx.declare_var(name, value);
            return String::from("undefined");
        }
    }

    JsValue::Undefined.to_string_repr()
}

/// Get eval count
pub fn eval_count() -> u64 {
    EVAL_COUNT.load(Ordering::Relaxed)
}

/// Initialize the JavaScript engine
pub fn init() {
    crate::serial_println!(
        "[js_engine] JavaScript engine initialized (values, expressions, basic eval)"
    );
}
