//! Lexer: source text -> `Vec<Token>` (with spans) plus any lexical errors.
//!
//! Scanning never aborts: a bad lexeme becomes a `TokenKind::Invalid` token
//! and an entry in `LexOutput::errors`, so one run reports every lexical error.
//! The parser consumes its own simplified token enum; `main.rs` bridges the two.

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Keyword,
    Delimiter,
    Identifier,
    Operator,
    Literal(Literal),
    Invalid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Str(String),
    Char(char),
    Bool(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub lexeme: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LexErrorKind {
    UnterminatedString,
    UnterminatedChar,
    EmptyCharLiteral,
    OverlongCharLiteral,
    InvalidEscape(char),
    UnexpectedChar(char),
    IntegerOverflow,
    FloatParseError,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub kind: LexErrorKind,
    pub span: Span,
}

impl LexError {
    fn new(kind: LexErrorKind, span: Span) -> Self {
        Self { kind, span }
    }
}

pub struct LexOutput {
    pub tokens: Vec<Token>,
    pub errors: Vec<LexError>,
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

    /// Builds a token covering `start..` up to the current position.
    fn make_token(&mut self, kind: TokenKind, start: usize, line: u32, column: u32) -> Token {
        let span = self.span_from(start, line, column);
        let lexeme = self.source[span.start..span.end].to_owned();
        Token { kind, lexeme, span }
    }

    /// Skips whitespace and `//` line comments.
    fn skip_trivia(&mut self) {
        loop {
            self.advance_while(char::is_whitespace);
            let at = self.peek_offset().unwrap_or(self.source.len());
            if !self.source[at..].starts_with("//") {
                break;
            }
            self.advance_while(|c| c != '\n');
        }
    }

    // Scanning functions
    //
    // Each expects to be called with the first character of the lexeme still
    // unconsumed (`lex` only peeks), and returns with the lexeme fully consumed.

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

        match (content, error_kind) {
            (Some(c), None) => {
                ScanResult::ok(self.make_token(TokenKind::Literal(Literal::Char(c)), start, line, col))
            }
            (_, kind) => {
                let token = self.make_token(TokenKind::Invalid, start, line, col);
                // Content is only ever missing alongside an error.
                let kind = kind.unwrap_or(LexErrorKind::UnterminatedChar);
                ScanResult::err(LexError::new(kind, token.span.clone()), token)
            }
        }
    }

    fn scan_string(&mut self, start: usize, line: u32, col: u32) -> ScanResult {
        self.advance(); // opening "

        let (value, errors, terminated) =
            StringChars::new(self, start, line, col)
            .fold((String::new(), Vec::new(), false), |(mut value, mut errors, _), item| {
                match item {
                    StringChar::Char(c) => { value.push(c); (value, errors, false) }
                    StringChar::BadEscape(c, e) => { value.push(c); errors.push(e); (value, errors, false) }
                    StringChar::Closed => (value, errors, true),
                    StringChar::Unterminated => (value, errors, false),
                }
            });

        if !terminated {
            // Unterminated supersedes any escape errors
            let token = self.make_token(TokenKind::Invalid, start, line, col);
            let error = LexError::new(LexErrorKind::UnterminatedString, token.span.clone());
            return ScanResult::err(error, token);
        }

        let token = self.make_token(TokenKind::Literal(Literal::Str(value)), start, line, col);
        ScanResult::with_errors(token, errors)
    }

    // Only supports base 10 currently
    fn scan_number(&mut self, start: usize, line: u32, col: u32) -> ScanResult {
        let is_float = NumberChars::new(self)
            .fold(false, |seen_dot, event| matches!(event, NumberChar::DecimalPoint) || seen_dot);

        let mut token = self.make_token(TokenKind::Invalid, start, line, col);
        let parsed = if is_float {
            token.lexeme.parse::<f64>().map(Literal::Float).map_err(|_| LexErrorKind::FloatParseError)
        } else {
            token.lexeme.parse::<i64>().map(Literal::Int).map_err(|_| LexErrorKind::IntegerOverflow)
        };

        match parsed {
            Ok(literal) => {
                token.kind = TokenKind::Literal(literal);
                ScanResult::ok(token)
            }
            Err(kind) => {
                let error = LexError::new(kind, token.span.clone());
                ScanResult::err(error, token)
            }
        }
    }

    fn scan_ident(&mut self, start: usize, line: u32, col: u32) -> ScanResult {
        self.advance_while(is_ident_cont);
        let mut token = self.make_token(TokenKind::Identifier, start, line, col);
        token.kind = match token.lexeme.as_str() {
            "true" => TokenKind::Literal(Literal::Bool(true)),
            "false" => TokenKind::Literal(Literal::Bool(false)),
            kw if is_keyword(kw) => TokenKind::Keyword,
            _ => TokenKind::Identifier,
        };
        ScanResult::ok(token)
    }

    fn scan_operator(&mut self, start_byte: usize, line: u32, col: u32) -> ScanResult {
        // All two-character operators are ASCII, so byte slicing is safe.
        let is_pair = TWO_CHAR_OPERATORS.iter().any(|op| self.source[start_byte..].starts_with(op));
        self.advance();
        if is_pair {
            self.advance();
        }
        ScanResult::ok(self.make_token(TokenKind::Operator, start_byte, line, col))
    }

    fn delimiter_token(&mut self, start_byte: usize, line: u32, col: u32) -> Token {
        self.advance();
        self.make_token(TokenKind::Delimiter, start_byte, line, col)
    }
}

/// Scans the whole source. Never fails outright: bad input yields `Invalid`
/// tokens plus entries in `LexOutput::errors`, and scanning carries on.
pub fn lex(source: &str) -> LexOutput {
    let mut lexer = Lexer::new(source);
    let mut output = LexOutput { tokens: Vec::new(), errors: Vec::new() };

    loop {
        lexer.skip_trivia();
        let (Some(c), Some(start)) = (lexer.peek(), lexer.peek_offset()) else { break };
        let (line, col) = (lexer.line, lexer.column);

        let ScanResult { token, errors } = match c {
            '"' => lexer.scan_string(start, line, col),
            '\'' => lexer.scan_char(start, line, col),
            c if c.is_ascii_digit() => lexer.scan_number(start, line, col),
            c if is_ident_start(c) => lexer.scan_ident(start, line, col),
            c if is_operator(c) => lexer.scan_operator(start, line, col),
            c if is_delimiter(c) => ScanResult::ok(lexer.delimiter_token(start, line, col)),
            c => {
                lexer.advance();
                let token = lexer.make_token(TokenKind::Invalid, start, line, col);
                let error = LexError::new(LexErrorKind::UnexpectedChar(c), token.span.clone());
                ScanResult::err(error, token)
            }
        };

        output.tokens.push(token);
        output.errors.extend(errors);
    }

    output
}

// Character helper functions

fn is_ident_start(c: char) -> bool { c.is_alphabetic() || c == '_' }
fn is_ident_cont(c: char) -> bool { c.is_alphanumeric() || c == '_' }
fn is_operator(c: char) ->  bool { "+-*/=<>!&|^%".contains(c) }
fn is_delimiter(c: char) -> bool { "(){}[];,:.".contains(c) }

const TWO_CHAR_OPERATORS: [&str; 7] = ["==", "!=", "<=", ">=", "&&", "||", "->"];

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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lexemes(src: &str) -> Vec<String> {
        lex(src).tokens.into_iter().map(|t| t.lexeme).collect()
    }

    #[test]
    fn scans_operators_and_delimiters_and_skips_comments() {
        let out = lex("fn f() -> i32 { a <= 1 } // trailing\n&& != x");
        assert!(out.errors.is_empty());
        assert_eq!(
            lexemes("fn f() -> i32 { a <= 1 } // trailing\n&& != x"),
            ["fn", "f", "(", ")", "->", "i32", "{", "a", "<=", "1", "}", "&&", "!=", "x"]
        );
    }

    #[test]
    fn bad_input_is_reported_and_scanning_continues() {
        let out = lex("let x = 1 @ 2;");
        assert_eq!(out.errors.len(), 1);
        assert_eq!(out.errors[0].kind, LexErrorKind::UnexpectedChar('@'));
        assert_eq!(out.tokens.len(), 7); // let x = 1 @ 2 ;
    }
}
