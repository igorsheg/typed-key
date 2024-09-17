use std::collections::HashMap;

use super::ast::extract_variables_and_options;
use super::docs::TypedKeyDocs;
use super::position::CursorPosition;
use super::utils::traverse_nodes;
use crate::lsp::position::TFunctionAnalyzer;
use ropey::Rope;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub(crate) async fn hover(
    params: HoverParams,
    document: &Rope,
    translation_keys: &HashMap<String, serde_json::Value>,
) -> Result<Option<Hover>> {
    let position = params.text_document_position_params.position;

    let document = document.to_string();

    let analyzer = TFunctionAnalyzer::new(&document, position)?;
    let cursor_position = analyzer.analyze();

    match cursor_position {
        CursorPosition::InFirstParam(key)
        | CursorPosition::InSecondParam {
            translation_key: key,
            ..
        } => provide_t_function_documentation(&key, &document, position, translation_keys),
        _ => Ok(None),
    }
}

fn provide_t_function_documentation(
    key: &str,
    document_content: &str,
    position: Position,
    translation_keys: &HashMap<String, serde_json::Value>,
) -> Result<Option<Hover>> {
    if let Some(value) = translation_keys.get(key) {
        let (variables, select_options) = extract_variables_and_options(value);
        let documentation = format_hover_documentation(key, value, &variables, &select_options);

        let analyzer = TFunctionAnalyzer::new(document_content, position)?;
        let root_node = analyzer.tree.root_node();
        let nodes = traverse_nodes(root_node);

        let t_function_node = nodes
            .into_iter()
            .find(|node| analyzer.is_t_function_call(node))
            .expect("T function node should exist");

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
