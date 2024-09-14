use crate::lsp::position::{CursorPosition, SecondParamPosition, TFunctionAnalyzer};
use std::collections::HashMap;

use crate::lsp::docs::TypedKeyDocs;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;

use crate::parse::{AstNode, Parser};

use super::typedkey_lsp::TypedKeyLspImpl;
use super::utils::{get_select_options, is_select_variable, traverse_ast_for_variables};

impl TypedKeyLspImpl {
    pub(crate) async fn handle_completion(
        &self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
        let position = params.text_document_position.position;
        let uri = params.text_document_position.text_document.uri;

        let document_content = self
            .document_map
            .get(&uri)
            .map(|content| content.clone())
            .unwrap_or_default();

        let t_function_analyzer = TFunctionAnalyzer::new(&document_content, position)?;
        let cursor_position = t_function_analyzer.analyze();

        match cursor_position {
            CursorPosition::InFirstParam(..) => self.provide_translation_key_completions().await,
            CursorPosition::InSecondParam {
                translation_key,
                position,
            } => {
                self.provide_second_param_completions(Some(translation_key), position)
                    .await
            }
            CursorPosition::OutsideTFunction => Ok(None),
        }
    }

    async fn provide_translation_key_completions(&self) -> Result<Option<CompletionResponse>> {
        let completions = self
            .translation_keys
            .iter()
            .map(|entry| {
                let key = entry.key();
                let value = entry.value();
                let (variables, select_options) = self.extract_variables_and_options(value);

                let detail = self.format_completion_detail(key, &variables, &select_options);
                let documentation =
                    self.format_completion_documentation(key, value, &variables, &select_options);

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

    fn format_completion_detail(
        &self,
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

    fn format_completion_documentation(
        &self,
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

    async fn provide_second_param_completions(
        &self,
        translation_key: Option<String>,
        position: SecondParamPosition,
    ) -> Result<Option<CompletionResponse>> {
        let Some(key) = translation_key else {
            return Ok(None);
        };

        let Some(value) = self.translation_keys.get(&key) else {
            return Ok(None);
        };

        let ast = Parser::new(value.as_str().unwrap_or_default())
            .parse()
            .map_err(|_| Error::internal_error())?;

        let completions = match position {
            SecondParamPosition::EmptyObject
            | SecondParamPosition::InObject
            | SecondParamPosition::InKey(_) => self.get_variable_completions(&ast, &key),
            SecondParamPosition::InValue(var_name) => {
                self.get_value_completions(&ast, &var_name, &key)
            }
        };

        Ok(Some(CompletionResponse::Array(completions)))
    }

    fn get_variable_completions(&self, ast: &AstNode, key: &str) -> Vec<CompletionItem> {
        let variables = self.extract_variables_from_ast(ast);
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

    fn get_value_completions(
        &self,
        ast: &AstNode,
        var_name: &str,
        key: &str,
    ) -> Vec<CompletionItem> {
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

    fn extract_variables_from_ast(&self, ast: &AstNode) -> Vec<String> {
        let mut variables = Vec::new();
        traverse_ast_for_variables(ast, &mut variables);
        variables
    }
}
