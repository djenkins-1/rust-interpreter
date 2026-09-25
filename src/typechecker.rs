//! Type checker for the language defined in `parser.rs`.
//!
//! Design:
//!  - Fail-fast: `Result<(), TypeError>`, stops at the first error found.
//!  - Validate-only: does not annotate or rebuild the AST. The interpreter
//!    consumes the *original* `Program` after this returns `Ok(())`; its
//!    `Value` enum is already self-describing at runtime, so a parallel
//!    typed-AST would just duplicate information for no benefit here.
//!  - Two-pass: pass 1 registers every function signature, pass 2 checks
//!    bodies -- this makes forward references and mutual recursion work
//!    without needing a separate "declare before use" restriction.
//!
//! Expected wiring in a real crate layout:
//!   src/parser.rs      -> Program, Expr, Stmt, Type, ... (already written)
//!   src/typechecker.rs -> this file, `use crate::parser::*;`
//!   src/interpreter.rs -> runs the same `Program` once `check_program` is `Ok`

use std::collections::HashMap;
use std::fmt;

<<<<<<< HEAD
use crate::builtins::BUILTINS;
=======
>>>>>>> 6e87f172949d03206b13b1febb156c7010fa09f9
use crate::parser::{
    BinOp, Block, ElseBranch, Expr, FunctionDecl, Item, Param, Program, Stmt, Type, UnOp,
};

// Errors

#[derive(Debug, Clone, PartialEq)]
pub enum TypeError {
    DuplicateFunction(String),
    UndefinedVariable(String),
    UndefinedFunction(String),
    AssignToImmutable(String),
    AssignTypeMismatch { name: String, expected: Type, found: Type },
    LetTypeMismatch { name: String, declared: Type, found: Type },
    ArgCountMismatch { func: String, expected: usize, found: usize },
    ArgTypeMismatch { func: String, index: usize, expected: Type, found: Type },
    ConditionNotBool { found: Type },
    IfBranchMismatch { then_ty: Type, else_ty: Type },
    UnaryOpTypeError { op: UnOp, found: Type },
    BinaryOpTypeError { op: BinOp, lhs: Type, rhs: Type },
    ReturnTypeMismatch { expected: Type, found: Type },
    FunctionBodyTypeMismatch { func: String, expected: Type, found: Type },
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeError::DuplicateFunction(name) => {
                write!(f, "function `{name}` is defined more than once")
            }
            TypeError::UndefinedVariable(name) => write!(f, "undefined variable `{name}`"),
            TypeError::UndefinedFunction(name) => write!(f, "undefined function `{name}`"),
            TypeError::AssignToImmutable(name) => write!(
                f,
                "cannot assign to immutable variable `{name}` (declare it `let mut`)"
            ),
            TypeError::AssignTypeMismatch { name, expected, found } => write!(
                f,
                "cannot assign value of type {found:?} to `{name}` of type {expected:?}"
            ),
            TypeError::LetTypeMismatch { name, declared, found } => write!(
                f,
                "`let {name}: {declared:?}` initializer has type {found:?}"
            ),
            TypeError::ArgCountMismatch { func, expected, found } => write!(
                f,
                "function `{func}` expects {expected} argument(s), found {found}"
            ),
            TypeError::ArgTypeMismatch { func, index, expected, found } => write!(
                f,
                "function `{func}` argument {index} expected {expected:?}, found {found:?}"
            ),
            TypeError::ConditionNotBool { found } => {
                write!(f, "expected a `bool` condition, found {found:?}")
            }
            TypeError::IfBranchMismatch { then_ty, else_ty } => write!(
                f,
                "`if` branches have incompatible types: {then_ty:?} vs {else_ty:?}"
            ),
            TypeError::UnaryOpTypeError { op, found } => {
                write!(f, "operator {op:?} is not defined for type {found:?}")
            }
            TypeError::BinaryOpTypeError { op, lhs, rhs } => write!(
                f,
                "operator {op:?} is not defined for types {lhs:?} and {rhs:?}"
            ),
            TypeError::ReturnTypeMismatch { expected, found } => {
                write!(f, "expected return type {expected:?}, found {found:?}")
            }
            TypeError::FunctionBodyTypeMismatch { func, expected, found } => write!(
                f,
                "function `{func}` declared to return {expected:?}, but its body evaluates to {found:?}"
            ),
        }
    }
}

impl std::error::Error for TypeError {}

type TResult<T> = Result<T, TypeError>;

// Symbol tables

#[derive(Debug, Clone)]
struct FunctionSig {
    params: Vec<Type>,
    return_type: Type,
}

#[derive(Debug, Clone)]
struct VarInfo {
    ty: Type,
    mutable: bool,
}

// Type checker

pub struct TypeChecker {
    functions: HashMap<String, FunctionSig>,
    scopes: Vec<HashMap<String, VarInfo>>, // stack of scopes
    current_fn_return: Type, // return type of function currently being checked
}

impl TypeChecker {
    pub fn new() -> Self {
<<<<<<< HEAD
        // Seed with builtin signatures so calls to e.g. `print` type-check
        // exactly like a call to any user-defined function; `register_function`
        // still catches a user program that redefines a builtin's name, since
        // it errors on any name already present in this map.
        let functions = BUILTINS
            .iter()
            .map(|b| {
                let sig = FunctionSig { params: b.params.to_vec(), return_type: b.ret.clone() };
                (b.name.to_string(), sig)
            })
            .collect();

        Self {
            functions,
=======
        Self {
            functions: HashMap::new(),
>>>>>>> 6e87f172949d03206b13b1febb156c7010fa09f9
            scopes: Vec::new(),
            current_fn_return: Type::Unit,
        }
    }

    // ---- entry point ----

    pub fn check_program(&mut self, program: &Program) -> TResult<()> {
        // Pass 1: register every signature up front. This is what lets a
        // function call another declared later in the file, including
        // mutual recursion (a and b calling each other).
        for item in &program.items {
            match item {
                Item::Function(f) => self.register_function(f)?,
            }
        }

        // Pass 2: check each body against the now-complete signature table.
        for item in &program.items {
            match item {
                Item::Function(f) => self.check_function(f)?,
            }
        }

        Ok(())
    }

    fn register_function(&mut self, f: &FunctionDecl) -> TResult<()> {
        if self.functions.contains_key(&f.name) {
            return Err(TypeError::DuplicateFunction(f.name.clone()));
        }
        let sig = FunctionSig {
            params: f.params.iter().map(|p: &Param| p.ty.clone()).collect(),
            return_type: f.return_type.clone(),
        };
        self.functions.insert(f.name.clone(), sig);
        Ok(())
    }

    // ---- functions / blocks ----

    fn check_function(&mut self, f: &FunctionDecl) -> TResult<()> {
        // Defensive reset: scopes should already be empty here (check_block
        // always pops what it pushes), but this doesn't assume that
        // invariant holds across a future refactor.
        self.scopes.clear();
        self.push_scope();

        for p in &f.params {
            // The grammar has no `mut` on parameters, so every parameter is
            // an immutable binding -- identical to a plain (non-`mut`) `let`.
            self.declare(p.name.clone(), p.ty.clone(), false);
        }

        let saved_return = std::mem::replace(&mut self.current_fn_return, f.return_type.clone());
        let body_ty = self.check_block(&f.body)?;
        self.current_fn_return = saved_return;

        if body_ty != f.return_type {
            return Err(TypeError::FunctionBodyTypeMismatch {
                func: f.name.clone(),
                expected: f.return_type.clone(),
                found: body_ty,
            });
        }

        self.pop_scope();
        Ok(())
    }

    fn check_block(&mut self, block: &Block) -> TResult<Type> {
        self.push_scope();

        for stmt in &block.stmts {
            self.check_stmt(stmt)?;
        }

        let result = match &block.tail {
            Some(expr) => self.check_expr(expr)?,
            None => Type::Unit,
        };

        self.pop_scope();
        Ok(result)
    }

    // ---- statements ----

    fn check_stmt(&mut self, stmt: &Stmt) -> TResult<()> {
        match stmt {
            Stmt::Let { name, mutable, ty, value } => {
                let found = self.check_expr(value)?;
                let bound_ty = match ty {
                    Some(declared) => {
                        if *declared != found {
                            return Err(TypeError::LetTypeMismatch {
                                name: name.clone(),
                                declared: declared.clone(),
                                found,
                            });
                        }
                        declared.clone()
                    }
                    None => found,
                };
                self.declare(name.clone(), bound_ty, *mutable);
                Ok(())
            }

            Stmt::Return(value) => {
                let found = match value {
                    Some(expr) => self.check_expr(expr)?,
                    None => Type::Unit,
                };
                if found != self.current_fn_return {
                    return Err(TypeError::ReturnTypeMismatch {
                        expected: self.current_fn_return.clone(),
                        found,
                    });
                }
                Ok(())
            }

            Stmt::While { cond, body } => {
                let cond_ty = self.check_expr(cond)?;
                if cond_ty != Type::Bool {
                    return Err(TypeError::ConditionNotBool { found: cond_ty });
                }
                self.check_block(body)?; // value discarded: a loop has no result
                Ok(())
            }

            Stmt::Expr(expr) => {
                self.check_expr(expr)?; // value discarded: expression-statement
                Ok(())
            }
        }
    }

    // ---- expressions ----

    fn check_expr(&mut self, expr: &Expr) -> TResult<Type> {
        match expr {
            Expr::IntLit(_) => Ok(Type::I32),
            Expr::BoolLit(_) => Ok(Type::Bool),

            Expr::Ident(name) => self
                .lookup(name)
                .map(|info| info.ty.clone())
                .ok_or_else(|| TypeError::UndefinedVariable(name.clone())),

            Expr::Unary { op, expr } => {
                let found = self.check_expr(expr)?;
                match op {
                    UnOp::Neg if found == Type::I32 => Ok(Type::I32),
                    UnOp::Not if found == Type::Bool => Ok(Type::Bool),
                    _ => Err(TypeError::UnaryOpTypeError { op: *op, found }),
                }
            }

            Expr::Binary { op, lhs, rhs } => {
                let lhs_ty = self.check_expr(lhs)?;
                let rhs_ty = self.check_expr(rhs)?;
                self.check_binary(*op, lhs_ty, rhs_ty)
            }

            Expr::Assign { name, value } => {
                let found = self.check_expr(value)?;
                let info = self
                    .lookup(name)
                    .ok_or_else(|| TypeError::UndefinedVariable(name.clone()))?
                    .clone();

                if !info.mutable {
                    return Err(TypeError::AssignToImmutable(name.clone()));
                }
                if info.ty != found {
                    return Err(TypeError::AssignTypeMismatch {
                        name: name.clone(),
                        expected: info.ty,
                        found,
                    });
                }
                // Matches real Rust: an assignment expression's own type is
                // `()`, so e.g. `x = (y = 5)` only type-checks if `x` is `()`.
                Ok(Type::Unit)
            }

            Expr::Call { callee, args } => {
                let sig = self
                    .functions
                    .get(callee)
                    .cloned()
                    .ok_or_else(|| TypeError::UndefinedFunction(callee.clone()))?;

                if args.len() != sig.params.len() {
                    return Err(TypeError::ArgCountMismatch {
                        func: callee.clone(),
                        expected: sig.params.len(),
                        found: args.len(),
                    });
                }

                for (i, (arg, expected)) in args.iter().zip(sig.params.iter()).enumerate() {
                    let found = self.check_expr(arg)?;
                    if found != *expected {
                        return Err(TypeError::ArgTypeMismatch {
                            func: callee.clone(),
                            index: i,
                            expected: expected.clone(),
                            found,
                        });
                    }
                }

                Ok(sig.return_type)
            }

            Expr::If { cond, then_branch, else_branch } => {
                let cond_ty = self.check_expr(cond)?;
                if cond_ty != Type::Bool {
                    return Err(TypeError::ConditionNotBool { found: cond_ty });
                }

                let then_ty = self.check_block(then_branch)?;

                match else_branch {
                    None => {
                        // No `else` => the implicit else branch is `()`,
                        // exactly like real Rust: `if cond { 5 }` is a type
                        // error even though the block alone checks to `i32`.
                        if then_ty != Type::Unit {
                            return Err(TypeError::IfBranchMismatch {
                                then_ty,
                                else_ty: Type::Unit,
                            });
                        }
                        Ok(Type::Unit)
                    }
                    Some(else_branch) => {
                        let else_ty = self.check_else_branch(else_branch)?;
                        if then_ty != else_ty {
                            return Err(TypeError::IfBranchMismatch { then_ty, else_ty });
                        }
                        Ok(then_ty)
                    }
                }
            }

            Expr::Block(block) => self.check_block(block),
        }
    }

    fn check_else_branch(&mut self, branch: &ElseBranch) -> TResult<Type> {
        match branch {
            ElseBranch::Block(block) => self.check_block(block),
            // The parser guarantees this box always holds an `Expr::If`;
            // routing through check_expr avoids duplicating If-handling here.
            ElseBranch::If(inner) => self.check_expr(inner),
        }
    }

    fn check_binary(&self, op: BinOp, lhs: Type, rhs: Type) -> TResult<Type> {
        use BinOp::*;
        match op {
            Add | Sub | Mul | Div | Mod => {
                if lhs == Type::I32 && rhs == Type::I32 {
                    Ok(Type::I32)
                } else {
                    Err(TypeError::BinaryOpTypeError { op, lhs, rhs })
                }
            }
            Lt | Gt | LtEq | GtEq => {
                if lhs == Type::I32 && rhs == Type::I32 {
                    Ok(Type::Bool)
                } else {
                    Err(TypeError::BinaryOpTypeError { op, lhs, rhs })
                }
            }
            Eq | NotEq => {
                // Equality needs matching operand types. `Unit == Unit` is
                // deliberately excluded too: it's vacuously true and not a
                // meaningful comparison to allow through.
                if lhs == rhs && lhs != Type::Unit {
                    Ok(Type::Bool)
                } else {
                    Err(TypeError::BinaryOpTypeError { op, lhs, rhs })
                }
            }
            And | Or => {
                if lhs == Type::Bool && rhs == Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(TypeError::BinaryOpTypeError { op, lhs, rhs })
                }
            }
        }
    }

    // ---- scope helpers ----

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: String, ty: Type, mutable: bool) {
        // `let` shadowing is legal (and idiomatic) in Rust-like languages,
        // so this overwrites any existing binding of the same name in the
        // *current* scope rather than erroring.
        self.scopes
            .last_mut()
            .expect("check_block/check_function always push a scope before declaring")
            .insert(name, VarInfo { ty, mutable });
    }

    fn lookup(&self, name: &str) -> Option<&VarInfo> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------
// Tests - I may revisit the tests at a later date so these will do for now
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
<<<<<<< HEAD
    use crate::test_util::*;
=======

    // Tiny AST-builder helpers so tests describe intent instead of drowning
    // in Box::new()/String::from() noise.
    fn int(n: i64) -> Expr { Expr::IntLit(n) }
    fn boolean(b: bool) -> Expr { Expr::BoolLit(b) }
    fn ident(n: &str) -> Expr { Expr::Ident(n.to_string()) }
    fn bin(op: BinOp, l: Expr, r: Expr) -> Expr {
        Expr::Binary { op, lhs: Box::new(l), rhs: Box::new(r) }
    }
    fn call(name: &str, args: Vec<Expr>) -> Expr {
        Expr::Call { callee: name.to_string(), args }
    }
    fn block(stmts: Vec<Stmt>, tail: Option<Expr>) -> Block {
        Block { stmts, tail: tail.map(Box::new) }
    }
    fn let_stmt(name: &str, mutable: bool, ty: Option<Type>, value: Expr) -> Stmt {
        Stmt::Let { name: name.to_string(), mutable, ty, value }
    }
    fn param(name: &str, ty: Type) -> Param {
        Param { name: name.to_string(), ty }
    }
    fn func(name: &str, params: Vec<Param>, return_type: Type, body: Block) -> FunctionDecl {
        FunctionDecl { name: name.to_string(), params, return_type, body }
    }
    fn program(fns: Vec<FunctionDecl>) -> Program {
        Program { items: fns.into_iter().map(Item::Function).collect() }
    }
>>>>>>> 6e87f172949d03206b13b1febb156c7010fa09f9

    #[test]
    fn valid_recursive_function_checks_ok() {
        // fn fact(n: i32) -> i32 { if n == 0 { 1 } else { n * fact(n - 1) } }
        let body = block(
            vec![],
            Some(Expr::If {
                cond: Box::new(bin(BinOp::Eq, ident("n"), int(0))),
                then_branch: block(vec![], Some(int(1))),
                else_branch: Some(Box::new(ElseBranch::Block(block(
                    vec![],
                    Some(bin(
                        BinOp::Mul,
                        ident("n"),
                        call("fact", vec![bin(BinOp::Sub, ident("n"), int(1))]),
                    )),
                )))),
            }),
        );
        let prog = program(vec![func("fact", vec![param("n", Type::I32)], Type::I32, body)]);
        assert!(TypeChecker::new().check_program(&prog).is_ok());
    }

    #[test]
    fn mutual_recursion_via_forward_reference_checks_ok() {
        // fn is_even(n: i32) -> bool { if n == 0 { true }  else { is_odd(n - 1) } }
        // fn is_odd(n: i32)  -> bool { if n == 0 { false } else { is_even(n - 1) } }
        let is_even_body = block(
            vec![],
            Some(Expr::If {
                cond: Box::new(bin(BinOp::Eq, ident("n"), int(0))),
                then_branch: block(vec![], Some(boolean(true))),
                else_branch: Some(Box::new(ElseBranch::Block(block(
                    vec![],
                    Some(call("is_odd", vec![bin(BinOp::Sub, ident("n"), int(1))])),
                )))),
            }),
        );
        let is_odd_body = block(
            vec![],
            Some(Expr::If {
                cond: Box::new(bin(BinOp::Eq, ident("n"), int(0))),
                then_branch: block(vec![], Some(boolean(false))),
                else_branch: Some(Box::new(ElseBranch::Block(block(
                    vec![],
                    Some(call("is_even", vec![bin(BinOp::Sub, ident("n"), int(1))])),
                )))),
            }),
        );
        let prog = program(vec![
            func("is_even", vec![param("n", Type::I32)], Type::Bool, is_even_body),
            func("is_odd", vec![param("n", Type::I32)], Type::Bool, is_odd_body),
        ]);
        assert!(TypeChecker::new().check_program(&prog).is_ok());
    }

    #[test]
    fn let_type_mismatch_is_rejected() {
        // fn f() { let x: bool = 1; }
        let body = block(vec![let_stmt("x", false, Some(Type::Bool), int(1))], None);
        let prog = program(vec![func("f", vec![], Type::Unit, body)]);
        assert_eq!(
            TypeChecker::new().check_program(&prog),
            Err(TypeError::LetTypeMismatch {
                name: "x".into(),
                declared: Type::Bool,
                found: Type::I32,
            })
        );
    }

    #[test]
    fn assign_to_immutable_is_rejected() {
        // fn f() { let x = 1; x = 2; }
        let body = block(
            vec![
                let_stmt("x", false, None, int(1)),
                Stmt::Expr(Expr::Assign { name: "x".into(), value: Box::new(int(2)) }),
            ],
            None,
        );
        let prog = program(vec![func("f", vec![], Type::Unit, body)]);
        assert_eq!(
            TypeChecker::new().check_program(&prog),
            Err(TypeError::AssignToImmutable("x".into()))
        );
    }

    #[test]
    fn assign_to_mutable_with_matching_type_is_ok() {
        // fn f() { let mut x = 1; x = 2; }
        let body = block(
            vec![
                let_stmt("x", true, None, int(1)),
                Stmt::Expr(Expr::Assign { name: "x".into(), value: Box::new(int(2)) }),
            ],
            None,
        );
        let prog = program(vec![func("f", vec![], Type::Unit, body)]);
        assert!(TypeChecker::new().check_program(&prog).is_ok());
    }

    #[test]
    fn binary_op_type_mismatch_is_rejected() {
        // fn f() -> i32 { true + 1 }
        let body = block(vec![], Some(bin(BinOp::Add, boolean(true), int(1))));
        let prog = program(vec![func("f", vec![], Type::I32, body)]);
        assert_eq!(
            TypeChecker::new().check_program(&prog),
            Err(TypeError::BinaryOpTypeError { op: BinOp::Add, lhs: Type::Bool, rhs: Type::I32 })
        );
    }

    #[test]
    fn if_branch_type_mismatch_is_rejected() {
        // fn f() -> i32 { if true { 1 } else { false } }
        let body = block(
            vec![],
            Some(Expr::If {
                cond: Box::new(boolean(true)),
                then_branch: block(vec![], Some(int(1))),
                else_branch: Some(Box::new(ElseBranch::Block(block(vec![], Some(boolean(false)))))),
            }),
        );
        let prog = program(vec![func("f", vec![], Type::I32, body)]);
        assert_eq!(
            TypeChecker::new().check_program(&prog),
            Err(TypeError::IfBranchMismatch { then_ty: Type::I32, else_ty: Type::Bool })
        );
    }

    #[test]
    fn if_without_else_must_be_unit() {
        // fn f() { if true { 1 } }  -- then-branch is i32, implicit else is ()
        let body = block(
            vec![Stmt::Expr(Expr::If {
                cond: Box::new(boolean(true)),
                then_branch: block(vec![], Some(int(1))),
                else_branch: None,
            })],
            None,
        );
        let prog = program(vec![func("f", vec![], Type::Unit, body)]);
        assert_eq!(
            TypeChecker::new().check_program(&prog),
            Err(TypeError::IfBranchMismatch { then_ty: Type::I32, else_ty: Type::Unit })
        );
    }

    #[test]
    fn if_without_else_and_unit_body_is_ok() {
        // fn f() { if true { } }
        let body = block(
            vec![Stmt::Expr(Expr::If {
                cond: Box::new(boolean(true)),
                then_branch: block(vec![], None),
                else_branch: None,
            })],
            None,
        );
        let prog = program(vec![func("f", vec![], Type::Unit, body)]);
        assert!(TypeChecker::new().check_program(&prog).is_ok());
    }

    #[test]
    fn variable_out_of_scope_after_block_ends() {
        // fn f() -> i32 { { let x = 1; } x }  -- x isn't visible outside the inner block
        let inner = block(vec![let_stmt("x", false, None, int(1))], None);
        let body = block(vec![Stmt::Expr(Expr::Block(inner))], Some(ident("x")));
        let prog = program(vec![func("f", vec![], Type::I32, body)]);
        assert_eq!(
            TypeChecker::new().check_program(&prog),
            Err(TypeError::UndefinedVariable("x".into()))
        );
    }

    #[test]
    fn wrong_arg_count_is_rejected() {
        // fn add(a: i32, b: i32) -> i32 { a + b }
        // fn f() -> i32 { add(1) }
        let add_body = block(vec![], Some(bin(BinOp::Add, ident("a"), ident("b"))));
        let add = func("add", vec![param("a", Type::I32), param("b", Type::I32)], Type::I32, add_body);
        let f = func("f", vec![], Type::I32, block(vec![], Some(call("add", vec![int(1)]))));
        let prog = program(vec![add, f]);
        assert_eq!(
            TypeChecker::new().check_program(&prog),
            Err(TypeError::ArgCountMismatch { func: "add".into(), expected: 2, found: 1 })
        );
    }

    #[test]
    fn wrong_arg_type_is_rejected() {
        // fn takes_bool(x: bool) -> bool { x }
        // fn f() -> bool { takes_bool(1) }
        let tb = func("takes_bool", vec![param("x", Type::Bool)], Type::Bool, block(vec![], Some(ident("x"))));
        let f = func("f", vec![], Type::Bool, block(vec![], Some(call("takes_bool", vec![int(1)]))));
        let prog = program(vec![tb, f]);
        assert_eq!(
            TypeChecker::new().check_program(&prog),
            Err(TypeError::ArgTypeMismatch {
                func: "takes_bool".into(),
                index: 0,
                expected: Type::Bool,
                found: Type::I32,
            })
        );
    }

    #[test]
    fn function_body_type_mismatch_is_rejected() {
        // fn f() -> i32 { true }
        let body = block(vec![], Some(boolean(true)));
        let prog = program(vec![func("f", vec![], Type::I32, body)]);
        assert_eq!(
            TypeChecker::new().check_program(&prog),
            Err(TypeError::FunctionBodyTypeMismatch {
                func: "f".into(),
                expected: Type::I32,
                found: Type::Bool,
            })
        );
    }

    #[test]
    fn return_type_mismatch_is_rejected() {
        // fn f() -> i32 { return true; }
        let body = block(vec![Stmt::Return(Some(boolean(true)))], None);
        let prog = program(vec![func("f", vec![], Type::I32, body)]);
        assert_eq!(
            TypeChecker::new().check_program(&prog),
            Err(TypeError::ReturnTypeMismatch { expected: Type::I32, found: Type::Bool })
        );
    }

    #[test]
    fn while_condition_must_be_bool() {
        // fn f() { while 1 { } }
        let body = block(vec![Stmt::While { cond: int(1), body: block(vec![], None) }], None);
        let prog = program(vec![func("f", vec![], Type::Unit, body)]);
        assert_eq!(
            TypeChecker::new().check_program(&prog),
            Err(TypeError::ConditionNotBool { found: Type::I32 })
        );
    }

    #[test]
    fn duplicate_function_definitions_are_rejected() {
        let f1 = func("f", vec![], Type::Unit, block(vec![], None));
        let f2 = func("f", vec![], Type::Unit, block(vec![], None));
        let prog = program(vec![f1, f2]);
        assert_eq!(
            TypeChecker::new().check_program(&prog),
            Err(TypeError::DuplicateFunction("f".into()))
        );
    }

    #[test]
    fn shadowing_within_same_scope_is_allowed() {
        // fn f() -> i32 { let x = true; let x = 1; x }  -- re-declaring `x` with a
        // different type in the same scope is legal shadowing, not an error.
        let body = block(
            vec![
                let_stmt("x", false, None, boolean(true)),
                let_stmt("x", false, None, int(1)),
            ],
            Some(ident("x")),
        );
        let prog = program(vec![func("f", vec![], Type::I32, body)]);
        assert!(TypeChecker::new().check_program(&prog).is_ok());
    }
}
