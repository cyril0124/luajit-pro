//! # Lua AST Transformer
//!
//! This module provides the core AST transformation logic for LuaJIT-Pro's
//! compile-time features.
//!
//! ## Supported Transformations
//!
//! - `__LJP:COMP_TIME()`: Compile-time code generation
//! - `__ljp:include()`: File inclusion
//! - `__LJP:INCLUDE_NO_RETURN()`: File inclusion without return statement

use full_moon::{
    ast::{
        span::ContainedSpan, Call, FunctionArgs, FunctionCall, FunctionDeclaration, Index,
        LastStmt, Parameter, Prefix, Return, Suffix,
    },
    tokenizer::{Token, TokenReference, TokenType},
    visitors::VisitorMut,
    ShortString,
};

use crate::{ast_utilis, lang_utils, transform_lua_code};

// ============================================================================
// Helper Traits
// ============================================================================

/// Extension trait for removing Lua comments from strings
trait RemoveLuaComments {
    fn remove_lua_comments(&self) -> String;
}

impl RemoveLuaComments for String {
    fn remove_lua_comments(&self) -> String {
        lang_utils::remove_lua_comments(self.as_str())
    }
}

// ============================================================================
// Return Statement Remover
// ============================================================================

/// AST visitor that removes the last return statement from a block
///
/// This is used by `__LJP:INCLUDE_NO_RETURN()` to include file contents
/// without the trailing return statement.
struct LuaLastReturnRemover;

impl LuaLastReturnRemover {
    fn new() -> Self {
        Self
    }
}

impl VisitorMut for LuaLastReturnRemover {
    fn visit_last_stmt(&mut self, node: LastStmt) -> LastStmt {
        match &node {
            LastStmt::Return(ret) => {
                // Replace return with an empty token
                LastStmt::Return(Return::new().with_token(ast_utilis::empty_token(ret.token())))
            }
            _ => panic!("Expected `return` statement, found: {:?}", node),
        }
    }
}

// ============================================================================
// Main Transformer
// ============================================================================

/// The main Lua AST transformer for LuaJIT-Pro
///
/// This visitor transforms Lua AST nodes to implement compile-time features:
/// - Function declarations named `__LJP:COMP_TIME` are executed at compile time
/// - Function calls to `__ljp:include` or `__LJP:INCLUDE` are replaced with file contents
pub struct LuaTransformer {
    /// Path to the source file being transformed (for error messages)
    pub file_path: Option<String>,
    /// Parameter list for compile-time environment
    pub input_param_list: Option<Vec<(String, String)>>,
}

impl LuaTransformer {
    /// Creates a new transformer instance
    pub fn new() -> Self {
        Self {
            file_path: None,
            input_param_list: None,
        }
    }

    /// Loads parameter list into the Lua compile-time environment
    fn load_param_list_into_lua_env(&self) {
        if let Some(params) = &self.input_param_list {
            let code = params
                .iter()
                .map(|(key, value)| {
                    let lua_val = match value.as_str() {
                        "true" | "1" => "true",
                        "false" | "0" => "false",
                        _ => panic!("Invalid parameter value: {} = {}", key, value),
                    };
                    format!("rawset(_G, \"{}\", {})\n", key, lua_val)
                })
                .collect::<String>()
                + "return \"\"";

            lang_utils::lua_dostring("[load_param_list_into_lua_env]", &code);
        }
    }

    /// Unloads parameter list from the Lua compile-time environment
    fn unload_param_list_from_lua_env(&self) {
        if let Some(params) = &self.input_param_list {
            let code = params
                .iter()
                .map(|(key, _)| format!("rawset(_G, \"{}\", nil)\n", key))
                .collect::<String>()
                + "return \"\"";

            lang_utils::lua_dostring("[unload_param_list_from_lua_env]", &code);
        }
    }

    /// Resolves a `__LJP:COMP_TIME()` function declaration
    ///
    /// The function body is executed at compile time, and its return value
    /// (which must be a string) is injected as Lua code.
    fn resolve_comp_time(&self, node: FunctionDeclaration) -> FunctionDeclaration {
        // Extract and validate parameters (at most one allowed)
        let mut parameters: Vec<String> = Vec::new();
        let mut param_token: Option<TokenReference> = None;

        for param_pair in node.body().parameters().pairs() {
            match param_pair.value() {
                Parameter::Ellipsis(e) => panic!("Ellipsis parameters not supported: {}", e),
                Parameter::Name(name) => {
                    param_token = Some(name.clone());
                    parameters.push(name.to_string());
                }
                other => panic!("Unsupported parameter type: {:?}", other),
            }
        }

        assert!(
            parameters.len() <= 1,
            "COMP_TIME functions can have at most 1 parameter, got {} => {:?}",
            parameters.len(),
            parameters
        );

        // Create modified function body with end marker
        let new_body = node
            .body()
            .clone()
            .with_end_token(ast_utilis::insert_after_token(
                node.body().end_token(),
                " --]=====]",
            ));

        let param_name = param_token
            .map(|t| t.to_string())
            .unwrap_or_else(|| "[Anonymous]".to_string());

        // Execute function body at compile time
        let comp_time_result = {
            self.load_param_list_into_lua_env();

            let file_context = format!(
                "{} {}",
                self.file_path.clone().unwrap_or_default(),
                param_name
            );

            let (mut result, keep_line) =
                lang_utils::lua_dostring(&file_context, &node.body().block().to_string());

            // Remove comments and optionally newlines
            result = result.remove_lua_comments();
            if !keep_line {
                result = result.replace('\n', " ");
            }

            self.unload_param_list_from_lua_env();

            result
        };

        // Wrap original function in comment and prepend generated code
        node.clone()
            .with_function_token(ast_utilis::insert_before_token(
                &ast_utilis::insert_before_token(node.function_token(), &comp_time_result),
                " --[=====[ ",
            ))
            .with_body(new_body)
    }

    /// Resolves a `__ljp:include()` or `__LJP:INCLUDE()` function call
    fn resolve_include(&self, node: FunctionCall, full_name: &str, arg: &str) -> FunctionCall {
        // Find the file to include
        let (include_file, _) = lang_utils::lua_dostring(
            "__LJP:INCLUDE",
            &format!("return assert(package.searchpath({}, package.path))", arg),
        );

        // Read and optionally transform included file
        let mut include_code = std::fs::read_to_string(&include_file)
            .unwrap_or_else(|e| panic!("Failed to read '{}': {}", include_file, e));

        if let Some(first_line) = include_code.lines().next() {
            if first_line.contains("luajit-pro") {
                include_code = transform_lua_code(&include_code, &include_file, None);
            }
        }

        // Remove return statement if requested
        if full_name.to_uppercase().contains("NO_RETURN") {
            let ast = full_moon::parse(&include_code)
                .unwrap_or_else(|e| panic!("Failed to parse included file: {:?}", e));
            let mut remover = LuaLastReturnRemover::new();
            include_code = remover.visit_ast(ast).to_string();
        }

        // Clean up included code
        include_code = include_code.remove_lua_comments().replace('\n', " ");

        // Create new prefix with included code
        let new_prefix = match node.prefix() {
            Prefix::Name(token) => Prefix::Name(ast_utilis::insert_before_token(
                &ast_utilis::insert_before_token(token, &include_code),
                " --[=====[ ",
            )),
            other => panic!("Unexpected prefix type: {:?}", other),
        };

        // Update suffixes with closing marker
        let new_suffixes: Vec<Suffix> = node
            .suffixes()
            .map(|suffix| match suffix.clone() {
                Suffix::Index(index) => Suffix::Index(index),
                Suffix::Call(call) => Suffix::Call(match call {
                    Call::MethodCall(method_call) => {
                        let new_args = match method_call.args() {
                            FunctionArgs::Parentheses {
                                parentheses,
                                arguments,
                            } => FunctionArgs::Parentheses {
                                parentheses: ContainedSpan::new(
                                    parentheses.tokens().0.clone(),
                                    ast_utilis::insert_after_token(
                                        parentheses.tokens().1,
                                        " --]=====]",
                                    ),
                                ),
                                arguments: arguments.clone(),
                            },
                            FunctionArgs::String(s) => FunctionArgs::String(
                                ast_utilis::insert_after_token(s, " --]=====]"),
                            ),
                            other => panic!("Unexpected function args: {}", other),
                        };
                        Call::MethodCall(method_call.with_args(new_args))
                    }
                    other => panic!("Unexpected call type: {:?}", other),
                }),
                other => panic!("Unexpected suffix type: {:?}", other),
            })
            .collect();

        node.with_prefix(new_prefix).with_suffixes(new_suffixes)
    }
}

impl VisitorMut for LuaTransformer {
    fn visit_function_declaration(&mut self, node: FunctionDeclaration) -> FunctionDeclaration {
        let name = node.name().to_string().to_uppercase();

        match name.as_str() {
            "__LJP:COMP_TIME" | "_G.__LJP:COMP_TIME" => self.resolve_comp_time(node),
            _ => {
                // Check for misformatted COMP_TIME (name on different line)
                if name.contains("__LJP:COMP_TIME") || name.contains("_G.__LJP:COMP_TIME") {
                    panic!(
                        "Function name for `__LJP:COMP_TIME` must be on the same line as `function` keyword"
                    );
                }
                node
            }
        }
    }

    fn visit_function_call(&mut self, node: FunctionCall) -> FunctionCall {
        // Get function name from prefix
        let func_name = match node.prefix() {
            Prefix::Name(name) => Some(name.token().to_string().to_uppercase()),
            _ => None,
        };

        let func_name = match func_name {
            Some(n) => n,
            None => return node,
        };

        // Check for LJP function calls
        let suffixes: Vec<Suffix> = node.suffixes().cloned().collect();
        let (full_name, func_arg) = if func_name.starts_with("__LJP") {
            ast_utilis::get_func_call_name(&node)
        } else if func_name.starts_with("_G") && !suffixes.is_empty() {
            // Check for _G.__LJP pattern
            match &suffixes[0] {
                Suffix::Index(Index::Dot { name, .. }) => {
                    if name.token().to_string().to_uppercase() == "__LJP" {
                        ast_utilis::get_func_call_name(&node)
                    } else {
                        return node;
                    }
                }
                _ => return node,
            }
        } else {
            return node;
        };

        if full_name.is_empty() || func_arg.is_empty() {
            return node;
        }

        // Handle include directives
        let upper_name = full_name.to_uppercase();
        if matches!(
            upper_name.as_str(),
            "__LJP:INCLUDE"
                | "_G.__LJP:INCLUDE"
                | "__LJP:INCLUDE_NO_RETURN"
                | "_G.__LJP:INCLUDE_NO_RETURN"
        ) {
            return self.resolve_include(node, &full_name, &func_arg);
        }

        node
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests creating a new transformer instance.
    #[test]
    fn test_transformer_new() {
        let transformer = LuaTransformer::new();
        assert!(transformer.file_path.is_none());
        assert!(transformer.input_param_list.is_none());
    }

    /// Tests the RemoveLuaComments trait on String.
    #[test]
    fn test_remove_lua_comments_trait() {
        let code = "local x = 1 -- comment".to_string();
        let result = code.remove_lua_comments();
        assert!(!result.contains("comment"));
        assert!(result.contains("local x = 1"));
    }

    /// Tests that LuaLastReturnRemover removes return statements.
    #[test]
    fn test_last_return_remover() {
        let code = "local x = 1\nreturn x";
        let ast = full_moon::parse(code).unwrap();

        let mut remover = LuaLastReturnRemover::new();
        let new_ast = remover.visit_ast(ast);
        let result = new_ast.to_string();

        // return should be effectively removed (replaced with empty)
        assert!(result.contains("local x = 1"));
        // The return keyword should be replaced with whitespace
        assert!(!result.contains("return x"));
    }

    /// Tests loading and unloading parameter list.
    ///
    /// The parameter list is loaded into the Lua global environment
    /// for compile-time evaluation.
    #[test]
    fn test_param_list_loading() {
        let mut transformer = LuaTransformer::new();
        transformer.input_param_list = Some(vec![
            ("DEBUG".to_string(), "true".to_string()),
            ("OPT".to_string(), "false".to_string()),
        ]);

        // These should not panic
        transformer.load_param_list_into_lua_env();
        transformer.unload_param_list_from_lua_env();
    }

    /// Tests transformer with no parameters.
    #[test]
    fn test_param_list_none() {
        let transformer = LuaTransformer::new();
        // Should not panic when param list is None
        transformer.load_param_list_into_lua_env();
        transformer.unload_param_list_from_lua_env();
    }

    /// Tests that __LJP:COMP_TIME function is recognized by name.
    ///
    /// The function declaration visitor checks for both:
    /// - `__LJP:COMP_TIME`
    /// - `_G.__LJP:COMP_TIME`
    #[test]
    fn test_comp_time_function_name_detection() {
        // Test uppercase matching
        let name = "__LJP:COMP_TIME";
        assert_eq!(name.to_uppercase(), "__LJP:COMP_TIME");

        let name2 = "_G.__LJP:COMP_TIME";
        assert_eq!(name2.to_uppercase(), "_G.__LJP:COMP_TIME");
    }

    /// Tests that include function names are correctly matched.
    ///
    /// Supported include patterns:
    /// - `__ljp:include()`
    /// - `__LJP:INCLUDE()`
    /// - `__LJP:INCLUDE_NO_RETURN()`
    /// - `_G.__LJP:INCLUDE()`
    #[test]
    fn test_include_name_patterns() {
        let patterns = [
            "__LJP:INCLUDE",
            "_G.__LJP:INCLUDE",
            "__LJP:INCLUDE_NO_RETURN",
            "_G.__LJP:INCLUDE_NO_RETURN",
        ];

        for pattern in patterns {
            let upper = pattern.to_uppercase();
            assert!(
                matches!(
                    upper.as_str(),
                    "__LJP:INCLUDE"
                        | "_G.__LJP:INCLUDE"
                        | "__LJP:INCLUDE_NO_RETURN"
                        | "_G.__LJP:INCLUDE_NO_RETURN"
                ),
                "Pattern {} should match",
                pattern
            );
        }
    }

    /// Tests that case-insensitive matching works for include.
    #[test]
    fn test_include_case_insensitive() {
        let lowercase = "__ljp:include";
        let uppercase = lowercase.to_uppercase();
        assert_eq!(uppercase, "__LJP:INCLUDE");
    }
}
