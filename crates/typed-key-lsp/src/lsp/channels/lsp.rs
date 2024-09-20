use tokio::sync::{mpsc, oneshot};
use tower_lsp::{
    lsp_types::{
        CodeActionParams, CodeActionProviderCapability, CodeActionResponse, CompletionOptions,
        CompletionParams, CompletionResponse, DidChangeConfigurationParams,
        DidChangeTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
        ExecuteCommandOptions, Hover, HoverParams, HoverProviderCapability, InitializeParams,
        InitializeResult, MessageType, OneOf, ServerCapabilities, ServerInfo,
        TextDocumentSyncCapability, TextDocumentSyncKind, WorkspaceFoldersServerCapabilities,
        WorkspaceServerCapabilities,
    },
    Client,
};

use crate::lsp::{
    action::handle_code_action,
    completion::handle_completion,
    config::BackendConfig,
    fs::{find_workspace_package, TypedKeyTranslations},
    hover::hover,
};

use super::diagnostics::{generate_diagnostics, DiagnosticMessage};

#[derive(Debug)]
pub enum LspMessage {
    Initialize(Box<InitializeParams>, oneshot::Sender<InitializeResult>),
    Initialized(oneshot::Sender<bool>),
    DidOpen(DidOpenTextDocumentParams),
    DidChange(DidChangeTextDocumentParams),
    DidSave(DidSaveTextDocumentParams),
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
    diagnostics_channel: mpsc::Sender<DiagnosticMessage>,
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
                            workspace: Some(WorkspaceServerCapabilities {
                                workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                                    supported: Some(true),
                                    change_notifications: Some(OneOf::Left(true)),
                                }),
                                file_operations: None,
                            }),
                            code_action_provider,
                            execute_command_provider,
                            hover_provider,
                            ..ServerCapabilities::default()
                        },
                        server_info: Some(ServerInfo {
                            name: String::from("typedkey-lsp"),
                            version: Some(String::from("0.1.80")),
                        }),
                        offset_encoding: None,
                    };
                    let _ = sender.send(msg);
                }
                LspMessage::Initialized(sender) => {
                    client
                        .log_message(
                            MessageType::INFO,
                            format!(
                                "Initialized {:?}",
                                lsp_data.config.translations_dir.as_path().to_str()
                            ),
                        )
                        .await;
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
                LspMessage::DidSave(params) => {
                    let uri = params.text_document.uri;
                    if let Some(rope) = lsp_data.documents.get(uri.as_str()) {
                        let diagnostics =
                            generate_diagnostics(&rope, lsp_data.get_translation_keys());
                        let _ = diagnostics_channel
                            .send(DiagnosticMessage::Errors(uri, diagnostics))
                            .await;
                    }
                }
                LspMessage::DidOpen(params) => {
                    let (sender, _) = oneshot::channel();
                    if let Some(package) = find_workspace_package(&params.text_document.uri) {
                        lsp_data.config.translations_dir = package.join(&config.translations_dir);
                        let _ = lsp_channel.send(LspMessage::Initialized(sender)).await;
                    }

                    let _ = lsp_data.did_open(params);
                }
                LspMessage::Completion(params, sender) => {
                    let mut completion_items = None;
                    let uri = params.text_document_position.text_document.uri.clone();
                    if let Some(rope) = lsp_data.documents.get(uri.as_str()) {
                        if let Ok(completion) =
                            handle_completion(params, &rope, lsp_data.get_translation_keys()).await
                        {
                            client
                                .log_message(
                                    MessageType::INFO,
                                    format!("Completion {:?}", completion_items),
                                )
                                .await;
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
            }
        }
    });
}
