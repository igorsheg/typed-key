use crate::lex::{Lexer, Token};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;

#[derive(Debug, PartialEq, Clone)]
pub enum AstNode {
    Root(Vec<AstNode>),
    Text(String),
    Variable(String),
    Plural {
        variable: String,
        options: HashMap<String, Vec<AstNode>>,
    },
    Select {
        variable: String,
        options: HashMap<String, Vec<AstNode>>,
    },
    HtmlTag {
        name: String,
        children: Vec<AstNode>,
    },
}

pub struct Parser {
    tokens: Vec<Token>,
}

impl Parser {
    pub fn new(input: &str) -> Self {
        let tokens: Vec<Token> = Lexer::new(input).collect();
        Parser { tokens }
    }

    pub fn parse(self) -> Result<AstNode, String> {
        let (root, _) = self.parse_nodes(0)?;
        Ok(root)
    }

    fn parse_nodes(&self, mut position: usize) -> Result<(AstNode, usize), String> {
        let mut nodes = Vec::new();
        while position < self.tokens.len() {
            let (node, new_position) = self.parse_node(position)?;
            position = new_position;
            match node {
                Some(n) => nodes.push(n),
                None => break,
            }
        }
        Ok((AstNode::Root(nodes), position))
    }

    fn parse_node(&self, position: usize) -> Result<(Option<AstNode>, usize), String> {
        match &self.tokens[position] {
            Token::Text(text) => Ok((Some(AstNode::Text(text.clone())), position + 1)),
            Token::Variable(var) => Ok((
                Some(AstNode::Variable(var[1..var.len() - 1].to_string())),
                position + 1,
            )),
            Token::Plural(plural) => self.parse_plural(plural, position),
            Token::Select(select) => self.parse_select(select, position),
            Token::HtmlTag(tag) => self.parse_html_tag(tag, position),
        }
    }

    fn parse_plural(
        &self,
        plural: &str,
        position: usize,
    ) -> Result<(Option<AstNode>, usize), String> {
        let parts: Vec<&str> = plural[1..plural.len() - 1].splitn(3, ',').collect();
        let variable = parts[0].trim().to_string();
        let (options, _) = self.parse_options(parts[2])?;

        Ok((Some(AstNode::Plural { variable, options }), position + 1))
    }

    fn parse_select(
        &self,
        select: &str,
        position: usize,
    ) -> Result<(Option<AstNode>, usize), String> {
        let parts: Vec<&str> = select[1..select.len() - 1].splitn(3, ',').collect();
        let variable = parts[0].trim().to_string();
        let (options, _) = self.parse_options(parts[2])?;

        Ok((Some(AstNode::Select { variable, options }), position + 1))
    }

    fn parse_options(
        &self,
        options_str: &str,
    ) -> Result<(HashMap<String, Vec<AstNode>>, usize), String> {
        let mut options = HashMap::new();
        let mut current_key = String::new();
        let mut current_value = String::new();
        let mut brace_count = 0;

        for ch in options_str.chars() {
            match ch {
                '{' => {
                    brace_count += 1;
                    if brace_count > 1 {
                        current_value.push(ch);
                    }
                }
                '}' => {
                    brace_count -= 1;
                    if brace_count == 0 {
                        let sub_parser = Parser::new(&current_value);
                        let sub_ast = sub_parser.parse()?;
                        options.insert(current_key.trim().to_string(), sub_ast.into_vec());
                        current_key.clear();
                        current_value.clear();
                    } else {
                        current_value.push(ch);
                    }
                }
                _ => {
                    if brace_count == 0 {
                        current_key.push(ch);
                    } else {
                        current_value.push(ch);
                    }
                }
            }
        }

        Ok((options, options_str.len()))
    }

    fn parse_html_tag(
        &self,
        tag: &str,
        position: usize,
    ) -> Result<(Option<AstNode>, usize), String> {
        if tag.starts_with("</") {
            return Ok((None, position + 1));
        }

        let name = tag[1..tag.len() - 1].to_string();
        let (children, new_position) = self.parse_nodes(position + 1)?;

        match children {
            AstNode::Root(child_nodes) => Ok((
                Some(AstNode::HtmlTag {
                    name,
                    children: child_nodes,
                }),
                new_position,
            )),
            _ => Err(format!(
                "Expected Root node for HTML tag children at position {}",
                position
            )),
        }
    }
}

impl AstNode {
    fn into_vec(self) -> Vec<AstNode> {
        if let AstNode::Root(nodes) = self {
            nodes
        } else {
            vec![self]
        }
    }
}

impl AstNode {
    pub fn to_json(&self) -> JsonValue {
        match self {
            AstNode::Root(nodes) => {
                json!({
                    "type": "root",
                    "children": nodes.iter().map(|node| node.to_json()).collect::<Vec<JsonValue>>()
                })
            }
            AstNode::Text(text) => {
                json!({
                    "type": "text",
                    "value": text
                })
            }
            AstNode::Variable(var) => {
                json!({
                    "type": "variable",
                    "name": var
                })
            }
            AstNode::Plural { variable, options } => {
                json!({
                    "type": "plural",
                    "variable": variable,
                    "options": options.iter().map(|(k, v)| (k.clone(), json!(v.iter().map(|node| node.to_json()).collect::<Vec<JsonValue>>()))).collect::<serde_json::Map<String, JsonValue>>()
                })
            }
            AstNode::Select { variable, options } => {
                json!({
                    "type": "select",
                    "variable": variable,
                    "options": options.iter().map(|(k, v)| (k.clone(), json!(v.iter().map(|node| node.to_json()).collect::<Vec<JsonValue>>()))).collect::<serde_json::Map<String, JsonValue>>()
                })
            }
            AstNode::HtmlTag { name, children } => {
                json!({
                    "type": "html_tag",
                    "name": name,
                    "children": children.iter().map(|node| node.to_json()).collect::<Vec<JsonValue>>()
                })
            }
        }
    }
}
