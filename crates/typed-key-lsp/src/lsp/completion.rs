use serde_json::Value;
use tower_lsp::jsonrpc::Error;
use tower_lsp::lsp_types::TextDocumentPositionParams;
use tower_lsp::{
    jsonrpc::Result,
    lsp_types::{
        CompletionItem, CompletionItemKind, CompletionParams, CompletionResponse, Documentation,
        InsertTextFormat, MarkupContent, MarkupKind, MessageType, Position,
    },
};

use crate::parse;
use crate::parse::AstNode;

use super::docs::TypedKeyDocs;
use super::typedkey_lsp::TypedKeyLspImpl;
use super::utils::is_position_in_node;

#[derive(Debug)]
pub(crate) enum TFunctionPosition {
    FirstParam,
    SecondParam(String),
    Outside,
}

impl TypedKeyLspImpl {
    // pub(crate) async fn handle_completion(
    //     &self,
    //     params: CompletionParams,
    // ) -> Result<Option<CompletionResponse>> {
    //     self.client
    //         .log_message(MessageType::INFO, "Received completion request")
    //         .await;
    //
    //     let position = params.text_document_position.position;
    //     let document_content = self
    //         .document_map
    //         .get(&params.text_document_position.text_document.uri)
    //         .map(|content| content.clone())
    //         .unwrap_or_default();
    //
    //     let t_function_position = self
    //         .get_tfunction_position_with_optimized_query(&document_content, position)
    //         .await?;
    //
    //     self.client
    //         .log_message(
    //             MessageType::INFO,
    //             format!("t_function_position: {:?}", t_function_position),
    //         )
    //         .await;
    //
    //     match t_function_position {
    //         TFunctionPosition::FirstParam => {
    //             let completions = self
    //                 .translation_keys
    //                 .iter()
    //                 .map(|entry| {
    //                     let key = entry.key();
    //                     let value = entry.value();
    //                     let (variables, select_options) = self.extract_variables_and_options(value);
    //
    //                     let mut detail = format!("Translation key: {}", key);
    //                     if !variables.is_empty() {
    //                         detail.push_str("\nParameters: ");
    //                         detail.push_str(&variables.join(", "));
    //                     }
    //                     if !select_options.is_empty() {
    //                         detail.push_str("\nSelect options:");
    //                         for (var, options) in select_options.iter() {
    //                             detail.push_str(&format!("\n  {}: {}", var, options.join(", ")));
    //                         }
    //                     }
    //
    //                     let typed_key_docs = TypedKeyDocs::new();
    //                     let documentation = typed_key_docs.format_documentation(
    //                         key,
    //                         value,
    //                         &variables,
    //                         &select_options
    //                             .iter()
    //                             .flat_map(|(_, v)| v)
    //                             .cloned()
    //                             .collect::<Vec<_>>(),
    //                     );
    //
    //                     CompletionItem {
    //                         label: key.to_owned(),
    //                         kind: Some(CompletionItemKind::CONSTANT),
    //                         detail: Some(detail),
    //                         documentation: Some(Documentation::MarkupContent(MarkupContent {
    //                             kind: MarkupKind::Markdown,
    //                             value: documentation,
    //                         })),
    //                         ..Default::default()
    //                     }
    //                 })
    //                 .collect();
    //
    //             Ok(Some(CompletionResponse::Array(completions)))
    //         }
    //         TFunctionPosition::SecondParam(key) => {
    //             if let Some(entry) = self.translation_keys.get(&key) {
    //                 let value = entry.value();
    //                 let (variables, select_options) = self.extract_variables_and_options(value);
    //
    //                 let (current_param, is_value_context, used_params) = self
    //                     .get_current_parameter_context(&params.text_document_position)
    //                     .await?;
    //
    //                 let mut completions: Vec<CompletionItem> = Vec::new();
    //
    //                 match (current_param, is_value_context) {
    //                     (Some(param), true) => {
    //                         // If we're editing a specific parameter's value, provide only its select options
    //                         if let Some(options) = select_options.get(&param) {
    //                             for option in options {
    //                                 completions.push(CompletionItem {
    //                                     label: option.clone(),
    //                                     kind: Some(CompletionItemKind::ENUM_MEMBER),
    //                                     detail: Some(format!(
    //                                         "Select option for {}: {}",
    //                                         param, key
    //                                     )),
    //                                     insert_text: Some(option.clone()),
    //                                     insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
    //                                     ..Default::default()
    //                                 });
    //                             }
    //                         }
    //                     }
    //                     _ => {
    //                         // If we're not editing a specific parameter's value, provide unused variables as completions
    //                         for var in &variables {
    //                             if !used_params.contains(var) {
    //                                 let kind = if select_options.contains_key(var) {
    //                                     CompletionItemKind::ENUM
    //                                 } else {
    //                                     CompletionItemKind::VARIABLE
    //                                 };
    //                                 completions.push(CompletionItem {
    //                                     label: var.clone(),
    //                                     kind: Some(kind),
    //                                     detail: Some(format!("Variable for key: {}", key)),
    //                                     insert_text: Some(format!("{}: ", var)),
    //                                     insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
    //                                     ..Default::default()
    //                                 });
    //                             }
    //                         }
    //                     }
    //                 }
    //
    //                 Ok(Some(CompletionResponse::Array(completions)))
    //             } else {
    //                 Ok(None)
    //             }
    //         }
    //         TFunctionPosition::Outside => Ok(None),
    //     }
    // }

    async fn get_current_parameter_context(
        &self,
        position: &TextDocumentPositionParams,
    ) -> Result<(Option<String>, bool, Vec<String>)> {
        let document_content = self
            .document_map
            .get(&position.text_document.uri)
            .map(|content| content.clone())
            .unwrap_or_default();

        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::language_typescript())
            .map_err(|_| Error::internal_error())?;

        let tree = parser
            .parse(&document_content, None)
            .ok_or_else(Error::internal_error)?;

        let point = tree_sitter::Point {
            row: position.position.line as usize,
            column: position.position.character as usize,
        };

        let node = tree
            .root_node()
            .descendant_for_point_range(point, point)
            .ok_or_else(Error::internal_error)?;

        self.find_current_parameter_context(node, &document_content, point)
    }

    fn find_current_parameter_context(
        &self,
        node: tree_sitter::Node,
        content: &str,
        point: tree_sitter::Point,
    ) -> Result<(Option<String>, bool, Vec<String>)> {
        let mut current_node = node;
        let mut used_params = Vec::new();
        let mut current_param = None;
        let mut is_value_context = false;

        while current_node.kind() != "call_expression" {
            if let Some(parent) = current_node.parent() {
                current_node = parent;
            } else {
                return Ok((None, false, used_params));
            }
        }

        if let Some(arguments_node) = current_node.child_by_field_name("arguments") {
            let mut walker = arguments_node.walk();
            let arg_nodes: Vec<_> = arguments_node.named_children(&mut walker).collect();

            if arg_nodes.len() >= 2 {
                let second_arg = arg_nodes[1];
                if second_arg.kind() == "object" {
                    for child in second_arg.named_children(&mut second_arg.walk()) {
                        if child.kind() == "pair" {
                            if let Some(key_node) = child.child_by_field_name("key") {
                                let param =
                                    key_node.utf8_text(content.as_bytes()).map_err(|_| {
                                        Error::invalid_params("Failed to parse parameter name")
                                    })?;
                                used_params.push(param.to_string());

                                if child.start_position() <= point && point <= child.end_position()
                                {
                                    current_param = Some(param.to_string());
                                    if let Some(value_node) = child.child_by_field_name("value") {
                                        is_value_context = value_node.start_position() <= point;
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok((current_param, is_value_context, used_params))
    }

    pub(crate) async fn get_tfunction_position_with_optimized_query(
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

        let mut current_node = node;
        while current_node.kind() != "call_expression" {
            if let Some(parent) = current_node.parent() {
                current_node = parent;
            } else {
                return Ok(TFunctionPosition::Outside);
            }
        }

        if let Some(arguments_node) = current_node.child_by_field_name("arguments") {
            let mut walker = arguments_node.walk();
            let mut arg_iter = arguments_node.named_children(&mut walker);

            if let Some(first_arg) = arg_iter.next() {
                if is_position_in_node(position, first_arg) {
                    return Ok(TFunctionPosition::FirstParam);
                }

                if let Some(second_arg) = arg_iter.next() {
                    if is_position_in_node(position, second_arg) {
                        let key = first_arg
                            .utf8_text(content.as_bytes())
                            .map_err(|_| Error::invalid_params("Failed to parse key text"))?;
                        let key = key.trim_matches(|c| c == '\'' || c == '"');
                        return Ok(TFunctionPosition::SecondParam(key.to_string()));
                    }
                }
            }
        }

        Ok(TFunctionPosition::Outside)
    }

    pub(crate) async fn get_translation_key_at_position(
        &self,
        content: &str,
        position: Position,
    ) -> Result<Option<(String, tower_lsp::lsp_types::Range)>> {
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

        let mut current_node = node;
        while current_node.kind() != "call_expression" {
            if let Some(parent) = current_node.parent() {
                current_node = parent;
            } else {
                return Ok(None);
            }
        }

        if let Some(func_node) = current_node.child_by_field_name("function") {
            let func_text = func_node
                .utf8_text(content.as_bytes())
                .map_err(|_| Error::invalid_params("Failed to parse function text"))?;

            if func_text == "t" {
                if let Some(arguments_node) = current_node.child_by_field_name("arguments") {
                    let mut walker = arguments_node.walk();
                    let mut arg_iter = arguments_node.named_children(&mut walker);

                    if let Some(first_arg) = arg_iter.next() {
                        if is_position_in_node(position, first_arg) {
                            let key = first_arg
                                .utf8_text(content.as_bytes())
                                .map_err(|_| Error::invalid_params("Failed to parse key text"))?;
                            let range = super::utils::node_to_range(first_arg);
                            let trimmed_key =
                                key.trim_matches(|c| c == '\'' || c == '"').to_string();
                            return Ok(Some((trimmed_key, range)));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    pub(crate) fn extract_variables_and_options(
        &self,
        value: &Value,
    ) -> (Vec<String>, std::collections::HashMap<String, Vec<String>>) {
        if let Value::String(s) = value {
            let parser = parse::Parser::new(s);
            if let Ok(ast) = parser.parse() {
                collect_variables_and_options(&ast)
            } else {
                (Vec::new(), std::collections::HashMap::new())
            }
        } else {
            (Vec::new(), std::collections::HashMap::new())
        }
    }
}

fn collect_variables_and_options(
    node: &AstNode,
) -> (Vec<String>, std::collections::HashMap<String, Vec<String>>) {
    let mut variables = Vec::new();
    let mut select_options = std::collections::HashMap::new();

    match node {
        AstNode::Root(children) => {
            for child in children {
                let (vars, opts) = collect_variables_and_options(child);
                variables.extend(vars);
                for (k, v) in opts {
                    select_options.entry(k).or_insert_with(Vec::new).extend(v);
                }
            }
        }
        AstNode::Variable(var) => {
            variables.push(var.clone());
        }
        AstNode::Plural { variable, options } | AstNode::Select { variable, options } => {
            variables.push(variable.clone());
            select_options.insert(variable.clone(), options.keys().cloned().collect());
            options.iter().for_each(|(_, option_nodes)| {
                for option_node in option_nodes {
                    let (vars, opts) = collect_variables_and_options(option_node);
                    variables.extend(vars);
                    for (k, v) in opts {
                        select_options.entry(k).or_insert_with(Vec::new).extend(v);
                    }
                }
            });
        }
        AstNode::HtmlTag { children, .. } => {
            for child in children {
                let (vars, opts) = collect_variables_and_options(child);
                variables.extend(vars);
                for (k, v) in opts {
                    select_options.entry(k).or_insert_with(Vec::new).extend(v);
                }
            }
        }
        _ => {}
    }

    variables.dedup();
    (variables, select_options)
}
