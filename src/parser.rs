//! =====================================================================
//! OUTLINE
//! =====================================================================
//!
//! Grammar (EBNF-ish), lowest to highest precedence:
//!
//!   program     := item*
//!   item        := fn_decl
//!   fn_decl     := "fn" IDENT "(" params? ")" ("->" type)? block
//!   params      := param ("," param)*
//!   param       := IDENT ":" type
//!   type        := "i32" | "bool" | "(" ")"          // unit type
//!
//!   block       := "{" stmt* expr? "}"               // last expr = tail value (rust-like)
//!   stmt        := let_stmt
//!                | "return" expr? ";"
//!                | while_stmt
//!                | expr_stmt                          // if/while/block used as stmt need no ";"
//!   let_stmt    := "let" "mut"? IDENT (":" type)? "=" expr ";"
//!   while_stmt  := "while" expr block
//!   expr_stmt   := expr ";"?                          // ";" required unless expr is block-like
//!                                                      // and it's the tail expression of a block
//!
//!   expr        := assignment
//!   assignment  := IDENT "=" assignment | logic_or
//!   logic_or    := logic_and ("||" logic_and)*
//!   logic_and   := equality ("&&" equality)*
//!   equality    := comparison (("=="|"!=") comparison)*
//!   comparison  := term (("<"|">"|"<="|">=") term)*
//!   term        := factor (("+"|"-") factor)*
//!   factor      := unary (("*"|"/"|"%") unary)*
//!   unary       := ("-"|"!") unary | call
//!   call        := primary ("(" args? ")")*
//!   args        := expr ("," expr)*
//!   primary     := INT | "true" | "false" | IDENT
//!                | "(" expr ")"
//!                | block                              // block is itself an expression
//!                | if_expr
//!   if_expr     := "if" expr block ("else" (block | if_expr))?
//!
//! Notes / design decisions:
//!  - if/while conditions have no parens (rust-style), block is required.
//!  - if is an expression (can appear as a block's tail expr); while is stmt-only
//!    (loops don't produce a useful value here, keeps things simple).
//!  - Precedence climbing is done via a cascade of methods (equality -> comparison
//!    -> term -> factor -> unary), each calling the next tighter-binding one.
//!    This mirrors the classic recursive-descent expression parser and is easy
//!    to extend (e.g. add bitwise ops as another rung on the ladder).
//!  - Assignment is right-associative and only valid when the LHS is an
//!    identifier (checked after parsing logic_or, similar to how many
//!    recursive-descent parsers handle assignment without a separate LHS grammar).
//!  - Type checking, name resolution etc. are out of scope for the parser;
//!    it just builds an AST. A later pass validates types/scopes.
//!
//! Parser implementation strategy:
//!  - Token stream + cursor (Vec<Token>, usize pos). peek()/advance()/check()/
//!    expect() helpers keep every production readable.
//!  - Each grammar rule -> one parser method returning Result<Node, ParseError>.
//!  - Errors carry the offending token + a message; no recovery/synchronization
//!    is implemented here (could add panic-mode recovery at statement
//!    boundaries by scanning to the next ";" or "}" if partial-parse diagnostics
//!    are needed later).
//!
//! =====================================================================

// ---------------------------------------------------------------------
// Tokens (produced by the lexer -- shown here only for context/testing)
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // literals / identifiers
    Int(i64),
    Ident(String),
    True,
    False,

    // keywords
    Fn,
    Let,
    Mut,
    If,
    Else,
    While,
    Return,

    // types
    TyI32,
    TyBool,

    // punctuation
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Semicolon,
    Arrow, // ->

    // operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,       // =
    EqEq,     // ==
    NotEq,    // !=
    Lt,
    Gt,
    LtEq,
    GtEq,
    AndAnd,
    OrOr,
    Bang,

    Eof,
}

// ---------------------------------------------------------------------
// AST
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    I32,
    Bool,
    Unit,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub items: Vec<Item>,
}

#[derive(Debug, Clone)]
pub enum Item {
    Function(FunctionDecl),
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Type, // Type::Unit if omitted
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>, // trailing expr with no ";" (block's value)
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Let {
        name: String,
        mutable: bool,
        ty: Option<Type>,
        value: Expr,
    },
    Return(Option<Expr>),
    While {
        cond: Expr,
        body: Block,
    },
    Expr(Expr), // expression statement (semicolon-terminated)
}

#[derive(Debug, Clone)]
pub enum Expr {
    IntLit(i64),
    BoolLit(bool),
    Ident(String),
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Assign {
        name: String,
        value: Box<Expr>,
    },
    Call {
        callee: String,
        args: Vec<Expr>,
    },
    If {
        cond: Box<Expr>,
        then_branch: Block,
        else_branch: Option<Box<ElseBranch>>,
    },
    Block(Block),
}

#[derive(Debug, Clone)]
pub enum ElseBranch {
    Block(Block),
    If(Box<Expr>), // must be Expr::If, kept generic to avoid a separate type
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    And,
    Or,
}

// ---------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub found: Token,
    pub pos: usize,
}

type PResult<T> = Result<T, ParseError>;

// ---------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    // ---- low-level cursor helpers (fully implemented: trivial) ----

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if !matches!(tok, Token::Eof) {
            self.pos += 1;
        }
        tok
    }

    fn check(&self, tok: &Token) -> bool {
        self.peek() == tok
    }

    fn matches(&mut self, tok: &Token) -> bool {
        if self.check(tok) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, tok: &Token, ctx: &str) -> PResult<Token> {
        if self.check(tok) {
            Ok(self.advance())
        } else {
            Err(self.error(&format!("expected {:?} {}", tok, ctx)))
        }
    }

    fn error(&self, message: &str) -> ParseError {
        ParseError {
            message: message.to_string(),
            found: self.peek().clone(),
            pos: self.pos,
        }
    }

    fn expect_ident(&mut self) -> PResult<String> {
        match self.advance() {
            Token::Ident(name) => Ok(name),
            other => Err(ParseError {
                message: "expected identifier".into(),
                found: other,
                pos: self.pos,
            }),
        }
    }

    // ---- top level ----

    pub fn parse_program(&mut self) -> PResult<Program> {
        let mut items = Vec::new();
        while !self.check(&Token::Eof) {
            items.push(self.parse_item()?);
        }
        Ok(Program { items })
    }

    fn parse_item(&mut self) -> PResult<Item> {
        // Only functions for now; extend here for structs/consts/etc.
        self.expect(&Token::Fn, "at start of item")?;
        Ok(Item::Function(self.parse_function_decl()?))
    }

    fn parse_function_decl(&mut self) -> PResult<FunctionDecl> {
        let name = self.expect_ident()?;
        self.expect(&Token::LParen, "after function name")?;
        let params = self.parse_params()?;
        self.expect(&Token::RParen, "after parameters")?;

        let return_type = if self.matches(&Token::Arrow) {
            self.parse_type()?
        } else {
            Type::Unit
        };

        let body = self.parse_block()?;

        Ok(FunctionDecl {
            name,
            params,
            return_type,
            body,
        })
    }

    fn parse_params(&mut self) -> PResult<Vec<Param>> {
        let mut params = Vec::new();
        if self.check(&Token::RParen) {
            return Ok(params);
        }
        loop {
            let name = self.expect_ident()?;
            self.expect(&Token::Colon, "after parameter name")?;
            let ty = self.parse_type()?;
            params.push(Param { name, ty });
            if !self.matches(&Token::Comma) {
                break;
            }
        }
        Ok(params)
    }

    fn parse_type(&mut self) -> PResult<Type> {
        match self.advance() {
            Token::TyI32 => Ok(Type::I32),
            Token::TyBool => Ok(Type::Bool),
            Token::LParen => {
                self.expect(&Token::RParen, "to close unit type '()'")?;
                Ok(Type::Unit)
            }
            other => Err(ParseError {
                message: "expected a type (i32, bool, or ())".into(),
                found: other,
                pos: self.pos,
            }),
        }
    }

    // ---- statements / blocks ----

    fn parse_block(&mut self) -> PResult<Block> {
        self.expect(&Token::LBrace, "to start block")?;

        let mut stmts = Vec::new();
        let mut tail = None;

        while !self.check(&Token::RBrace) {
            if self.starts_stmt_keyword() {
                stmts.push(self.parse_stmt()?);
                continue;
            }

            let expr = self.parse_expr()?;
            let is_block_like = matches!(expr, Expr::If { .. } | Expr::Block(_));

            if self.matches(&Token::Semicolon) {
                // Explicit ";" always makes it a statement, whatever the expr is.
                stmts.push(Stmt::Expr(expr));
            } else if self.check(&Token::RBrace) {
                // Nothing follows -> this is the block's tail value.
                tail = Some(Box::new(expr));
                break;
            } else if is_block_like {
                // Rust-style rule: block-like expressions (if/else, {}) don't
                // need a trailing ";" to be used as a statement, e.g.
                //   if cond { do_a(); } else { do_b(); }
                //   let x = 1;
                stmts.push(Stmt::Expr(expr));
            } else {
                // Anything else with more tokens following but no ";" and no
                // "}" is malformed input (e.g. `x + 1 y`) -- reject it rather
                // than silently treating it as a (wrong) tail expression.
                return Err(self.error("expected `;` after expression statement"));
            }
        }

        self.expect(&Token::RBrace, "to close block")?;
        Ok(Block { stmts, tail })
    }

    /// True if the upcoming token unambiguously starts a non-expression
    /// statement (let / return / while). Anything else is parsed as an
    /// expression-statement (covers if-as-statement, blocks, calls, etc).
    fn starts_stmt_keyword(&self) -> bool {
        matches!(self.peek(), Token::Let | Token::Return | Token::While)
    }

    fn parse_stmt(&mut self) -> PResult<Stmt> {
        match self.peek() {
            Token::Let => self.parse_let_stmt(),
            Token::Return => self.parse_return_stmt(),
            Token::While => self.parse_while_stmt(),
            _ => unreachable!("guarded by starts_stmt_keyword"),
        }
    }

    fn parse_let_stmt(&mut self) -> PResult<Stmt> {
        self.expect(&Token::Let, "at start of let statement")?;
        let mutable = self.matches(&Token::Mut);
        let name = self.expect_ident()?;

        let ty = if self.matches(&Token::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(&Token::Eq, "in let statement")?;
        let value = self.parse_expr()?;
        self.expect(&Token::Semicolon, "after let statement")?;

        Ok(Stmt::Let {
            name,
            mutable,
            ty,
            value,
        })
    }

    fn parse_return_stmt(&mut self) -> PResult<Stmt> {
        self.expect(&Token::Return, "at start of return statement")?;
        let value = if self.check(&Token::Semicolon) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(&Token::Semicolon, "after return statement")?;
        Ok(Stmt::Return(value))
    }

    fn parse_while_stmt(&mut self) -> PResult<Stmt> {
        self.expect(&Token::While, "at start of while statement")?;
        let cond = self.parse_expr()?;
        let body = self.parse_block()?;
        Ok(Stmt::While { cond, body })
    }

    // ---- expressions (precedence climbing) ----

    fn parse_expr(&mut self) -> PResult<Expr> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> PResult<Expr> {
        // Parse the lower-precedence chain first; if we land on a bare
        // identifier immediately followed by '=', reinterpret as assignment.
        let expr = self.parse_logic_or()?;

        if self.matches(&Token::Eq) {
            let value = self.parse_assignment()?; // right-associative
            match expr {
                Expr::Ident(name) => Ok(Expr::Assign {
                    name,
                    value: Box::new(value),
                }),
                _ => Err(self.error("invalid assignment target")),
            }
        } else {
            Ok(expr)
        }
    }

    fn parse_logic_or(&mut self) -> PResult<Expr> {
        let mut expr = self.parse_logic_and()?;
        while self.matches(&Token::OrOr) {
            let rhs = self.parse_logic_and()?;
            expr = Expr::Binary {
                op: BinOp::Or,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        Ok(expr)
    }

    fn parse_logic_and(&mut self) -> PResult<Expr> {
        let mut expr = self.parse_equality()?;
        while self.matches(&Token::AndAnd) {
            let rhs = self.parse_equality()?;
            expr = Expr::Binary {
                op: BinOp::And,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> PResult<Expr> {
        let mut expr = self.parse_comparison()?;
        loop {
            let op = match self.peek() {
                Token::EqEq => BinOp::Eq,
                Token::NotEq => BinOp::NotEq,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_comparison()?;
            expr = Expr::Binary {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        Ok(expr)
    }

    fn parse_comparison(&mut self) -> PResult<Expr> {
        let mut expr = self.parse_term()?;
        loop {
            let op = match self.peek() {
                Token::Lt => BinOp::Lt,
                Token::Gt => BinOp::Gt,
                Token::LtEq => BinOp::LtEq,
                Token::GtEq => BinOp::GtEq,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_term()?;
            expr = Expr::Binary {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        Ok(expr)
    }

    fn parse_term(&mut self) -> PResult<Expr> {
        let mut expr = self.parse_factor()?;
        loop {
            let op = match self.peek() {
                Token::Plus => BinOp::Add,
                Token::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_factor()?;
            expr = Expr::Binary {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        Ok(expr)
    }

    fn parse_factor(&mut self) -> PResult<Expr> {
        let mut expr = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Star => BinOp::Mul,
                Token::Slash => BinOp::Div,
                Token::Percent => BinOp::Mod,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_unary()?;
            expr = Expr::Binary {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> PResult<Expr> {
        let op = match self.peek() {
            Token::Minus => Some(UnOp::Neg),
            Token::Bang => Some(UnOp::Not),
            _ => None,
        };
        if let Some(op) = op {
            self.advance();
            let expr = self.parse_unary()?;
            Ok(Expr::Unary {
                op,
                expr: Box::new(expr),
            })
        } else {
            self.parse_call()
        }
    }

    fn parse_call(&mut self) -> PResult<Expr> {
        let expr = self.parse_primary()?;

        if self.check(&Token::LParen) {
            let callee = match expr {
                Expr::Ident(name) => name,
                _ => return Err(self.error("expression is not callable")),
            };
            self.advance(); // consume '('
            let args = self.parse_args()?;
            self.expect(&Token::RParen, "after call arguments")?;
            Ok(Expr::Call { callee, args })
        } else {
            Ok(expr)
        }
    }

    fn parse_args(&mut self) -> PResult<Vec<Expr>> {
        let mut args = Vec::new();
        if self.check(&Token::RParen) {
            return Ok(args);
        }
        loop {
            args.push(self.parse_expr()?);
            if !self.matches(&Token::Comma) {
                break;
            }
        }
        Ok(args)
    }

    fn parse_primary(&mut self) -> PResult<Expr> {
        match self.peek().clone() {
            Token::Int(n) => {
                self.advance();
                Ok(Expr::IntLit(n))
            }
            Token::True => {
                self.advance();
                Ok(Expr::BoolLit(true))
            }
            Token::False => {
                self.advance();
                Ok(Expr::BoolLit(false))
            }
            Token::Ident(name) => {
                self.advance();
                Ok(Expr::Ident(name))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen, "to close parenthesized expression")?;
                Ok(expr)
            }
            Token::LBrace => Ok(Expr::Block(self.parse_block()?)),
            Token::If => self.parse_if_expr(),
            other => Err(ParseError {
                message: "expected an expression".into(),
                found: other,
                pos: self.pos,
            }),
        }
    }

    fn parse_if_expr(&mut self) -> PResult<Expr> {
        self.expect(&Token::If, "at start of if expression")?;
        let cond = self.parse_expr()?; // no parens, rust-style
        let then_branch = self.parse_block()?;

        let else_branch = if self.matches(&Token::Else) {
            if self.check(&Token::If) {
                Some(Box::new(ElseBranch::If(Box::new(self.parse_if_expr()?))))
            } else {
                Some(Box::new(ElseBranch::Block(self.parse_block()?)))
            }
        } else {
            None
        };

        Ok(Expr::If {
            cond: Box::new(cond),
            then_branch,
            else_branch,
        })
    }
}

// ---------------------------------------------------------------------
// Extension points not implemented (kept out to avoid scope creep):
//  - arrays / structs / enums
//  - for-loops, break/continue
//  - operator precedence table for user-defined ops
//  - error recovery (currently stops at first ParseError)
// Each would slot into the grammar above at an obvious point (e.g. `for`
// alongside `while` in parse_stmt; struct/enum as new Item variants).
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_function() {
        // fn add(a: i32, b: i32) -> i32 { a + b }
        let tokens = vec![
            Token::Fn,
            Token::Ident("add".into()),
            Token::LParen,
            Token::Ident("a".into()),
            Token::Colon,
            Token::TyI32,
            Token::Comma,
            Token::Ident("b".into()),
            Token::Colon,
            Token::TyI32,
            Token::RParen,
            Token::Arrow,
            Token::TyI32,
            Token::LBrace,
            Token::Ident("a".into()),
            Token::Plus,
            Token::Ident("b".into()),
            Token::RBrace,
            Token::Eof,
        ];
        let program = Parser::new(tokens).parse_program().unwrap();
        assert_eq!(program.items.len(), 1);
    }

    /// fn f() { if true { } let y = 1; }
    /// `if` used as a *statement* (not the tail) must not require a ';'.
    #[test]
    fn if_statement_without_semicolon_is_followed_by_more_statements() {
        let tokens = vec![
            Token::Fn,
            Token::Ident("f".into()),
            Token::LParen,
            Token::RParen,
            Token::LBrace,
            Token::If,
            Token::True,
            Token::LBrace,
            Token::RBrace,
            Token::Let,
            Token::Ident("y".into()),
            Token::Eq,
            Token::Int(1),
            Token::Semicolon,
            Token::RBrace,
            Token::Eof,
        ];
        let program = Parser::new(tokens).parse_program().unwrap();
        let Item::Function(f) = &program.items[0];
        // Both the `if` and the `let` should show up as statements, with no tail.
        assert_eq!(f.body.stmts.len(), 2);
        assert!(f.body.tail.is_none());
    }

    /// fn f() -> i32 { if true { 1 } else { 2 } }
    /// `if` with no trailing tokens before '}' is still correctly treated
    /// as the block's tail expression.
    #[test]
    fn if_as_tail_expression_still_works() {
        let tokens = vec![
            Token::Fn,
            Token::Ident("f".into()),
            Token::LParen,
            Token::RParen,
            Token::Arrow,
            Token::TyI32,
            Token::LBrace,
            Token::If,
            Token::True,
            Token::LBrace,
            Token::Int(1),
            Token::RBrace,
            Token::Else,
            Token::LBrace,
            Token::Int(2),
            Token::RBrace,
            Token::RBrace,
            Token::Eof,
        ];
        let program = Parser::new(tokens).parse_program().unwrap();
        let Item::Function(f) = &program.items[0];
        assert!(f.body.stmts.is_empty());
        assert!(matches!(f.body.tail.as_deref(), Some(Expr::If { .. })));
    }

    /// fn f() { x y }  -- missing ';' between two non-block-like expressions
    /// must be a parse error, not silently accepted.
    #[test]
    fn missing_semicolon_between_expressions_is_an_error() {
        let tokens = vec![
            Token::Fn,
            Token::Ident("f".into()),
            Token::LParen,
            Token::RParen,
            Token::LBrace,
            Token::Ident("x".into()),
            Token::Ident("y".into()),
            Token::RBrace,
            Token::Eof,
        ];
        let result = Parser::new(tokens).parse_program();
        assert!(result.is_err());
    }
}
