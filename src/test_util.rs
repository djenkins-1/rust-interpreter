//! AST-builder helpers shared by the typechecker and interpreter tests, so
//! test programs describe intent instead of drowning in `Box::new()` noise.

use crate::parser::*;

pub fn int(n: i64) -> Expr { Expr::IntLit(n) }
pub fn boolean(b: bool) -> Expr { Expr::BoolLit(b) }
pub fn ident(n: &str) -> Expr { Expr::Ident(n.to_string()) }
pub fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
    Expr::Binary { op, lhs: Box::new(l), rhs: Box::new(r) }
}
pub fn call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::Call { callee: name.to_string(), args }
}
pub fn block(stmts: Vec<Stmt>, tail: Option<Expr>) -> Block {
    Block { stmts, tail: tail.map(Box::new) }
}
pub fn let_stmt(name: &str, mutable: bool, ty: Option<Type>, value: Expr) -> Stmt {
    Stmt::Let { name: name.to_string(), mutable, ty, value }
}
pub fn param(name: &str, ty: Type) -> Param {
    Param { name: name.to_string(), ty }
}
pub fn func(name: &str, params: Vec<Param>, return_type: Type, body: Block) -> FunctionDecl {
    FunctionDecl { name: name.to_string(), params, return_type, body }
}
pub fn program(fns: Vec<FunctionDecl>) -> Program {
    Program { items: fns.into_iter().map(Item::Function).collect() }
}
