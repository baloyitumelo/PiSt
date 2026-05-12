use crate::lexer::{tokenize, LexError, Token, TokenKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Item {
    LetBinding {
        name: String,
        value: Expr,
    },
    LetAbstraction {
        name: String,
        params: Vec<String>,
        body: Expr,
    },
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Expr {
    Var(String),
    Nat(u64),
    Bool(bool),
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
    Binary {
        op: BinOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
    },
    App {
        function: Box<Expr>,
        arguments: Vec<Expr>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnOp {
    Not,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

pub fn parse_source(source: &str) -> Result<Vec<Item>, ParseError> {
    let tokens = tokenize(source).map_err(ParseError::from_lex_error)?;
    parse_tokens(tokens)
}

pub fn parse_tokens(tokens: Vec<Token>) -> Result<Vec<Item>, ParseError> {
    let mut parser = Parser::new(tokens);
    parser.parse_items()
}

impl ParseError {
    fn from_lex_error(err: LexError) -> Self {
        Self {
            message: err.message,
            position: err.position,
        }
    }
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, index: 0 }
    }

    fn parse_items(&mut self) -> Result<Vec<Item>, ParseError> {
        let mut items = Vec::new();
        self.skip_separators();

        while !self.is_eof() {
            let item = if self.is_current(&TokenKind::Let) {
                self.parse_let_item()?
            } else {
                Item::Expr(self.parse_expr()?)
            };
            items.push(item);
            self.skip_separators();
        }

        Ok(items)
    }

    fn parse_let_item(&mut self) -> Result<Item, ParseError> {
        self.expect_simple(&TokenKind::Let, "Expected 'let'")?;
        let name = self.parse_ident("Expected name after 'let'")?;
        let mut params = Vec::new();

        while let Some(param) = self.try_parse_ident() {
            params.push(param);
        }

        self.expect_simple(&TokenKind::Equals, "Expected '=' in let declaration")?;
        let expr = self.parse_expr()?;

        if params.is_empty() {
            Ok(Item::LetBinding { name, value: expr })
        } else {
            Ok(Item::LetAbstraction {
                name,
                params,
                body: expr,
            })
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_if_expr()
    }

    fn parse_if_expr(&mut self) -> Result<Expr, ParseError> {
        if self.is_current(&TokenKind::If) {
            self.advance();
            let condition = self.parse_expr()?;
            self.expect_simple(&TokenKind::Then, "Expected 'then' in if expression")?;
            let then_branch = self.parse_expr()?;
            self.expect_simple(&TokenKind::Else, "Expected 'else' in if expression")?;
            let else_branch = self.parse_expr()?;
            Ok(Expr::If {
                condition: Box::new(condition),
                then_branch: Box::new(then_branch),
                else_branch: Box::new(else_branch),
            })
        } else {
            self.parse_or_expr()
        }
    }

    fn parse_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_and_expr()?;
        while self.is_current(&TokenKind::Or) {
            self.advance();
            let rhs = self.parse_and_expr()?;
            expr = Expr::Binary {
                op: BinOp::Or,
                left: Box::new(expr),
                right: Box::new(rhs),
            };
        }
        Ok(expr)
    }

    fn parse_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_comparison_expr()?;
        while self.is_current(&TokenKind::And) {
            self.advance();
            let rhs = self.parse_comparison_expr()?;
            expr = Expr::Binary {
                op: BinOp::And,
                left: Box::new(expr),
                right: Box::new(rhs),
            };
        }
        Ok(expr)
    }

    fn parse_comparison_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_additive_expr()?;
        loop {
            let op = if self.is_current(&TokenKind::EqEq) {
                Some(BinOp::Eq)
            } else if self.is_current(&TokenKind::NotEq) {
                Some(BinOp::NotEq)
            } else if self.is_current(&TokenKind::Less) {
                Some(BinOp::Lt)
            } else if self.is_current(&TokenKind::LessEq) {
                Some(BinOp::Le)
            } else if self.is_current(&TokenKind::Greater) {
                Some(BinOp::Gt)
            } else if self.is_current(&TokenKind::GreaterEq) {
                Some(BinOp::Ge)
            } else {
                None
            };

            if let Some(op) = op {
                self.advance();
                let rhs = self.parse_additive_expr()?;
                expr = Expr::Binary {
                    op,
                    left: Box::new(expr),
                    right: Box::new(rhs),
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_additive_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_multiplicative_expr()?;
        loop {
            let op = if self.is_current(&TokenKind::Plus) {
                Some(BinOp::Add)
            } else if self.is_current(&TokenKind::Minus) {
                Some(BinOp::Sub)
            } else {
                None
            };

            if let Some(op) = op {
                self.advance();
                let rhs = self.parse_multiplicative_expr()?;
                expr = Expr::Binary {
                    op,
                    left: Box::new(expr),
                    right: Box::new(rhs),
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_multiplicative_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_unary_expr()?;
        loop {
            let op = if self.is_current(&TokenKind::Star) {
                Some(BinOp::Mul)
            } else if self.is_current(&TokenKind::Slash) {
                Some(BinOp::Div)
            } else {
                None
            };

            if let Some(op) = op {
                self.advance();
                let rhs = self.parse_unary_expr()?;
                expr = Expr::Binary {
                    op,
                    left: Box::new(expr),
                    right: Box::new(rhs),
                };
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
        if self.is_current(&TokenKind::Not) {
            self.advance();
            let expr = self.parse_unary_expr()?;
            Ok(Expr::Unary {
                op: UnOp::Not,
                expr: Box::new(expr),
            })
        } else {
            self.parse_application_expr()
        }
    }

    fn parse_application_expr(&mut self) -> Result<Expr, ParseError> {
        let first = self.parse_atom()?;
        let mut args = Vec::new();

        while self.can_start_atom() {
            args.push(self.parse_atom()?);
        }

        if args.is_empty() {
            Ok(first)
        } else {
            Ok(Expr::App {
                function: Box::new(first),
                arguments: args,
            })
        }
    }

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        if let Some(name) = self.try_parse_ident() {
            return Ok(Expr::Var(name));
        }

        if let TokenKind::Number(value) = self.current().kind.clone() {
            self.advance();
            return Ok(Expr::Nat(value));
        }

        if self.is_current(&TokenKind::True) {
            self.advance();
            return Ok(Expr::Bool(true));
        }
        if self.is_current(&TokenKind::False) {
            self.advance();
            return Ok(Expr::Bool(false));
        }

        if self.is_current(&TokenKind::LParen) {
            self.advance();
            let expr = self.parse_expr()?;
            self.expect_simple(&TokenKind::RParen, "Expected ')'")?;
            return Ok(expr);
        }

        Err(self.error_here("Expected expression"))
    }

    fn parse_ident(&mut self, message: &str) -> Result<String, ParseError> {
        match &self.current().kind {
            TokenKind::Ident(name) => {
                let value = name.clone();
                self.advance();
                Ok(value)
            }
            _ => Err(self.error_here(message)),
        }
    }

    fn try_parse_ident(&mut self) -> Option<String> {
        match &self.current().kind {
            TokenKind::Ident(name) => {
                let value = name.clone();
                self.advance();
                Some(value)
            }
            _ => None,
        }
    }

    fn can_start_atom(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Ident(_)
                | TokenKind::Number(_)
                | TokenKind::True
                | TokenKind::False
                | TokenKind::LParen
        )
    }

    fn skip_separators(&mut self) {
        while matches!(
            self.current().kind,
            TokenKind::Semicolon | TokenKind::Newline
        ) {
            self.advance();
        }
    }

    fn expect_simple(&mut self, expected: &TokenKind, message: &str) -> Result<(), ParseError> {
        if self.is_current(expected) {
            self.advance();
            Ok(())
        } else {
            Err(self.error_here(message))
        }
    }

    fn is_current(&self, expected: &TokenKind) -> bool {
        match (&self.current().kind, expected) {
            (TokenKind::Let, TokenKind::Let)
            | (TokenKind::If, TokenKind::If)
            | (TokenKind::Then, TokenKind::Then)
            | (TokenKind::Else, TokenKind::Else)
            | (TokenKind::True, TokenKind::True)
            | (TokenKind::False, TokenKind::False)
            | (TokenKind::And, TokenKind::And)
            | (TokenKind::Or, TokenKind::Or)
            | (TokenKind::Not, TokenKind::Not)
            | (TokenKind::Equals, TokenKind::Equals)
            | (TokenKind::EqEq, TokenKind::EqEq)
            | (TokenKind::NotEq, TokenKind::NotEq)
            | (TokenKind::Less, TokenKind::Less)
            | (TokenKind::LessEq, TokenKind::LessEq)
            | (TokenKind::Greater, TokenKind::Greater)
            | (TokenKind::GreaterEq, TokenKind::GreaterEq)
            | (TokenKind::Plus, TokenKind::Plus)
            | (TokenKind::Minus, TokenKind::Minus)
            | (TokenKind::Star, TokenKind::Star)
            | (TokenKind::Slash, TokenKind::Slash)
            | (TokenKind::LParen, TokenKind::LParen)
            | (TokenKind::RParen, TokenKind::RParen)
            | (TokenKind::Semicolon, TokenKind::Semicolon)
            | (TokenKind::Newline, TokenKind::Newline)
            | (TokenKind::Eof, TokenKind::Eof) => true,
            _ => false,
        }
    }

    fn is_eof(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    fn current(&self) -> &Token {
        &self.tokens[self.index]
    }

    fn advance(&mut self) {
        if !self.is_eof() {
            self.index += 1;
        }
    }

    fn error_here(&self, message: &str) -> ParseError {
        ParseError {
            message: message.to_string(),
            position: self.current().start,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_source, BinOp, Expr, Item, UnOp};
    use crate::lexer::{tokenize, TokenKind};

    #[test]
    fn tokenizes_keywords_and_punctuation() {
        let tokens = tokenize("let id x = x + 2 * 3;\nif not false and 1 <= 2 then 1 else 0")
            .expect("Lexer should accept valid sample")
            .into_iter()
            .map(|t| t.kind)
            .collect::<Vec<_>>();

        assert_eq!(
            tokens,
            vec![
                TokenKind::Let,
                TokenKind::Ident("id".to_string()),
                TokenKind::Ident("x".to_string()),
                TokenKind::Equals,
                TokenKind::Ident("x".to_string()),
                TokenKind::Plus,
                TokenKind::Number(2),
                TokenKind::Star,
                TokenKind::Number(3),
                TokenKind::Semicolon,
                TokenKind::Newline,
                TokenKind::If,
                TokenKind::Not,
                TokenKind::False,
                TokenKind::And,
                TokenKind::Number(1),
                TokenKind::LessEq,
                TokenKind::Number(2),
                TokenKind::Then,
                TokenKind::Number(1),
                TokenKind::Else,
                TokenKind::Number(0),
                TokenKind::Eof
            ]
        );
    }

    #[test]
    fn parses_let_binding_and_abstraction() {
        let source = "let id x = x\nlet value = id x";
        let items = parse_source(source).expect("Parser should parse let declarations");

        assert_eq!(
            items,
            vec![
                Item::LetAbstraction {
                    name: "id".to_string(),
                    params: vec!["x".to_string()],
                    body: Expr::Var("x".to_string())
                },
                Item::LetBinding {
                    name: "value".to_string(),
                    value: Expr::App {
                        function: Box::new(Expr::Var("id".to_string())),
                        arguments: vec![Expr::Var("x".to_string())]
                    }
                }
            ]
        );
    }

    #[test]
    fn parses_multi_argument_application_left_associative_shape() {
        let items =
            parse_source("f a b c").expect("Parser should parse plain function application");

        assert_eq!(
            items,
            vec![Item::Expr(Expr::App {
                function: Box::new(Expr::Var("f".to_string())),
                arguments: vec![
                    Expr::Var("a".to_string()),
                    Expr::Var("b".to_string()),
                    Expr::Var("c".to_string()),
                ]
            })]
        );
    }

    #[test]
    fn parses_parenthesized_expression_as_argument() {
        let items = parse_source("f (g x) y")
            .expect("Parser should parse parenthesized expression in application");

        assert_eq!(
            items,
            vec![Item::Expr(Expr::App {
                function: Box::new(Expr::Var("f".to_string())),
                arguments: vec![
                    Expr::App {
                        function: Box::new(Expr::Var("g".to_string())),
                        arguments: vec![Expr::Var("x".to_string())]
                    },
                    Expr::Var("y".to_string()),
                ]
            })]
        );
    }

    #[test]
    fn parses_if_else_and_arithmetic_precedence() {
        let items = parse_source("if 1 + 2 * 3 >= 7 then 10 else 0")
            .expect("Parser should parse if and arithmetic");

        assert_eq!(
            items,
            vec![Item::Expr(Expr::If {
                condition: Box::new(Expr::Binary {
                    op: BinOp::Ge,
                    left: Box::new(Expr::Binary {
                        op: BinOp::Add,
                        left: Box::new(Expr::Nat(1)),
                        right: Box::new(Expr::Binary {
                            op: BinOp::Mul,
                            left: Box::new(Expr::Nat(2)),
                            right: Box::new(Expr::Nat(3)),
                        }),
                    }),
                    right: Box::new(Expr::Nat(7)),
                }),
                then_branch: Box::new(Expr::Nat(10)),
                else_branch: Box::new(Expr::Nat(0))
            })]
        );
    }

    #[test]
    fn parses_boolean_operator_precedence() {
        let items = parse_source("if not false and 1 < 2 or false then 1 else 0")
            .expect("Parser should parse boolean operators");

        assert_eq!(
            items,
            vec![Item::Expr(Expr::If {
                condition: Box::new(Expr::Binary {
                    op: BinOp::Or,
                    left: Box::new(Expr::Binary {
                        op: BinOp::And,
                        left: Box::new(Expr::Unary {
                            op: UnOp::Not,
                            expr: Box::new(Expr::Bool(false)),
                        }),
                        right: Box::new(Expr::Binary {
                            op: BinOp::Lt,
                            left: Box::new(Expr::Nat(1)),
                            right: Box::new(Expr::Nat(2)),
                        }),
                    }),
                    right: Box::new(Expr::Bool(false)),
                }),
                then_branch: Box::new(Expr::Nat(1)),
                else_branch: Box::new(Expr::Nat(0)),
            })]
        );
    }
}
