use alloc::boxed::Box;
use alloc::vec::Vec;

use super::ast::{BinOp, Expr, Stmt, UnaryOp};

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
