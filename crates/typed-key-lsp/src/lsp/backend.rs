use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use super::config::BackendConfig;
use super::typedkey_lsp::TypedKeyLspImpl;

pub struct TypedKeyLsp(Arc<RwLock<TypedKeyLspImpl>>);

impl TypedKeyLsp {
    pub fn new(client: Client) -> Self {
        Self(Arc::new(RwLock::new(TypedKeyLspImpl {
            client,
            config: BackendConfig::default(),
            document_map: DashMap::new(),
            translation_keys: DashMap::new(),
        })))
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for TypedKeyLsp {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![
                        "t(".to_string(),
                        "\"".to_string(),
                        "'".to_string(),
                        "`".to_string(),
                        "{".to_string(),
                        "}".to_string(),
                        ",".to_string(),
                        ":".to_string(),
                        ".".to_string(),
                    ]),
                    all_commit_characters: Some(vec![
                        "'".to_string(),
                        "\"".to_string(),
                        "`".to_string(),
                        ")".to_string(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, params: InitializedParams) {
        self.0.write().await.initialized(params).await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.0.write().await.did_open(params).await
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.0.write().await.did_change(params).await
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.0.write().await.did_close(params).await
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        self.0.write().await.handle_completion(params).await
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        self.0.write().await.hover(params).await
    }
    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        self.0.write().await.did_change_configuration(params).await
    }
}
