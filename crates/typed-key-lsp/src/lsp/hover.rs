use std::collections::HashMap;

use super::ast::extract_variables_and_options;
use super::docs::TypedKeyDocs;
use super::visitor::{TFunctionInfo, TFunctionVisitor};
use oxc::span::Span;
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

    let t_visitor = TFunctionVisitor::new(None);

    match t_visitor.analyze(&document_str, position) {
        TFunctionInfo::NotInFunction => Ok(None),
        TFunctionInfo::InFunction(context) => match (&context.first_param, &context.second_param) {
            (Some(first_param), _) => {
                provide_t_function_documentation(&first_param, context.span, translation_keys)
            }
            _ => Ok(None),
        },
    }
}

fn provide_t_function_documentation(
    key: &str,
    span: Option<Span>,
    translation_keys: &HashMap<String, serde_json::Value>,
) -> Result<Option<Hover>> {
    if let Some(value) = translation_keys.get(key) {
        let (variables, select_options) = extract_variables_and_options(value);
        let documentation = format_hover_documentation(key, value, &variables, &select_options);

        if let Some(span) = span {
            let range = Range {
                start: Position::new(span.start as u32, span.start),
                end: Position::new(span.end as u32, span.end as u32),
            };
            Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: documentation,
                }),
                range: Some(range),
            }))
        } else {
            Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: documentation,
                }),
                range: None,
            }))
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
