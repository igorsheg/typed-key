use std::collections::HashMap;

use oxc::{
    ast::{
        ast::{CallExpression, Expression},
        AstKind, Visit,
    },
    span::Span,
};
use ropey::Rope;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc::Receiver;
use tower_lsp::{
    lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range, Url},
    Client,
};

use crate::{lsp::utils::traverse_ast_for_variables, Parser};

#[derive(Debug)]
pub enum DiagnosticMessage {
    Errors(Url, Vec<Diagnostic>),
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
                DiagnosticMessage::Errors(uri, diagnostics) => {
                    client.publish_diagnostics(uri, diagnostics, None).await;
                }
            }
        }
    });
}

pub struct DiagnosticsVisitor<'a> {
    diagnostics: Vec<Diagnostic>,
    translation_keys: &'a HashMap<String, Value>,
    content: &'a Rope,
}

impl<'a> DiagnosticsVisitor<'a> {
    pub fn new(translation_keys: &'a HashMap<String, Value>, content: &'a Rope) -> Self {
        Self {
            diagnostics: Vec::new(),
            translation_keys,
            content,
        }
    }

    fn is_t_function_call(&self, call_expr: &CallExpression) -> bool {
        match &call_expr.callee {
            Expression::Identifier(ident) => ident.name == "t",
            Expression::StaticMemberExpression(static_member) => static_member.property.name == "t",
            _ => false,
        }
    }

    fn add_diagnostic(&mut self, key: &str, missing_var: &str, span: Span) {
        let range = self.span_to_range(span);
        self.diagnostics.push(Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::WARNING),
            code: None,
            code_description: None,
            source: Some("typedkey".to_string()),
            message: format!(
                "Missing required variable: {} for key: {}",
                missing_var, key
            ),
            related_information: None,
            tags: None,
            data: Some(
                serde_json::to_value(MissingVariableDiagnosticData {
                    key: key.to_string(),
                    missing_variable: missing_var.to_string(),
                })
                .expect("Failed to serialize diagnostic data"),
            ),
        });
    }

    fn span_to_range(&self, span: Span) -> Range {
        let start_position = self.offset_to_position(span.start as usize);
        let end_position = self.offset_to_position(span.end as usize);
        Range::new(start_position, end_position)
    }

    fn offset_to_position(&self, offset: usize) -> Position {
        let line_index = self.content.char_to_line(offset);
        let line_start = self.content.line_to_char(line_index);
        let column = offset - line_start;
        Position::new(line_index as u32, column as u32)
    }

    fn extract_provided_variables(
        &self,
        obj_expr: &oxc::ast::ast::ObjectExpression,
    ) -> Vec<String> {
        obj_expr
            .properties
            .iter()
            .filter_map(|prop| {
                if let oxc::ast::ast::ObjectPropertyKind::ObjectProperty(prop) = prop {
                    prop.key.static_name().map(|name| name.to_string())
                } else {
                    None
                }
            })
            .collect()
    }
}

impl<'a> Visit<'a> for DiagnosticsVisitor<'a> {
    fn enter_node(&mut self, kind: AstKind<'a>) {
        if let AstKind::CallExpression(call_expr) = kind {
            if self.is_t_function_call(call_expr) {
                if let Some(first_arg) = call_expr.arguments.first() {
                    if let Expression::StringLiteral(key_literal) = &first_arg.to_expression() {
                        let key = key_literal.value.to_string();
                        if let Some(translation_value) = self.translation_keys.get(&key) {
                            if let Some(translation_str) = translation_value.as_str() {
                                let parser = Parser::new(translation_str);
                                if let Ok(ast) = parser.parse() {
                                    let mut required_vars = Vec::new();
                                    traverse_ast_for_variables(&ast, &mut required_vars);

                                    let provided_vars =
                                        if let Some(second_arg) = call_expr.arguments.get(1) {
                                            if let Expression::ObjectExpression(obj_expr) =
                                                &second_arg.to_expression()
                                            {
                                                self.extract_provided_variables(obj_expr)
                                            } else {
                                                Vec::new()
                                            }
                                        } else {
                                            Vec::new()
                                        };

                                    for var in required_vars {
                                        if !provided_vars.contains(&var) {
                                            self.add_diagnostic(&key, &var, call_expr.span);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn generate_diagnostics(
    content: &Rope,
    translation_keys: &HashMap<String, Value>,
) -> Vec<Diagnostic> {
    let allocator = oxc::allocator::Allocator::default();
    let source_type = oxc::span::SourceType::default()
        .with_typescript(true)
        .with_module(true)
        .with_jsx(true);

    let document_str = content.to_string();

    let parse_result = oxc::parser::Parser::new(&allocator, &document_str, source_type).parse();
    let program = parse_result.program;

    let mut visitor = DiagnosticsVisitor::new(translation_keys, content);
    visitor.visit_program(&program);
    visitor.diagnostics
}
