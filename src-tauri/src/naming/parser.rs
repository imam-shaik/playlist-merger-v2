use crate::naming::validator::{NamingError, NamingErrorCode};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Literal(String),
    Variable(String),
}

#[derive(Debug, Clone)]
pub struct Token(pub TokenKind);

#[derive(Debug, Clone)]
pub struct TemplateParser;

impl TemplateParser {
    pub fn tokenize(template: &str) -> Result<Vec<Token>, NamingError> {
        let mut tokens = Vec::new();
        let mut chars = template.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '{' {
                let mut var_name = String::new();
                for nc in chars.by_ref() {
                    if nc == '}' {
                        break;
                    }
                    if nc.is_alphanumeric() || nc == '_' {
                        var_name.push(nc);
                    } else {
                        return Err(NamingError {
                            code: NamingErrorCode::UnknownVariable,
                            message: format!("Invalid character '{}' in variable name", nc),
                            position: None,
                        });
                    }
                }

                if var_name.is_empty() {
                    return Err(NamingError {
                        code: NamingErrorCode::UnclosedBrace,
                        message: "Empty variable name".to_string(),
                        position: None,
                    });
                }

                tokens.push(Token(TokenKind::Variable(var_name)));
            } else {
                let mut literal = String::from(c);
                while let Some(&nc) = chars.peek() {
                    if nc == '{' {
                        break;
                    }
                    literal.push(chars.next().unwrap());
                }
                tokens.push(Token(TokenKind::Literal(literal)));
            }
        }

        if template.contains('{') && !template.contains('}') {
            return Err(NamingError {
                code: NamingErrorCode::UnclosedBrace,
                message: "Unclosed '{' in template".to_string(),
                position: None,
            });
        }

        Ok(tokens)
    }

}