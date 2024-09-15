use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::sync::{mpsc::Sender, oneshot};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};

use super::channels::lsp::{lsp_task, LspMessage};
use super::config::BackendConfig;
use super::typedkey_lsp::TypedKeyLspImpl;

pub struct Backend {
    main_channel: Sender<LspMessage>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let (sender, rx) = oneshot::channel();
        let _ = self
            .main_channel
            .send(LspMessage::Initialize(Box::new(params), sender))
            .await;
        if let Ok(msg) = rx.await {
            Ok(msg)
        } else {
            Ok(InitializeResult::default())
        }
    }

    async fn initialized(&self, _params: InitializedParams) {
        let (sender, _) = oneshot::channel();
        let _ = self
            .main_channel
            .send(LspMessage::Initialized(sender))
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        let _ = self
            .main_channel
            .send(LspMessage::DidChangeConfiguration(Box::new(params)))
            .await;
    }
}

impl Backend {
    pub fn _new(client: Client) -> Self {
        let (lsp_sender, lsp_recv) = mpsc::channel(50);
        lsp_task(client.clone(), lsp_sender.clone(), lsp_recv);
        Self {
            main_channel: lsp_sender,
        }
    }
}
