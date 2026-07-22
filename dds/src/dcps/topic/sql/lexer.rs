//! Lexical analyzer for SQL filter expressions.
//!
//! The `Lexer` tokenizes SQL-like filter expression strings into a sequence of tokens
//! that can be processed by the parser. It handles identifiers, literals, operators,
//! keywords, and parameters.

use crate::topic::sql::ast::{Parameter, Token, TokenType};

#[derive(Debug)]
pub(crate) struct Lexer {
    input: String,
    position: usize,
}

impl Lexer {
    pub(crate) fn new(input: String) -> Self {
        Self { input, position: 0 }
    }

    fn current_char(&self) -> Option<char> {
        self.input.chars().nth(self.position)
    }

    fn peek_char(&self) -> Option<char> {
        self.input.chars().nth(self.position + 1)
    }

    fn advance(&mut self) {
        self.position += 1;
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.current_char() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_number(&mut self) -> (TokenType, Parameter) {
        let mut result = String::new();
        let mut is_float = false;
        let mut is_hex = false;

        if let Some(ch) = self.current_char() {
            if ch == '+' || ch == '-' {
                result.push(ch);
                self.advance();
            }
        }

        if self.current_char() == Some('0') && self.peek_char() == Some('x') {
            result.push_str("0x");
            self.advance();
            self.advance();
            is_hex = true;

            while let Some(ch) = self.current_char() {
                if ch.is_ascii_hexdigit() {
                    result.push(ch);
                    self.advance();
                } else {
                    break;
                }
            }
        } else {
            while let Some(ch) = self.current_char() {
                if ch.is_ascii_digit() {
                    result.push(ch);
                    self.advance();
                } else if ch == '.' && !is_float {
                    is_float = true;
                    result.push(ch);
                    self.advance();
                } else if (ch == 'e' || ch == 'E') && !is_hex {
                    is_float = true;
                    result.push(ch);
                    self.advance();

                    if let Some(next_ch) = self.current_char() {
                        if next_ch == '+' || next_ch == '-' {
                            result.push(next_ch);
                            self.advance();
                        }
                    }
                } else {
                    break;
                }
            }
        }

        if is_hex {
            let val = i128::from_str_radix(&result[2..], 16).unwrap_or(0);
            (TokenType::IntegerValue, Parameter::IntegerValue(val))
        } else if is_float {
            let val = result.parse::<f64>().unwrap_or(0.0);
            (TokenType::FloatValue, Parameter::FloatValue(val))
        } else {
            let val = result.parse::<i128>().unwrap_or(0);
            (TokenType::IntegerValue, Parameter::IntegerValue(val))
        }
    }

    // Read string (enclosed in ')
    fn read_string(&mut self) -> String {
        let mut result = String::new();
        self.advance();

        while let Some(ch) = self.current_char() {
            if ch == '\'' {
                self.advance(); // Skip ending '
                break;
            } else {
                result.push(ch);
                self.advance();
            }
        }
        result
    }

    fn read_identifier(&mut self) -> String {
        let mut result = String::new();

        while let Some(ch) = self.current_char() {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                // First character cannot be a digit (topic name rule)
                if result.is_empty() && ch.is_ascii_digit() {
                    break;
                }
                result.push(ch);
                self.advance();
            } else if ch == '.' {
                result.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        result
    }

    // Read parameter (%n form)
    fn read_parameter(&mut self) -> Parameter {
        let mut num_str = String::new();
        self.advance(); // Skip %

        while let Some(ch) = self.current_char() {
            if ch.is_ascii_digit() {
                num_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        let param_num =
            num_str.parse::<usize>().expect("Failed to convert parameter value to usize");
        if param_num >= 100 {
            panic!("A parameter is of the form %n, where n represents a natural number (zero included) smaller than 100.");
        }

        Parameter::Parameter(param_num)
    }

    fn identify_keyword(&self, ident: &str) -> TokenType {
        match ident.to_uppercase().as_str() {
            "SELECT" => TokenType::Select,
            "FROM" => TokenType::From,
            "WHERE" => TokenType::Where,
            "ORDER" => TokenType::OrderBy, // "ORDER BY" is handled as two tokens
            "BY" => TokenType::OrderBy,
            "AS" => TokenType::As,
            "AND" => TokenType::And,
            "OR" => TokenType::Or,
            "NOT" => TokenType::Not,
            "BETWEEN" => TokenType::Between,
            "LIKE" => TokenType::Like,
            "NATURAL" => TokenType::Natural,
            "JOIN" => TokenType::Join,
            "INNER" => TokenType::Inner,
            _ => {
                // If contains dot, treat as field name; otherwise treat as topic name
                TokenType::Identifier
            }
        }
    }

    fn next_token(&mut self) -> Token {
        self.skip_whitespace();
        let start_pos = self.position;

        match self.current_char() {
            None => Token {
                token_type: TokenType::Eof,
                value: Parameter::String("".to_string()),
                position: start_pos,
            },
            Some('=') => {
                self.advance();
                Token {
                    token_type: TokenType::Equal,
                    value: Parameter::String("=".to_string()),
                    position: start_pos,
                }
            }
            Some('>') => {
                self.advance();
                if self.current_char() == Some('=') {
                    self.advance();
                    Token {
                        token_type: TokenType::GreaterEqual,
                        value: Parameter::String(">=".to_string()),
                        position: start_pos,
                    }
                } else {
                    Token {
                        token_type: TokenType::Greater,
                        value: Parameter::String(">".to_string()),
                        position: start_pos,
                    }
                }
            }
            Some('<') => {
                self.advance();
                match self.current_char() {
                    Some('=') => {
                        self.advance();
                        Token {
                            token_type: TokenType::LessEqual,
                            value: Parameter::String("<=".to_string()),
                            position: start_pos,
                        }
                    }
                    Some('>') => {
                        self.advance();
                        Token {
                            token_type: TokenType::NotEqual,
                            value: Parameter::String("<>".to_string()),
                            position: start_pos,
                        }
                    }
                    _ => Token {
                        token_type: TokenType::Less,
                        value: Parameter::String("<".to_string()),
                        position: start_pos,
                    },
                }
            }
            Some('(') => {
                self.advance();
                Token {
                    token_type: TokenType::LeftParen,
                    value: Parameter::String("(".to_string()),
                    position: start_pos,
                }
            }
            Some(')') => {
                self.advance();
                Token {
                    token_type: TokenType::RightParen,
                    value: Parameter::String(")".to_string()),
                    position: start_pos,
                }
            }
            Some(',') => {
                self.advance();
                Token {
                    token_type: TokenType::Comma,
                    value: Parameter::String(",".to_string()),
                    position: start_pos,
                }
            }
            Some(';') => {
                self.advance();
                Token {
                    token_type: TokenType::Semicolon,
                    value: Parameter::String(";".to_string()),
                    position: start_pos,
                }
            }
            Some('*') => {
                self.advance();
                Token {
                    token_type: TokenType::Asterisk,
                    value: Parameter::String("*".to_string()),
                    position: start_pos,
                }
            }
            Some('.') => {
                self.advance();
                if let Some(next_ch) = self.peek_char() {
                    if next_ch.is_ascii_digit() {
                        let (token_type, value) = self.read_number();
                        return Token { token_type, value, position: start_pos };
                    }
                }
                self.advance();
                Token {
                    token_type: TokenType::Dot,
                    value: Parameter::String(".".to_string()),
                    position: start_pos,
                }
            }
            Some('\'') => {
                let content = self.read_string();
                // If single character, CharValue; if multiple characters, String or EnumeratedValue
                let (token_type, value) = if content.len() == 1 {
                    (
                        TokenType::CharValue,
                        Parameter::CharValue(content.chars().next().unwrap_or_else(|| {
                            panic!("Expected single character, got {:?}", content)
                        })),
                    )
                } else {
                    (TokenType::String, Parameter::String(content))
                };

                Token { token_type, value, position: start_pos }
            }

            // Parameter (%n)
            Some('%') => {
                let param = self.read_parameter();
                Token { token_type: TokenType::Parameter, value: param, position: start_pos }
            }
            // Number
            Some(ch) if ch.is_ascii_digit() || ch == '+' || ch == '-' => {
                // Check if +/- is before a number
                if (ch == '+' || ch == '-') && !self.peek_char().is_some_and(|c| c.is_ascii_digit())
                {
                    panic!("Invalid char: {}", ch);
                }
                let (token_type, value) = self.read_number();
                Token { token_type, value, position: start_pos }
            }

            // Identifier (keyword, field name, topic name)
            Some(ch) if ch.is_ascii_alphabetic() || ch == '_' => {
                let ident = self.read_identifier();
                let token_type = self.identify_keyword(&ident);
                Token { token_type, value: Parameter::String(ident), position: start_pos }
            }
            Some(ch) => {
                panic!("Unknown char: {}", ch);
            }
        }
    }

    pub(crate) fn tokenize(&mut self) -> Vec<Token> {
        let mut result = Vec::new();
        loop {
            let token = self.next_token();
            let is_eof = token.token_type == TokenType::Eof;
            result.push(token);
            if is_eof {
                break;
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let mut lexer = Lexer::new("SELECT * FROM Location WHERE x > 10".to_string());
        let tokens = lexer.tokenize();

        assert_eq!(tokens[0].token_type, TokenType::Select);
        assert_eq!(tokens[1].token_type, TokenType::Asterisk);
        assert_eq!(tokens[2].token_type, TokenType::From);
        assert_eq!(tokens[3].token_type, TokenType::Identifier); // Location
        assert_eq!(tokens[4].token_type, TokenType::Where);
        assert_eq!(tokens[5].token_type, TokenType::Identifier); // x
        assert_eq!(tokens[6].token_type, TokenType::Greater);
        assert_eq!(tokens[7].token_type, TokenType::IntegerValue);
    }

    #[test]
    fn test_integer_literal_exceeds_i32_range() {
        // A CFT bound above i32::MAX (e.g. `seq > 3000000000`) must tokenize at
        // full 64-bit width. Under the old i32 parse it fell through to
        // unwrap_or(0), silently comparing against 0.
        let mut lexer = Lexer::new("seq > 3000000000".to_string());
        let tokens = lexer.tokenize();
        assert_eq!(tokens[2].token_type, TokenType::IntegerValue);
        assert_eq!(tokens[2].value, Parameter::IntegerValue(3_000_000_000));
    }
}
