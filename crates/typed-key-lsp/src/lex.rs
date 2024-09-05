#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    Variable(String),
    Plural(String),
    Select(String),
    HtmlTag(String),
    Text(String),
}

pub struct Lexer<'a> {
    #[allow(dead_code)]
    input: &'a str,
    chars: std::str::Chars<'a>,
    current_char: Option<char>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut chars = input.chars();
        let current_char = chars.next();
        Lexer {
            input,
            chars,
            current_char,
        }
    }

    fn advance(&mut self) {
        self.current_char = self.chars.next();
    }

    fn next_token(&mut self) -> Option<Token> {
        match self.current_char {
            Some('{') => self.lex_complex_token(),
            Some('<') => self.lex_html_tag(),
            Some(_) => self.lex_text(),
            None => None,
        }
    }

    fn lex_complex_token(&mut self) -> Option<Token> {
        let mut depth = 1;
        let mut content = String::new();
        content.push(self.current_char?);
        self.advance();

        while depth > 0 {
            match self.current_char {
                Some('{') => {
                    depth += 1;
                    content.push('{');
                }
                Some('}') => {
                    depth -= 1;
                    content.push('}');
                }
                Some(c) => content.push(c),
                None => break,
            }
            self.advance();
        }

        if content.contains("plural") {
            Some(Token::Plural(content))
        } else if content.contains("select") {
            Some(Token::Select(content))
        } else {
            Some(Token::Variable(content))
        }
    }

    fn lex_html_tag(&mut self) -> Option<Token> {
        let mut content = String::new();
        content.push(self.current_char?);
        self.advance();

        while self.current_char != Some('>') && self.current_char.is_some() {
            content.push(self.current_char.unwrap());
            self.advance();
        }

        content.push(self.current_char?); // Append the closing '>'
        self.advance();
        Some(Token::HtmlTag(content))
    }

    fn lex_text(&mut self) -> Option<Token> {
        let mut content = String::new();

        while let Some(c) = self.current_char {
            if c == '{' || c == '<' {
                break;
            }
            content.push(c);
            self.advance();
        }

        if !content.is_empty() {
            Some(Token::Text(content))
        } else {
            self.next_token() // Skip empty text and get the next token
        }
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_token()
    }
}
