<<<<<<< HEAD
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
=======
use tree_ds::prelude::*;
use std::collections::HashSet;

fn main() {
    println!("Hello, world!");
}

enum TokenKind {
    Keyword,
    Delimiter,
    Identifier,
    Operator,
    Literal(LiteralKind),
    Invalid,
}

enum LiteralKind {
    Int,
    Float,
    Str,
    Char,
    Bool,
}

enum Operator {
    Addition,
    Subtraction,
    Multiplication,
    Division,
    Assignment,
}

#[derive(Clone)]
struct Span {
    start: usize,
    end: usize,
    line: u32,
    column: u32,
}

enum Literal {
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Bool(bool),
}

struct Token {
    kind: TokenKind,
    lexeme: String,
    literal: Option<Literal>,
    span: Span,
}

enum LexErrorKind {
    UnterminatedString,
    UnterminatedChar,
    EmptyCharLiteral,
    OverlongCharLiteral,
    InvalidEscape(char),
    UnexpectedChar(char),
    IntegerOverflow,
    FloatParseError,
}

struct LexError {
    kind: LexErrorKind,
    span: Span,
}

impl LexError {
    fn new(kind: LexErrorKind, span: Span) -> Self {
        Self { kind, span }
    }
}

struct LexOutput {
    tokens: Vec<Token>,
    errors: Vec<LexError>,
}

struct ScanResult {
    token: Token,
    errors: Vec<LexError>,
}

impl ScanResult {
    // No errors
    fn ok(token: Token) -> Self {
        Self { token, errors: Vec::new() }
    }

    // Token is 'Invalid' placeholder and there is exactly one error
    fn err(error: LexError, token: Token) -> Self {
        Self { token, errors: vec![error] }
    }

    // Created a token but with one or more errors e.g. bad escapes
    fn with_errors(token: Token, errors: Vec<LexError>) -> Self {
        Self { token, errors }
    }
}

enum StringChar {
    // Successful character
    Char(char),
    BadEscape(char, LexError),
    // Closing " was found - valid string
    Closed,
    // EOF was reached before closing "
    Unterminated,
}

struct StringChars<'lex, 'src> {
    lexer: &'lex mut Lexer<'src>,
    done: bool,
    // Byte offset and position of opening "
    err_start: usize,
    err_line: u32,
    err_col: u32,
}

impl<'lex, 'src> StringChars<'lex, 'src> {
    fn new(lexer: &'lex mut Lexer<'src>, start: usize, line: u32, col: u32) -> Self {
        Self { lexer, done: false, err_start: start, err_line: line, err_col: col }
    }
}

impl Iterator for StringChars<'_, '_> {
    type Item = StringChar;

    fn next(&mut self) -> Option<StringChar> {
        if self.done { return None; }

        let item = match self.lexer.advance() {
            Some('"') => { self.done = true; StringChar::Closed }
            Some('\\') => match self.lexer.advance() {
                Some(c) => match unescape(c) {
                    Ok(u) => StringChar::Char(u),
                    Err(()) => {
                        let span = self.lexer.span_from(self.err_start, self.err_line, self.err_col);
                        StringChar::BadEscape(c, LexError::new(LexErrorKind::InvalidEscape(c), span))
                    }
                }
                None => { self.done = true; StringChar::Unterminated }
            },
            Some(c) => StringChar::Char(c),
            None => { self.done = true; StringChar::Unterminated }
        };

        Some(item)
    }
}

enum NumberChar {
    Digit(char),
    DecimalPoint,
}

struct NumberChars<'lex, 'src> {
    lexer: &'lex mut Lexer<'src>,
    seen_dot: bool,
}

impl<'lex, 'src> NumberChars<'lex, 'src> {
    fn new(lexer: &'lex mut Lexer<'src>) -> Self {
        Self { lexer, seen_dot: false }
    }
}

impl Iterator for NumberChars<'_, '_> {
    type Item = NumberChar;

    fn next(&mut self) -> Option<NumberChar> {
        match self.lexer.peek() {
            Some(c) if c.is_ascii_digit() => {
                self.lexer.advance();
                Some(NumberChar::Digit(c))
            }
            // Don't consume dot speculatively - could be 3.method()
            Some('.') if !self.seen_dot => {
                let next_is_digit = self.lexer.chars.clone().nth(1).map_or(false, |(_, c)| c.is_ascii_digit());

                if next_is_digit {
                    self.seen_dot = true;
                    self.lexer.advance();
                    Some(NumberChar::DecimalPoint)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

struct Lexer<'a> {
    source: &'a str,
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    line: u32,
    column: u32,
}

impl<'a> Lexer<'a> {
    // Utility functions

    fn new(source: &'a str) -> Self {
        Lexer {
            source,
            chars: source.char_indices().peekable(), 
            line: 1, 
            column: 1, 
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek().map(|&(_, c)| c) 
    }

    fn peek_offset(&mut self) -> Option<usize> {
        self.chars.peek().map(|&(i, _)| i)
    }

    fn advance(&mut self) -> Option<char> {
        self.chars.next().map(|(_, c)| {
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            c
        })
    }

    fn advance_while(&mut self, predicate: impl Fn(char) -> bool) -> String {
        std::iter::from_fn(|| {
            if self.peek().map_or(false, &predicate) { self.advance() } else { None }
        })
        .collect()
    }

    fn advance_if(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) { self.advance(); true } else { false }
    }

    fn span_from(&mut self, start: usize, line: u32, column: u32) -> Span {
        let end = self.peek_offset().unwrap_or(self.source.len());
        Span { start, end, line, column }
    }

    fn slice(&self, span: &Span) -> &'a str {
        &self.source[span.start..span.end]
    }

    fn at_eof(&mut self) -> bool {
        self.chars.peek().is_none()
    }

    // Scanning functions

    fn scan_char(&mut self, start: usize, line: u32, col: u32) -> ScanResult {
        self.advance(); // opening '

        let (content, error_kind): (Option<char>, Option<LexErrorKind>) = match self.peek() {
            Some('\'') => { self.advance(); (None, Some(LexErrorKind::EmptyCharLiteral)) }
            None => (None, Some(LexErrorKind::UnterminatedChar)),
            Some('\\') => {
                self.advance();
                match self.advance() {
                    Some(c) => match unescape(c) {
                        Ok(u) => (Some(u), None),
                        Err(()) => (Some(c), Some(LexErrorKind::InvalidEscape(c))),
                    },
                    None => (None, Some(LexErrorKind::UnterminatedChar)),
                }
            }
            Some(c) => { self.advance(); (Some(c), None) }
        };

        let error_kind = error_kind.or_else(|| match self.peek() {
            Some('\'') => { self.advance(); None }
            Some(_) => {
                self.advance_while(|c| c != '\'');
                self.advance_if('\'');
                Some(LexErrorKind::OverlongCharLiteral)
            }
            None => Some(LexErrorKind::UnterminatedChar),
        });

        let span = self.span_from(start, line, col);
        let lexeme = self.slice(&span).to_owned();

        match error_kind {
            Some(kind) => {
                let error = LexError::new(kind, span.clone());
                ScanResult::err(error, Token { kind: TokenKind::Invalid, lexeme, literal: None, span })
            }
            None => ScanResult::ok(Token {
                kind: TokenKind::Literal(LiteralKind::Char),
                lexeme,
                literal: content.map(Literal::Char),
                span,
            }),
        }
    }

    fn scan_string(&mut self, start: usize, line: u32, col: u32) -> ScanResult {
        self.advance(); // opening "

        let (value, errors, terminated) = 
            StringChars::new(self, start, line, col)
            .fold((String::new(), Vec::new(), false), |(mut value, mut error, _), item| {
                match item {
                    StringChar::Char(c) => { value.push(c); (value, errors, false) }
                    StringChar::BadEscape(c, e) => { value.push(c); errors.push(e); (value, errors, false) }
                    StringChar::Closed => (value, errors, true),
                    StringChar::Unterminated => (value, errors, false),
                }
            });

        let span = self.span_from(start, line, col);
        let lexeme = self.slice(&span).to_owned();

        match (terminated, errors.is_empty()) {
            (false, _) => {
                // Unterminated supersedes any escape errors
                let error = LexError::new(LexErrorKind::UnterminatedString, span.clone());
                ScanResult::err(error, Token { kind: TokenKind::Invalid, lexeme, literal: None, span })
            }
            (true, false) => {
                ScanResult::with_errors( Token {
                    kind: TokenKind::Literal(LiteralKind::Str),
                    lexeme,
                    literal: Some(Literal::Str(value)),
                    span
                },
                errors,)
            }
            (true, true) => {
                ScanResult::ok(Token {
                    kind: TokenKind::Literal(LiteralKind::Str),
                    lexeme,
                    literal: Some(Literal::Str(value)),
                    span,
                })
            }
        }
    }

    // Only supports base 10 currently
    fn scan_number(&mut self, start: usize, line: u32, col: u32) -> ScanResult {
        let is_float = NumberChars::new(self)
            .fold(false, |seen_dot, event| matches!(event, NumberChar::DecimalPoint) || seen_dot);

        let span = self.span_from(start, line, col);
        let lexeme = self.slice(&span).to_owned();

        let (kind, literal) = if is_float {
            (TokenKind::Literal(LiteralKind::Float), Literal::Float(lexeme.parse().map_err(|_| LexErrorKind::FloatParseError)?))
        } else {
            (TokenKind::Literal(LiteralKind::Int), Literal::Int(lexeme.parse().map_err(|_| LexErrorKind::IntegerOverflow)?))
        };

        ScanResult::ok(Token { kind, lexeme, literal: Some(literal), span })
    }

    fn scan_ident(&mut self, start: usize, line: u32, col: u32) -> ScanResult {
        self.advance_while(is_ident_cont);
        let span = self.span_from(start, line, col);
        let lexeme = self.slice(&span).to_owned();

        let (kind, literal) = match lexeme.as_str() {
            "true" => (TokenKind::Literal(LiteralKind::Bool), Some(Literal::Bool(true))),
            "false" => (TokenKind::Literal(LiteralKind::Bool), Some(Literal::Bool(false))),
            kw if is_keyword(kw) => (TokenKind::Keyword, None),
            _ => (TokenKind::Identifier, None),
        };

        ScanResult::ok(Token { kind, lexeme, literal, span })
    }

    fn scan_operator(&mut self, start_byte: usize, line: u32, col: u32) -> ScanResult {

    }

    fn delimiter_token(&mut self, start_byte: usize, line: u32, col: u32) -> Token {

    }
}

// Character helper functions

fn is_ident_start(c: char) -> bool { c.is_alphabetic() || c == '_' }
fn is_ident_cont(c: char) -> bool { c.is_alphanumeric() || c == '_' }
fn is_operator(c: char) ->  bool { "+-*/=<>!&|^%".contains(c) }
fn is_delimiter(c: char) -> bool { "(){}[];,:.".contains(c) }

fn is_keyword(s: &str) -> bool {
    matches!(s,
        "fn" | "if" | "else" | "struct" | "enum" | "let" | "mut" | "return" | "loop" |
        "while" | "for" | "in" | "match" | "use" | "pub" | "mod" | "impl" | "self" |
        "Self" | "type" | "where"
    )
}

// Missing certain escapes, including unicode characters (\u)
fn unescape(c: char) -> Result<char, ()> {
    match c {
        'n' => Ok('\n'),
        't' => Ok('\t'),
        'r' => Ok('\r'),
        '0' => Ok('\0'),
        '\\' => Ok('\\'),
        '"' => Ok('"'),
        '\'' => Ok('\''),
        _ => Err(()),
>>>>>>> 6e87f172949d03206b13b1febb156c7010fa09f9
    }
}
