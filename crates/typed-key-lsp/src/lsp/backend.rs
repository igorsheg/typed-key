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

impl TypedKeyLsp {
    async fn handle_document_change(&self, uri: Url) {
        let inner = self.0.read().await;
        inner.publish_diagnostics(uri).await;
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
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
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
        let uri = params.text_document.uri.clone();
        {
            let inner = self.0.read().await;
            inner.did_open(params).await;
        }
        self.handle_document_change(uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        {
            let inner = self.0.read().await;
            inner.did_change(params).await;
        }
        self.handle_document_change(uri).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.handle_document_change(uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        {
            let inner = self.0.read().await;
            inner.did_close(params).await;
        }
        self.0
            .read()
            .await
            .client
            .publish_diagnostics(uri, vec![], None)
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        self.0.read().await.handle_completion(params).await
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        self.0.read().await.hover(params).await
    }
    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        self.0.write().await.did_change_configuration(params).await;
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let inner = self.0.read().await;
        inner.handle_code_action(params).await
    }
}
