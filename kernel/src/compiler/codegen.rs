use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use super::ast::{BinOp, CompilationUnit, Expr, Item, Stmt};

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
