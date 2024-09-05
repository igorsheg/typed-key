use serde_json::Value;

use crate::parse::{self, AstNode};

pub struct TypedKeyDocs {}

impl TypedKeyDocs {
    pub fn new() -> Self {
        Self {}
    }

    pub fn format_documentation(
        &self,
        key: &str,
        value: &Value,
        variables: &[String],
        select_options: &[String],
    ) -> String {
        let mut doc = String::new();

        // Parse the translation value to determine variable types
        let (plural_vars, select_vars, simple_vars) = self.categorize_variables(value, variables);

        // Function signature
        doc.push_str("```typescript\n");
        doc.push_str(&format!("t(key: '{}', params?: {{", key));

        // Add variables with their types
        for var in &plural_vars {
            doc.push_str(&format!("\n  {}: number;", var));
        }
        for var in &select_vars {
            let options = select_options.join(" | ");
            doc.push_str(&format!("\n  {}: '{}';", var, options));
        }
        for var in &simple_vars {
            doc.push_str(&format!("\n  {}: string | number;", var));
        }

        doc.push_str("\n}): string\n```\n\n");

        // Translation string
        doc.push_str("**Translation:**\n");
        doc.push_str("```i18n\n");
        doc.push_str(value.as_str().unwrap_or_default());
        doc.push_str("\n```\n\n");

        // Parameters
        if !variables.is_empty() {
            doc.push_str("**Parameters:**\n");
            for var in variables {
                doc.push_str(&format!("- `{}`: ", var));
                if plural_vars.contains(var) {
                    doc.push_str("Number for plural form.\n");
                } else if select_vars.contains(var) {
                    doc.push_str(&format!("One of: {}.\n", select_options.join(", ")));
                } else {
                    doc.push_str("Value to interpolate.\n");
                }
            }
            doc.push('\n');
        }

        // Example
        doc.push_str("**Example:**\n");
        doc.push_str("```typescript\n");
        if variables.is_empty() {
            doc.push_str(&format!("t('{}');\n", key));
        } else {
            doc.push_str(&format!("t('{}', {{ ", key));
            for (i, var) in variables.iter().enumerate() {
                if i > 0 {
                    doc.push_str(", ");
                }
                if plural_vars.contains(var) {
                    doc.push_str(&format!("{}: 1", var));
                } else if select_vars.contains(var) {
                    doc.push_str(&format!(
                        "{}: '{}'",
                        var,
                        select_options.first().unwrap_or(&String::new())
                    ));
                } else {
                    doc.push_str(&format!("{}: 'value'", var));
                }
            }
            doc.push_str(" });\n");
        }
        doc.push_str("```\n");

        doc
    }

    fn categorize_variables(
        &self,
        value: &Value,
        variables: &[String],
    ) -> (Vec<String>, Vec<String>, Vec<String>) {
        let mut plural_vars = Vec::new();
        let mut select_vars = Vec::new();
        let mut simple_vars = Vec::new();

        if let Value::String(s) = value {
            let parser = parse::Parser::new(s);
            if let Ok(ast) = parser.parse() {
                categorize_variables_from_ast(
                    &ast,
                    variables,
                    &mut plural_vars,
                    &mut select_vars,
                    &mut simple_vars,
                );
            }
        }

        // Any variables not categorized are assumed to be simple
        for var in variables {
            if !plural_vars.contains(var)
                && !select_vars.contains(var)
                && !simple_vars.contains(var)
            {
                simple_vars.push(var.clone());
            }
        }

        (plural_vars, select_vars, simple_vars)
    }
}

fn categorize_variables_from_ast(
    node: &AstNode,
    variables: &[String],
    plural_vars: &mut Vec<String>,
    select_vars: &mut Vec<String>,
    simple_vars: &mut Vec<String>,
) {
    match node {
        AstNode::Root(children) => {
            for child in children {
                categorize_variables_from_ast(
                    child,
                    variables,
                    plural_vars,
                    select_vars,
                    simple_vars,
                );
            }
        }
        AstNode::Plural { variable, .. } => {
            if variables.contains(variable) && !plural_vars.contains(variable) {
                plural_vars.push(variable.clone());
            }
        }
        AstNode::Select { variable, .. } => {
            if variables.contains(variable) && !select_vars.contains(variable) {
                select_vars.push(variable.clone());
            }
        }
        AstNode::Variable(var) => {
            if variables.contains(var)
                && !simple_vars.contains(var)
                && !plural_vars.contains(var)
                && !select_vars.contains(var)
            {
                simple_vars.push(var.clone());
            }
        }
        AstNode::HtmlTag { children, .. } => {
            for child in children {
                categorize_variables_from_ast(
                    child,
                    variables,
                    plural_vars,
                    select_vars,
                    simple_vars,
                );
            }
        }
        _ => {}
    }
}
