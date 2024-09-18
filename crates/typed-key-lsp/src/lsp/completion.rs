use crate::lsp::position::SecondParamPosition;
use std::collections::HashMap;
use std::path::Path;

use crate::lsp::docs::TypedKeyDocs;
use ropey::Rope;
use tower_lsp::jsonrpc::Result;
use tower_lsp::{lsp_types::*, Client};
use tracing::{debug, error};
use tree_sitter::{Node, Point, QueryCursor};
use tree_sitter_typescript::{language_tsx, language_typescript};

use crate::parse::{AstNode, Parser};

use super::ast::extract_variables_and_options;
use super::queries::Queries;
use super::utils::{get_select_options, is_select_variable, traverse_ast_for_variables};

pub async fn handle_completion(
    params: CompletionParams,
    document: &Rope,
    translation_keys: &HashMap<String, serde_json::Value>,
) -> Result<Option<CompletionResponse>> {
    debug!("Entering handle_completion");
    let position = params.text_document_position.position;
    let document_str = document.to_string();

    debug!("Document position: {:?}", position);
    debug!("Document content:\n{}", document_str);

    let queries = Queries::default();

    // Determine if we're dealing with a .ts or .tsx file
    let file_path = params.text_document_position.text_document.uri.path();
    let is_tsx = Path::new(file_path)
        .extension()
        .map_or(false, |ext| ext == "tsx");

    let (language, query) = if is_tsx {
        (language_tsx(), &queries.tsx_t_function)
    } else {
        (language_typescript(), &queries.ts_t_function)
    };

    debug!("File type: {}", if is_tsx { "TSX" } else { "TS" });
    debug!("Query capture names: {:?}", query.capture_names());
    debug!("Query pattern count: {}", query.pattern_count());

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&language)
        .expect("Error loading language grammar");

    let tree = parser
        .parse(&document_str, None)
        .expect("Failed to parse document");

    let node = tree
        .root_node()
        .named_descendant_for_point_range(
            Point {
                row: position.line as usize,
                column: position.character as usize,
            },
            Point {
                row: position.line as usize,
                column: position.character as usize,
            },
        )
        .expect("Failed to find node at position");

    debug!("Found node at position: {:?}", node.kind());
    debug!("Node text: {:?}", node.utf8_text(document_str.as_bytes()));
    debug!(
        "Node start: {:?}, Node end: {:?}",
        node.start_position(),
        node.end_position()
    );

    let mut query_cursor = QueryCursor::new();
    let matches = query_cursor.matches(query, tree.root_node(), document_str.as_bytes());

    for match_ in matches {
        debug!("Match found: {:?}", match_.pattern_index);
        let mut func_name_node = None;
        let mut first_arg_node = None;
        let mut second_arg_node = None;

        for capture in match_.captures {
            let capture_name = query.capture_names()[capture.index as usize];
            let capture_text = capture
                .node
                .utf8_text(document_str.as_bytes())
                .unwrap_or("ERROR");
            debug!("Capture: {} = {:?}", capture_name, capture_text);

            match capture_name {
                "func_name" => func_name_node = Some(capture.node),
                "first_arg" => first_arg_node = Some(capture.node),
                "second_arg" => second_arg_node = Some(capture.node),
                _ => {}
            }
        }

        if let (Some(func), Some(first), _) = (func_name_node, first_arg_node, second_arg_node) {
            if func.utf8_text(document_str.as_bytes()).unwrap_or("") == "t" {
                if is_cursor_in_node(first, position) {
                    debug!("Cursor is in first argument");
                    return provide_translation_key_completions(translation_keys);
                } else if let Some(second) = second_arg_node {
                    if is_cursor_in_node(second, position) {
                        debug!("Cursor is in second argument");
                        let key = first
                            .utf8_text(document_str.as_bytes())
                            .map(|s| s.trim_matches('"').to_string())
                            .ok();
                        return provide_second_param_completions(
                            key,
                            SecondParamPosition::InObject,
                            translation_keys,
                        );
                    }
                }
            }
        }
    }

    debug!("No completions provided, returning None");
    Ok(None)
}

fn is_cursor_in_node(node: Node, position: Position) -> bool {
    let start = node.start_position();
    let end = node.end_position();

    (start.row as u32 <= position.line && position.line <= end.row as u32)
        && (start.row as u32 != position.line || start.column as u32 <= position.character)
        && (end.row as u32 != position.line || position.character <= end.column as u32)
}

fn provide_translation_key_completions(
    translation_keys: &HashMap<String, serde_json::Value>,
) -> Result<Option<CompletionResponse>> {
    let completions = translation_keys
        .iter()
        .map(|(key, value)| {
            let (variables, select_options) = extract_variables_and_options(value);

            let detail = format_completion_detail(key, &variables, &select_options);
            let documentation =
                format_completion_documentation(key, value, &variables, &select_options);

            CompletionItem {
                label: key.to_owned(),
                kind: Some(CompletionItemKind::CONSTANT),
                detail: Some(detail),
                documentation: Some(documentation),
                ..Default::default()
            }
        })
        .collect();

    Ok(Some(CompletionResponse::Array(completions)))
}

fn provide_second_param_completions(
    translation_key: Option<String>,
    position: SecondParamPosition,
    translation_keys: &HashMap<String, serde_json::Value>,
) -> Result<Option<CompletionResponse>> {
    let Some(key) = translation_key else {
        return Ok(None);
    };

    let Some(value) = translation_keys.get(&key) else {
        return Ok(None);
    };

    let ast = Parser::new(value.as_str().unwrap_or_default())
        .parse()
        .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

    let completions = match position {
        SecondParamPosition::EmptyObject
        | SecondParamPosition::InObject
        | SecondParamPosition::InKey(_) => get_variable_completions(&ast, &key),
        SecondParamPosition::InValue(var_name) => get_value_completions(&ast, &var_name, &key),
    };

    Ok(Some(CompletionResponse::Array(completions)))
}

fn format_completion_documentation(
    key: &str,
    value: &serde_json::Value,
    variables: &[String],
    select_options: &HashMap<String, Vec<String>>,
) -> Documentation {
    let typed_key_docs = TypedKeyDocs::new();
    let documentation = typed_key_docs.format_documentation(
        key,
        value,
        variables,
        &select_options
            .values()
            .flatten()
            .cloned()
            .collect::<Vec<_>>(),
    );

    Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value: documentation,
    })
}

fn get_variable_completions(ast: &AstNode, key: &str) -> Vec<CompletionItem> {
    let variables = extract_variables_from_ast(ast);
    variables
        .into_iter()
        .map(|var| {
            let kind = if is_select_variable(ast, &var) {
                CompletionItemKind::ENUM
            } else {
                CompletionItemKind::VARIABLE
            };
            CompletionItem {
                label: var.clone(),
                kind: Some(kind),
                detail: Some(format!("Variable for key: {}", key)),
                insert_text: Some(format!("{}: ", var)),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            }
        })
        .collect()
}

fn extract_variables_from_ast(ast: &AstNode) -> Vec<String> {
    let mut variables = Vec::new();
    traverse_ast_for_variables(ast, &mut variables);
    variables
}

fn format_completion_detail(
    key: &str,
    variables: &[String],
    select_options: &HashMap<String, Vec<String>>,
) -> String {
    let mut detail = format!("Translation key: {}", key);
    if !variables.is_empty() {
        detail.push_str("\nParameters: ");
        detail.push_str(&variables.join(", "));
    }
    if !select_options.is_empty() {
        detail.push_str("\nSelect options:");
        for (var, options) in select_options {
            detail.push_str(&format!("\n  {}: {}", var, options.join(", ")));
        }
    }
    detail
}

fn get_value_completions(ast: &AstNode, var_name: &str, key: &str) -> Vec<CompletionItem> {
    if let Some(options) = get_select_options(ast, var_name) {
        options
            .into_iter()
            .map(|option| CompletionItem {
                label: option.clone(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some(format!("Select option for {}: {}", var_name, key)),
                insert_text: Some(option),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            })
            .collect()
    } else {
        vec![CompletionItem {
            label: "value".to_string(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some(format!("Value for {}: {}", var_name, key)),
            insert_text: Some("value".to_string()),
            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            ..Default::default()
        }]
    }
}
