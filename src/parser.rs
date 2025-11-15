use crate::ast::{BinaryOp, ControlFlowExpr, Expr, Program, Statement, UnaryOp};
use crate::lexer::{Span, Token, TokenKind};
use indexmap::IndexMap;
use thiserror::Error;

/// Recursive-descent parser that produces Molang AST nodes from lexer tokens.
pub struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
}

impl<'a> Parser<'a> {
    /// Creates a new parser over the provided token slice.
    pub fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    /// Parses zero or more statements until `EOF`, returning a `Program`.
    pub fn parse_program(&mut self) -> Result<Program, ParseError> {
        let mut statements = Vec::new();
        while !self.is_at_end() {
            statements.push(self.parse_statement()?);
            while self.match_semicolon() {}
        }
        Ok(Program { statements })
    }

    /// Parses a standalone expression (used for legacy eval paths and unit tests).
    pub fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_null_coalesce()?;
        self.expect_kind(|kind| matches!(kind, TokenKind::EOF), "end of input")?;
        Ok(expr)
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        if self.match_token(TokenKind::LBrace) {
            return self.parse_block();
        }

        if self.check_identifier("loop") {
            return self.parse_loop_statement();
        }

        if self.check_identifier("for_each") {
            return self.parse_for_each_statement();
        }

        if self.check_identifier("return") {
            self.advance();
            if self.match_semicolon() || self.check(TokenKind::RBrace) {
                return Ok(Statement::Return(None));
            }
            let value = self.parse_null_coalesce()?;
            return Ok(Statement::Return(Some(value)));
        } else if self.check_identifier("break") {
            self.advance();
            return Ok(Statement::Expr(Expr::Flow(ControlFlowExpr::Break)));
        } else if self.check_identifier("continue") {
            self.advance();
            return Ok(Statement::Expr(Expr::Flow(ControlFlowExpr::Continue)));
        }

        self.parse_assignment_or_expr_statement()
    }

    fn parse_block(&mut self) -> Result<Statement, ParseError> {
        let mut statements = Vec::new();
        while !self.check(TokenKind::RBrace) && !self.is_at_end() {
            statements.push(self.parse_statement()?);
            while self.match_semicolon() {}
        }
        self.expect_token(TokenKind::RBrace, "'}' to close block")?;
        Ok(Statement::Block(statements))
    }

    fn parse_assignment_or_expr_statement(&mut self) -> Result<Statement, ParseError> {
        let expr = self.parse_null_coalesce()?;
        if self.match_token(TokenKind::Equal) {
            let value = self.parse_null_coalesce()?;
            if let Expr::Path(target) = expr {
                Ok(Statement::Assignment { target, value })
            } else {
                Err(ParseError::InvalidAssignmentTarget {
                    span: self
                        .previous()
                        .map(|tok| tok.span)
                        .unwrap_or(Span { start: 0, end: 0 }),
                })
            }
        } else {
            Ok(Statement::Expr(expr))
        }
    }

    fn parse_loop_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance(); // consume loop
        self.expect_token(TokenKind::LParen, "'(' after loop keyword")?;
        let count = self.parse_null_coalesce()?;
        self.expect_token(TokenKind::Comma, "',' after loop count")?;
        let body = self.parse_embedded_body()?;
        self.expect_token(TokenKind::RParen, "')' to close loop")?;
        Ok(Statement::Loop {
            count,
            body: Box::new(body),
        })
    }

    fn parse_for_each_statement(&mut self) -> Result<Statement, ParseError> {
        self.advance(); // consume for_each
        self.expect_token(TokenKind::LParen, "'(' after for_each")?;
        let variable = self.parse_path_segments()?;
        self.expect_token(TokenKind::Comma, "',' after for_each variable")?;
        let collection = self.parse_null_coalesce()?;
        self.expect_token(TokenKind::Comma, "',' after for_each collection")?;
        let body = self.parse_embedded_body()?;
        self.expect_token(TokenKind::RParen, "')' to close for_each")?;
        Ok(Statement::ForEach {
            variable,
            collection,
            body: Box::new(body),
        })
    }

    fn parse_embedded_body(&mut self) -> Result<Statement, ParseError> {
        if self.match_token(TokenKind::LBrace) {
            self.parse_block()
        } else {
            self.parse_assignment_or_expr_statement()
        }
    }

    fn parse_path_segments(&mut self) -> Result<Vec<String>, ParseError> {
        match &self.current().kind {
            TokenKind::Identifier(_) => {
                let expr = self.parse_path_expression()?;
                if let Expr::Path(parts) = expr {
                    Ok(parts)
                } else {
                    unreachable!()
                }
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "identifier",
                found: self.current().clone(),
                span: self.current().span,
            }),
        }
    }

    fn parse_null_coalesce(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_conditional()?;
        while self.match_token(TokenKind::QuestionQuestion) {
            let right = self.parse_conditional()?;
            expr = Expr::Binary {
                op: BinaryOp::NullCoalesce,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_conditional(&mut self) -> Result<Expr, ParseError> {
        let condition = self.parse_logical_or()?;
        if self.match_token(TokenKind::Question) {
            let then_branch = self.parse_null_coalesce()?;
            let else_branch = if self.match_token(TokenKind::Colon) {
                Some(self.parse_null_coalesce()?)
            } else {
                None
            };
            Ok(Expr::Conditional {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch: else_branch.map(Box::new),
            })
        } else {
            Ok(condition)
        }
    }

    fn parse_logical_or(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_logical_and()?;
        while self.match_token(TokenKind::OrOr) {
            let right = self.parse_logical_and()?;
            expr = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_equality()?;
        while self.match_token(TokenKind::AndAnd) {
            let right = self.parse_equality()?;
            expr = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_comparison()?;
        loop {
            let op = if self.match_token(TokenKind::EqualEqual) {
                Some(BinaryOp::Equal)
            } else if self.match_token(TokenKind::BangEqual) {
                Some(BinaryOp::NotEqual)
            } else {
                None
            };
            if let Some(op) = op {
                let right = self.parse_comparison()?;
                expr = Expr::Binary {
                    op,
                    left: Box::new(expr),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_additive()?;
        loop {
            let op = if self.match_token(TokenKind::Less) {
                Some(BinaryOp::Less)
            } else if self.match_token(TokenKind::LessEqual) {
                Some(BinaryOp::LessEqual)
            } else if self.match_token(TokenKind::Greater) {
                Some(BinaryOp::Greater)
            } else if self.match_token(TokenKind::GreaterEqual) {
                Some(BinaryOp::GreaterEqual)
            } else {
                None
            };
            if let Some(op) = op {
                let right = self.parse_additive()?;
                expr = Expr::Binary {
                    op,
                    left: Box::new(expr),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_additive(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_multiplicative()?;
        loop {
            let op = if self.match_token(TokenKind::Plus) {
                Some(BinaryOp::Add)
            } else if self.match_token(TokenKind::Minus) {
                Some(BinaryOp::Sub)
            } else {
                None
            };
            if let Some(op) = op {
                let right = self.parse_multiplicative()?;
                expr = Expr::Binary {
                    op,
                    left: Box::new(expr),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_unary()?;
        loop {
            let op = if self.match_token(TokenKind::Star) {
                Some(BinaryOp::Mul)
            } else if self.match_token(TokenKind::Slash) {
                Some(BinaryOp::Div)
            } else {
                None
            };
            if let Some(op) = op {
                let right = self.parse_unary()?;
                expr = Expr::Binary {
                    op,
                    left: Box::new(expr),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.match_token(TokenKind::Plus) {
            let expr = self.parse_unary()?;
            Ok(Expr::Unary {
                op: UnaryOp::Plus,
                expr: Box::new(expr),
            })
        } else if self.match_token(TokenKind::Minus) {
            let expr = self.parse_unary()?;
            Ok(Expr::Unary {
                op: UnaryOp::Minus,
                expr: Box::new(expr),
            })
        } else if self.match_token(TokenKind::Bang) {
            let expr = self.parse_unary()?;
            Ok(Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr),
            })
        } else {
            self.parse_call()
        }
    }

    fn parse_call(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.match_token(TokenKind::LParen) {
                expr = self.finish_call(expr)?;
            } else if self.match_token(TokenKind::Dot) {
                expr = self.extend_path(expr)?;
            } else if self.match_token(TokenKind::LBracket) {
                let index = self.parse_null_coalesce()?;
                self.expect_token(TokenKind::RBracket, "']' after index expression")?;
                expr = Expr::Index {
                    target: Box::new(expr),
                    index: Box::new(index),
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        match &self.current().kind {
            TokenKind::Number(value) => {
                let number = *value;
                self.advance();
                Ok(Expr::Number(number))
            }
            TokenKind::String(value) => {
                let literal = value.clone();
                self.advance();
                Ok(Expr::String(literal))
            }
            TokenKind::LBrace => {
                self.advance();
                self.parse_struct_literal()
            }
            TokenKind::Identifier(name) => {
                if name.eq_ignore_ascii_case("break") {
                    self.advance();
                    return Ok(Expr::Flow(ControlFlowExpr::Break));
                } else if name.eq_ignore_ascii_case("continue") {
                    self.advance();
                    return Ok(Expr::Flow(ControlFlowExpr::Continue));
                }
                self.parse_path_expression()
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_null_coalesce()?;
                self.expect_token(TokenKind::RParen, "')' after expression")?;
                Ok(expr)
            }
            TokenKind::LBracket => self.parse_array_literal(),
            _ => Err(ParseError::UnexpectedToken {
                expected: "expression",
                found: self.current().clone(),
                span: self.current().span,
            }),
        }
    }

    fn parse_array_literal(&mut self) -> Result<Expr, ParseError> {
        self.expect_token(TokenKind::LBracket, "'[' to start array")?;
        let mut elements = Vec::new();
        if !self.check(TokenKind::RBracket) {
            loop {
                elements.push(self.parse_null_coalesce()?);
                if self.match_token(TokenKind::Comma) {
                    continue;
                }
                break;
            }
        }
        self.expect_token(TokenKind::RBracket, "']' to close array")?;
        Ok(Expr::Array(elements))
    }

    fn parse_struct_literal(&mut self) -> Result<Expr, ParseError> {
        let mut fields = IndexMap::new();
        if !self.check(TokenKind::RBrace) {
            loop {
                let key = match &self.current().kind {
                    TokenKind::Identifier(name) | TokenKind::String(name) => {
                        let ident = name.clone();
                        self.advance();
                        ident
                    }
                    _ => {
                        return Err(ParseError::UnexpectedToken {
                            expected: "struct field name",
                            found: self.current().clone(),
                            span: self.current().span,
                        })
                    }
                };
                self.expect_token(TokenKind::Colon, "':' after struct field")?;
                let value = self.parse_null_coalesce()?;
                if fields.insert(key.clone(), value).is_some() {
                    return Err(ParseError::DuplicateStructField { name: key });
                }
                if self.match_token(TokenKind::Comma) {
                    continue;
                }
                break;
            }
        }
        self.expect_token(TokenKind::RBrace, "'}' to close struct literal")?;
        Ok(Expr::Struct(fields))
    }

    fn parse_path_expression(&mut self) -> Result<Expr, ParseError> {
        let mut segments = Vec::new();
        segments.push(self.expect_identifier()?);
        while self.match_token(TokenKind::Dot) {
            segments.push(self.expect_identifier()?);
        }
        Ok(Expr::Path(segments))
    }

    fn finish_call(&mut self, target: Expr) -> Result<Expr, ParseError> {
        let mut args = Vec::new();
        if !self.check(TokenKind::RParen) {
            loop {
                args.push(self.parse_null_coalesce()?);
                if self.match_token(TokenKind::Comma) {
                    continue;
                }
                break;
            }
        }
        self.expect_token(TokenKind::RParen, "')' to close call")?;
        Ok(Expr::Call {
            target: Box::new(target),
            args,
        })
    }

    fn extend_path(&mut self, target: Expr) -> Result<Expr, ParseError> {
        if let Expr::Path(mut segments) = target {
            segments.push(self.expect_identifier()?);
            Ok(Expr::Path(segments))
        } else {
            Err(ParseError::UnexpectedToken {
                expected: "path",
                found: self.previous().cloned().unwrap_or_else(|| Token {
                    kind: TokenKind::EOF,
                    span: Span { start: 0, end: 0 },
                }),
                span: self.current().span,
            })
        }
    }

    fn expect_identifier(&mut self) -> Result<String, ParseError> {
        match &self.current().kind {
            TokenKind::Identifier(value) => {
                let result = value.clone();
                self.advance();
                Ok(result)
            }
            _ => Err(ParseError::UnexpectedToken {
                expected: "identifier",
                found: self.current().clone(),
                span: self.current().span,
            }),
        }
    }

    fn match_semicolon(&mut self) -> bool {
        self.match_token(TokenKind::Semicolon)
    }

    fn match_token(&mut self, expected: TokenKind) -> bool {
        if kind_eq(&self.current().kind, &expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_token(
        &mut self,
        expected: TokenKind,
        message: &'static str,
    ) -> Result<(), ParseError> {
        if self.match_token(expected) {
            Ok(())
        } else {
            Err(ParseError::UnexpectedToken {
                expected: message,
                found: self.current().clone(),
                span: self.current().span,
            })
        }
    }

    fn expect_kind(
        &mut self,
        predicate: impl Fn(&TokenKind) -> bool,
        expected: &'static str,
    ) -> Result<(), ParseError> {
        if predicate(&self.current().kind) {
            self.advance();
            Ok(())
        } else {
            Err(ParseError::UnexpectedToken {
                expected,
                found: self.current().clone(),
                span: self.current().span,
            })
        }
    }

    fn check(&self, kind: TokenKind) -> bool {
        kind_eq(&self.current().kind, &kind)
    }

    fn check_identifier(&self, expected: &str) -> bool {
        matches!(&self.current().kind, TokenKind::Identifier(name) if name.eq_ignore_ascii_case(expected))
    }

    fn advance(&mut self) {
        if !self.is_at_end() {
            self.position += 1;
        }
    }

    fn previous(&self) -> Option<&Token> {
        if self.position == 0 {
            None
        } else {
            self.tokens.get(self.position - 1)
        }
    }

    fn is_at_end(&self) -> bool {
        matches!(self.current().kind, TokenKind::EOF)
    }

    fn current(&self) -> &Token {
        self.tokens
            .get(self.position)
            .unwrap_or_else(|| self.tokens.last().expect("tokens not empty"))
    }
}

fn kind_eq(a: &TokenKind, b: &TokenKind) -> bool {
    use TokenKind::*;
    matches!(
        (a, b),
        (Plus, Plus)
            | (Minus, Minus)
            | (Star, Star)
            | (Slash, Slash)
            | (Dot, Dot)
            | (Comma, Comma)
            | (LParen, LParen)
            | (RParen, RParen)
            | (LBrace, LBrace)
            | (RBrace, RBrace)
            | (LBracket, LBracket)
            | (RBracket, RBracket)
            | (Semicolon, Semicolon)
            | (Question, Question)
            | (QuestionQuestion, QuestionQuestion)
            | (Colon, Colon)
            | (Equal, Equal)
            | (EqualEqual, EqualEqual)
            | (Bang, Bang)
            | (BangEqual, BangEqual)
            | (Less, Less)
            | (LessEqual, LessEqual)
            | (Greater, Greater)
            | (GreaterEqual, GreaterEqual)
            | (AndAnd, AndAnd)
            | (OrOr, OrOr)
            | (Arrow, Arrow)
            | (EOF, EOF)
    )
}

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("unexpected token while parsing {expected}: found {found:?} at {span:?}")]
    UnexpectedToken {
        expected: &'static str,
        found: Token,
        span: Span,
    },
    #[error("duplicate field `{name}` in struct literal")]
    DuplicateStructField { name: String },
    #[error("invalid assignment target at {span:?}")]
    InvalidAssignmentTarget { span: Span },
}
