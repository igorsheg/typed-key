use ropey::Rope;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::DidChangeTextDocumentParams;
use tower_lsp::lsp_types::DidOpenTextDocumentParams;
use tower_lsp::lsp_types::DidSaveTextDocumentParams;
use tower_lsp::lsp_types::TextDocumentIdentifier;
use walkdir::WalkDir;

use super::channels::lsp::LspMessage;
use super::config::BackendConfig;

pub struct TypedKeyTranslations {
    translation_keys: HashMap<String, Value>,
    pub config: BackendConfig,
    main_channel: Option<std::sync::mpsc::Sender<LspMessage>>,
    pub documents: HashMap<String, Rope>,
    pub is_vscode: bool,
}

impl Clone for TypedKeyTranslations {
    fn clone(&self) -> Self {
        Self {
            translation_keys: self.translation_keys.clone(),
            config: self.config.clone(),
            main_channel: self.main_channel.clone(),
            documents: HashMap::new(),
            is_vscode: self.is_vscode,
        }
    }
}

impl TypedKeyTranslations {
    pub fn default() -> Self {
        Self {
            translation_keys: HashMap::new(),
            config: BackendConfig::default(),
            main_channel: None,
            documents: HashMap::new(),
            is_vscode: false,
        }
    }

    pub fn load_translations(&mut self) -> io::Result<()> {
        let translation_files: Vec<PathBuf> = WalkDir::new(&self.config.translations_dir)
            .into_iter()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .map_or(false, |ext| ext.eq_ignore_ascii_case("json"))
            })
            .map(|entry| entry.into_path())
            .collect();

        if translation_files.is_empty() {
            return Ok(());
        }

        self.translation_keys.clear(); // Clear existing keys before inserting new ones

        for file_path in translation_files {
            match process_file(&file_path) {
                Ok(keys) => {
                    for (key, value) in keys {
                        self.translation_keys.insert(key, value);
                    }
                }
                Err(e) => {
                    eprintln!("Error processing file {:?}: {}", file_path, e);
                }
            }
        }

        Ok(())
    }

    pub fn get_translation_keys(&self) -> &HashMap<String, Value> {
        &self.translation_keys
    }

    pub fn did_open(&mut self, params: DidOpenTextDocumentParams) {
        let name = params.text_document.uri.as_str();
        let file_content = params.text_document.text;
        let rope = Rope::from_str(&file_content);
        self.documents.insert(name.to_string(), rope);
    }

    pub fn did_change(&mut self, params: DidChangeTextDocumentParams) -> Option<()> {
        let uri = params.text_document.uri.to_string();
        let rope = self.documents.get_mut(&uri)?;

        for change in params.content_changes {
            match change.range {
                Some(range) => {
                    let start_char = rope.line_to_char(range.start.line as usize)
                        + range.start.character as usize;
                    let end_char =
                        rope.line_to_char(range.end.line as usize) + range.end.character as usize;

                    rope.remove(start_char..end_char);

                    rope.insert(start_char, &change.text);
                }
                None => {
                    *rope = Rope::from_str(&change.text);
                }
            }
        }
        Some(())
    }
}

fn process_file(path: &Path) -> io::Result<Vec<(String, Value)>> {
    let content = fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&content).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Error parsing JSON in file {:?}: {}", path, e),
        )
    })?;
    let keys = extract_keys(&json, String::new());
    Ok(keys)
}

fn extract_keys(value: &Value, prefix: String) -> Vec<(String, Value)> {
    match value {
        Value::Object(map) => map
            .iter()
            .flat_map(|(key, val)| {
                let new_prefix = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", prefix, key)
                };
                extract_keys(val, new_prefix)
            })
            .collect(),
        Value::Array(arr) => arr
            .iter()
            .enumerate()
            .flat_map(|(index, val)| {
                let new_prefix = format!("{}[{}]", prefix, index);
                extract_keys(val, new_prefix)
            })
            .collect(),
        _ => vec![(prefix, value.clone())],
    }
}
