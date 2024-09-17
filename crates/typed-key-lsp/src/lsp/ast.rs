use serde_json::Value;

use crate::parse;
use crate::parse::AstNode;

pub(crate) fn extract_variables_and_options(
    value: &Value,
) -> (Vec<String>, std::collections::HashMap<String, Vec<String>>) {
    if let Value::String(s) = value {
        let parser = parse::Parser::new(s);
        if let Ok(ast) = parser.parse() {
            collect_variables_and_options(&ast)
        } else {
            (Vec::new(), std::collections::HashMap::new())
        }
    } else {
        (Vec::new(), std::collections::HashMap::new())
    }
}

fn collect_variables_and_options(
    node: &AstNode,
) -> (Vec<String>, std::collections::HashMap<String, Vec<String>>) {
    let mut variables = Vec::new();
    let mut select_options = std::collections::HashMap::new();

    match node {
        AstNode::Root(children) => {
            for child in children {
                let (vars, opts) = collect_variables_and_options(child);
                variables.extend(vars);
                for (k, v) in opts {
                    select_options.entry(k).or_insert_with(Vec::new).extend(v);
                }
            }
        }
        AstNode::Variable(var) => {
            variables.push(var.clone());
        }
        AstNode::Plural { variable, options } | AstNode::Select { variable, options } => {
            variables.push(variable.clone());
            select_options.insert(variable.clone(), options.keys().cloned().collect());
            options.iter().for_each(|(_, option_nodes)| {
                for option_node in option_nodes {
                    let (vars, opts) = collect_variables_and_options(option_node);
                    variables.extend(vars);
                    for (k, v) in opts {
                        select_options.entry(k).or_insert_with(Vec::new).extend(v);
                    }
                }
            });
        }
        AstNode::HtmlTag { children, .. } => {
            for child in children {
                let (vars, opts) = collect_variables_and_options(child);
                variables.extend(vars);
                for (k, v) in opts {
                    select_options.entry(k).or_insert_with(Vec::new).extend(v);
                }
            }
        }
        _ => {}
    }

    variables.dedup();
    (variables, select_options)
}
