use std::path::Path;

use tokio::sync::{mpsc, oneshot};
use tower_lsp::{
    lsp_types::{
        CodeActionParams, CodeActionProviderCapability, CodeActionResponse, CompletionOptions,
        CompletionParams, CompletionResponse, DidChangeConfigurationParams,
        DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        DidSaveTextDocumentParams, ExecuteCommandOptions, Hover, HoverParams,
        HoverProviderCapability, InitializeParams, InitializeResult, InitializedParams,
        MessageType, OneOf, ServerCapabilities, ServerInfo, TextDocumentSyncCapability,
        TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions,
    },
    Client,
};

use crate::lsp::{config::BackendConfig, fs::TypedKeyTranslations};

#[derive(Debug)]
pub enum LspMessage {
    Initialize(Box<InitializeParams>, oneshot::Sender<InitializeResult>),
    Initialized(oneshot::Sender<bool>),
    Shutdown(oneshot::Sender<()>),
    DidOpen(DidOpenTextDocumentParams),
    DidChange(DidChangeTextDocumentParams),
    DidSave(DidSaveTextDocumentParams),
    DidClose(DidCloseTextDocumentParams),
    Completion(
        CompletionParams,
        oneshot::Sender<Option<CompletionResponse>>,
    ),
    Hover(HoverParams, oneshot::Sender<Option<Hover>>),
    DidChangeConfiguration(Box<DidChangeConfigurationParams>),
    CodeAction(
        CodeActionParams,
        oneshot::Sender<Option<CodeActionResponse>>,
    ),
}

pub fn lsp_task(
    client: Client,
    lsp_channel: mpsc::Sender<LspMessage>,
    mut lsp_recv: mpsc::Receiver<LspMessage>,
) {
    let mut config = BackendConfig::default();
    let mut lsp_data = TypedKeyTranslations::default();
    tokio::spawn(async move {
        while let Some(msg) = lsp_recv.recv().await {
            match msg {
                LspMessage::Initialize(params, sender) => {
                    if let Some(client_info) = params.client_info {
                        if client_info.name == "Visual Studio Code" {
                            lsp_data.is_vscode = true;
                        }
                    }
                    params
                        .initialization_options
                        .map(serde_json::from_value)
                        .map(|res| res.ok())
                        .and_then(|c| -> Option<()> {
                            config = c?;
                            None
                        });

                    let definition_provider = Some(OneOf::Left(true));
                    let references_provider = None;
                    // let code_action_provider = Some(CodeActionProviderCapability::Simple(true));
                    let hover_provider = Some(HoverProviderCapability::Simple(true));
                    let execute_command_provider = Some(ExecuteCommandOptions {
                        commands: vec!["reset_variables".to_string(), "warn".to_string()],
                        ..Default::default()
                    });
                    // let document_symbol_provider = Some(OneOf::Left(true));

                    let msg = InitializeResult {
                        capabilities: ServerCapabilities {
                            text_document_sync: Some(TextDocumentSyncCapability::Options(
                                TextDocumentSyncOptions {
                                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                                    will_save: Some(true),
                                    save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                                    ..Default::default()
                                },
                            )),
                            completion_provider: Some(CompletionOptions {
                                resolve_provider: Some(false),
                                trigger_characters: Some(vec![
                                    "-".to_string(),
                                    "\"".to_string(),
                                    " ".to_string(),
                                    "%".to_string(),
                                    "{".to_string(),
                                ]),
                                all_commit_characters: None,
                                work_done_progress_options: Default::default(),
                                completion_item: None,
                            }),
                            definition_provider,
                            references_provider,
                            // code_action_provider,
                            execute_command_provider,
                            // document_symbol_provider,
                            // hover_provider,
                            ..ServerCapabilities::default()
                        },
                        server_info: Some(ServerInfo {
                            name: String::from("typedkey-lsp"),
                            version: Some(String::from("0.1.80")),
                        }),
                    };
                    let _ = sender.send(msg);
                }
                LspMessage::Initialized(sender) => {
                    client.log_message(MessageType::INFO, "Initialized").await;

                    client
                        .log_message(
                            MessageType::INFO,
                            format!("CONFIG 2! {:?} ", config.clone()),
                        )
                        .await;
                    lsp_data.config = config.clone();

                    match lsp_data.load_translations() {
                        Ok(_) => {
                            client
                                .log_message(
                                    MessageType::INFO,
                                    format!(
                                        "Loaded {} translation keys",
                                        lsp_data.get_translation_keys().len()
                                    ),
                                )
                                .await;
                        }
                        Err(e) => {
                            client
                                .log_message(
                                    MessageType::ERROR,
                                    format!("Failed to load translations: {}", e),
                                )
                                .await;
                        }
                    }

                    let _ = sender.send(true);
                }
                LspMessage::DidChangeConfiguration(params) => {
                    let (sender, _) = oneshot::channel();

                    client
                        .log_message(
                            MessageType::INFO,
                            format!("CONFIG as params! {:?} ", params.settings),
                        )
                        .await;

                    match serde_json::from_value(params.settings) {
                        Ok(c) => {
                            client
                                .log_message(MessageType::INFO, format!("CONFIG! {:?} ", c))
                                .await;
                            config = c;
                            let _ = lsp_channel.send(LspMessage::Initialized(sender)).await;
                        }
                        Err(err) => {
                            client
                                .log_message(
                                    MessageType::ERROR,
                                    format!("Failed at der! {:?} ", err),
                                )
                                .await;
                        }
                    }
                    // if let Ok(c) = serde_json::from_value(params.settings) {}
                }
                _ => {}
            }
        }
    });
}
