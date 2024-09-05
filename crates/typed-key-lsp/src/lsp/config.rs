use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendConfig {
    pub translations_dir: String,
}

impl BackendConfig {
    pub fn default() -> Self {
        Self {
            translations_dir: String::from("src/assets/locales"),
        }
    }
}
