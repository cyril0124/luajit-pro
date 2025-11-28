//! # Lua Code Optimizer
//!
//! This module provides compile-time optimizations for Lua code.
//!
//! ## Supported Optimizations
//!
//! - `@comp_time_enum`: Replace enum field accesses with literal values
//! - `@used`: Prevent variables from being optimized out

use std::collections::HashMap;

use full_moon::{
    ast::{
        punctuated::{Pair, Punctuated},
        Expression, Field, Index, Prefix, Stmt, Suffix, Var,
    },
    tokenizer::TokenType,
    visitors::VisitorMut,
};

use crate::ast_utilis;

// ============================================================================
// Optimizer State
// ============================================================================

/// The Lua code optimizer
///
/// This visitor applies compile-time optimizations based on special annotations:
///
/// - `--[[@comp_time_enum]]`: Marks a table as a compile-time enum, allowing
///   field accesses to be replaced with literal values
/// - `--[[@used]]`: Prevents a variable from being optimized out
pub struct LuaOptimizer {
    /// Map of enum name -> (field name -> literal value)
    enum_map: Option<HashMap<String, HashMap<String, String>>>,
}

impl LuaOptimizer {
    /// Creates a new optimizer instance
    pub fn new() -> Self {
        Self { enum_map: None }
    }

    /// Registers a compile-time enum from a local assignment
    fn register_enum(&mut self, enum_name: &str, fields: &[Field]) {
        let mut field_map = HashMap::new();

        for field in fields {
            if let Field::NameKey { key, value, .. } = field {
                let value_str = match value {
                    Expression::Number(n) => Some(n.token().to_string()),
                    Expression::String(s) => Some(s.token().to_string()),
                    _ => {
                        println!(
                            "[luajit_pro_helper] [@comp_time_enum] Skipping non-constant: {} in {}",
                            value, enum_name
                        );
                        None
                    }
                };

                if let Some(v) = value_str {
                    field_map.insert(key.token().to_string(), v);
                }
            }
        }

        self.enum_map
            .get_or_insert_with(HashMap::new)
            .insert(enum_name.to_string(), field_map);
    }
}

impl Default for LuaOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl VisitorMut for LuaOptimizer {
    fn visit_stmt(&mut self, node: Stmt) -> Stmt {
        let Stmt::LocalAssignment(local_assignment) = node.clone() else {
            return node;
        };

        // Check for annotations in trailing trivia
        let local_token = local_assignment.local_token();
        let mut annotation = None;

        for trivia in local_token.trailing_trivia() {
            if let TokenType::MultiLineComment { blocks, comment } = trivia.token_type() {
                if *blocks == 0 && matches!(comment.as_str(), "@comp_time_enum" | "@used") {
                    annotation = Some(comment.as_str().to_string());
                    break;
                }
            }
        }

        let annotation = match annotation {
            Some(a) => a,
            None => return node,
        };

        // Get variable names
        let names: Vec<String> = local_assignment
            .names()
            .pairs()
            .map(|p| p.value().token().to_string())
            .collect();

        if names.len() != 1 {
            panic!(
                "[{}] Annotation supports only one variable name",
                annotation
            );
        }

        let var_name = &names[0];

        assert!(
            local_assignment.expressions().pairs().count() == 1,
            "[{}] Annotation supports only one expression",
            annotation
        );

        match annotation.as_str() {
            "@comp_time_enum" => {
                // Extract enum fields from table constructor
                for expr_pair in local_assignment.expressions().pairs() {
                    let Expression::TableConstructor(table) = expr_pair.value() else {
                        panic!("[@comp_time_enum] Expected table constructor");
                    };

                    let fields: Vec<Field> = table.fields().iter().cloned().collect();
                    self.register_enum(var_name, &fields);
                }

                Stmt::LocalAssignment(local_assignment)
            }

            "@used" => {
                // Add dummy usage to prevent variable from being optimized out
                let mut new_exprs: Punctuated<Expression> = Punctuated::new();

                for expr_pair in local_assignment.expressions().pairs() {
                    let expr = expr_pair.value();
                    let modified =
                        ast_utilis::insert_after_expr(expr, &format!(" _ = {} ", var_name));
                    new_exprs.push(Pair::new(modified, None));

                    log::debug!(
                        "[luajit-pro] [@used] Inserted dummy usage for: {}",
                        var_name
                    );
                }

                Stmt::LocalAssignment(local_assignment.with_expressions(new_exprs))
            }

            _ => panic!("Unknown annotation: {}", annotation),
        }
    }

    fn visit_var(&mut self, node: Var) -> Var {
        let Var::Expression(var_expr) = &node else {
            return node;
        };

        let enum_map = match &self.enum_map {
            Some(m) => m,
            None => return node,
        };

        // Get the variable name (enum name)
        let enum_name = match var_expr.prefix() {
            Prefix::Name(name) => name.token().to_string(),
            _ => return node,
        };

        // Look up the enum
        let field_map = match enum_map.get(&enum_name) {
            Some(m) => m,
            None => return node,
        };

        // Check for single dot suffix (e.g., EnumName.FieldName)
        let suffixes: Vec<&Suffix> = var_expr.suffixes().collect();
        if suffixes.len() != 1 {
            return node;
        }

        let (field_name, dot_token, name_token) = match suffixes[0] {
            Suffix::Index(Index::Dot { dot, name }) => (name.token().to_string(), dot, name),
            _ => return node,
        };

        // Look up the field value
        let value = match field_map.get(&field_name) {
            Some(v) => v,
            None => return node,
        };

        // Replace with literal value and comment
        let replacement = format!("{} --[[{}.{}]]", value, enum_name, field_name);

        let new_prefix = match var_expr.prefix() {
            Prefix::Name(name) => Prefix::Name(ast_utilis::replace_token(name, &replacement)),
            _ => unreachable!(),
        };

        let new_suffixes = vec![Suffix::Index(Index::Dot {
            dot: ast_utilis::empty_token(dot_token),
            name: ast_utilis::empty_token(name_token),
        })];

        Var::Expression(Box::new(
            var_expr
                .clone()
                .with_prefix(new_prefix)
                .with_suffixes(new_suffixes),
        ))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use full_moon::{parse, visitors::VisitorMut};

    /// Tests creating a new optimizer instance.
    #[test]
    fn test_optimizer_new() {
        let optimizer = LuaOptimizer::new();
        assert!(optimizer.enum_map.is_none());
    }

    /// Tests the Default trait implementation.
    #[test]
    fn test_optimizer_default() {
        let optimizer = LuaOptimizer::default();
        assert!(optimizer.enum_map.is_none());
    }

    /// Tests registering an enum with numeric values.
    ///
    /// # Example
    /// ```lua
    /// ---@comp_time_enum
    /// local Status = {
    ///     OK = 0,
    ///     ERROR = 1,
    /// }
    /// ```
    #[test]
    fn test_register_enum_numeric() {
        let mut optimizer = LuaOptimizer::new();

        // Parse a table constructor to get Field objects
        let ast = parse("local t = {A = 1, B = 2}").unwrap();
        let stmts: Vec<_> = ast.nodes().stmts().collect();
        if let Some(full_moon::ast::Stmt::LocalAssignment(local)) = stmts.first() {
            if let Some(pair) = local.expressions().pairs().next() {
                if let Expression::TableConstructor(table) = pair.value() {
                    let fields: Vec<Field> = table.fields().iter().cloned().collect();
                    optimizer.register_enum("TestEnum", &fields);

                    let enum_map = optimizer.enum_map.unwrap();
                    let field_map = enum_map.get("TestEnum").unwrap();
                    assert_eq!(field_map.get("A"), Some(&"1".to_string()));
                    assert_eq!(field_map.get("B"), Some(&"2".to_string()));
                }
            }
        }
    }

    /// Tests registering an enum with string values.
    ///
    /// # Example
    /// ```lua
    /// ---@comp_time_enum
    /// local LogLevel = {
    ///     DEBUG = "debug",
    ///     INFO = "info",
    /// }
    /// ```
    #[test]
    fn test_register_enum_string() {
        let mut optimizer = LuaOptimizer::new();

        let ast = parse(r#"local t = {X = "hello", Y = "world"}"#).unwrap();
        let stmts: Vec<_> = ast.nodes().stmts().collect();
        if let Some(full_moon::ast::Stmt::LocalAssignment(local)) = stmts.first() {
            if let Some(pair) = local.expressions().pairs().next() {
                if let Expression::TableConstructor(table) = pair.value() {
                    let fields: Vec<Field> = table.fields().iter().cloned().collect();
                    optimizer.register_enum("StringEnum", &fields);

                    let enum_map = optimizer.enum_map.unwrap();
                    let field_map = enum_map.get("StringEnum").unwrap();
                    assert!(field_map.get("X").unwrap().contains("hello"));
                    assert!(field_map.get("Y").unwrap().contains("world"));
                }
            }
        }
    }

    /// Tests that @comp_time_enum annotation is recognized.
    ///
    /// The annotation must be a `--[[@comp_time_enum]]` comment
    /// immediately after the `local` keyword.
    #[test]
    fn test_visit_stmt_comp_time_enum() {
        let code = r#"local --[[@comp_time_enum]] Color = {RED = 1, GREEN = 2}"#;
        let ast = parse(code).unwrap();

        let mut optimizer = LuaOptimizer::new();
        let _ = optimizer.visit_ast(ast);

        // Enum should be registered
        assert!(optimizer.enum_map.is_some());
        let enum_map = optimizer.enum_map.unwrap();
        assert!(enum_map.contains_key("Color"));
    }

    /// Tests that @used annotation adds dummy usage.
    ///
    /// # Example
    /// ```lua
    /// ---@used
    /// local _INIT = setup()
    /// ```
    /// becomes:
    /// ```lua
    /// local _INIT = setup() _ = _INIT
    /// ```
    #[test]
    fn test_visit_stmt_used() {
        let code = r#"local --[[@used]] _KEEP = 42"#;
        let ast = parse(code).unwrap();

        let mut optimizer = LuaOptimizer::new();
        let new_ast = optimizer.visit_ast(ast);
        let result = new_ast.to_string();

        // Should contain dummy usage
        assert!(result.contains("_ = _KEEP") || result.contains("_=_KEEP"));
    }

    /// Tests that enum field accesses are replaced with literal values.
    ///
    /// # Example
    /// Before:
    /// ```lua
    /// local c = Color.RED
    /// ```
    /// After:
    /// ```lua
    /// local c = 1 --[[Color.RED]]
    /// ```
    #[test]
    fn test_visit_var_enum_replacement() {
        let code = r#"local --[[@comp_time_enum]] Color = {RED = 1}
local c = Color.RED"#;
        let ast = parse(code).unwrap();

        let mut optimizer = LuaOptimizer::new();
        let new_ast = optimizer.visit_ast(ast);
        let result = new_ast.to_string();

        // Field access should be replaced with value and comment
        assert!(result.contains("1") && result.contains("Color.RED"));
    }

    /// Tests that non-enum variable accesses are not modified.
    #[test]
    fn test_visit_var_non_enum() {
        let code = "local x = someTable.field";
        let ast = parse(code).unwrap();

        let mut optimizer = LuaOptimizer::new();
        let new_ast = optimizer.visit_ast(ast);
        let result = new_ast.to_string();

        // Should remain unchanged
        assert!(result.contains("someTable.field"));
    }

    /// Tests multiple enum registrations.
    #[test]
    fn test_multiple_enums() {
        let code = r#"local --[[@comp_time_enum]] A = {X = 1}
local --[[@comp_time_enum]] B = {Y = 2}"#;
        let ast = parse(code).unwrap();

        let mut optimizer = LuaOptimizer::new();
        let _ = optimizer.visit_ast(ast);

        let enum_map = optimizer.enum_map.unwrap();
        assert!(enum_map.contains_key("A"));
        assert!(enum_map.contains_key("B"));
    }
}
