use std::collections::HashSet;

use serde_json::Value;
use tower_lsp::jsonrpc::Error;
use tower_lsp::{
    jsonrpc::Result,
    lsp_types::{
        CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse, Documentation,
        InsertTextFormat, MarkupContent, MarkupKind, MessageType, Position, Range,
    },
};
use tree_sitter::{Parser, Query, QueryCursor};

use crate::parse;
use crate::parse::AstNode;

use super::docs::TypedKeyDocs;
use super::typedkey_lsp::TypedKeyLspImpl;
use super::utils::is_position_in_node;
use super::utils::node_to_range;

#[derive(Debug)]
pub(crate) enum TFunctionPosition {
    FirstParam,
    SecondParam(String),
    Outside,
}

impl TypedKeyLspImpl {
    pub(crate) async fn handle_completion(
        &self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
        self.client
            .log_message(MessageType::INFO, "received completion req")
            .await;
        let position = params.text_document_position.position;

        let document_content = self
            .document_map
            .get(&params.text_document_position.text_document.uri)
            .map(|content| content.clone())
            .unwrap_or_default();

        let t_function_position = self
            .get_tfunction_position(&document_content, position)
            .await?;

        self.client
            .log_message(
                MessageType::INFO,
                format!("t_function_poision: {:?}", t_function_position),
            )
            .await;

        match t_function_position {
            TFunctionPosition::FirstParam => {
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
                            detail.push_str("\nSelect options: ");
                            detail.push_str(&select_options.join(", "));
                        }
                        let typed_key_docs = TypedKeyDocs::new();
                        let documentation = typed_key_docs.format_documentation(
                            key,
                            value,
                            &variables,
                            &select_options,
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
            TFunctionPosition::SecondParam(key) => {
                if let Some(value) = self.translation_keys.get(&key) {
                    let (variables, select_options) = self.extract_variables_and_options(&value);
                    let mut completions: Vec<CompletionItem> = variables
                        .into_iter()
                        .map(|var| CompletionItem {
                            label: var.clone(),
                            kind: Some(CompletionItemKind::VARIABLE),
                            detail: Some(format!("Variable for key: {}", key)),
                            insert_text: Some(format!("{}: ", var)),
                            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                            ..Default::default()
                        })
                        .collect();

                    // Add select options as completions
                    for option in select_options {
                        completions.push(CompletionItem {
                            label: option.clone(),
                            kind: Some(CompletionItemKind::ENUM_MEMBER),
                            detail: Some(format!("Select option for key: {}", key)),
                            insert_text: Some(option),
                            insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                            ..Default::default()
                        });
                    }

                    Ok(Some(CompletionResponse::Array(completions)))
                } else {
                    Ok(None)
                }
            }
            TFunctionPosition::Outside => Ok(None),
        }
    }

    pub(crate) async fn get_tfunction_position(
        &self,
        content: &str,
        position: Position,
    ) -> Result<TFunctionPosition> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::language_typescript())
            .map_err(|_| Error::internal_error())?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(Error::internal_error)?;

        let point = tree_sitter::Point {
            row: position.line as usize,
            column: position.character as usize,
        };

        let node = tree
            .root_node()
            .descendant_for_point_range(point, point)
            .ok_or_else(Error::internal_error)?;

        // Traverse up the tree to find the call_expression
        let mut current_node = node;
        while current_node.kind() != "call_expression" {
            if let Some(parent) = current_node.parent() {
                current_node = parent;
            } else {
                return Ok(TFunctionPosition::Outside);
            }
        }

        let query = Query::new(
            &tree_sitter_typescript::language_typescript(),
            "(call_expression
            function: (identifier) @func (#eq? @func \"t\")
            arguments: (arguments
                (string (string_fragment) @key)?
                (object)? @options))",
        )
        .map_err(|_| Error::internal_error())?;

        let mut query_cursor = QueryCursor::new();
        let matches = query_cursor.matches(&query, current_node, content.as_bytes());

        if let Some(match_) = matches.into_iter().next() {
            let func_node = match_.captures[0].node;
            let args_node = current_node
                .child_by_field_name("arguments")
                .ok_or_else(Error::internal_error)?;

            if is_position_in_node(position, func_node) {
                return Ok(TFunctionPosition::FirstParam);
            }

            for (i, arg) in args_node.named_children(&mut args_node.walk()).enumerate() {
                if is_position_in_node(position, arg) {
                    if i == 0 {
                        return Ok(TFunctionPosition::FirstParam);
                    } else if i == 1 {
                        let key = match_.captures[1]
                            .node
                            .utf8_text(content.as_bytes())
                            .map_err(|_| Error::internal_error())?
                            .to_string();
                        return Ok(TFunctionPosition::SecondParam(key));
                    }
                }
            }
        }

        Ok(TFunctionPosition::Outside)
    }

    pub(crate) fn extract_variables_and_options(
        &self,
        value: &Value,
    ) -> (Vec<String>, Vec<String>) {
        if let Value::String(s) = value {
            let parser = parse::Parser::new(s);
            if let Ok(ast) = parser.parse() {
                collect_variables_and_options(&ast)
            } else {
                (Vec::new(), Vec::new())
            }
        } else {
            (Vec::new(), Vec::new())
        }
    }

    pub(crate) async fn get_translation_key_at_position(
        &self,
        content: &str,
        position: Position,
    ) -> Result<Option<(String, Range)>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::language_typescript())
            .map_err(|_| Error::internal_error())?;

        let tree = parser
            .parse(content, None)
            .ok_or_else(Error::internal_error)?;
        let root_node = tree.root_node();

        let query = Query::new(
            &tree_sitter_typescript::language_typescript(),
            "(call_expression
                function: (identifier) @func (#eq? @func \"t\")
                arguments: (arguments
                    (string (string_fragment) @key)))",
        )
        .map_err(|_| Error::internal_error())?;

        let mut query_cursor = QueryCursor::new();
        let matches = query_cursor.matches(&query, root_node, content.as_bytes());

        for match_ in matches {
            if let Some(key_node) = match_.nodes_for_capture_index(1).next() {
                if is_position_in_node(position, key_node) {
                    let key = key_node.utf8_text(content.as_bytes()).unwrap().to_string();
                    let range = node_to_range(key_node);
                    return Ok(Some((key, range)));
                }
            }
        }

        Ok(None)
    }
}

fn collect_variables_and_options(node: &AstNode) -> (Vec<String>, Vec<String>) {
    let mut variables = HashSet::new();
    let mut select_options = HashSet::new();

    match node {
        AstNode::Root(children) => {
            for child in children {
                let (vars, options) = collect_variables_and_options(child);
                variables.extend(vars);
                select_options.extend(options);
            }
        }
        AstNode::Variable(var) => {
            variables.insert(var.clone());
        }
        AstNode::Plural { variable, options } => {
            variables.insert(variable.clone());
            for (option_key, option_nodes) in options {
                select_options.insert(option_key.clone());
                for option_node in option_nodes {
                    let (vars, opts) = collect_variables_and_options(option_node);
                    variables.extend(vars);
                    select_options.extend(opts);
                }
            }
        }
        AstNode::Select { variable, options } => {
            variables.insert(variable.clone());
            for (option_key, option_nodes) in options {
                select_options.insert(option_key.clone());
                for option_node in option_nodes {
                    let (vars, opts) = collect_variables_and_options(option_node);
                    variables.extend(vars);
                    select_options.extend(opts);
                }
            }
        }
        AstNode::HtmlTag { children, .. } => {
            for child in children {
                let (vars, options) = collect_variables_and_options(child);
                variables.extend(vars);
                select_options.extend(options);
            }
        }
        _ => {}
    }

    (
        variables.into_iter().collect(),
        select_options.into_iter().collect(),
    )
}
