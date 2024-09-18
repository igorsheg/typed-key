use tree_sitter::Query;
use tree_sitter_typescript::{language_tsx, language_typescript};

#[derive(Debug)]
pub struct Queries {
    pub ts_t_function: Query,
    pub tsx_t_function: Query,
}

impl Clone for Queries {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl Default for Queries {
    fn default() -> Self {
        Self {
            ts_t_function: Query::new(&language_typescript(), T_FUNCTION_QUERY).unwrap(),
            tsx_t_function: Query::new(&language_tsx(), T_FUNCTION_QUERY).unwrap(),
        }
    }
}

pub static T_FUNCTION_QUERY: &str = r#"
(
  (call_expression
    function: [
      (identifier) @func_name
      (member_expression
        property: (property_identifier) @func_name)
    ]
    arguments: (arguments
      (_)? @first_arg
      (_)? @second_arg)
    (#eq? @func_name "t"))
)
"#;
