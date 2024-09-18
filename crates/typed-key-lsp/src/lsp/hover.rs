use std::collections::HashMap;

use super::ast::extract_variables_and_options;
use super::docs::TypedKeyDocs;
use super::position::{TFunctionParser, TFunctionPosition};
use ropey::Rope;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub(crate) async fn hover(
    params: HoverParams,
    document: &Rope,
    translation_keys: &HashMap<String, serde_json::Value>,
) -> Result<Option<Hover>> {
    let position = params.text_document_position_params.position;
    let document_str = document.to_string();
    let file_path = params
        .text_document_position_params
        .text_document
        .uri
        .path();

    let parser = TFunctionParser::new(&document_str, position, file_path)
        .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

    match parser.parse() {
        TFunctionPosition::InFirstArgument(key)
        | TFunctionPosition::InSecondArgument { key, .. } => {
            provide_t_function_documentation(&key, position, translation_keys, &parser)
        }
        _ => Ok(None),
    }
}

fn provide_t_function_documentation(
    key: &str,
    position: Position,
    translation_keys: &HashMap<String, serde_json::Value>,
    parser: &TFunctionParser,
) -> Result<Option<Hover>> {
    if let Some(value) = translation_keys.get(key) {
        let (variables, select_options) = extract_variables_and_options(value);
        let documentation = format_hover_documentation(key, value, &variables, &select_options);

        if let Some(t_function_node) = parser.find_parent_t_function(
            &parser
                .tree()
                .root_node()
                .named_descendant_for_point_range(
                    tree_sitter::Point {
                        row: position.line as usize,
                        column: position.character as usize,
                    },
                    tree_sitter::Point {
                        row: position.line as usize,
                        column: position.character as usize,
                    },
                )
                .expect("Failed to find node at position"),
        ) {
            let range = Range {
                start: Position::new(
                    t_function_node.start_position().row as u32,
                    t_function_node.start_position().column as u32,
                ),
                end: Position::new(
                    t_function_node.end_position().row as u32,
                    t_function_node.end_position().column as u32,
                ),
            };

            Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: documentation,
                }),
                range: Some(range),
            }))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
}

fn format_hover_documentation(
    key: &str,
    value: &serde_json::Value,
    variables: &[String],
    select_options: &std::collections::HashMap<String, Vec<String>>,
) -> String {
    let typed_key_docs = TypedKeyDocs::new();
    typed_key_docs.format_documentation(
        key,
        value,
        variables,
        &select_options
            .values()
            .flatten()
            .cloned()
            .collect::<Vec<_>>(),
    )
}
