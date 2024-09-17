use ropey::Rope;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    Position, Range, TextEdit, WorkspaceEdit,
};

use super::diagnostics::MissingVariableDiagnosticData;

pub(crate) async fn handle_code_action(
    params: CodeActionParams,
    document: &Rope,
) -> tower_lsp::jsonrpc::Result<Option<CodeActionResponse>> {
    let uri = params.text_document.uri;

    let mut actions = Vec::new();

    for diagnostic in params.context.diagnostics {
        if let Some(data) = diagnostic.data.as_ref() {
            if let Ok(diagnostic_data) =
                serde_json::from_value::<MissingVariableDiagnosticData>(data.clone())
            {
                if let Some(edit) = create_insert_variable_edit(
                    &document.to_string(),
                    diagnostic.range,
                    &diagnostic_data.missing_variable,
                ) {
                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: format!(
                            "Insert missing variable: {}",
                            diagnostic_data.missing_variable
                        ),
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(vec![diagnostic.clone()]),
                        edit: Some(WorkspaceEdit {
                            changes: Some([(uri.clone(), vec![edit])].into_iter().collect()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }));
                }
            }
        }
    }

    if actions.is_empty() {
        Ok(None)
    } else {
        Ok(Some(actions))
    }
}

fn create_insert_variable_edit(
    content: &str,
    diagnostic_range: Range,
    missing_var: &str,
) -> Option<TextEdit> {
    let lines: Vec<&str> = content.lines().collect();
    let start_line = diagnostic_range.start.line as usize;

    let (t_call_start_line, t_call_start_char) = find_t_call_start(&lines, start_line)?;

    let (t_call_end_line, t_call_end_char) =
        find_t_call_end(&lines, t_call_start_line, t_call_start_char)?;

    let options_info = find_options_object(
        &lines,
        t_call_start_line,
        t_call_start_char,
        t_call_end_line,
        t_call_end_char,
    );

    match options_info {
        Some((options_start_line, options_start_char, options_end_line, options_end_char)) => {
            let insert_line = options_end_line;
            let insert_char = if options_start_line == options_end_line {
                options_end_char - 1
            } else {
                lines[insert_line].len() as u32
            };

            let insert_position = Position::new(insert_line as u32, insert_char);
            let options_content = if options_start_line == options_end_line {
                &lines[options_start_line]
                    [options_start_char as usize + 1..options_end_char as usize - 1]
            } else {
                &lines[options_start_line][options_start_char as usize + 1..]
            };

            let trimmed_content = options_content.trim();

            let new_text = if trimmed_content.is_empty() {
                format!("{}: \"\"", missing_var)
            } else if trimmed_content.ends_with(',') {
                format!(" {}: \"\"", missing_var)
            } else {
                format!(", {}: \"\"", missing_var)
            };

            Some(TextEdit {
                range: Range::new(insert_position, insert_position),
                new_text,
            })
        }
        None => {
            let insert_position = Position::new(t_call_end_line as u32, t_call_end_char - 1);
            Some(TextEdit {
                range: Range::new(insert_position, insert_position),
                new_text: format!(", {{ {}: \"\" }}", missing_var),
            })
        }
    }
}

fn find_t_call_start(lines: &[&str], start_line: usize) -> Option<(usize, u32)> {
    for (i, line) in lines.iter().enumerate().skip(start_line) {
        if let Some(index) = line.find("t(") {
            return Some((i, index as u32));
        }
    }
    None
}

fn find_t_call_end(lines: &[&str], start_line: usize, start_char: u32) -> Option<(usize, u32)> {
    let mut paren_count = 0;
    for (i, line) in lines.iter().enumerate().skip(start_line) {
        let start = if i == start_line {
            start_char as usize
        } else {
            0
        };
        for (j, c) in line[start..].char_indices() {
            if c == '(' {
                paren_count += 1;
            } else if c == ')' {
                paren_count -= 1;
                if paren_count == 0 {
                    return Some((i, (start + j + 1) as u32));
                }
            }
        }
    }
    None
}

fn find_options_object(
    lines: &[&str],
    start_line: usize,
    start_char: u32,
    end_line: usize,
    end_char: u32,
) -> Option<(usize, u32, usize, u32)> {
    let mut brace_count = 0;
    let mut options_start = None;
    let mut in_string = false;
    let mut escape = false;

    for (i, line) in lines.iter().enumerate().skip(start_line) {
        let start = if i == start_line {
            start_char as usize
        } else {
            0
        };
        let end = if i == end_line {
            end_char as usize
        } else {
            line.len()
        };

        for (j, c) in line[start..end].char_indices() {
            if !in_string {
                if c == '{' {
                    if brace_count == 0 {
                        options_start = Some((i, (start + j) as u32));
                    }
                    brace_count += 1;
                } else if c == '}' {
                    brace_count -= 1;
                    if brace_count == 0 && options_start.is_some() {
                        if let Some(options_start) = options_start {
                            return Some((
                                options_start.0,
                                options_start.1,
                                i,
                                (start + j + 1) as u32,
                            ));
                        }
                    }
                } else if c == '"' || c == '\'' {
                    in_string = true;
                }
            } else if !escape {
                if c == '"' || c == '\'' {
                    in_string = false;
                } else if c == '\\' {
                    escape = true;
                }
            } else {
                escape = false;
            }
        }
    }
    None
}
