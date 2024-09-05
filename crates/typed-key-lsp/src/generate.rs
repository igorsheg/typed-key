use serde_json::Value;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use walkdir::WalkDir;

use crate::parse::{AstNode, Parser};

pub struct TypeScriptGenerator {
    translations: HashMap<String, String>,
}

impl TypeScriptGenerator {
    pub fn new() -> Self {
        TypeScriptGenerator {
            translations: HashMap::new(),
        }
    }

    pub fn process_directory(&mut self, dir_path: &str) -> std::io::Result<()> {
        for entry in WalkDir::new(dir_path).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                self.process_file(path)?;
            }
        }
        Ok(())
    }

    fn process_file(&mut self, file_path: &Path) -> std::io::Result<()> {
        let content = fs::read_to_string(file_path)?;
        let json: Value = serde_json::from_str(&content)?;
        self.extract_translations(&json, String::new());
        Ok(())
    }

    fn extract_translations(&mut self, value: &Value, prefix: String) {
        match value {
            Value::Object(map) => {
                for (k, v) in map {
                    let new_key = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", prefix, k)
                    };
                    self.extract_translations(v, new_key);
                }
            }
            Value::String(s) => {
                self.translations.insert(prefix, s.clone());
            }
            _ => {}
        }
    }

    pub fn generate_typescript_definitions(&self, output_path: &str) -> std::io::Result<()> {
        let mut file = File::create(output_path)?;
        writeln!(file, "export type Translations = {{")?;

        for (key, value) in &self.translations {
            let parser = Parser::new(value);
            if let Ok(ast) = parser.parse() {
                let params = extract_params(&ast);
                let param_string = self.format_params(&params);
                writeln!(file, "  \"{}\": (params: {}) => string,", key, param_string)?;
            }
        }

        writeln!(file, "}}")
    }

    fn format_params(&self, params: &[(String, String)]) -> String {
        let param_strings: Vec<String> = params
            .iter()
            .map(|(name, typ)| format!("{}: {}", name, typ))
            .collect();

        if param_strings.is_empty() {
            "{}".to_string()
        } else {
            format!("{{ {} }}", param_strings.join(", "))
        }
    }
}

impl Default for TypeScriptGenerator {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_params(node: &AstNode) -> Vec<(String, String)> {
    let mut params = Vec::new();
    match node {
        AstNode::Root(children) => {
            for child in children {
                params.extend(extract_params(child));
            }
        }
        AstNode::Variable(var) => {
            params.push((var.clone(), "string".to_string()));
        }
        AstNode::Plural { variable, .. } => {
            params.push((variable.clone(), "number".to_string()));
        }
        AstNode::Select { variable, options } => {
            let option_types = options
                .keys()
                .map(|s| format!("\"{}\"", s))
                .collect::<Vec<_>>()
                .join(" | ");
            params.push((variable.clone(), option_types));
        }
        AstNode::HtmlTag { children, .. } => {
            for child in children {
                params.extend(extract_params(child));
            }
        }
        _ => {}
    }
    params
}
