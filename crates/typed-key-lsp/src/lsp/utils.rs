use tower_lsp::lsp_types::{Position, Range};
use tree_sitter::Node;

pub(crate) fn is_position_in_node(position: Position, node: Node) -> bool {
    let start = node.start_position();
    let end = node.end_position();
    let pos = tree_sitter::Point {
        row: position.line as usize,
        column: position.character as usize,
    };
    start <= pos && pos < end
}

pub(crate) fn position_to_index(content: &str, position: Position) -> usize {
    content
        .lines()
        .take(position.line as usize)
        .map(|line| line.len() + 1)
        .sum::<usize>()
        + position.character as usize
}

pub(crate) fn node_to_range(node: Node) -> Range {
    let start = node.start_position();
    let end = node.end_position();
    Range {
        start: Position {
            line: start.row as u32,
            character: start.column as u32,
        },
        end: Position {
            line: end.row as u32,
            character: end.column as u32,
        },
    }
}
