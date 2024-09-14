use log::debug;
use tower_lsp::lsp_types::Position;
use tree_sitter::{Node, Parser, Point, Tree};

#[derive(Debug, PartialEq)]
pub enum CursorPosition {
    OutsideTFunction,
    InFirstParam,
    InSecondParam {
        translation_key: Option<String>,
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

pub struct TFunctionParser {
    code: String,
    cursor: Position,
    tree: Tree,
}

impl TFunctionParser {
    pub fn new(code: String, cursor: Position) -> Result<Self, Box<dyn std::error::Error>> {
        let mut parser = Parser::new();
        parser.set_language(&tree_sitter_typescript::language_typescript())?;
        let tree = parser.parse(&code, None).ok_or("Failed to parse code")?;

        Ok(Self { code, cursor, tree })
    }

    pub fn analyze(&self) -> CursorPosition {
        debug!("Starting analysis of code: {:?}", self.code);
        let root_node = self.tree.root_node();
        debug!("Root node: {:?}", root_node);

        let cursor_point = Point {
            row: self.cursor.line as usize,
            column: self.cursor.character as usize,
        };
        debug!("Cursor point: {:?}", cursor_point);

        self.find_t_function_call(root_node, cursor_point)
    }

    fn find_t_function_call(&self, node: Node, cursor_point: Point) -> CursorPosition {
        if node.kind() == "call_expression" {
            if let Some(func_node) = node.child_by_field_name("function") {
                if func_node.kind() == "identifier"
                    && func_node.utf8_text(self.code.as_bytes()).unwrap() == "t"
                {
                    debug!("Found 't' function call: {:?}", node);
                    if self.point_in_range(cursor_point, node.start_position(), node.end_position())
                    {
                        return self.analyze_t_call(node, cursor_point);
                    }
                }
            }
        }

        for child in node.named_children(&mut node.walk()) {
            let result = self.find_t_function_call(child, cursor_point);
            if result != CursorPosition::OutsideTFunction {
                return result;
            }
        }

        CursorPosition::OutsideTFunction
    }

    fn analyze_t_call(&self, node: Node, cursor_point: Point) -> CursorPosition {
        debug!("Analyzing t call at {:?}", cursor_point);
        let arguments_node = node
            .child_by_field_name("arguments")
            .expect("t() call should have arguments");
        let argument_nodes: Vec<_> = arguments_node
            .named_children(&mut arguments_node.walk())
            .collect();
        debug!("Number of argument nodes: {}", argument_nodes.len());

        let translation_key = argument_nodes
            .first()
            .filter(|&arg| arg.kind() == "string")
            .map(|arg| self.unquote_string(arg.utf8_text(self.code.as_bytes()).unwrap()));
        debug!("Translation key: {:?}", translation_key);

        if let Some(first_arg) = argument_nodes.first() {
            if self.point_in_range(
                cursor_point,
                first_arg.start_position(),
                first_arg.end_position(),
            ) {
                debug!("Cursor in first parameter");
                return CursorPosition::InFirstParam;
            }
        }

        if let Some(second_arg) = argument_nodes.get(1) {
            if self.point_in_range(
                cursor_point,
                second_arg.start_position(),
                second_arg.end_position(),
            ) {
                return self.analyze_second_param(second_arg, cursor_point, translation_key);
            }
        }

        debug!("Cursor outside t function");
        CursorPosition::OutsideTFunction
    }

    fn analyze_second_param(
        &self,
        node: &Node,
        cursor_point: Point,
        translation_key: Option<String>,
    ) -> CursorPosition {
        if node.kind() == "object" || node.kind() == "object_pattern" {
            if node.named_child_count() == 0 {
                return CursorPosition::InSecondParam {
                    translation_key,
                    position: SecondParamPosition::EmptyObject,
                };
            }

            for prop_node in node.named_children(&mut node.walk()) {
                if self.point_in_range(
                    cursor_point,
                    prop_node.start_position(),
                    prop_node.end_position(),
                ) {
                    if let Some(key) = prop_node.child_by_field_name("key") {
                        let key_text = key
                            .utf8_text(self.code.as_bytes())
                            .unwrap_or("")
                            .to_string();
                        if self.point_in_range(
                            cursor_point,
                            key.start_position(),
                            key.end_position(),
                        ) {
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
                        if self.point_in_range(
                            cursor_point,
                            value.start_position(),
                            value.end_position(),
                        ) {
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

    fn point_in_range(&self, point: Point, start: Point, end: Point) -> bool {
        (start.row < point.row || (start.row == point.row && start.column <= point.column))
            && (point.row < end.row || (point.row == end.row && point.column <= end.column))
    }

    fn unquote_string(&self, s: &str) -> String {
        s.trim_matches(|c| c == '"' || c == '\'' || c == '`')
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use env_logger;

    fn init() {
        let _ = env_logger::builder().is_test(true).try_init();
    }

    #[test]
    fn test_cursor_outside_t_function() {
        init();
        let code = r#"
            const x = 10;
            function test() {
                console.log("Hello, world!");
            }
        "#;
        let cursor = Position {
            line: 2,
            character: 10,
        };
        let parser = TFunctionParser::new(code.to_string(), cursor).unwrap();
        let result = parser.analyze();
        assert_eq!(result, CursorPosition::OutsideTFunction);
    }

    #[test]
    fn test_cursor_in_first_param() {
        init();
        let code = r#"t("some.key", { dynamicKey: "value" });"#;
        let cursor = Position {
            line: 0,
            character: 5,
        };
        let parser = TFunctionParser::new(code.to_string(), cursor).unwrap();
        let result = parser.analyze();
        assert_eq!(result, CursorPosition::InFirstParam);
    }

    #[test]
    fn test_cursor_in_second_param_empty_object() {
        init();
        let code = r#"t("some.key", {});"#;
        let cursor = Position {
            line: 0,
            character: 15,
        };
        let parser = TFunctionParser::new(code.to_string(), cursor).unwrap();
        let result = parser.analyze();
        assert_eq!(
            result,
            CursorPosition::InSecondParam {
                translation_key: Some("some.key".to_string()),
                position: SecondParamPosition::EmptyObject,
            }
        );
    }

    #[test]
    fn test_cursor_in_second_param_in_key() {
        init();
        let code = r#"t("some.key", { dynamicKey: "value" });"#;
        let key_start = code.find("dynamicKey").unwrap();
        let key_end = key_start + "dynamicKey".len();

        for pos in key_start..key_end {
            let cursor = Position {
                line: 0,
                character: pos as u32,
            };
            let parser = TFunctionParser::new(code.to_string(), cursor).unwrap();
            let result = parser.analyze();
            assert_eq!(
                result,
                CursorPosition::InSecondParam {
                    translation_key: Some("some.key".to_string()),
                    position: SecondParamPosition::InKey("dynamicKey".to_string()),
                }
            );
        }
    }
}

