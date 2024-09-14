use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;
use tree_sitter::{Node, Point};

#[derive(Debug, PartialEq)]
pub enum CursorPosition {
    OutsideTFunction,
    InFirstParam(String),
    InSecondParam {
        translation_key: String,
        position: SecondParamPosition,
    },
}

#[derive(Debug, PartialEq)]
pub enum SecondParamPosition {
    EmptyObject,
    InObject,
    InKey(String),
    InValue(String),
}

pub struct TFunctionAnalyzer<'a> {
    code: &'a str,
    cursor: Position,
    pub(crate) tree: tree_sitter::Tree,
}

impl<'a> TFunctionAnalyzer<'a> {
    pub(crate) fn new(code: &'a str, cursor: Position) -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::language_typescript())
            .map_err(|_| Error::internal_error())?;
        let tree = parser.parse(code, None).ok_or_else(Error::internal_error)?;

        Ok(Self { code, cursor, tree })
    }

    pub(crate) fn analyze(&self) -> CursorPosition {
        let root_node = self.tree.root_node();
        let cursor_point = Point {
            row: self.cursor.line as usize,
            column: self.cursor.character as usize,
        };

        self.find_t_function_call(root_node, cursor_point)
    }

    fn find_t_function_call(&self, node: Node, cursor_point: Point) -> CursorPosition {
        if self.is_t_function_call(&node) {
            return self.analyze_t_call(node, cursor_point);
        }

        for child in node.named_children(&mut node.walk()) {
            let result = self.find_t_function_call(child, cursor_point);
            if result != CursorPosition::OutsideTFunction {
                return result;
            }
        }

        CursorPosition::OutsideTFunction
    }

    pub(crate) fn is_t_function_call(&self, node: &Node) -> bool {
        node.kind() == "call_expression"
            && node
                .child_by_field_name("function")
                .map(|func_node| {
                    func_node.kind() == "identifier"
                        && func_node.utf8_text(self.code.as_bytes()).unwrap_or("") == "t"
                })
                .unwrap_or(false)
    }

    fn analyze_t_call(&self, node: Node, cursor_point: Point) -> CursorPosition {
        let arguments_node = node
            .child_by_field_name("arguments")
            .expect("t() call should have arguments");
        let argument_nodes: Vec<_> = arguments_node
            .named_children(&mut arguments_node.walk())
            .collect();

        let translation_key = argument_nodes
            .first()
            .filter(|&arg| arg.kind() == "string")
            .and_then(|arg| arg.utf8_text(self.code.as_bytes()).ok())
            .map(|s| {
                s.trim_matches(|c| c == '"' || c == '\'' || c == '`')
                    .to_string()
            })
            .unwrap_or_default();

        if let Some(first_arg) = argument_nodes.first() {
            if self.point_in_range(cursor_point, first_arg) {
                return CursorPosition::InFirstParam(translation_key);
            }
        }

        if let Some(second_arg) = argument_nodes.get(1) {
            if self.point_in_range(cursor_point, second_arg) {
                return self.analyze_second_param(second_arg, cursor_point, translation_key);
            }
        }

        CursorPosition::OutsideTFunction
    }

    fn analyze_second_param(
        &self,
        node: &Node,
        cursor_point: Point,
        translation_key: String,
    ) -> CursorPosition {
        if node.kind() == "object" || node.kind() == "object_pattern" {
            if node.named_child_count() == 0 {
                return CursorPosition::InSecondParam {
                    translation_key,
                    position: SecondParamPosition::EmptyObject,
                };
            }

            for prop_node in node.named_children(&mut node.walk()) {
                if self.point_in_range(cursor_point, &prop_node) {
                    if let Some(key) = prop_node.child_by_field_name("key") {
                        let key_text = key
                            .utf8_text(self.code.as_bytes())
                            .unwrap_or("")
                            .to_string();
                        if self.point_in_range(cursor_point, &key) {
                            return CursorPosition::InSecondParam {
                                translation_key,
                                position: SecondParamPosition::InKey(key_text),
                            };
                        }
                    }
                    if let Some(value) = prop_node.child_by_field_name("value") {
                        let key_text = prop_node
                            .child_by_field_name("key")
                            .and_then(|k| k.utf8_text(self.code.as_bytes()).ok())
                            .unwrap_or("")
                            .to_string();
                        if self.point_in_range(cursor_point, &value) {
                            return CursorPosition::InSecondParam {
                                translation_key,
                                position: SecondParamPosition::InValue(key_text),
                            };
                        }
                    }
                }
            }
        }

        CursorPosition::InSecondParam {
            translation_key,
            position: SecondParamPosition::InObject,
        }
    }

    fn point_in_range(&self, point: Point, node: &Node) -> bool {
        let start = node.start_position();
        let end = node.end_position();
        (start.row < point.row || (start.row == point.row && start.column <= point.column))
            && (point.row < end.row || (point.row == end.row && point.column <= end.column))
    }
}
