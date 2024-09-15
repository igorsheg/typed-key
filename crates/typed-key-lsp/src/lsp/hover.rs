use super::docs::TypedKeyDocs;
use super::position::CursorPosition;
use super::typedkey_lsp::TypedKeyLspImpl;
use super::utils::traverse_nodes;
use crate::lsp::position::TFunctionAnalyzer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

impl TypedKeyLspImpl {
    pub(crate) async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;

        let document_content = self
            .document_map
            .get(&uri)
            .map(|content| content.clone())
            .unwrap_or_default();

        let analyzer = TFunctionAnalyzer::new(&document_content, position)?;
        let cursor_position = analyzer.analyze();

        match cursor_position {
            CursorPosition::InFirstParam(key)
            | CursorPosition::InSecondParam {
                translation_key: key,
                ..
            } => self.provide_t_function_documentation(&key, &document_content, position),
            _ => Ok(None),
        }
    }

    fn provide_t_function_documentation(
        &self,
        key: &str,
        document_content: &str,
        position: Position,
    ) -> Result<Option<Hover>> {
        if let Some(entry) = self.translation_keys.get(key) {
            let value = entry.value();
            let (variables, select_options) = self.extract_variables_and_options(value);
            let documentation =
                self.format_hover_documentation(key, value, &variables, &select_options);

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
        &self,
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
}
