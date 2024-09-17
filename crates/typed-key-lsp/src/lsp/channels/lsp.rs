use tokio::sync::{mpsc, oneshot};
use tower_lsp::{
    lsp_types::{
        CodeActionParams, CodeActionProviderCapability, CodeActionResponse, CompletionOptions,
        CompletionParams, CompletionResponse, DidChangeConfigurationParams,
        DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        DidSaveTextDocumentParams, ExecuteCommandOptions, Hover, HoverParams,
        HoverProviderCapability, InitializeParams, InitializeResult, MessageType, OneOf,
        ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    },
    Client,
};

use crate::lsp::{
    action::handle_code_action, completion::handle_completion, config::BackendConfig,
    fs::TypedKeyTranslations, hover::hover,
};

#[derive(Debug)]
pub enum LspMessage {
    Initialize(Box<InitializeParams>, oneshot::Sender<InitializeResult>),
    Initialized(oneshot::Sender<bool>),
    DidOpen(DidOpenTextDocumentParams),
    DidChange(DidChangeTextDocumentParams),
    DidSave(DidSaveTextDocumentParams),
    DidClose(DidCloseTextDocumentParams),
    Completion(
        CompletionParams,
        oneshot::Sender<Option<CompletionResponse>>,
    ),
    Hover(HoverParams, oneshot::Sender<Option<Hover>>),
    DidChangeConfiguration(DidChangeConfigurationParams),
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
                    let code_action_provider = Some(CodeActionProviderCapability::Simple(true));
                    let hover_provider = Some(HoverProviderCapability::Simple(true));
                    let execute_command_provider = Some(ExecuteCommandOptions {
                        commands: vec!["reset_variables".to_string(), "warn".to_string()],
                        ..Default::default()
                    });

                    let msg = InitializeResult {
                        capabilities: ServerCapabilities {
                            text_document_sync: Some(TextDocumentSyncCapability::Kind(
                                TextDocumentSyncKind::INCREMENTAL,
                            )),
                            completion_provider: Some(CompletionOptions {
                                resolve_provider: Some(false),
                                trigger_characters: Some(vec![
                                    "(".to_string(),
                                    ",".to_string(),
                                    "{".to_string(),
                                    "\"".to_string(),
                                    "'".to_string(),
                                    "`".to_string(),
                                    ":".to_string(),
                                ]),
                                all_commit_characters: None,
                                work_done_progress_options: Default::default(),
                                completion_item: None,
                            }),
                            definition_provider,
                            references_provider,
                            code_action_provider,
                            execute_command_provider,
                            hover_provider,
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
                    lsp_data.config = config.clone();
                    let _ = lsp_data.load_translations();
                    let _ = sender.send(true);
                }
                LspMessage::DidChangeConfiguration(params) => {
                    let (sender, _) = oneshot::channel();
                    if let Ok(c) = serde_json::from_value(params.settings) {
                        config = c;
                        let _ = lsp_channel.send(LspMessage::Initialized(sender)).await;
                    }
                }
                LspMessage::DidChange(params) => {
                    let _ = lsp_data.did_change(params);
                }
                LspMessage::DidOpen(params) => {
                    let _ = lsp_data.did_open(params);
                }
                LspMessage::Completion(params, sender) => {
                    let mut completion_items = None;
                    let uri = params.text_document_position.text_document.uri.clone();
                    if let Some(rope) = lsp_data.documents.get(uri.as_str()) {
                        if let Ok(completion) =
                            handle_completion(params, &rope, lsp_data.get_translation_keys()).await
                        {
                            completion_items = completion
                        }
                        let _ = sender.send(completion_items);
                    };
                }
                LspMessage::Hover(params, sender) => {
                    let mut completion_items = None;
                    let uri = params
                        .text_document_position_params
                        .text_document
                        .uri
                        .clone();
                    if let Some(rope) = lsp_data.documents.get(uri.as_str()) {
                        if let Ok(completion) =
                            hover(params, &rope, lsp_data.get_translation_keys()).await
                        {
                            completion_items = completion
                        }
                        let _ = sender.send(completion_items);
                    };
                }
                LspMessage::CodeAction(params, sender) => {
                    let mut completion_items = None;
                    let uri = params.text_document.uri.clone();
                    if let Some(rope) = lsp_data.documents.get(uri.as_str()) {
                        if let Ok(completion) = handle_code_action(params, &rope).await {
                            completion_items = completion
                        }
                        let _ = sender.send(completion_items);
                    };
                }
                _ => {}
            }
        }
    });
}
