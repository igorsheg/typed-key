use std::sync::Arc;

use dashmap::DashMap;
use serde_json::Value;
use tokio::sync::RwLock;
use tower_lsp::{
    lsp_types::{
        ConfigurationItem, DidChangeConfigurationParams, DidChangeTextDocumentParams,
        DidCloseTextDocumentParams, InitializedParams, MessageType, Url,
    },
    Client,
};

use super::{config::BackendConfig, utils::position_to_index};

pub struct TypedKeyLspImpl {
    pub client: Client,
    pub translation_keys: DashMap<String, Value>,
    pub config: Arc<RwLock<BackendConfig>>,
    pub document_map: DashMap<Url, String>,
}

impl TypedKeyLspImpl {
    pub async fn initialized(&self, _: InitializedParams) {
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
                if let Ok(new_config) = serde_json::from_value(config) {
                    *self.config.write().await = new_config;
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

        // let _ = self.load_translations().await;
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

    pub(crate) async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        if let Some(settings) = params.settings.as_object() {
            if let Some(typedkey_settings) = settings.get("typedkey") {
                if let Ok(new_config) =
                    serde_json::from_value::<BackendConfig>(typedkey_settings.clone())
                {
                    *self.config.write().await = new_config;
                    self.client
                        .log_message(MessageType::INFO, "Configuration updated")
                        .await;
                    // Reload translations
                    // if let Err(e) = self.load_translations().await {
                    //     self.client
                    //         .log_message(
                    //             MessageType::ERROR,
                    //             format!("Failed to reload translations: {}", e),
                    //         )
                    //         .await;
                    // }
                } else {
                    self.client
                        .log_message(MessageType::ERROR, "Failed to parse new configuration")
                        .await;
                }
            }
        }
    }
}
