use tree_sitter::Query;

#[derive(Debug)]
pub struct Queries {
    pub typescript_t_function: Query,
}

impl Clone for Queries {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl Default for Queries {
    fn default() -> Self {
        Self {
            typescript_t_function: Query::new(
                &tree_sitter_typescript::language_typescript().into(),
                TYPESCRIPT_T_FUNCTION,
            )
            .unwrap(),
        }
    }
}

impl Queries {
    pub fn update(&mut self) {
        // If you need to update queries dynamically, implement the logic here
    }
}

pub static TYPESCRIPT_T_FUNCTION: &str = r#"
(
  (call_expression
    function: [
      (identifier) @func_name
      (member_expression
        property: (property_identifier) @func_name)
    ]
    arguments: (arguments
      (string_literal) @first_arg
      (object)? @second_arg)
    (#eq? @func_name "t")
  ) @t_call

  (call_expression
    function: [
      (identifier) @func_name
      (member_expression
        property: (property_identifier) @func_name)
    ]
    arguments: (arguments
      (string_literal) @first_arg
      (string_literal) @default_value
      (object)? @second_arg)
    (#eq? @func_name "t")
  ) @t_call_with_default
)
"#;
