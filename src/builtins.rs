//! Built-in functions callable from source programs.
//!
//! Each entry carries its own signature, so the type checker (which seeds its
//! function table from `BUILTINS`) and the interpreter (which dispatches through
//! `lookup_builtin`) can never disagree about what a builtin accepts or returns.

use crate::interpreter::Value;
use crate::parser::Type;

pub struct Builtin {
    pub name: &'static str,
    pub params: &'static [Type],
    pub ret: Type,
    pub call: fn(&[Value]) -> Value,
}

pub static BUILTINS: &[Builtin] = &[
    Builtin { name: "print", params: &[Type::I32], ret: Type::Unit, call: builtin_print },
    Builtin { name: "print_bool", params: &[Type::Bool], ret: Type::Unit, call: builtin_print },
];

pub fn lookup_builtin(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|b| b.name == name)
}

// Argument count and types are guaranteed by the type checker.
fn builtin_print(args: &[Value]) -> Value {
    println!("{}", args[0]);
    Value::Unit
}
