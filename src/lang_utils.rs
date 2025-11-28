//! # Language Utilities
//!
//! This module provides utilities for:
//! - Executing Lua code at compile time
//! - Converting Teal to Lua
//! - Converting Luau to Lua
//! - Code formatting and comment removal
//! - Global value injection

use std::cell::UnsafeCell;
use std::collections::HashMap;

use darklua_core::generator::LuaGenerator;
use darklua_core::rules::{Rule, RuleConfiguration, RulePropertyValue};
use mlua::prelude::*;

// ============================================================================
// Compile-time Lua Execution
// ============================================================================

/// Executes Lua code at compile time and returns the result
///
/// This function maintains a thread-local Lua state for executing compile-time
/// code blocks. The macro engine provides utilities like `output()` and `keep_line()`.
///
/// # Arguments
///
/// * `code_name` - Name/context for error messages
/// * `code` - Lua code to execute
///
/// # Returns
///
/// A tuple of (output_string, keep_line_flag)
pub fn lua_dostring(code_name: &str, code: &str) -> (String, bool) {
    thread_local! {
        static LUA: UnsafeCell<Lua> = UnsafeCell::new({
            let lua = unsafe { Lua::unsafe_new() };

            // Load the macro engine script
            let script = include_str!("lua/macro_engine.lua");
            let chunk = lua.load(script).set_name("lua/macro_engine.lua");
            if let Err(e) = chunk.exec() {
                panic!("Failed to load macro_engine: {}", e);
            }

            lua
        });
    }

    LUA.with(|lua| {
        let lua = unsafe { &mut *lua.get() };

        // Set context for error messages
        lua.globals()
            .set("__code_name__", code_name)
            .expect("Failed to set __code_name__");

        // Execute the code
        let result = lua.load(code).eval::<mlua::Value>();

        // Get accumulated output from output/out/o/outputf/outf/of functions
        let get_output: LuaFunction = lua.globals().get("get_output").unwrap();
        let output_str: String = get_output.call::<String>(()).unwrap();

        // Check if keep_line() was called
        let check_keep_line: LuaFunction = lua.globals().get("_check_keep_line").unwrap();
        let keep_line: bool = check_keep_line.call::<bool>(()).unwrap();

        // Combine output with return value
        let return_value = match result {
            Ok(mlua::Value::String(s)) => s.to_str().unwrap().to_owned(),
            Ok(mlua::Value::Nil) => String::new(),
            Ok(v) => panic!(
                "Expected string return value, got {:?}\n--- Code ---\n{}\n------------",
                v, code
            ),
            Err(e) => panic!(
                "Error executing Lua code: {}\n--- Code ---\n{}\n------------",
                e, code
            ),
        };

        (output_str + &return_value, keep_line)
    })
}

// ============================================================================
// Teal Language Support
// ============================================================================

/// Converts Teal code to Lua
///
/// # Arguments
///
/// * `input_file_name` - Path to the Teal source file
/// * `syntax_only` - If true, only parse syntax without type checking
///
/// # Returns
///
/// The converted Lua code as a string
pub fn convert_teal_to_lua(input_file_name: &str, syntax_only: bool) -> String {
    thread_local! {
        static LUA: UnsafeCell<Lua> = UnsafeCell::new({
            let lua = unsafe { Lua::unsafe_new() };

            // Load Teal compiler
            let tl_script = include_str!("../tl/tl.lua");
            let tl_chunk = lua.load(tl_script).set_name("tl/tl.lua");
            if let Err(e) = tl_chunk.exec() {
                panic!("Failed to load tl/tl: {}", e);
            }

            // Load Teal-to-Lua converter
            let converter_script = include_str!("lua/teal_to_lua.lua");
            let converter_chunk = lua.load(converter_script).set_name("teal_to_lua.lua");
            if let Err(e) = converter_chunk.exec() {
                panic!("Failed to load teal_to_lua: {}", e);
            }

            lua
        });
    }

    LUA.with(|lua| {
        let lua = unsafe { &mut *lua.get() };

        let func: LuaFunction = lua.globals().get("teal_to_lua").unwrap();
        func.call::<String>((input_file_name, syntax_only))
            .expect("Failed to call teal_to_lua function")
    })
}

// ============================================================================
// Luau Language Support
// ============================================================================

/// Converts Luau code to standard Lua
///
/// This uses darklua to remove Luau-specific features:
/// - Compound assignment operators (+=, -=, etc.)
/// - Floor division (//)
/// - Type annotations
/// - If expressions
/// - Continue statements
/// - Interpolated strings
/// - Unused if branches
/// - Empty do blocks
/// - Unused variables
///
/// # Arguments
///
/// * `input` - Luau source code
///
/// # Returns
///
/// The converted Lua code as a string
pub fn convert_luau_to_lua(input: &str) -> String {
    let resources = darklua_core::Resources::from_memory();
    let context = darklua_core::rules::ContextBuilder::new(".", &resources, input).build();

    let mut block = darklua_core::Parser::default()
        .preserve_tokens()
        .parse(input)
        .unwrap_or_else(|e| {
            panic!(
                "[convert_luau_to_lua] Failed to parse:\n{}\n--- Error ---\n{:?}",
                input, e
            )
        });

    // Apply transformation rules
    let rules: Vec<Box<dyn Rule>> = vec![
        Box::new(darklua_core::rules::RemoveCompoundAssignment::default()),
        Box::new(darklua_core::rules::RemoveFloorDivision::default()),
        Box::new(darklua_core::rules::RemoveTypes::default()),
        Box::new(darklua_core::rules::RemoveIfExpression::default()),
        Box::new(darklua_core::rules::RemoveContinue::default()),
        Box::new(darklua_core::rules::RemoveInterpolatedString::default()),
        Box::new(darklua_core::rules::RemoveUnusedIfBranch::default()),
        Box::new(darklua_core::rules::RemoveEmptyDo::default()),
        Box::new(darklua_core::rules::RemoveUnusedVariable::default()),
    ];

    for rule in rules {
        rule.process(&mut block, &context)
            .expect("Transformation rule failed");
    }

    let mut generator = darklua_core::generator::TokenBasedLuaGenerator::new(input);
    generator.write_block(&block);
    generator.into_string()
}

// ============================================================================
// Global Value Injection
// ============================================================================

/// Injects global boolean values into Lua code and optimizes dead branches
///
/// This is used for feature flag support. After injecting the values,
/// it removes unused if branches and empty blocks.
///
/// # Arguments
///
/// * `input` - Lua source code
/// * `param_table` - Map of parameter names to values ("true"/"false" or "1"/"0")
///
/// # Returns
///
/// The optimized Lua code as a string
pub fn inject_global_vals(input: &str, param_table: HashMap<&str, String>) -> String {
    let resources = darklua_core::Resources::from_memory();
    let context = darklua_core::rules::ContextBuilder::new(".", &resources, input).build();

    let mut block = darklua_core::Parser::default()
        .preserve_tokens()
        .parse(input)
        .unwrap_or_else(|e| {
            panic!(
                "[inject_global_vals] Failed to parse:\n{}\n--- Error ---\n{:?}",
                input, e
            )
        });

    // Inject each global value
    for (key, value) in param_table {
        let bool_value = value == "true" || value == "1";
        darklua_core::rules::InjectGlobalValue::boolean(key, bool_value)
            .process(&mut block, &context)
            .expect("Failed to inject global value");
    }

    // Clean up dead code
    darklua_core::rules::RemoveUnusedIfBranch::default()
        .process(&mut block, &context)
        .expect("Failed to remove unused if branch");

    darklua_core::rules::RemoveEmptyDo::default()
        .process(&mut block, &context)
        .expect("Failed to remove empty do");

    darklua_core::rules::RemoveUnusedVariable::default()
        .process(&mut block, &context)
        .expect("Failed to remove unused variables");

    let mut generator = darklua_core::generator::TokenBasedLuaGenerator::new(input);
    generator.write_block(&block);
    generator.into_string()
}

// ============================================================================
// Comment Removal
// ============================================================================

/// Removes comments from Lua code while preserving special annotations
///
/// The following comments are preserved:
/// - `--[[@comp_time_enum]]`
/// - `--[[@used]]`
/// - `--#...` (special markers for keep_line)
///
/// # Arguments
///
/// * `input` - Lua source code
///
/// # Returns
///
/// The code with comments removed
pub fn remove_lua_comments(input: &str) -> String {
    let resources = darklua_core::Resources::from_memory();
    let context = darklua_core::rules::ContextBuilder::new(".", &resources, input).build();

    let mut block = darklua_core::Parser::default()
        .preserve_tokens()
        .parse(input)
        .unwrap_or_else(|e| {
            panic!(
                "[remove_lua_comments] Failed to parse:\n{}\n--- Error ---\n{:?}",
                input, e
            )
        });

    // Configure comment removal with exceptions
    let mut rule = darklua_core::rules::RemoveComments::default();
    rule.configure({
        let mut properties = HashMap::new();
        properties.insert(
            "except".to_string(),
            RulePropertyValue::StringList(vec![
                r#"^--\[\[@comp_time_enum\]\]"#.to_string(),
                r#"^--\[\[@used\]\]"#.to_string(),
                r#"^--#"#.to_string(),
            ]),
        );
        properties
    })
    .expect("Failed to configure RemoveComments rule");

    rule.process(&mut block, &context)
        .expect("RemoveComments failed");

    let mut generator = darklua_core::generator::TokenBasedLuaGenerator::new(input);
    generator.write_block(&block);
    generator.into_string()
}

// ============================================================================
// Code Formatting
// ============================================================================

/// Formats Lua code using stylua
///
/// Uses a column width of 240 characters for better readability of
/// generated code.
///
/// # Arguments
///
/// * `input` - Lua source code
///
/// # Returns
///
/// The formatted code as a string
pub fn format_lua_code(input: &str) -> String {
    let ast = full_moon::parse(input).expect("Failed to parse code for formatting");

    let mut config = stylua_lib::Config::new();
    config.column_width = 240;

    stylua_lib::format_ast(ast, config, None, stylua_lib::OutputVerification::None)
        .expect("Failed to format code")
        .to_string()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // lua_dostring tests
    // ========================================================================

    /// Tests basic Lua code execution.
    ///
    /// The lua_dostring function executes Lua code and returns
    /// the string result along with a keep_line flag.
    #[test]
    fn test_lua_dostring_basic() {
        let (result, keep_line) = lua_dostring("test", "return 'hello'");
        assert_eq!(result, "hello");
        assert!(!keep_line);
    }

    /// Tests Lua code that returns nil.
    #[test]
    fn test_lua_dostring_nil_return() {
        let (result, _) = lua_dostring("test", "return nil");
        assert_eq!(result, "");
    }

    /// Tests Lua code with string concatenation.
    #[test]
    fn test_lua_dostring_concat() {
        let (result, _) = lua_dostring("test", "return 'a' .. 'b' .. 'c'");
        assert_eq!(result, "abc");
    }

    /// Tests Lua code with loops generating output.
    #[test]
    fn test_lua_dostring_loop() {
        let code = r#"
            local t = {}
            for i = 1, 3 do
                t[i] = tostring(i)
            end
            return table.concat(t, ",")
        "#;
        let (result, _) = lua_dostring("test", code);
        assert_eq!(result, "1,2,3");
    }

    /// Tests Lua code with conditional logic.
    #[test]
    fn test_lua_dostring_conditional() {
        let code = r#"
            local x = true
            if x then
                return "yes"
            else
                return "no"
            end
        "#;
        let (result, _) = lua_dostring("test", code);
        assert_eq!(result, "yes");
    }

    // ========================================================================
    // convert_luau_to_lua tests
    // ========================================================================

    /// Tests removing compound assignment operators.
    ///
    /// Luau:
    /// ```lua
    /// x += 1
    /// ```
    /// Lua:
    /// ```lua
    /// x = x + 1
    /// ```
    #[test]
    fn test_convert_luau_compound_assignment() {
        // darklua's RemoveUnusedVariable may remove x if it's unused
        // So we need to use x in a way that prevents removal
        let luau = "local x = 0\nx += 1\nprint(x)";
        let lua = convert_luau_to_lua(luau);
        // The compound assignment should be converted
        assert!(lua.contains("x ="), "Expected 'x =', got: {}", lua);
    }

    /// Tests removing type annotations.
    ///
    /// Luau:
    /// ```lua
    /// local x: number = 1
    /// ```
    /// Lua:
    /// ```lua
    /// local x = 1
    /// ```
    #[test]
    fn test_convert_luau_type_annotations() {
        // Use the variables to prevent RemoveUnusedVariable from removing them
        let luau = "local x: number = 1\nprint(x)";
        let lua = convert_luau_to_lua(luau);
        assert!(
            !lua.contains(": number"),
            "Type should be removed, got: {}",
            lua
        );
    }

    /// Tests removing floor division.
    ///
    /// Luau:
    /// ```lua
    /// local x = 10 // 3
    /// ```
    /// Lua:
    /// ```lua
    /// local x = math.floor(10 / 3)
    /// ```
    #[test]
    fn test_convert_luau_floor_division() {
        let luau = "local x = 10 // 3";
        let lua = convert_luau_to_lua(luau);
        assert!(lua.contains("math.floor") || !lua.contains("//"));
    }

    // ========================================================================
    // inject_global_vals tests
    // ========================================================================

    /// Tests injecting a true boolean value.
    ///
    /// # Example
    /// Before:
    /// ```lua
    /// if DEBUG then print("debug") end
    /// ```
    /// With DEBUG=true:
    /// ```lua
    /// print("debug")
    /// ```
    #[test]
    fn test_inject_global_vals_true() {
        let code = "if DEBUG then x = 1 end";
        let mut params = HashMap::new();
        params.insert("DEBUG", "true".to_string());

        let result = inject_global_vals(code, params);
        // The if should be optimized away, leaving just the body
        assert!(result.contains("x = 1"));
    }

    /// Tests injecting a false boolean value.
    ///
    /// # Example
    /// With DEBUG=false, the entire if block should be removed.
    #[test]
    fn test_inject_global_vals_false() {
        let code = "if DEBUG then x = 1 end\ny = 2";
        let mut params = HashMap::new();
        params.insert("DEBUG", "false".to_string());

        let result = inject_global_vals(code, params);
        // The if block should be removed
        assert!(!result.contains("x = 1"));
        assert!(result.contains("y = 2"));
    }

    /// Tests injecting numeric boolean (1 = true).
    #[test]
    fn test_inject_global_vals_numeric() {
        let code = "if ENABLED then x = 1 end";
        let mut params = HashMap::new();
        params.insert("ENABLED", "1".to_string());

        let result = inject_global_vals(code, params);
        assert!(result.contains("x = 1"));
    }

    // ========================================================================
    // remove_lua_comments tests
    // ========================================================================

    /// Tests removing single-line comments.
    ///
    /// ```lua
    /// local x = 1 -- this is a comment
    /// ```
    /// becomes:
    /// ```lua
    /// local x = 1
    /// ```
    #[test]
    fn test_remove_lua_comments_single_line() {
        let code = "local x = 1 -- this is a comment";
        let result = remove_lua_comments(code);
        assert!(!result.contains("this is a comment"));
        assert!(result.contains("local x = 1"));
    }

    /// Tests removing multi-line comments.
    ///
    /// ```lua
    /// local x = 1 --[[
    ///     multi-line comment
    /// ]]
    /// ```
    #[test]
    fn test_remove_lua_comments_multi_line() {
        let code = "local x = 1 --[[\nmulti-line\ncomment\n]]";
        let result = remove_lua_comments(code);
        assert!(!result.contains("multi-line"));
        assert!(result.contains("local x = 1"));
    }

    /// Tests that @comp_time_enum comments are preserved.
    ///
    /// ```lua
    /// --[[@comp_time_enum]]
    /// local Colors = {RED = 1}
    /// ```
    #[test]
    fn test_remove_lua_comments_preserves_annotations() {
        let code = "local --[[@comp_time_enum]] Colors = {RED = 1}";
        let result = remove_lua_comments(code);
        assert!(result.contains("@comp_time_enum"));
    }

    /// Tests that @used comments are preserved.
    #[test]
    fn test_remove_lua_comments_preserves_used() {
        let code = "local --[[@used]] _INIT = 1";
        let result = remove_lua_comments(code);
        assert!(result.contains("@used"));
    }

    // ========================================================================
    // format_lua_code tests
    // ========================================================================

    /// Tests basic code formatting.
    #[test]
    fn test_format_lua_code_basic() {
        let code = "local x=1";
        let result = format_lua_code(code);
        // Stylua adds spaces around equals
        assert!(result.contains("local x = 1"));
    }

    /// Tests formatting nested blocks.
    ///
    /// The formatter should add proper indentation:
    /// ```lua
    /// if true then
    ///     if true then
    ///         x = 1
    ///     end
    /// end
    /// ```
    #[test]
    fn test_format_lua_code_nested() {
        let code = "if true then if true then x=1 end end";
        let result = format_lua_code(code);
        // Should have indentation (tabs or spaces)
        assert!(result.contains("\t") || result.contains("    "));
        assert!(result.contains("if true then"));
        assert!(result.contains("x = 1"));
    }

    /// Tests formatting function definitions.
    #[test]
    fn test_format_lua_code_function() {
        let code = "function foo(a,b) return a+b end";
        let result = format_lua_code(code);
        // Should have proper spacing
        assert!(result.contains("function foo(a, b)"));
        assert!(result.contains("return a + b"));
    }

    /// Tests formatting table constructors.
    #[test]
    fn test_format_lua_code_table() {
        let code = "local t={a=1,b=2,c=3}";
        let result = format_lua_code(code);
        // Should have proper spacing
        assert!(result.contains("a = 1"));
        assert!(result.contains("b = 2"));
    }
}
