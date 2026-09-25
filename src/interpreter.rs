//! Tree-walking interpreter for the language defined in `parser.rs`.
//!
//! Precondition: the `Program` passed to `Interpreter::new` must already
//! have passed `typechecker::check_program` with `Ok(())`. This interpreter
//! does not re-validate types, scoping, or mutability -- a violation of any
//! of those invariants (undefined variable, undefined function, wrong
//! operand type) indicates a bug in the type checker, not a condition a
//! well-typed program's user could trigger, so those paths panic with a
//! diagnostic message rather than returning a `RuntimeError`.
//!
//! `RuntimeError` is reserved for conditions the type checker fundamentally
//! *cannot* rule out ahead of time: division/modulo by zero (depends on a
//! runtime value), integer overflow (depends on runtime values), an integer
//! literal that doesn't fit in `i32` (a gap deliberately left for this
//! stage to close -- see the `IntLit` arm below), and a missing `main`
//! entry point (a whole-program property the type checker doesn't check).
//!
//! Control flow: `return` can appear anywhere a statement can, including
//! inside an `if`/block used as a *sub-expression* of a larger expression
//! (e.g. `1 + (if c { return 5; 2 } else { 3 })`), since `if` and `{}` are
//! expressions in this grammar. `Flow` threads that possibility through
//! every expression evaluation, not just statement execution, so an early
//! `return` buried arbitrarily deep still unwinds correctly to the
//! enclosing function call.

use std::collections::HashMap;
use std::fmt;

use crate::builtins::lookup_builtin;
use crate::parser::{BinOp, Block, ElseBranch, Expr, FunctionDecl, Item, Program, Stmt, UnOp};

// ---------------------------------------------------------------------
// Runtime values
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    I32(i32),
    Bool(bool),
    Unit,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::I32(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Unit => write!(f, "()"),
        }
    }
}

// ---------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeError {
    DivisionByZero,
    ModuloByZero,
    IntegerOverflow,
    /// An integer literal in the source didn't fit in `i32`. The type
    /// checker doesn't range-check literals (it only tracks that an int
    /// literal has type `i32`), so this is the first point in the pipeline
    /// where the actual value is available to check against the range.
    LiteralOutOfRange(i64),
    NoMainFunction,
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::DivisionByZero => write!(f, "division by zero"),
            RuntimeError::ModuloByZero => write!(f, "modulo by zero"),
            RuntimeError::IntegerOverflow => write!(f, "integer overflow"),
            RuntimeError::LiteralOutOfRange(n) => {
                write!(f, "integer literal {n} does not fit in i32")
            }
            RuntimeError::NoMainFunction => write!(f, "no `main` function defined"),
        }
    }
}

impl std::error::Error for RuntimeError {}

type RResult<T> = Result<T, RuntimeError>;

// ---------------------------------------------------------------------
// Control flow
// ---------------------------------------------------------------------

/// The result of evaluating an expression or executing a statement/block.
/// `Return` carries a `return`-statement's value and must propagate,
/// unwinding, all the way up to the nearest enclosing function call --
/// through arbitrarily nested blocks, `if`s, and operator operands.
enum Flow {
    Normal(Value),
    Return(Value),
}

/// Evaluate `$e`; if that sub-evaluation produced an early `return`,
/// immediately bubble the `Flow::Return` out of the *enclosing* function.
/// This has to be a macro rather than a method: like `?`, it needs to
/// `return` from the caller's own stack frame, not just yield a value back
/// to it. Every method below that evaluates a sub-expression as part of a
/// larger one (a binary operand, a call argument, an assigned value, ...)
/// goes through this so a buried `return` is never silently discarded.
macro_rules! propagate {
    ($self:ident, $e:expr, $env:ident) => {
        match $self.eval($e, $env)? {
            Flow::Normal(v) => v,
            flow @ Flow::Return(_) => return Ok(flow),
        }
    };
}

// ---------------------------------------------------------------------
// Environment (variable scoping)
// ---------------------------------------------------------------------

struct Env {
    scopes: Vec<HashMap<String, Value>>,
}

impl Env {
    fn new() -> Self {
        // One scope to start: this is where a function's parameters live.
        Self { scopes: vec![HashMap::new()] }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Introduces a new binding (or shadows an existing one with the same
    /// name) in the *current* scope. Used for both function parameters and
    /// `let`.
    fn declare(&mut self, name: String, value: Value) {
        self.scopes
            .last_mut()
            .expect("Env always has at least one scope")
            .insert(name, value);
    }

    fn get(&self, name: &str) -> Value {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .cloned()
            .unwrap_or_else(|| {
                panic!("interpreter bug: undefined variable `{name}` (should have been caught by the type checker)")
            })
    }

    /// Mutates an *existing* binding in place (walks outward to find which
    /// scope owns it). The type checker already confirmed the variable
    /// exists and is `mut`, so failing to find it here is a checker bug.
    fn assign(&mut self, name: &str, value: Value) {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(slot) = scope.get_mut(name) {
                *slot = value;
                return;
            }
        }
        panic!("interpreter bug: assignment to undefined variable `{name}` (should have been caught by the type checker)");
    }
}

// ---------------------------------------------------------------------
// Interpreter
// ---------------------------------------------------------------------

pub struct Interpreter<'a> {
    functions: HashMap<&'a str, &'a FunctionDecl>,
}

impl<'a> Interpreter<'a> {
    pub fn new(program: &'a Program) -> Self {
        let mut functions = HashMap::new();
        for item in &program.items {
            match item {
                Item::Function(f) => {
                    functions.insert(f.name.as_str(), f);
                }
            }
        }
        Self { functions }
    }

    /// Entry point: calls `main()` with no arguments and returns its value.
    pub fn run(&self) -> RResult<Value> {
        let main = self.functions.get("main").ok_or(RuntimeError::NoMainFunction)?;
        self.call_function(main, Vec::new())
    }

    // ---- functions ----

    fn call_function(&self, f: &FunctionDecl, args: Vec<Value>) -> RResult<Value> {
        let mut env = Env::new();
        for (p, arg) in f.params.iter().zip(args) {
            env.declare(p.name.clone(), arg);
        }
        // A function's own `return` terminates *this* call only; whether the
        // body finished via an explicit `return` or its tail expression, the
        // caller just gets the resulting value either way.
        match self.exec_block(&f.body, &mut env)? {
            Flow::Normal(v) => Ok(v),
            Flow::Return(v) => Ok(v),
        }
    }

    // ---- blocks / statements ----

    fn exec_block(&self, block: &Block, env: &mut Env) -> RResult<Flow> {
        env.push_scope();

        for stmt in &block.stmts {
            if let Flow::Return(v) = self.exec_stmt(stmt, env)? {
                // Deliberately not popping the scope here: on this path the
                // whole call is unwinding (and on an `Err`, the whole
                // program is aborting), so `env` is discarded shortly after
                // regardless -- a temporarily unbalanced scope stack inside
                // an about-to-be-dropped `Env` can't affect anything.
                return Ok(Flow::Return(v));
            }
        }

        let result = match &block.tail {
            Some(expr) => propagate!(self, expr, env),
            None => Value::Unit,
        };

        env.pop_scope();
        Ok(Flow::Normal(result))
    }

    fn exec_stmt(&self, stmt: &Stmt, env: &mut Env) -> RResult<Flow> {
        match stmt {
            Stmt::Let { name, value, .. } => {
                let v = propagate!(self, value, env);
                env.declare(name.clone(), v);
                Ok(Flow::Normal(Value::Unit))
            }

            Stmt::Return(value) => {
                let v = match value {
                    Some(expr) => propagate!(self, expr, env),
                    None => Value::Unit,
                };
                Ok(Flow::Return(v))
            }

            Stmt::While { cond, body } => {
                loop {
                    let c = propagate!(self, cond, env);
                    let Value::Bool(c) = c else {
                        unreachable!("type checker guarantees a bool while-condition")
                    };
                    if !c {
                        break;
                    }
                    if let Flow::Return(v) = self.exec_block(body, env)? {
                        return Ok(Flow::Return(v));
                    }
                }
                Ok(Flow::Normal(Value::Unit))
            }

            Stmt::Expr(expr) => {
                // Pass the Flow straight through: Normal's value is discarded
                // by the caller (a statement's value is never used), but a
                // Return buried in here (e.g. inside an `if` used as a bare
                // statement) must still propagate.
                self.eval(expr, env)
            }
        }
    }

    // ---- expressions ----

    fn eval(&self, expr: &Expr, env: &mut Env) -> RResult<Flow> {
        match expr {
            Expr::IntLit(n) => {
                let v = i32::try_from(*n).map_err(|_| RuntimeError::LiteralOutOfRange(*n))?;
                Ok(Flow::Normal(Value::I32(v)))
            }

            Expr::BoolLit(b) => Ok(Flow::Normal(Value::Bool(*b))),

            Expr::Ident(name) => Ok(Flow::Normal(env.get(name))),

            Expr::Unary { op, expr } => {
                let v = propagate!(self, expr, env);
                let result = match (op, &v) {
                    (UnOp::Neg, Value::I32(n)) => {
                        Value::I32(n.checked_neg().ok_or(RuntimeError::IntegerOverflow)?)
                    }
                    (UnOp::Not, Value::Bool(b)) => Value::Bool(!b),
                    _ => unreachable!("type checker guarantees unary operand types"),
                };
                Ok(Flow::Normal(result))
            }

            Expr::Binary { op, lhs, rhs } => self.eval_binary(*op, lhs, rhs, env),

            Expr::Assign { name, value } => {
                let v = propagate!(self, value, env);
                env.assign(name, v);
                // Matches the type checker: an assignment expression's own
                // value is always `()`.
                Ok(Flow::Normal(Value::Unit))
            }

            Expr::Call { callee, args } => {
                let mut arg_values = Vec::with_capacity(args.len());
                for a in args {
                    arg_values.push(propagate!(self, a, env));
                }

                if let Some(builtin) = lookup_builtin(callee) {
                    return Ok(Flow::Normal((builtin.call)(&arg_values)));
                }

                let f = self
                    .functions
                    .get(callee.as_str())
                    .unwrap_or_else(|| {
                        panic!("interpreter bug: undefined function `{callee}` (should have been caught by the type checker)")
                    });
                Ok(Flow::Normal(self.call_function(f, arg_values)?))
            }

            Expr::If { cond, then_branch, else_branch } => {
                let c = propagate!(self, cond, env);
                let Value::Bool(c) = c else {
                    unreachable!("type checker guarantees a bool if-condition")
                };

                if c {
                    self.exec_block(then_branch, env)
                } else {
                    match else_branch {
                        None => Ok(Flow::Normal(Value::Unit)),
                        Some(branch) => match branch.as_ref() {
                            ElseBranch::Block(b) => self.exec_block(b, env),
                            // Grammar guarantees this is always Expr::If.
                            ElseBranch::If(inner) => self.eval(inner, env),
                        },
                    }
                }
            }

            Expr::Block(block) => self.exec_block(block, env),
        }
    }

    /// Split out from `eval` for readability: handles `&&`/`||` short-circuit
    /// evaluation specially (the right operand -- and any `return` or
    /// division-by-zero buried in it -- must not be evaluated at all once
    /// the left operand already determines the result), then dispatches
    /// every other operator to `apply_binop`.
    fn eval_binary(&self, op: BinOp, lhs: &Expr, rhs: &Expr, env: &mut Env) -> RResult<Flow> {
        if matches!(op, BinOp::And | BinOp::Or) {
            let l = propagate!(self, lhs, env);
            let Value::Bool(l_bool) = l else {
                unreachable!("type checker guarantees bool operands for && / ||")
            };
            let short_circuits = matches!((op, l_bool), (BinOp::And, false) | (BinOp::Or, true));
            if short_circuits {
                return Ok(Flow::Normal(Value::Bool(l_bool)));
            }
            let r = propagate!(self, rhs, env);
            return Ok(Flow::Normal(r));
        }

        let l = propagate!(self, lhs, env);
        let r = propagate!(self, rhs, env);
        Ok(Flow::Normal(self.apply_binop(op, l, r)?))
    }

    fn apply_binop(&self, op: BinOp, lhs: Value, rhs: Value) -> RResult<Value> {
        use BinOp::*;
        match (op, lhs, rhs) {
            (Add, Value::I32(a), Value::I32(b)) => {
                Ok(Value::I32(a.checked_add(b).ok_or(RuntimeError::IntegerOverflow)?))
            }
            (Sub, Value::I32(a), Value::I32(b)) => {
                Ok(Value::I32(a.checked_sub(b).ok_or(RuntimeError::IntegerOverflow)?))
            }
            (Mul, Value::I32(a), Value::I32(b)) => {
                Ok(Value::I32(a.checked_mul(b).ok_or(RuntimeError::IntegerOverflow)?))
            }
            (Div, Value::I32(a), Value::I32(b)) => {
                if b == 0 {
                    return Err(RuntimeError::DivisionByZero);
                }
                // Only remaining failure is i32::MIN / -1, which overflows.
                Ok(Value::I32(a.checked_div(b).ok_or(RuntimeError::IntegerOverflow)?))
            }
            (Mod, Value::I32(a), Value::I32(b)) => {
                if b == 0 {
                    return Err(RuntimeError::ModuloByZero);
                }
                Ok(Value::I32(a.checked_rem(b).ok_or(RuntimeError::IntegerOverflow)?))
            }
            (Lt, Value::I32(a), Value::I32(b)) => Ok(Value::Bool(a < b)),
            (Gt, Value::I32(a), Value::I32(b)) => Ok(Value::Bool(a > b)),
            (LtEq, Value::I32(a), Value::I32(b)) => Ok(Value::Bool(a <= b)),
            (GtEq, Value::I32(a), Value::I32(b)) => Ok(Value::Bool(a >= b)),
            (Eq, a, b) => Ok(Value::Bool(a == b)),
            (NotEq, a, b) => Ok(Value::Bool(a != b)),
            (And, _, _) | (Or, _, _) => {
                unreachable!("&&/|| are short-circuited in eval_binary, never reach here")
            }
            _ => unreachable!("type checker guarantees operand types for every other operator"),
        }
    }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Type;
    use crate::test_util::*;
    use crate::typechecker::TypeChecker;

    // `test_util::let_stmt` takes an explicit `ty`; every call site here
    // wants `None` (type inferred from the initializer).
    fn let_stmt(name: &str, mutable: bool, value: Expr) -> Stmt {
        crate::test_util::let_stmt(name, mutable, None, value)
    }

    /// Type-checks (asserting it passes, since that's a test-setup
    /// precondition, not what's under test) then runs -- mirrors the real
    /// parse -> typecheck -> interpret pipeline end to end.
    fn run_checked(program: &Program) -> RResult<Value> {
        TypeChecker::new()
            .check_program(program)
            .expect("test program should type-check");
        Interpreter::new(program).run()
    }

    #[test]
    fn evaluates_arithmetic_with_precedence() {
        // fn main() -> i32 { 1 + 2 * 3 }
        let body = block(vec![], Some(bin(BinOp::Add, int(1), bin(BinOp::Mul, int(2), int(3)))));
        let prog = program(vec![func("main", vec![], Type::I32, body)]);
        assert_eq!(run_checked(&prog), Ok(Value::I32(7)));
    }

    #[test]
    fn recursive_factorial() {
        // fn fact(n: i32) -> i32 { if n == 0 { 1 } else { n * fact(n - 1) } }
        // fn main() -> i32 { fact(5) }
        let fact_body = block(
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
        let fact = func("fact", vec![param("n", Type::I32)], Type::I32, fact_body);
        let main = func("main", vec![], Type::I32, block(vec![], Some(call("fact", vec![int(5)]))));
        let prog = program(vec![fact, main]);
        assert_eq!(run_checked(&prog), Ok(Value::I32(120)));
    }

    #[test]
    fn while_loop_accumulates_with_mutation() {
        // fn main() -> i32 {
        //     let mut sum = 0; let mut i = 1;
        //     while i <= 5 { sum = sum + i; i = i + 1; }
        //     sum
        // }
        let body = block(
            vec![
                let_stmt("sum", true, int(0)),
                let_stmt("i", true, int(1)),
                Stmt::While {
                    cond: bin(BinOp::LtEq, ident("i"), int(5)),
                    body: block(
                        vec![
                            Stmt::Expr(Expr::Assign {
                                name: "sum".into(),
                                value: Box::new(bin(BinOp::Add, ident("sum"), ident("i"))),
                            }),
                            Stmt::Expr(Expr::Assign {
                                name: "i".into(),
                                value: Box::new(bin(BinOp::Add, ident("i"), int(1))),
                            }),
                        ],
                        None,
                    ),
                },
            ],
            Some(ident("sum")),
        );
        let prog = program(vec![func("main", vec![], Type::I32, body)]);
        assert_eq!(run_checked(&prog), Ok(Value::I32(15)));
    }

    #[test]
    fn early_return_from_if_used_as_a_statement() {
        // fn main() -> i32 { if true { return 1; } 2 }
        let body = block(
            vec![Stmt::Expr(Expr::If {
                cond: Box::new(boolean(true)),
                then_branch: block(vec![Stmt::Return(Some(int(1)))], None),
                else_branch: None,
            })],
            Some(int(2)),
        );
        let prog = program(vec![func("main", vec![], Type::I32, body)]);
        assert_eq!(run_checked(&prog), Ok(Value::I32(1)));
    }

    #[test]
    fn short_circuit_and_never_evaluates_rhs() {
        // fn main() -> bool { false && (1 / 0 == 0) }
        // Without real short-circuiting this would hit DivisionByZero.
        let body = block(
            vec![],
            Some(bin(
                BinOp::And,
                boolean(false),
                bin(BinOp::Eq, bin(BinOp::Div, int(1), int(0)), int(0)),
            )),
        );
        let prog = program(vec![func("main", vec![], Type::Bool, body)]);
        assert_eq!(run_checked(&prog), Ok(Value::Bool(false)));
    }

    #[test]
    fn short_circuit_or_never_evaluates_rhs() {
        // fn main() -> bool { true || (1 / 0 == 0) }
        let body = block(
            vec![],
            Some(bin(
                BinOp::Or,
                boolean(true),
                bin(BinOp::Eq, bin(BinOp::Div, int(1), int(0)), int(0)),
            )),
        );
        let prog = program(vec![func("main", vec![], Type::Bool, body)]);
        assert_eq!(run_checked(&prog), Ok(Value::Bool(true)));
    }

    #[test]
    fn division_by_zero_is_a_runtime_error() {
        // fn main() -> i32 { 1 / 0 }
        let body = block(vec![], Some(bin(BinOp::Div, int(1), int(0))));
        let prog = program(vec![func("main", vec![], Type::I32, body)]);
        assert_eq!(run_checked(&prog), Err(RuntimeError::DivisionByZero));
    }

    #[test]
    fn modulo_by_zero_is_a_runtime_error() {
        // fn main() -> i32 { 1 % 0 }
        let body = block(vec![], Some(bin(BinOp::Mod, int(1), int(0))));
        let prog = program(vec![func("main", vec![], Type::I32, body)]);
        assert_eq!(run_checked(&prog), Err(RuntimeError::ModuloByZero));
    }

    #[test]
    fn integer_overflow_is_a_runtime_error() {
        // fn main() -> i32 { 2147483647 + 1 }  (i32::MAX + 1)
        let body = block(vec![], Some(bin(BinOp::Add, int(i32::MAX as i64), int(1))));
        let prog = program(vec![func("main", vec![], Type::I32, body)]);
        assert_eq!(run_checked(&prog), Err(RuntimeError::IntegerOverflow));
    }

    #[test]
    fn integer_literal_out_of_i32_range_is_a_runtime_error() {
        // fn main() -> i32 { 5000000000 }  -- doesn't fit in i32
        let body = block(vec![], Some(int(5_000_000_000)));
        let prog = program(vec![func("main", vec![], Type::I32, body)]);
        assert_eq!(
            run_checked(&prog),
            Err(RuntimeError::LiteralOutOfRange(5_000_000_000))
        );
    }

    #[test]
    fn missing_main_is_a_runtime_error() {
        let prog = program(vec![func("helper", vec![], Type::I32, block(vec![], Some(int(1))))]);
        assert_eq!(run_checked(&prog), Err(RuntimeError::NoMainFunction));
    }

    #[test]
    fn print_builtin_executes_and_returns_unit() {
        // fn main() { print(42); }
        let body = block(vec![Stmt::Expr(call("print", vec![int(42)]))], None);
        let prog = program(vec![func("main", vec![], Type::Unit, body)]);
        assert_eq!(run_checked(&prog), Ok(Value::Unit));
    }
}
