use crate::lsp::queries::Queries;
use std::path::Path;
use tower_lsp::lsp_types::*;
use tree_sitter::{Node, Point, QueryCursor};
use tree_sitter_typescript::{language_tsx, language_typescript};

pub struct TFunctionParser<'a> {
    document: &'a str,
    position: Position,
    tree: tree_sitter::Tree,
    query: tree_sitter::Query,
}

#[derive(Debug, PartialEq)]
pub enum SecondParamPosition {
    EmptyObject,
    InObject,
    InKey(String),
    InValue(String),
}

pub enum TFunctionPosition {
    Outside,
    InFunctionName,
    InFirstArgument(String),
    InSecondArgument {
        key: String,
        position: SecondParamPosition,
    },
}

impl<'a> TFunctionParser<'a> {
    pub fn new(
        document: &'a str,
        position: Position,
        file_path: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let queries = Queries::default();
        let is_tsx = Path::new(file_path)
            .extension()
            .map_or(false, |ext| ext == "tsx");

        let (language, query) = if is_tsx {
            (language_tsx(), queries.tsx_t_function)
        } else {
            (language_typescript(), queries.ts_t_function)
        };

        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&language)?;

        let tree = parser
            .parse(document, None)
            .ok_or("Failed to parse document")?;

        Ok(Self {
            document,
            position,
            tree,
            query,
        })
    }

    pub fn parse(&self) -> TFunctionPosition {
        let node = self
            .tree
            .root_node()
            .named_descendant_for_point_range(
                Point {
                    row: self.position.line as usize,
                    column: self.position.character as usize,
                },
                Point {
                    row: self.position.line as usize,
                    column: self.position.character as usize,
                },
            )
            .expect("Failed to find node at position");

        if let Some(t_function_node) = self.find_parent_t_function(&node) {
            let mut query_cursor = QueryCursor::new();
            let matches =
                query_cursor.matches(&self.query, t_function_node, self.document.as_bytes());

            for match_ in matches {
                let mut func_name_node = None;
                let mut first_arg_node = None;
                let mut second_arg_node = None;

                for capture in match_.captures {
                    let capture_name = self.query.capture_names()[capture.index as usize];
                    match capture_name {
                        "func_name" => func_name_node = Some(capture.node),
                        "first_arg" => first_arg_node = Some(capture.node),
                        "second_arg" => second_arg_node = Some(capture.node),
                        _ => {}
                    }
                }

                if let (Some(func), Some(first), second) =
                    (func_name_node, first_arg_node, second_arg_node)
                {
                    if func.utf8_text(self.document.as_bytes()).unwrap_or("") == "t" {
                        if self.is_cursor_in_node(func) {
                            return TFunctionPosition::InFunctionName;
                        } else if self.is_cursor_in_node(first) {
                            let key = first
                                .utf8_text(self.document.as_bytes())
                                .map(|s| s.trim_matches('"').to_string())
                                .unwrap_or_default();
                            return TFunctionPosition::InFirstArgument(key);
                        } else if let Some(second) = second {
                            if self.is_cursor_in_node(second) {
                                let key = first
                                    .utf8_text(self.document.as_bytes())
                                    .map(|s| s.trim_matches('"').to_string())
                                    .unwrap_or_default();

                                let position = self.determine_second_param_position(second);
                                return TFunctionPosition::InSecondArgument { key, position };
                            }
                        }
                    }
                }
            }
        }

        TFunctionPosition::Outside
    }

    pub fn find_parent_t_function<'tree>(&self, node: &'tree Node<'tree>) -> Option<Node<'tree>> {
        let mut current = *node;
        while let Some(parent) = current.parent() {
            if parent.kind() == "call_expression"
                && parent.child(0).map_or(false, |child| {
                    child.kind() == "identifier"
                        && child.utf8_text(self.document.as_bytes()).unwrap_or("") == "t"
                })
            {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    fn determine_second_param_position(&self, node: Node) -> SecondParamPosition {
        let node_text = node.utf8_text(self.document.as_bytes()).unwrap_or("");
        if node_text.trim().is_empty() || node_text == "{}" {
            return SecondParamPosition::EmptyObject;
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if self.is_cursor_in_node(child) {
                match child.kind() {
                    "string_content" => {
                        let parent = child.parent().expect("String content should have a parent");
                        if parent.kind() == "string" {
                            let grandparent = parent.parent().expect("String should have a parent");
                            if grandparent.kind() == "pair" {
                                let key = grandparent
                                    .child(0)
                                    .and_then(|n| n.utf8_text(self.document.as_bytes()).ok());
                                if let Some(key) = key {
                                    return SecondParamPosition::InValue(
                                        key.trim_matches('"').to_string(),
                                    );
                                }
                            }
                        }
                        return SecondParamPosition::InValue(String::new());
                    }
                    "string" => {
                        let parent = child.parent().expect("String should have a parent");

                        let key_node = parent.child_by_field_name("key").unwrap();
                        let key_text = key_node.utf8_text(self.document.as_bytes()).ok();

                        if parent.kind() == "pair"
                            && parent.child(0).map_or(false, |n| n.id() == child.id())
                        {
                            return SecondParamPosition::InKey(key_text.unwrap_or("").to_string());
                        } else {
                            return SecondParamPosition::InValue(
                                key_text.unwrap_or("").to_string(),
                            );
                        }
                    }
                    "identifier" => {
                        let text = child.utf8_text(self.document.as_bytes()).unwrap_or("");
                        if child.prev_sibling().map_or(false, |n| n.kind() == ":") {
                            return SecondParamPosition::InValue(text.to_string());
                        } else {
                            return SecondParamPosition::InKey(text.to_string());
                        }
                    }
                    ":" => {
                        if let Some(prev) = child.prev_sibling() {
                            if prev.kind() == "string" || prev.kind() == "identifier" {
                                let key = prev
                                    .utf8_text(self.document.as_bytes())
                                    .unwrap_or("")
                                    .trim_matches('"')
                                    .to_string();
                                return SecondParamPosition::InKey(key);
                            }
                        }
                        return SecondParamPosition::InObject;
                    }
                    "{" | "}" => return SecondParamPosition::InObject,
                    _ => {
                        if child.is_named() {
                            return self.determine_second_param_position(child);
                        }
                    }
                }
            }
        }

        SecondParamPosition::InObject
    }

    fn is_cursor_in_node(&self, node: Node) -> bool {
        let start = node.start_position();
        let end = node.end_position();

        (start.row as u32 <= self.position.line && self.position.line <= end.row as u32)
            && (start.row as u32 != self.position.line
                || start.column as u32 <= self.position.character)
            && (end.row as u32 != self.position.line
                || self.position.character <= end.column as u32)
    }

    pub fn tree(&self) -> &tree_sitter::Tree {
        &self.tree
    }
}
