use super::typedkey_lsp::TypedKeyLspImpl;
use rayon::prelude::*;
use serde_json::Value;
use std::fs;
use std::path::Path;
use tower_lsp::lsp_types::MessageType;
use walkdir::WalkDir;

impl TypedKeyLspImpl {
    pub(crate) async fn load_translations(&self) -> std::io::Result<()> {
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Loading translations from debug {} ",
                    self.config.translations_dir
                ),
            )
            .await;

        let translation_files: Vec<_> = WalkDir::new(&self.config.translations_dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
            .collect();

        translation_files.par_iter().for_each(|entry| {
            if let Ok(keys) = process_file(entry.path()) {
                for (key, value) in keys {
                    self.translation_keys.insert(key, value);
                }
            }
        });

        self.client
            .log_message(
                MessageType::INFO,
                format!("Loaded {} translation keys", self.translation_keys.len()),
            )
            .await;

        Ok(())
    }
}

fn process_file(path: &Path) -> std::io::Result<Vec<(String, Value)>> {
    let content = fs::read_to_string(path)?;
    let json: Value = serde_json::from_str(&content)?;
    let mut keys = Vec::new();
    extract_keys(&json, String::new(), &mut keys);
    Ok(keys)
}

fn extract_keys(value: &Value, prefix: String, keys: &mut Vec<(String, Value)>) {
    match value {
        Value::Object(map) => {
            for (key, val) in map {
                let new_prefix = if prefix.is_empty() {
                    key.to_string()
                } else {
                    format!("{}.{}", prefix, key)
                };
                extract_keys(val, new_prefix, keys);
            }
        }
        Value::Array(arr) => {
            for (index, val) in arr.iter().enumerate() {
                let new_prefix = format!("{}[{}]", prefix, index);
                extract_keys(val, new_prefix, keys);
            }
        }
        _ => {
            keys.push((prefix, value.clone()));
        }
    }
}

