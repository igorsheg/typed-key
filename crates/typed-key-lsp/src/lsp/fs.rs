use std::fs;

use serde_json::Value;
use tower_lsp::lsp_types::MessageType;
use walkdir::WalkDir;

use super::typedkey_lsp::TypedKeyLspImpl;

impl TypedKeyLspImpl {
    pub(crate) async fn load_translations(&self) -> std::io::Result<()> {
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Loading translations from {} ",
                    self.config.translations_dir
                ),
            )
            .await;

        for entry in WalkDir::new(&self.config.translations_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
        {
            let path = entry.path();
            let content = fs::read_to_string(path)?;
            let json: Value = serde_json::from_str(&content)?;

            Self::extract_keys(&json, String::new(), &self.translation_keys);
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
