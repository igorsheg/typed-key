use dashmap::DashMap;
use serde_json::Value;
use tower_lsp::{
    lsp_types::{
        ConfigurationItem, DidChangeConfigurationParams, DidChangeTextDocumentParams,
        DidCloseTextDocumentParams, DidOpenTextDocumentParams, Hover, HoverContents, HoverParams,
        InitializedParams, MarkupContent, MarkupKind, MessageType, Url,
    },
    Client,
};

use super::{config::BackendConfig, docs::TypedKeyDocs, utils::position_to_index};

pub struct TypedKeyLspImpl {
    pub client: Client,
    pub translation_keys: DashMap<String, Value>,
    pub config: BackendConfig,
    pub document_map: DashMap<Url, String>,
}

impl TypedKeyLspImpl {
    pub async fn initialized(&mut self, _: InitializedParams) {
        let config_items = self
            .client
            .configuration(vec![ConfigurationItem {
                scope_uri: None,
                section: Some("typedkey".to_string()),
            }])
            .await;

        let mut updated_config = false;

        if let Ok(config_items) = config_items {
            if let Some(config) = config_items.into_iter().next() {
                if let Ok(config) = serde_json::from_value(config) {
                    self.config = config;
                    updated_config = true;
                }
            }
        }

        if !updated_config {
            self.client
                .log_message(
                    MessageType::ERROR,
                    "Failed to retrieve configuration from client.",
                )
                .await;
        }

        self.client
            .log_message(
                MessageType::INFO,
                format!("TypedKey Language Server v{}", env!("CARGO_PKG_VERSION")),
            )
            .await;

        let _ = self.load_translations().await;
    }

    pub(crate) async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.document_map
            .insert(params.text_document.uri.clone(), params.text_document.text);
    }

    pub(crate) async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(mut content) = self.document_map.get_mut(&params.text_document.uri) {
            for change in params.content_changes {
                if let Some(range) = change.range {
                    let start_pos = position_to_index(&content, range.start);
                    let end_pos = position_to_index(&content, range.end);
                    content.replace_range(start_pos..end_pos, &change.text);
                } else {
                    *content = change.text;
                }
            }
        }
    }

    pub(crate) async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.document_map.remove(&params.text_document.uri);
    }

    pub(crate) async fn hover(
        &self,
        params: HoverParams,
    ) -> tower_lsp::jsonrpc::Result<Option<Hover>> {
        let position = params.text_document_position_params.position;
        let uri = params.text_document_position_params.text_document.uri;

        let document_content = self
            .document_map
            .get(&uri)
            .map(|content| content.clone())
            .unwrap_or_default();

        if let Some((key, range)) = self
            .get_translation_key_at_position(&document_content, position)
            .await?
        {
            if let Some(value) = self.translation_keys.get(&key) {
                let (variables, select_options) = self.extract_variables_and_options(&value);
                let typed_key_docs = TypedKeyDocs::new();
                let documentation = typed_key_docs.format_documentation(
                    &key,
                    &value,
                    &variables,
                    &select_options
                        .iter()
                        .flat_map(|(_, v)| v)
                        .cloned()
                        .collect::<Vec<_>>(),
                );

                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: documentation,
                    }),
                    range: Some(range),
                }));
            }
        }

        Ok(None)
    }

    pub(crate) async fn did_change_configuration(&mut self, params: DidChangeConfigurationParams) {
        if let Some(settings) = params.settings.as_object() {
            if let Some(typedkey_settings) = settings.get("typedkey") {
                if let Ok(new_config) =
                    serde_json::from_value::<BackendConfig>(typedkey_settings.clone())
                {
                    self.config = new_config;
                    self.client
                        .log_message(MessageType::INFO, "Configuration updated")
                        .await;

                    if let Err(e) = self.load_translations().await {
                        self.client
                            .log_message(
                                MessageType::ERROR,
                                format!("Failed to reload translations: {}", e),
                            )
                            .await;
                    }
                } else {
                    self.client
                        .log_message(MessageType::ERROR, "Failed to parse new configuration")
                        .await;
                }
            }
        }
    }
}
