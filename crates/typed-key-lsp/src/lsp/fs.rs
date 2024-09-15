use super::typedkey_lsp::TypedKeyLspImpl;
use futures::future::join_all;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;
use tower_lsp::lsp_types::MessageType;
use walkdir::WalkDir;

impl TypedKeyLspImpl {
    pub(crate) async fn load_translations(&self) -> std::io::Result<()> {
        let config = self.config.read().await;
        let translations_dir = &config.translations_dir;

        self.client
            .log_message(
                MessageType::INFO,
                format!("Loading translations from {}", translations_dir),
            )
            .await;

        // Collect all JSON translation files
        let translation_files: Vec<PathBuf> = WalkDir::new(translations_dir)
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

        self.client
            .log_message(
                MessageType::INFO,
                format!("Found {} translation files", translation_files.len()),
            )
            .await;

        // Shared collection for all keys
        let all_keys = Arc::new(Mutex::new(Vec::new()));

        // Process files asynchronously
        let tasks: Vec<_> = translation_files
            .into_iter()
            .map(|path| {
                let all_keys = all_keys.clone();
                async move {
                    match process_file_async(&path).await {
                        Ok(keys) => {
                            let mut all_keys = all_keys.lock().await;
                            all_keys.extend(keys);
                        }
                        Err(e) => {
                            eprintln!("Error processing file {:?}: {}", path, e);
                        }
                    }
                }
            })
            .collect();

        // Await all tasks concurrently
        join_all(tasks).await;

        // Insert all keys into translation_keys
        let all_keys = Arc::try_unwrap(all_keys)
            .unwrap_or_else(|_| panic!("Arc has more than one strong reference"))
            .into_inner();

        for (key, value) in all_keys {
            self.translation_keys.insert(key, value);
        }

        self.client
            .log_message(
                MessageType::INFO,
                format!("Loaded {} translation keys", self.translation_keys.len()),
            )
            .await;

        Ok(())
    }
}

async fn process_file_async(path: &Path) -> std::io::Result<Vec<(String, Value)>> {
    let content = fs::read_to_string(path).await?;
    let json: Value = serde_json::from_str(&content).map_err(|e| {
        eprintln!("Error parsing JSON in file {:?}: {}", path, e);
        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
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

