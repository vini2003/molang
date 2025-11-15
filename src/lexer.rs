use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Number(f64),
    Identifier(String),
    String(String),
    Plus,
    Minus,
    Star,
    Slash,
    Dot,
    Comma,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semicolon,
    Question,
    QuestionQuestion,
    Colon,
    Equal,
    EqualEqual,
    Bang,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    AndAnd,
    OrOr,
    Arrow,
    EOF,
}

#[derive(Debug, Error)]
pub enum LexError {
    #[error("unexpected character `{ch}` at {index}")]
    UnexpectedCharacter { ch: char, index: usize },
    #[error("failed to parse number at {span:?}")]
    InvalidNumber { span: Span },
    #[error("unterminated string starting at {start}")]
    UnterminatedString { start: usize },
}

pub fn lex(input: &str) -> Result<Vec<Token>, LexError> {
    let mut chars = input.char_indices().peekable();
    let mut tokens = Vec::new();

    while let Some((idx, ch)) = chars.next() {
        if ch.is_whitespace() {
            continue;
        }

        if ch.is_ascii_digit() {
            tokens.push(read_number(idx, ch, &mut chars)?);
            continue;
        }

        if ch == '.' {
            if let Some(&(_, next)) = chars.peek() {
                if next.is_ascii_digit() {
                    tokens.push(read_number(idx, ch, &mut chars)?);
                    continue;
                }
            }
            tokens.push(token(TokenKind::Dot, idx, idx));
            continue;
        }

        if ch == '"' || ch == '\'' {
            tokens.push(read_string(idx, ch, &mut chars)?);
            continue;
        }

        if is_ident_start(ch) {
            tokens.push(read_identifier(idx, ch, &mut chars));
            continue;
        }

        let token = match ch {
            '+' => token(TokenKind::Plus, idx, idx),
            '-' => {
                if matches_next_char(&mut chars, '>') {
                    token(TokenKind::Arrow, idx, idx + 1)
                } else {
                    token(TokenKind::Minus, idx, idx)
                }
            }
            '*' => token(TokenKind::Star, idx, idx),
            '/' => token(TokenKind::Slash, idx, idx),
            ',' => token(TokenKind::Comma, idx, idx),
            '(' => token(TokenKind::LParen, idx, idx),
            ')' => token(TokenKind::RParen, idx, idx),
            '{' => token(TokenKind::LBrace, idx, idx),
            '}' => token(TokenKind::RBrace, idx, idx),
            '[' => token(TokenKind::LBracket, idx, idx),
            ']' => token(TokenKind::RBracket, idx, idx),
            ';' => token(TokenKind::Semicolon, idx, idx),
            '?' => {
                if matches_next_char(&mut chars, '?') {
                    token(TokenKind::QuestionQuestion, idx, idx + 1)
                } else {
                    token(TokenKind::Question, idx, idx)
                }
            }
            ':' => token(TokenKind::Colon, idx, idx),
            '=' => {
                if matches_next_char(&mut chars, '=') {
                    token(TokenKind::EqualEqual, idx, idx + 1)
                } else {
                    token(TokenKind::Equal, idx, idx)
                }
            }
            '!' => {
                if matches_next_char(&mut chars, '=') {
                    token(TokenKind::BangEqual, idx, idx + 1)
                } else {
                    token(TokenKind::Bang, idx, idx)
                }
            }
            '<' => {
                if matches_next_char(&mut chars, '=') {
                    token(TokenKind::LessEqual, idx, idx + 1)
                } else {
                    token(TokenKind::Less, idx, idx)
                }
            }
            '>' => {
                if matches_next_char(&mut chars, '=') {
                    token(TokenKind::GreaterEqual, idx, idx + 1)
                } else {
                    token(TokenKind::Greater, idx, idx)
                }
            }
            '&' => {
                if matches_next_char(&mut chars, '&') {
                    token(TokenKind::AndAnd, idx, idx + 1)
                } else {
                    return Err(LexError::UnexpectedCharacter { ch, index: idx });
                }
            }
            '|' => {
                if matches_next_char(&mut chars, '|') {
                    token(TokenKind::OrOr, idx, idx + 1)
                } else {
                    return Err(LexError::UnexpectedCharacter { ch, index: idx });
                }
            }
            _ => {
                return Err(LexError::UnexpectedCharacter { ch, index: idx });
            }
        };
        tokens.push(token);
    }

    tokens.push(Token {
        kind: TokenKind::EOF,
        span: Span {
            start: input.len(),
            end: input.len(),
        },
    });

    Ok(tokens)
}

fn read_number<I>(
    start_idx: usize,
    start_ch: char,
    chars: &mut std::iter::Peekable<I>,
) -> Result<Token, LexError>
where
    I: Iterator<Item = (usize, char)>,
{
    let mut literal = String::new();
    literal.push(start_ch);
    let mut end_idx = start_idx;
    let mut has_dot = start_ch == '.';

    while let Some(&(idx, ch)) = chars.peek() {
        if ch.is_ascii_digit() {
            literal.push(ch);
            end_idx = idx;
            chars.next();
        } else if ch == '.' && !has_dot {
            has_dot = true;
            literal.push(ch);
            end_idx = idx;
            chars.next();
        } else {
            break;
        }
    }

    let value = literal
        .parse::<f64>()
        .map_err(|_| LexError::InvalidNumber {
            span: Span {
                start: start_idx,
                end: end_idx,
            },
        })?;

    Ok(Token {
        kind: TokenKind::Number(value),
        span: Span {
            start: start_idx,
            end: end_idx,
        },
    })
}

fn read_string<I>(
    start_idx: usize,
    quote: char,
    chars: &mut std::iter::Peekable<I>,
) -> Result<Token, LexError>
where
    I: Iterator<Item = (usize, char)>,
{
    let mut literal = String::new();

    while let Some((idx, ch)) = chars.next() {
        if ch == quote {
            return Ok(Token {
                kind: TokenKind::String(literal),
                span: Span {
                    start: start_idx,
                    end: idx,
                },
            });
        } else if ch == '\\' {
            if let Some((_, next_ch)) = chars.next() {
                literal.push(next_ch);
            }
        } else {
            literal.push(ch);
        }
    }

    Err(LexError::UnterminatedString { start: start_idx })
}

fn read_identifier<I>(start_idx: usize, first: char, chars: &mut std::iter::Peekable<I>) -> Token
where
    I: Iterator<Item = (usize, char)>,
{
    let mut literal = String::new();
    literal.push(first);
    let mut end_idx = start_idx;

    while let Some(&(idx, ch)) = chars.peek() {
        if is_ident_continue(ch) {
            literal.push(ch);
            end_idx = idx;
            chars.next();
        } else {
            break;
        }
    }

    Token {
        kind: TokenKind::Identifier(literal),
        span: Span {
            start: start_idx,
            end: end_idx,
        },
    }
}

fn token(kind: TokenKind, start: usize, end: usize) -> Token {
    Token {
        kind,
        span: Span { start, end },
    }
}

fn matches_next_char<I>(chars: &mut std::iter::Peekable<I>, expected: char) -> bool
where
    I: Iterator<Item = (usize, char)>,
{
    if let Some(&(_, ch)) = chars.peek() {
        if ch == expected {
            chars.next();
            return true;
        }
    }
    false
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    is_ident_start(ch) || ch.is_ascii_digit()
}
