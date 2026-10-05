//! CLI entry point: source text -> lex -> parse -> typecheck -> interpret.
//!
//! `parser::Token` is a flat enum tailored to this grammar (no spans, no
//! separate literal-kind wrapper), while `lexer::Token` carries source
//! position and a superset of literal/keyword kinds the parser doesn't
//! support (floats, strings, chars, struct/enum/loop/for/...). `bridge_tokens`
//! is the glue between the two: it re-maps what it can and reports a clear
//! error for anything outside this language's supported subset, rather than
//! silently mis-converting it.

mod builtins;
mod interpreter;
mod lexer;
mod parser;
mod typechecker;

#[cfg(test)]
mod test_util;

use std::process::ExitCode;

use interpreter::{Interpreter, Value};
use lexer::{Literal as LexLiteral, TokenKind};
use parser::{Parser, Token as PToken};
use typechecker::TypeChecker;

const SAMPLE_PROGRAM: &str = r#"
fn fib(n: i32) -> i32 {
    if n <= 1 {
        n
    } else {
        fib(n - 1) + fib(n - 2)
    }
}

fn main() -> i32 {
    let mut i = 0;
    while i < 10 {
        print(fib(i));
        i = i + 1;
    }
    0
}
"#;

fn main() -> ExitCode {
    let source = match std::env::args().nth(1) {
        Some(path) => match std::fs::read_to_string(&path) {
            Ok(src) => src,
            Err(e) => {
                eprintln!("error: couldn't read {path}: {e}");
                return ExitCode::FAILURE;
            }
        },
        // No file given: run a small built-in sample so `cargo run` does
        // something useful out of the box.
        None => {
            eprintln!("(no source file given, running the built-in sample program)\n");
            SAMPLE_PROGRAM.to_string()
        }
    };

    match run(&source) {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("{msg}");
            ExitCode::FAILURE
        }
    }
}

/// Runs the full pipeline and returns the value `main()` evaluated to, or a
/// single message describing whichever stage failed first.
fn run(source: &str) -> Result<Value, String> {
    let lexed = lexer::lex(source);
    if !lexed.errors.is_empty() {
        let details: Vec<String> = lexed
            .errors
            .iter()
            .map(|e| format!("line {}, column {}: {:?}", e.span.line, e.span.column, e.kind))
            .collect();
        return Err(format!("lex error(s):\n{}", details.join("\n")));
    }

    let tokens = bridge_tokens(lexed.tokens)?;

    let program = Parser::new(tokens)
        .parse_program()
        .map_err(|e| format!("parse error: {} (found {:?})", e.message, e.found))?;

    TypeChecker::new()
        .check_program(&program)
        .map_err(|e| format!("type error: {e}"))?;

    Interpreter::new(&program).run().map_err(|e| format!("runtime error: {e}"))
}

/// Converts lexer tokens into the parser's flat token enum, appending a
/// trailing `Eof`. Fails on any lexeme outside the supported subset (floats,
/// strings, chars, and keywords with no grammar support here) rather than
/// guessing at a mapping.
fn bridge_tokens(tokens: Vec<lexer::Token>) -> Result<Vec<PToken>, String> {
    let mut out = Vec::with_capacity(tokens.len() + 1);

    for tok in tokens {
        let mapped = match &tok.kind {
            TokenKind::Literal(LexLiteral::Int(n)) => PToken::Int(*n),
            TokenKind::Literal(LexLiteral::Bool(true)) => PToken::True,
            TokenKind::Literal(LexLiteral::Bool(false)) => PToken::False,
            TokenKind::Literal(LexLiteral::Float(_) | LexLiteral::Str(_) | LexLiteral::Char(_)) => {
                return Err(format!("unsupported literal `{}` (only i32 and bool literals are supported)", tok.lexeme));
            }

            TokenKind::Identifier => match tok.lexeme.as_str() {
                "i32" => PToken::TyI32,
                "bool" => PToken::TyBool,
                _ => PToken::Ident(tok.lexeme.clone()),
            },

            TokenKind::Keyword => match tok.lexeme.as_str() {
                "fn" => PToken::Fn,
                "let" => PToken::Let,
                "mut" => PToken::Mut,
                "if" => PToken::If,
                "else" => PToken::Else,
                "while" => PToken::While,
                "return" => PToken::Return,
                other => return Err(format!("`{other}` is not supported by this language subset")),
            },

            TokenKind::Delimiter => match tok.lexeme.as_str() {
                "(" => PToken::LParen,
                ")" => PToken::RParen,
                "{" => PToken::LBrace,
                "}" => PToken::RBrace,
                "," => PToken::Comma,
                ":" => PToken::Colon,
                ";" => PToken::Semicolon,
                other => return Err(format!("delimiter `{other}` is not supported by this language subset")),
            },

            TokenKind::Operator => match tok.lexeme.as_str() {
                "+" => PToken::Plus,
                "-" => PToken::Minus,
                "*" => PToken::Star,
                "/" => PToken::Slash,
                "%" => PToken::Percent,
                "=" => PToken::Eq,
                "==" => PToken::EqEq,
                "!=" => PToken::NotEq,
                "<" => PToken::Lt,
                ">" => PToken::Gt,
                "<=" => PToken::LtEq,
                ">=" => PToken::GtEq,
                "&&" => PToken::AndAnd,
                "||" => PToken::OrOr,
                "!" => PToken::Bang,
                "->" => PToken::Arrow,
                other => return Err(format!("operator `{other}` is not supported by this language subset")),
            },

            TokenKind::Invalid => {
                return Err(format!("invalid token `{}` at line {}", tok.lexeme, tok.span.line));
            }
        };
        out.push(mapped);
    }

    out.push(PToken::Eof);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_to_end_pipeline_runs_the_sample_program() {
        assert_eq!(run(SAMPLE_PROGRAM), Ok(Value::I32(0)));
    }

    #[test]
    fn bridges_a_small_program_to_matching_parser_tokens() {
        let tokens = lexer::lex("fn f(x: i32) -> bool { x == 1 }").tokens;
        let bridged = bridge_tokens(tokens).unwrap();
        assert_eq!(
            bridged,
            vec![
                PToken::Fn,
                PToken::Ident("f".into()),
                PToken::LParen,
                PToken::Ident("x".into()),
                PToken::Colon,
                PToken::TyI32,
                PToken::RParen,
                PToken::Arrow,
                PToken::TyBool,
                PToken::LBrace,
                PToken::Ident("x".into()),
                PToken::EqEq,
                PToken::Int(1),
                PToken::RBrace,
                PToken::Eof,
            ]
        );
    }

    #[test]
    fn unsupported_literal_is_a_clear_bridge_error() {
        let tokens = lexer::lex(r#""hi""#).tokens;
        assert!(bridge_tokens(tokens).is_err());
    }
}
