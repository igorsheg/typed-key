use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{Receiver, Sender};
use tower_lsp::{
    lsp_types::{Diagnostic, DiagnosticSeverity, MessageType, Position, Range, Url},
    Client,
};
use tree_sitter::Node;

use crate::lsp::{ast::extract_variables_and_options, utils::traverse_nodes};

#[derive(Debug)]
pub enum DiagnosticMessage {
    PublishDiagnostics(Url, Vec<Diagnostic>),
    Str(String),
}

#[derive(Serialize, Deserialize)]
pub struct MissingVariableDiagnosticData {
    pub key: String,
    pub missing_variable: String,
}

pub fn diagnostics_task(client: Client, mut receiver: Receiver<DiagnosticMessage>) {
    tokio::spawn(async move {
        while let Some(msg) = receiver.recv().await {
            match msg {
                DiagnosticMessage::Str(msg) => {
                    client.log_message(MessageType::INFO, msg).await;
                }
                DiagnosticMessage::PublishDiagnostics(uri, diagnostics) => {
                    client.publish_diagnostics(uri, diagnostics, None).await;
                }
            }
        }
    });
}

pub fn generate_diagnostics(
    content: &str,
    translation_keys: &HashMap<String, serde_json::Value>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::language_typescript())
        .expect("Failed to load TypeScript grammar");

    let tree = match parser.parse(content, None) {
        Some(tree) => tree,
        None => return diagnostics,
    };

    let root_node = tree.root_node();

    for node in traverse_nodes(root_node) {
        if node.kind() == "call_expression" {
            if let Some(func_node) = node.child_by_field_name("function") {
                let func_name = func_node.utf8_text(content.as_bytes()).unwrap_or("");
                if func_name == "t" {
                    if let Some(diagnostics_for_node) =
                        check_t_function_call(content, node, translation_keys)
                    {
                        diagnostics.extend(diagnostics_for_node);
                    }
                }
            }
        }
    }

    diagnostics
}

fn check_t_function_call<'a>(
    content: &str,
    node: Node<'a>,
    translation_keys: &HashMap<String, serde_json::Value>,
) -> Option<Vec<Diagnostic>> {
    let mut diagnostics = Vec::new();

    if let Some(arguments_node) = node.child_by_field_name("arguments") {
        let mut walker = arguments_node.walk();
        let arg_nodes: Vec<_> = arguments_node.named_children(&mut walker).collect();

        if !arg_nodes.is_empty() {
            let key_node = &arg_nodes[0];
            let key = key_node.utf8_text(content.as_bytes()).ok()?;
            let key = key.trim_matches(|c| c == '\'' || c == '"');

            if let Some(translation_value) = translation_keys.get(key) {
                let (required_vars, _) = extract_variables_and_options(translation_value);
                let provided_vars = extract_provided_variables(content, arg_nodes.get(1));

                for var in required_vars.iter() {
                    if !provided_vars.contains(var) {
                        let range = node_to_range(*key_node);
                        let diagnostic_data = MissingVariableDiagnosticData {
                            key: key.to_string(),
                            missing_variable: var.to_string(),
                        };
                        diagnostics.push(Diagnostic {
                            range,
                            severity: Some(DiagnosticSeverity::WARNING),
                            code: None,
                            code_description: None,
                            source: Some("typedkey".to_string()),
                            message: format!("Missing required variable: {} for key: {}", var, key),
                            related_information: None,
                            tags: None,
                            data: Some(serde_json::to_value(diagnostic_data).ok()?),
                        });
                    }
                }
            }
        }
    }

    Some(diagnostics)
}

fn extract_provided_variables(content: &str, options_node: Option<&Node<'_>>) -> Vec<String> {
    let mut provided_vars = Vec::new();

    if let Some(node) = options_node {
        if node.kind() == "object" {
            for child in node.named_children(&mut node.walk()) {
                match child.kind() {
                    "pair" => {
                        if let Some(key_node) = child.child_by_field_name("key") {
                            if let Ok(var) = key_node.utf8_text(content.as_bytes()) {
                                provided_vars.push(var.to_string());
                            }
                        }
                    }
                    "shorthand_property_identifier" => {
                        if let Ok(var) = child.utf8_text(content.as_bytes()) {
                            provided_vars.push(var.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    provided_vars
}

fn node_to_range(node: Node) -> Range {
    Range {
        start: Position {
            line: node.start_position().row as u32,
            character: node.start_position().column as u32,
        },
        end: Position {
            line: node.end_position().row as u32,
            character: node.end_position().column as u32,
        },
    }
}
