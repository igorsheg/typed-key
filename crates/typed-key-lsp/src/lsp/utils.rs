use tower_lsp::lsp_types::{Position, Range};
use tree_sitter::Node;

use crate::parse::AstNode;

pub(crate) fn traverse_nodes(node: Node<'_>) -> Vec<Node<'_>> {
    let mut nodes = vec![node];
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        nodes.extend(traverse_nodes(child));
    }

    nodes
}

pub(crate) fn traverse_ast_for_variables(node: &AstNode, variables: &mut Vec<String>) {
    match node {
        AstNode::Root(children) => {
            for child in children {
                traverse_ast_for_variables(child, variables);
            }
        }
        AstNode::Variable(var) => {
            if !variables.contains(var) {
                variables.push(var.clone());
            }
        }
        AstNode::Plural { variable, options } | AstNode::Select { variable, options } => {
            if !variables.contains(variable) {
                variables.push(variable.clone());
            }
            options.iter().for_each(|(_, value)| {
                for child in value {
                    traverse_ast_for_variables(child, variables);
                }
            });
        }
        AstNode::HtmlTag { children, .. } => {
            for child in children {
                traverse_ast_for_variables(child, variables);
            }
        }
        _ => {}
    }
}

pub(crate) fn is_select_variable(ast: &AstNode, var_name: &str) -> bool {
    matches!(ast, AstNode::Select { variable, .. } if variable == var_name)
        || matches!(ast, AstNode::Root(children) if children.iter().any(|child| is_select_variable(child, var_name)))
}

pub(crate) fn get_select_options(ast: &AstNode, var_name: &str) -> Option<Vec<String>> {
    match ast {
        AstNode::Root(children) => children
            .iter()
            .find_map(|child| get_select_options(child, var_name)),
        AstNode::Select { variable, options } if variable == var_name => {
            Some(options.keys().cloned().collect())
        }
        _ => None,
    }
}
pub fn node_to_range(node: Node) -> Range {
    Range {
        start: Position {
            line: node.start_position().row as u32,
            character: node.start_position().column as u32,
        },
        end: Position {
            line: node.end_position().row as u32,
            character: node.end_position().column as u32,
        },
    }
}
