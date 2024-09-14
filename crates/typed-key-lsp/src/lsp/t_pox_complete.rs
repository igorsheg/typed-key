use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;

use crate::parse::AstNode;
use crate::Parser;

use super::{docs::TypedKeyDocs, t_pox::*, typedkey_lsp::TypedKeyLspImpl};

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

        let t_function_parser = TFunctionParser::new(document_content.clone(), position)
            .map_err(|_| Error::internal_error())?;
        let cursor_position = t_function_parser.analyze();

        match cursor_position {
            CursorPosition::InFirstParam => self.provide_translation_key_completions().await,
            CursorPosition::InSecondParam {
                translation_key,
                position,
            } => {
                self.provide_second_param_completions(translation_key, position)
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

                let mut detail = format!("Translation key: {}", key);
                if !variables.is_empty() {
                    detail.push_str("\nParameters: ");
                    detail.push_str(&variables.join(", "));
                }
                if !select_options.is_empty() {
                    detail.push_str("\nSelect options:");
                    for (var, options) in select_options.iter() {
                        detail.push_str(&format!("\n  {}: {}", var, options.join(", ")));
                    }
                }

                let typed_key_docs = TypedKeyDocs::new();
                let documentation = typed_key_docs.format_documentation(
                    key,
                    value,
                    &variables,
                    &select_options
                        .iter()
                        .flat_map(|(_, v)| v)
                        .cloned()
                        .collect::<Vec<_>>(),
                );

                CompletionItem {
                    label: key.to_owned(),
                    kind: Some(CompletionItemKind::CONSTANT),
                    detail: Some(detail),
                    documentation: Some(Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: documentation,
                    })),
                    ..Default::default()
                }
            })
            .collect();

        Ok(Some(CompletionResponse::Array(completions)))
    }
    async fn provide_second_param_completions(
        &self,
        translation_key: Option<String>,
        position: SecondParamPosition,
    ) -> Result<Option<CompletionResponse>> {
        if let Some(key) = translation_key {
            if let Some(value) = self.translation_keys.get(&key) {
                let ast = Parser::new(value.as_str().unwrap_or_default())
                    .parse()
                    .map_err(|_| Error::internal_error())?;

                let mut completions = Vec::new();

                match position {
                    SecondParamPosition::EmptyObject | SecondParamPosition::InObject => {
                        self.provide_variable_completions(&ast, &mut completions, &key);
                    }
                    SecondParamPosition::InKey(_) => {
                        // Provide completions for variable names
                        self.provide_variable_completions(&ast, &mut completions, &key);
                    }
                    SecondParamPosition::InValue(var_name) => {
                        self.provide_value_completions(&ast, &var_name, &mut completions, &key);
                    }
                }

                Ok(Some(CompletionResponse::Array(completions)))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    fn provide_variable_completions(
        &self,
        ast: &AstNode,
        completions: &mut Vec<CompletionItem>,
        key: &str,
    ) {
        let variables = self.extract_variables_from_ast(ast);
        for var in variables {
            let kind = if self.is_select_variable(ast, &var) {
                CompletionItemKind::ENUM
            } else {
                CompletionItemKind::VARIABLE
            };
            completions.push(CompletionItem {
                label: var.clone(),
                kind: Some(kind),
                detail: Some(format!("Variable for key: {}", key)),
                insert_text: Some(format!("{}: ", var)),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            });
        }
    }

    fn provide_value_completions(
        &self,
        ast: &AstNode,
        var_name: &str,
        completions: &mut Vec<CompletionItem>,
        key: &str,
    ) {
        if let Some(options) = self.get_select_options(ast, var_name) {
            for option in options {
                completions.push(CompletionItem {
                    label: option.clone(),
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    detail: Some(format!("Select option for {}: {}", var_name, key)),
                    insert_text: Some(option.clone()),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                    ..Default::default()
                });
            }
        } else {
            // If it's not a select variable, we don't provide specific completions
            completions.push(CompletionItem {
                label: "value".to_string(),
                kind: Some(CompletionItemKind::VALUE),
                detail: Some(format!("Value for {}: {}", var_name, key)),
                insert_text: Some("value".to_string()),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                ..Default::default()
            });
        }
    }

    fn is_select_variable(&self, ast: &AstNode, var_name: &str) -> bool {
        match ast {
            AstNode::Root(children) => children
                .iter()
                .any(|child| self.is_select_variable(child, var_name)),
            AstNode::Select { variable, .. } => variable == var_name,
            _ => false,
        }
    }

    fn get_select_options(&self, ast: &AstNode, var_name: &str) -> Option<Vec<String>> {
        match ast {
            AstNode::Root(children) => {
                for child in children {
                    if let Some(options) = self.get_select_options(child, var_name) {
                        return Some(options);
                    }
                }
                None
            }
            AstNode::Select { variable, options } if variable == var_name => {
                Some(options.keys().cloned().collect())
            }
            _ => None,
        }
    }

    fn extract_variables_from_ast(&self, ast: &AstNode) -> Vec<String> {
        let mut variables = Vec::new();
        self.traverse_ast_for_variables(ast, &mut variables);
        variables
    }

    fn traverse_ast_for_variables(&self, node: &AstNode, variables: &mut Vec<String>) {
        match node {
            AstNode::Root(children) => {
                for child in children {
                    self.traverse_ast_for_variables(child, variables);
                }
            }
            AstNode::Variable(var) => {
                if !variables.contains(var) {
                    variables.push(var.clone());
                }
            }
            AstNode::Plural { variable, options } | AstNode::Select { variable, options } => {
                if !variables.contains(variable) {
                    variables.push(variable.clone());
                }
                for (_, value) in options {
                    for child in value {
                        self.traverse_ast_for_variables(child, variables);
                    }
                }
            }
            AstNode::HtmlTag { children, .. } => {
                for child in children {
                    self.traverse_ast_for_variables(child, variables);
                }
            }
            _ => {}
        }
    }
}
