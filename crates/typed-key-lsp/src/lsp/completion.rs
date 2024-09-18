use std::collections::HashMap;

use crate::lsp::docs::TypedKeyDocs;
use crate::parse::AstNode;
use crate::Parser;
use ropey::Rope;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;
use tracing::debug;

use super::ast::extract_variables_and_options;
use super::position::{SecondParamPosition, TFunctionParser, TFunctionPosition};
use super::utils::{get_select_options, is_select_variable, traverse_ast_for_variables};

pub async fn handle_completion(
    params: CompletionParams,
    document: &Rope,
    translation_keys: &HashMap<String, serde_json::Value>,
) -> Result<Option<CompletionResponse>> {
    let document_str = document.to_string();

    let position = params.text_document_position.position;
    let file_path = params.text_document_position.text_document.uri.path();

    let parser = TFunctionParser::new(&document_str, position, file_path)
        .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;

    match parser.parse() {
        TFunctionPosition::InFirstArgument(_) => {
            provide_translation_key_completions(translation_keys)
        }
        TFunctionPosition::InSecondArgument { key, position } => {
            provide_second_param_completions(Some(key), position, translation_keys)
        }
        _ => Ok(None),
    }
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
        .map_err(|_| Error::internal_error())?;

    debug!("IN POSITION --------> {:?}", position);

    let completions = match position {
        SecondParamPosition::EmptyObject
        | SecondParamPosition::InObject
        | SecondParamPosition::InKey(_) => get_variable_completions(&ast, &key),
        SecondParamPosition::InValue(var_name) => get_value_completions(&ast, &var_name, &key),
    };

    Ok(Some(CompletionResponse::Array(completions)))
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

fn extract_variables_from_ast(ast: &AstNode) -> Vec<String> {
    let mut variables = Vec::new();
    traverse_ast_for_variables(ast, &mut variables);
    variables
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
