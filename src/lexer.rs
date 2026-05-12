#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Let,
    If,
    Then,
    Else,
    True,
    False,
    And,
    Or,
    Not,
    Ident(String),
    Number(u64),
    Equals,
    EqEq,
    NotEq,
    Less,
    LessEq,
    Greater,
    GreaterEq,
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    Semicolon,
    Newline,
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    pub position: usize,
}

pub fn tokenize(input: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = Lexer::new(input);
    lexer.tokenize()
}

struct Lexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();

        while let Some(ch) = self.peek_char() {
            match ch {
                ' ' | '\t' | '\r' => {
                    self.bump_char();
                }
                '\n' => {
                    let start = self.pos;
                    self.bump_char();
                    tokens.push(Token {
                        kind: TokenKind::Newline,
                        start,
                        end: self.pos,
                    });
                }
                '=' => {
                    let start = self.pos;
                    self.bump_char();
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        tokens.push(Token {
                            kind: TokenKind::EqEq,
                            start,
                            end: self.pos,
                        });
                        continue;
                    }
                    tokens.push(Token {
                        kind: TokenKind::Equals,
                        start,
                        end: self.pos,
                    });
                }
                '!' => {
                    let start = self.pos;
                    self.bump_char();
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        tokens.push(Token {
                            kind: TokenKind::NotEq,
                            start,
                            end: self.pos,
                        });
                    } else {
                        return Err(LexError {
                            message: "Unexpected character '!' (did you mean '!=')".to_string(),
                            position: start,
                        });
                    }
                }
                '<' => {
                    let start = self.pos;
                    self.bump_char();
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        tokens.push(Token {
                            kind: TokenKind::LessEq,
                            start,
                            end: self.pos,
                        });
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Less,
                            start,
                            end: self.pos,
                        });
                    }
                }
                '>' => {
                    let start = self.pos;
                    self.bump_char();
                    if self.peek_char() == Some('=') {
                        self.bump_char();
                        tokens.push(Token {
                            kind: TokenKind::GreaterEq,
                            start,
                            end: self.pos,
                        });
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Greater,
                            start,
                            end: self.pos,
                        });
                    }
                }
                '+' => {
                    let start = self.pos;
                    self.bump_char();
                    tokens.push(Token {
                        kind: TokenKind::Plus,
                        start,
                        end: self.pos,
                    });
                }
                '-' => {
                    let start = self.pos;
                    self.bump_char();
                    tokens.push(Token {
                        kind: TokenKind::Minus,
                        start,
                        end: self.pos,
                    });
                }
                '*' => {
                    let start = self.pos;
                    self.bump_char();
                    tokens.push(Token {
                        kind: TokenKind::Star,
                        start,
                        end: self.pos,
                    });
                }
                '/' => {
                    let start = self.pos;
                    self.bump_char();
                    tokens.push(Token {
                        kind: TokenKind::Slash,
                        start,
                        end: self.pos,
                    });
                }
                '(' => {
                    let start = self.pos;
                    self.bump_char();
                    tokens.push(Token {
                        kind: TokenKind::LParen,
                        start,
                        end: self.pos,
                    });
                }
                ')' => {
                    let start = self.pos;
                    self.bump_char();
                    tokens.push(Token {
                        kind: TokenKind::RParen,
                        start,
                        end: self.pos,
                    });
                }
                ';' => {
                    let start = self.pos;
                    self.bump_char();
                    tokens.push(Token {
                        kind: TokenKind::Semicolon,
                        start,
                        end: self.pos,
                    });
                }
                c if is_ident_start(c) => {
                    let start = self.pos;
                    let ident = self.lex_ident();
                    let kind = match ident.as_str() {
                        "let" => TokenKind::Let,
                        "if" => TokenKind::If,
                        "then" => TokenKind::Then,
                        "else" => TokenKind::Else,
                        "true" => TokenKind::True,
                        "false" => TokenKind::False,
                        "and" => TokenKind::And,
                        "or" => TokenKind::Or,
                        "not" => TokenKind::Not,
                        _ => TokenKind::Ident(ident),
                    };
                    tokens.push(Token {
                        kind,
                        start,
                        end: self.pos,
                    });
                }
                c if c.is_ascii_digit() => {
                    let start = self.pos;
                    let number = self.lex_number()?;
                    tokens.push(Token {
                        kind: TokenKind::Number(number),
                        start,
                        end: self.pos,
                    });
                }
                _ => {
                    return Err(LexError {
                        message: format!("Unexpected character '{ch}'"),
                        position: self.pos,
                    });
                }
            }
        }

        tokens.push(Token {
            kind: TokenKind::Eof,
            start: self.pos,
            end: self.pos,
        });

        Ok(tokens)
    }

    fn lex_ident(&mut self) -> String {
        let start = self.pos;
        self.bump_char();
        while let Some(ch) = self.peek_char() {
            if is_ident_continue(ch) {
                self.bump_char();
            } else {
                break;
            }
        }
        self.input[start..self.pos].to_string()
    }

    fn lex_number(&mut self) -> Result<u64, LexError> {
        let start = self.pos;
        self.bump_char();
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() {
                self.bump_char();
            } else {
                break;
            }
        }
        self.input[start..self.pos]
            .parse::<u64>()
            .map_err(|_| LexError {
                message: "Invalid natural number literal".to_string(),
                position: start,
            })
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}
