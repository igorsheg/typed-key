use oxc::allocator::Allocator;
use oxc::ast::ast::*;
use oxc::ast::visit::Visit;
use oxc::ast::AstKind;
use oxc::parser::Parser;
use oxc::span::GetSpan;
use oxc::span::Span;
use std::marker::PhantomData;
use tower_lsp::lsp_types::Position;

#[derive(Debug, Clone, PartialEq)]
pub enum TFunctionInfo {
    NotInFunction,
    InFunction(FunctionContext),
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionContext {
    pub first_param: Option<String>,
    pub second_param: Option<SecondParamInfo>,
    pub span: Option<Span>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SecondParamInfo {
    InObject(String),
    InObjectKey(String),
    InObjectKeyValue(String),
}

pub struct TFunctionVisitor<'a> {
    offset: u32,
    current_context: Vec<TFunctionInfo>,
    cursor_info: Option<TFunctionInfo>,
    _phantom: PhantomData<&'a ()>,
}

impl<'a> TFunctionVisitor<'a> {
    pub fn new(offset: Option<u32>) -> Self {
        Self {
            offset: offset.unwrap_or(0),
            current_context: Vec::new(),
            cursor_info: None,
            _phantom: PhantomData,
        }
    }

    pub fn analyze(&self, source: &str, position: Position) -> TFunctionInfo {
        let allocator = Allocator::default();
        let source_type = SourceType::default()
            .with_typescript(true)
            .with_module(true)
            .with_jsx(true);

        let parse_result = Parser::new(&allocator, source, source_type).parse();
        let program = parse_result.program;

        let offset = self.position_to_offset(&position, source);

        let mut visitor = TFunctionVisitor::new(Some(offset));
        (&mut visitor).visit_program(&program);
        visitor.cursor_info.unwrap_or(TFunctionInfo::NotInFunction)
    }

    fn is_offset_in_span(&self, span: Span) -> bool {
        self.offset >= span.start && self.offset < span.end
    }

    fn enter_t_function(&mut self, first_param: Option<String>, span: Span) {
        self.current_context
            .push(TFunctionInfo::InFunction(FunctionContext {
                first_param,
                second_param: None,
                span: Some(span),
            }));
    }

    fn leave_t_function(&mut self) {
        self.current_context.pop();
    }

    fn update_current_info(&mut self, update: impl FnOnce(&mut FunctionContext)) {
        if let Some(TFunctionInfo::InFunction(context)) = self.current_context.last_mut() {
            update(context);
            self.cursor_info = Some(TFunctionInfo::InFunction(context.clone()));
        }
    }

    fn is_t_function_call(&self, call_expr: &CallExpression) -> bool {
        match &call_expr.callee {
            Expression::Identifier(ident) => ident.name == "t",
            Expression::StaticMemberExpression(static_member) => static_member.property.name == "t",
            _ => false,
        }
    }

    fn get_key_name(&self, key: &PropertyKey) -> Option<String> {
        match key {
            _ => Some(key.name().unwrap_or_default().into_owned()),
        }
    }

    fn position_to_offset(&self, position: &Position, source: &str) -> u32 {
        source
            .lines()
            .take(position.line as usize)
            .map(|line| line.len() + 1)
            .sum::<usize>() as u32
            + position.character as u32
    }
}

impl<'a> Visit<'a> for TFunctionVisitor<'a> {
    fn enter_node(&mut self, kind: AstKind<'a>) {
        match kind {
            AstKind::CallExpression(call_expr) => {
                if self.is_t_function_call(call_expr) {
                    let first_param = call_expr.arguments.get(0).and_then(|arg| {
                        if let Expression::StringLiteral(lit) = &arg.to_expression() {
                            Some(lit.value.to_string())
                        } else {
                            None
                        }
                    });
                    self.enter_t_function(first_param, call_expr.span);

                    if self.is_offset_in_span(call_expr.span) {
                        self.cursor_info = self.current_context.last().cloned();
                    }
                }
            }
            AstKind::StringLiteral(string_literal) => {
                if !self.current_context.is_empty() && self.is_offset_in_span(string_literal.span) {
                    // Only update first_param if it's empty or None
                    self.update_current_info(|context| {
                        if context.first_param.as_ref().map_or(true, |s| s.is_empty()) {
                            context.first_param = Some(string_literal.value.to_string());
                        } else {
                        }
                    });
                }
            }
            AstKind::ObjectExpression(obj_expr) => {
                if !self.current_context.is_empty() && self.is_offset_in_span(obj_expr.span) {
                    self.update_current_info(|context| {
                        context.second_param = Some(SecondParamInfo::InObject(
                            context.first_param.clone().unwrap_or("".to_string()),
                        ));
                    });
                }
            }
            AstKind::ObjectProperty(property) => {
                if !self.current_context.is_empty() {
                    if self.is_offset_in_span(property.key.span()) {
                        if let Some(key_name) = self.get_key_name(&property.key) {
                            self.update_current_info(|context| {
                                context.second_param = Some(SecondParamInfo::InObjectKey(key_name));
                            });
                        }
                    } else if self.is_offset_in_span(property.value.span()) {
                        if let Some(key_name) = self.get_key_name(&property.key) {
                            self.update_current_info(|context| {
                                context.second_param =
                                    Some(SecondParamInfo::InObjectKeyValue(key_name));
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn leave_node(&mut self, kind: AstKind<'a>) {
        if let AstKind::CallExpression(call_expr) = kind {
            if self.is_t_function_call(call_expr) {
                self.leave_t_function();
            }
        }
    }
}
