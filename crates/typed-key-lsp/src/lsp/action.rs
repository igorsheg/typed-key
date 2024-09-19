use oxc::allocator::Allocator;
use oxc::parser::Parser;
use ropey::Rope;
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, CodeActionResponse,
    Position, Range, TextEdit, WorkspaceEdit,
};

use super::channels::diagnostics::MissingVariableDiagnosticData;
use super::visitor::{TFunctionInfo, TFunctionVisitor};

use oxc::ast::visit::Visit;
use oxc::ast::AstKind;
use oxc::ast::{ast::*, AstBuilder};
use oxc::span::Span;

struct OptionsObjectVisitor {
    t_function_span: Span,
    options_object_span: Option<Span>,
    has_second_argument: bool,
}

impl OptionsObjectVisitor {
    fn new(t_function_span: Span) -> Self {
        Self {
            t_function_span,
            options_object_span: None,
            has_second_argument: false,
        }
    }
}

impl<'a> Visit<'a> for OptionsObjectVisitor {
    fn enter_node(&mut self, kind: AstKind<'a>) {
        if let AstKind::CallExpression(call_expr) = kind {
            if call_expr.span == self.t_function_span {
                self.has_second_argument = call_expr.arguments.len() > 1;
                if let Some(second_arg) = call_expr.arguments.get(1) {
                    if let Expression::ObjectExpression(obj_expr) = &second_arg.to_expression() {
                        self.options_object_span = Some(obj_expr.span);
                    }
                }
            }
        }
    }
}

fn find_options_object(program: &Program, t_function_span: Span) -> (Option<Span>, bool) {
    let mut visitor = OptionsObjectVisitor::new(t_function_span);
    visitor.visit_program(&program);
    let obj = visitor.options_object_span;
    (obj, visitor.has_second_argument)
}

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
    let allocator = Allocator::default();
    let source_type = SourceType::default()
        .with_typescript(true)
        .with_module(true)
        .with_jsx(true);

    let parse_result = Parser::new(&allocator, content, source_type).parse();
    let program = parse_result.program;

    let position = diagnostic_range.start;
    let offset = position_to_offset(&position, content);

    let t_visitor = TFunctionVisitor::new(Some(offset));

    if let TFunctionInfo::InFunction(context) = t_visitor.analyze(&content, position) {
        if let Some(t_function_span) = context.span {
            let (options_span, has_second_argument) =
                find_options_object(&program, t_function_span);
            return create_edit_from_span(
                &allocator,
                content,
                t_function_span,
                options_span,
                has_second_argument,
                missing_var,
            );
        }
    }

    None
}

fn position_to_offset(position: &Position, source: &str) -> u32 {
    source
        .lines()
        .take(position.line as usize)
        .map(|line| line.len() + 1)
        .sum::<usize>() as u32
        + position.character as u32
}

fn create_edit_from_span(
    allocator: &Allocator,
    content: &str,
    t_function_span: Span,
    options_span: Option<Span>,
    has_second_argument: bool,
    missing_var: &str,
) -> Option<TextEdit> {
    let builder = AstBuilder::new(allocator);

    let new_property = builder.object_property(
        Span::default(),
        PropertyKind::Init,
        builder.property_key_identifier_name(Span::default(), missing_var),
        builder.expression_string_literal(Span::default(), "".to_string()),
        None,
        false,
        false,
        false,
    );

    let (insert_position, new_text) = if let Some(options_span) = options_span {
        // Options object exists, add new property
        let insert_position = position_from_offset(content, options_span.end - 1);

        let new_text = format!(
            ", {}",
            ast_to_string(&AstKind::ObjectProperty(&new_property))
        );
        (insert_position, new_text)
    } else if has_second_argument {
        // Second argument exists but is not an object, replace with new object
        let insert_position = position_from_offset(content, t_function_span.end - 1);
        let new_object = builder.object_expression(
            Span::default(),
            builder.vec1(ObjectPropertyKind::ObjectProperty(
                builder.alloc(new_property),
            )),
            None,
        );

        let new_text = format!(
            ", {}",
            (ast_to_string(&AstKind::ObjectExpression(&new_object)))
        );
        (insert_position, new_text)
    } else {
        // No second argument, create new object
        let insert_position = position_from_offset(content, t_function_span.end - 1);
        let new_object = builder.object_expression(
            Span::default(),
            builder.vec1(ObjectPropertyKind::ObjectProperty(
                builder.alloc(new_property),
            )),
            None,
        );

        let new_text = format!(
            ", {}",
            (ast_to_string(&AstKind::ObjectExpression(&new_object)))
        );
        (insert_position, new_text)
    };

    Some(TextEdit {
        range: Range::new(insert_position, insert_position),
        new_text,
    })
}

fn ast_to_string(node: &AstKind) -> String {
    match node {
        AstKind::ObjectExpression(obj) => {
            let properties = obj
                .properties
                .iter()
                .map(|prop| match prop {
                    ObjectPropertyKind::ObjectProperty(p) => format!(
                        "{}: {}",
                        ast_to_string(&AstKind::PropertyKey(&p.key)),
                        ast_to_string(&AstKind::from_expression(&p.value))
                    ),
                    _ => "".to_string(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{ {} }}", properties)
        }
        AstKind::ObjectProperty(obj_prop) => {
            let key = ast_to_string(&AstKind::PropertyKey(&obj_prop.key));
            format!("{}: ", key)
        }
        AstKind::StringLiteral(s) => format!("\"{}\"", s.value),
        AstKind::IdentifierName(id) => id.name.to_string(),
        AstKind::PropertyKey(key) => key.name().unwrap_or_default().to_string(),
        _ => "[unsupported node]".to_string(),
    }
}

fn position_from_offset(content: &str, offset: u32) -> Position {
    let mut line = 0;
    let mut column = 0;
    for (index, c) in content.char_indices() {
        if index as u32 == offset {
            break;
        }
        if c == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    Position::new(line, column)
}
