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
    Literal,
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
                    Ok(u) => String::Char(u),
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

struct Lexer<'a> {
    source: &'a str
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
    
    fn peek(&self) -> Option<char> {
        self.chars.peek().map(|&(_, c)| c) 
    }

    fn peek_offset(&self) -> Option<usize> {
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
        let end = self.peek_offset().unwrap_or(self.source_len());
        Span { start, end, line, column }
    }

    fn slice(&self, span: &Span) -> &'a str {
        &self.source[span.start..span.end]
    }

    fn at_eof(&mut self) -> bool {
        self.cahrs.peek().is_none()
    }

    // Scanning functions
    
    fn scan_char(&mut self, start_byte: usize, line: u32, col: u32) -> ScanResult {
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
                literal: content.map(LiteralValue::Char),
                span,
            }),
        }
    }

    fn scan_string(&mut self, start_byte: usize, line: u32, col: u32) -> ScanResult {
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
                    literal: Some(LiteralValue::Str(value)),
                    span
                },
                errors,)
            }
            (true, true) => {
                ScanResult::ok(Token {
                    kind: TokenKind::Literal(LiteralKind::Str),
                    lexeme,
                    literal: Some(LiteralValue::Str(value)),
                    span,
                })
            }
        }
    }

    fn number_token(&mut self, start_byte: usize, line: u32, col: u32) -> Token {

    }

    fn ident_token(&mut self, start_byte: usize, line: u32, col: u32) -> Token {
        
    }

    fn operator_token(&mut self, start_byte: usize, line: u32, col: u32) -> Token {

    }

    fn delimiter_token(&mut self, start_byte: usize, line: u32, col: u32) -> Token {

    }
}

fn lex(input: &String) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    let chars = input.chars();

    let mut current = String::new();
    
    for character in chars {
        if !character.is_alphanumeric() && character != '-' && character != '_' {
            tokens.push(create_token(&current));
            current = String::new();
        }
        current.push(character);
    }
    tokens.push(create_token(&current));

    // return stream of tokens
    tokens
}

fn create_token(lexeme: &String) -> Token {
    // this is kinda bad
    let operators = HashSet::from(["+", "-", "*", "/", "="]); // and more
    let keywords = HashSet::from(["fn", "if", "struct", "enum", "let", "mut"]);

    if operators.contains(lexeme) {

    } else if keywords.contains(lexeme) {

    }
    ()
}

fn parse(tokens: &Vec<Token>) {
    let mut tree: Tree<AutomatedId, &Token> = Tree::new(Some("AST")); 

    for token in tokens.iter() {
        match token.kind {
            TokenKind::Keyword => ;
            TokenKind::Delimiter => ;
            TokenKind::Identifier => ;
            TokenKind::Operator => ;
            TokenKind::Literal => ;
        }
    }
}
