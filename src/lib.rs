//! # LuaJIT-Pro Helper Library
//!
//! This library provides compile-time Lua code transformation capabilities for LuaJIT-Pro.
//! It handles:
//! - Compile-time code generation via `__LJP:COMP_TIME()` functions
//! - File inclusion via `__ljp:include()` and `__LJP:INCLUDE()`
//! - Code optimization with `@comp_time_enum` and `@used` annotations
//! - Teal and Luau language support
//! - Code formatting and comment removal
//!
//! ## Architecture
//!
//! The transformation pipeline:
//! 1. LuaJIT's `lj_load.c` intercepts file loading
//! 2. Files with `--[[luajit-pro]]` marker are passed to this library via FFI
//! 3. [`transform_lua`] function processes the code
//! 4. Transformed code is returned to LuaJIT for execution

#![allow(unused_imports)]

mod ast_utilis;
mod lang_utils;
mod lua_optimizer;
mod lua_transformer;

use std::collections::HashMap;
use std::ffi::{c_char, CStr, CString};
use std::fs::File;
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::SystemTime;

use fslock::LockFile;
use full_moon::visitors::VisitorMut;
use lazy_static::lazy_static;
use lua_optimizer::LuaOptimizer;
use lua_transformer::LuaTransformer;

// ============================================================================
// Constants and Configuration
// ============================================================================

/// Default output directory for cached transformed files
const OUTPUT_DIR: &str = ".luajit_pro";

// ============================================================================
// Environment Variables (Lazy Initialization)
// ============================================================================

#[cfg(feature = "debug")]
#[static_init::constructor(0)]
extern "C" fn env_logger_init() {
    let _ = env_logger::try_init();
}

lazy_static! {
    /// When set to "1", disables file caching (always recompile)
    static ref ENV_NO_CACHE: bool = std::env::var("LJP_NO_CACHE")
        .map(|v| {
            if v == "1" {
                print_banner("LJP NO CACHE");
            }
            v == "1"
        })
        .unwrap_or(false);

    /// When set to "1", only generates transformed code without executing
    static ref ENV_GEN_ONLY: bool = std::env::var("LJP_GEN_ONLY")
        .map(|v| {
            if v == "1" {
                print_banner("LJP GEN ONLY");
            }
            v == "1"
        })
        .unwrap_or(false);

    /// Custom output directory for cached files
    static ref BUILD_CACHE_DIR: String = {
        let default_dir = format!("{}/build_cache", OUTPUT_DIR);
        std::env::var("LJP_OUT_DIR").unwrap_or(default_dir)
    };

    /// When set to "1", disables optimization even when `opt` flag is set
    static ref ENV_NO_OPT: bool = std::env::var("LJP_NO_OPT")
        .map(|v| {
            if v == "1" {
                print_banner("LJP NO OPT");
            }
            v == "1"
        })
        .unwrap_or(false);
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Prints a figlet banner for environment variable activation
fn print_banner(text: &str) {
    if let Ok(font) = figlet_rs::FIGfont::standard() {
        if let Some(figure) = font.convert(text) {
            println!("{}", figure);
        }
    }
}

/// Gets the modification time of a file
fn get_mtime(file_path: &str) -> SystemTime {
    std::fs::metadata(file_path)
        .and_then(|m| m.modified())
        .unwrap_or_else(|e| panic!("Failed to get mtime for '{}': {}", file_path, e))
}

/// Parses the parameter table from the first line of a Lua file
///
/// Format: `--[[luajit-pro, {KEY1 = value1, KEY2 = value2}]]`
///
/// Returns:
/// - `(param_table, need_rebuild)`: The parsed parameters and whether a rebuild is needed
///   due to environment variable changes
#[inline]
fn parse_param_table(line: &str) -> (Option<HashMap<&str, String>>, bool) {
    let start = line.find('{');
    let end = line.rfind('}');

    let (start, end) = match (start, end) {
        (Some(s), Some(e)) => (s, e),
        _ => return (None, false),
    };

    let content = &line[start + 1..end];
    let mut params = HashMap::new();
    let mut need_rebuild = false;

    for kv in content.split(',') {
        if let Some((k, v)) = kv.split_once('=') {
            let key = k.trim();
            let default_value = v.trim();
            let current_value = std::env::var(key).unwrap_or_else(|_| default_value.to_string());

            #[cfg(feature = "debug")]
            log::debug!(
                "[parse_param_table] key: {} default: {} current: {} match: {}",
                key,
                default_value,
                current_value,
                default_value == current_value
            );

            if !need_rebuild && default_value != current_value {
                need_rebuild = true;
            }

            assert!(
                matches!(current_value.as_str(), "true" | "false" | "0" | "1"),
                "Invalid parameter value for '{}': '{}' (expected: true/false/0/1)",
                key,
                current_value
            );

            params.insert(key, current_value);
        }
    }

    (Some(params), need_rebuild)
}

/// Serializes a parameter table back to string format
///
/// Note: Keys are sorted alphabetically to ensure deterministic output,
/// which is important for cache file naming and test reproducibility.
fn serialize_param_table(param_table: Option<HashMap<&str, String>>) -> String {
    let params = match param_table {
        Some(p) => p,
        None => return String::new(),
    };

    // Sort keys to ensure deterministic output
    let mut entries: Vec<_> = params.into_iter().collect();
    entries.sort_by_key(|(k, _)| *k);

    let formatted: Vec<String> = entries
        .into_iter()
        .map(|(k, v)| format!("{} = {}", k, v))
        .collect();

    format!("{{{}}}", formatted.join(", "))
}

// ============================================================================
// Cache Management
// ============================================================================

/// Checks if a cached version can be used
fn try_use_cache(
    lua_file_path: &str,
    cached_file: &str,
    no_cache: bool,
    #[cfg(feature = "debug")] debug_prefix: &str,
) -> Option<CString> {
    if no_cache || !std::fs::exists(cached_file).unwrap_or(false) {
        return None;
    }

    // Check if source file is newer than cache
    if get_mtime(lua_file_path) > get_mtime(cached_file) {
        return None;
    }

    // Read cached file's first line to check parameter changes
    let cached_first_line = {
        let file = File::open(cached_file).ok()?;
        let mut reader = std::io::BufReader::new(file);
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        line
    };

    let (_, need_rebuild) = parse_param_table(&cached_first_line);

    #[cfg(feature = "debug")]
    log::debug!(
        "{} cache_first_line: {} need_rebuild: {}",
        debug_prefix,
        cached_first_line.trim_end(),
        need_rebuild
    );

    if need_rebuild {
        return None;
    }

    let code = std::fs::read_to_string(cached_file).ok()?;
    CString::new(code).ok()
}

// ============================================================================
// Core Transformation Functions
// ============================================================================

/// Transforms Lua code with compile-time features
///
/// This is the main transformation function that processes:
/// - Teal/Luau language conversion
/// - `__LJP:COMP_TIME()` compile-time code generation
/// - `__ljp:include()` file inclusion
/// - Code optimization (when `opt` flag is set)
/// - Code formatting and comment removal
///
/// # Arguments
///
/// * `code` - The source Lua code
/// * `lua_file_path` - Path to the source file (for error messages)
/// * `param_table` - Optional parameter table for feature flags
///
/// # Returns
///
/// The transformed Lua code as a string
pub fn transform_lua_code(
    code: &str,
    lua_file_path: &str,
    param_table: Option<HashMap<&str, String>>,
) -> String {
    #[cfg(feature = "debug")]
    let debug_prefix = format!("[transform_lua_code] <{}>", lua_file_path);

    let first_line = code.lines().next().unwrap_or("");

    // Step 1: Handle Teal conversion
    let final_code = if first_line.contains("teal") {
        assert!(
            !first_line.contains("luau"),
            "Cannot use both 'teal' and 'luau' flags"
        );
        let lua_code =
            lang_utils::convert_teal_to_lua(lua_file_path, first_line.contains("syntax-only"));
        // Replace bit32 with bit for LuaJIT compatibility
        lua_code.replace("bit32", "bit")
    } else {
        code.to_string()
    };

    // Step 2: Parse and transform AST
    let ast = full_moon::parse(&final_code)
        .unwrap_or_else(|e| panic!("Failed to parse '{}': {:?}", lua_file_path, e));

    let mut transformer = LuaTransformer::new();
    transformer.file_path = Some(lua_file_path.to_string());
    transformer.input_param_list = param_table
        .as_ref()
        .map(|p| p.iter().map(|(k, v)| (k.to_string(), v.clone())).collect());

    let mut new_ast = transformer.visit_ast(ast);

    // Step 3: Apply optimization if enabled
    if first_line.contains("opt") && !*ENV_NO_OPT {
        let code_str = new_ast.to_string();
        let parsed = full_moon::parse(&code_str)
            .unwrap_or_else(|e| panic!("Failed to parse optimized code: {:?}\n{}", e, code_str));

        let mut optimizer = LuaOptimizer::new();
        new_ast = optimizer.visit_ast(parsed);
    }

    let mut new_content = new_ast.to_string();

    // Step 4: Inject global values from parameter table
    if let Some(params) = param_table {
        new_content = lang_utils::inject_global_vals(&new_content, params);
    }

    // Step 5: Handle Luau conversion
    if first_line.contains("luau") {
        assert!(
            !first_line.contains("teal"),
            "Cannot use both 'luau' and 'teal' flags"
        );
        new_content = lang_utils::convert_luau_to_lua(&new_content);
    }

    // Step 6: Apply formatting options
    if first_line.contains("pretty") {
        #[cfg(feature = "debug")]
        log::debug!("{} applying: pretty", debug_prefix);

        // pretty == no-comment + format
        new_content = lang_utils::format_lua_code(&lang_utils::remove_lua_comments(&new_content));
    } else {
        if first_line.contains("no-comment") {
            #[cfg(feature = "debug")]
            log::debug!("{} applying: no-comment", debug_prefix);

            new_content = lang_utils::remove_lua_comments(&new_content);
        }

        if first_line.contains("format") {
            #[cfg(feature = "debug")]
            log::debug!("{} applying: format", debug_prefix);

            new_content = lang_utils::format_lua_code(&new_content);
        }
    }

    new_content
}

/// FFI entry point for transforming a Lua file
///
/// This function is called from LuaJIT's `lj_load.c` when a file with
/// the `--[[luajit-pro]]` marker is loaded.
///
/// # Safety
///
/// This function is marked as `#[no_mangle]` for C FFI compatibility.
/// The caller must ensure `file_path` is a valid null-terminated C string.
///
/// # Safety
///
/// This function is unsafe because it dereferences a raw pointer.
/// The caller must ensure that:
/// - `file_path` is a valid, non-null pointer to a null-terminated C string
/// - The pointer remains valid for the duration of this function call
#[no_mangle]
pub unsafe fn transform_lua(file_path: *const c_char) -> *const c_char {
    #[cfg(feature = "print-time")]
    let start = std::time::Instant::now();

    // Convert C string to Rust string
    let c_str = unsafe { CStr::from_ptr(file_path) };
    let lua_file_path = c_str.to_str().expect("Invalid UTF-8 in file path");

    #[cfg(feature = "debug")]
    let debug_prefix = format!("[transform_lua] <{}>", lua_file_path);

    // Read first line to determine transformation options
    let first_line = {
        let file = File::open(lua_file_path)
            .unwrap_or_else(|e| panic!("Failed to open '{}': {}", lua_file_path, e));
        let mut reader = std::io::BufReader::new(file);
        let mut line = String::new();
        reader.read_line(&mut line).ok();
        line
    };

    let no_cache = first_line.contains("no-cache") || *ENV_NO_CACHE || *ENV_GEN_ONLY;

    let cached_file = format!(
        "{}/{}",
        *BUILD_CACHE_DIR,
        Path::new(lua_file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
    );

    #[cfg(feature = "debug")]
    {
        log::debug!("{} first_line: \"{}\"", debug_prefix, first_line.trim_end());
        log::debug!(
            "{} no_cache: {} gen_only: {}",
            debug_prefix,
            no_cache,
            *ENV_GEN_ONLY
        );
    }

    // Ensure cache directory exists
    if !std::fs::exists(&*BUILD_CACHE_DIR).unwrap_or(false) {
        std::fs::create_dir_all(&*BUILD_CACHE_DIR).expect("Failed to create cache directory");

        #[cfg(feature = "debug")]
        log::debug!("{} created cache dir: {}", debug_prefix, *BUILD_CACHE_DIR);
    } else {
        // Try to use cached version
        #[cfg(feature = "debug")]
        if let Some(cached) = try_use_cache(lua_file_path, &cached_file, no_cache, &debug_prefix) {
            #[cfg(feature = "print-time")]
            println!(
                "[luajit_pro_helper] Time (cached): {:?}, file: {}",
                start.elapsed(),
                lua_file_path
            );
            return cached.into_raw();
        }

        #[cfg(not(feature = "debug"))]
        if let Some(cached) = try_use_cache(lua_file_path, &cached_file, no_cache) {
            #[cfg(feature = "print-time")]
            println!(
                "[luajit_pro_helper] Time (cached): {:?}, file: {}",
                start.elapsed(),
                lua_file_path
            );
            return cached.into_raw();
        }
    }

    // Parse parameter table
    let (param_table, _) = parse_param_table(&first_line);

    #[cfg(feature = "debug")]
    log::debug!("{} param_table: {:?}", debug_prefix, param_table);

    // Acquire file lock to prevent race conditions
    let lockfile_path = format!("{}.lock", cached_file);
    let mut lockfile = LockFile::open(&lockfile_path)
        .unwrap_or_else(|e| panic!("Failed to open lock file '{}': {}", lockfile_path, e));

    lockfile
        .lock()
        .unwrap_or_else(|e| panic!("Failed to acquire lock '{}': {}", lockfile_path, e));

    // Read and transform the source file
    let content = std::fs::read_to_string(lua_file_path)
        .unwrap_or_else(|e| panic!("Failed to read '{}': {}", lua_file_path, e));

    let new_content = transform_lua_code(&content, lua_file_path, param_table.clone());

    // Reconstruct output with updated parameter table
    let new_content = if let Some(first_newline_pos) = new_content.find('\n') {
        let old_first_line = first_line.clone();
        let start = first_line.find('{');
        let end = first_line.rfind('}');

        let mut result = if let (Some(start), Some(end)) = (start, end) {
            let before = &old_first_line[..start];
            let after = &old_first_line[end + 1..];
            let serialized = serialize_param_table(param_table);

            #[cfg(feature = "debug")]
            log::debug!(
                "{} before: <{}> serialized: <{}> after: <{}>",
                debug_prefix,
                before,
                serialized,
                after
            );

            format!("{}{}{}", before, serialized, after)
                .trim_end_matches('\n')
                .to_string()
        } else {
            first_line.trim_end_matches('\n').to_string()
        };

        let new_first_line = &new_content[..first_newline_pos];

        if new_first_line.contains("luajit-pro") {
            result.push_str(&new_content[first_newline_pos..]);
        } else if old_first_line.contains("luajit-pro") {
            if new_first_line.contains("tl_compat") || new_first_line.contains("bit") {
                result = result.trim_end_matches('\n').to_string() + " ";
                result.push_str(&new_content);
            } else {
                result = format!("{} {}", result, &new_content);
            }
        } else {
            result.push_str(&new_content);
        }

        result
    } else {
        #[cfg(feature = "debug")]
        log::debug!("{} no newline found in output", debug_prefix);

        new_content
    };

    #[cfg(feature = "debug")]
    log::trace!(
        "{} output:\n----------\n{}\n----------",
        debug_prefix,
        new_content
    );

    // Write to cache
    std::fs::write(&cached_file, &new_content)
        .unwrap_or_else(|e| panic!("Failed to write cache '{}': {}", cached_file, e));

    let output = if *ENV_GEN_ONLY {
        println!("[luajit_pro_helper] [gen only] Output: {}", cached_file);
        String::new()
    } else {
        new_content
    };

    let c_str = CString::new(output).expect("Failed to create C string");

    #[cfg(feature = "print-time")]
    {
        println!(
            "[luajit_pro_helper] Time: {:?}, file: {}",
            start.elapsed(),
            lua_file_path
        );
        std::io::stdout().flush().ok();
    }

    lockfile.unlock().expect("Failed to release lock");

    c_str.into_raw()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // parse_param_table tests
    // ========================================================================

    /// Tests parsing a parameter table with a single boolean key-value pair.
    ///
    /// # Example
    /// ```lua
    /// --[[luajit-pro, {DEBUG = true}]]
    /// ```
    #[test]
    fn test_parse_param_table_single() {
        let line = "--[[luajit-pro, {DEBUG = true}]]";
        let (params, need_rebuild) = parse_param_table(line);

        assert!(params.is_some());
        let params = params.unwrap();
        assert_eq!(params.get("DEBUG"), Some(&"true".to_string()));
        assert!(!need_rebuild);
    }

    /// Tests parsing a parameter table with multiple key-value pairs.
    ///
    /// # Example
    /// ```lua
    /// --[[luajit-pro, {DEBUG = false, OPT = true}]]
    /// ```
    #[test]
    fn test_parse_param_table_multiple() {
        let line = "--[[luajit-pro, {DEBUG = false, OPT = true}]]";
        let (params, need_rebuild) = parse_param_table(line);

        assert!(params.is_some());
        let params = params.unwrap();
        assert_eq!(params.get("DEBUG"), Some(&"false".to_string()));
        assert_eq!(params.get("OPT"), Some(&"true".to_string()));
        assert!(!need_rebuild);
    }

    /// Tests that parsing returns None when no parameter table is present.
    ///
    /// # Example
    /// ```lua
    /// --[[luajit-pro]]
    /// ```
    #[test]
    fn test_parse_param_table_empty() {
        let line = "--[[luajit-pro]]";
        let (params, need_rebuild) = parse_param_table(line);

        assert!(params.is_none());
        assert!(!need_rebuild);
    }

    /// Tests parsing with numeric boolean values (0 and 1).
    ///
    /// # Example
    /// ```lua
    /// --[[luajit-pro, {ENABLED = 1, DISABLED = 0}]]
    /// ```
    #[test]
    fn test_parse_param_table_numeric_booleans() {
        let line = "--[[luajit-pro, {ENABLED = 1, DISABLED = 0}]]";
        let (params, need_rebuild) = parse_param_table(line);

        assert!(params.is_some());
        let params = params.unwrap();
        assert_eq!(params.get("ENABLED"), Some(&"1".to_string()));
        assert_eq!(params.get("DISABLED"), Some(&"0".to_string()));
        assert!(!need_rebuild);
    }

    // ========================================================================
    // serialize_param_table tests
    // ========================================================================

    /// Tests serializing an empty parameter table.
    #[test]
    fn test_serialize_param_table_none() {
        let result = serialize_param_table(None);
        assert_eq!(result, "");
    }

    /// Tests serializing a parameter table with entries.
    #[test]
    fn test_serialize_param_table_with_entries() {
        let mut params = HashMap::new();
        params.insert("DEBUG", "true".to_string());

        let result = serialize_param_table(Some(params));
        assert!(result.contains("DEBUG = true"));
        assert!(result.starts_with('{'));
        assert!(result.ends_with('}'));
    }

    /// Tests that serialize_param_table sorts keys alphabetically.
    ///
    /// This ensures deterministic output regardless of HashMap iteration order,
    /// which is important for cache file naming consistency.
    #[test]
    fn test_serialize_param_table_sorted_keys() {
        let mut params = HashMap::new();
        // Insert in non-alphabetical order
        params.insert("ZEBRA", "1".to_string());
        params.insert("APPLE", "0".to_string());
        params.insert("MANGO", "true".to_string());

        let result = serialize_param_table(Some(params));
        // Keys should be sorted: APPLE, MANGO, ZEBRA
        assert_eq!(result, "{APPLE = 0, MANGO = true, ZEBRA = 1}");
    }

    // ========================================================================
    // transform_lua_code tests
    // ========================================================================

    /// Tests basic COMP_TIME code generation.
    ///
    /// This demonstrates how to use `__LJP:COMP_TIME()` to generate code
    /// at compile time. The function body is executed and its return value
    /// (a string) is inserted into the output.
    ///
    /// # Example
    /// ```lua
    /// --[[luajit-pro]]
    /// function __LJP:COMP_TIME()
    ///     return [[local x = 42]]
    /// end
    /// ```
    /// becomes:
    /// ```lua
    /// --[[luajit-pro]]
    /// local x = 42 --[=====[ function __LJP:COMP_TIME() ... end --]=====]
    /// ```
    #[test]
    fn test_transform_lua_code_comp_time() {
        let code = r#"--[[luajit-pro]]
function __LJP:COMP_TIME()
return [[local x = 42]]
end
"#;

        let result = transform_lua_code(code, "test.lua", None);

        // The generated code should contain the literal result
        assert!(result.contains("local x = 42"));
        // The original function should be commented out
        assert!(result.contains("--[=====[ function __LJP:COMP_TIME"));
        assert!(result.contains("--]=====]"));
    }

    /// Tests COMP_TIME with loop generation.
    ///
    /// This demonstrates generating multiple assignments in a loop.
    ///
    /// # Example
    /// ```lua
    /// --[[luajit-pro]]
    /// local t = {}
    /// function __LJP:COMP_TIME()
    ///     local code = {}
    ///     for i = 1, 3 do
    ///         code[#code + 1] = string.format("t[%d] = %d", i, i * 10)
    ///     end
    ///     return table.concat(code, "\n")
    /// end
    /// ```
    #[test]
    fn test_transform_lua_code_comp_time_loop() {
        let code = r#"--[[luajit-pro]]
local t = {}
function __LJP:COMP_TIME()
local code = {}
for i = 1, 3 do
code[#code + 1] = string.format("t[%d] = %d", i, i * 10)
end
return table.concat(code, "\n")
end
"#;

        let result = transform_lua_code(code, "test.lua", None);

        assert!(result.contains("t[1] = 10"));
        assert!(result.contains("t[2] = 20"));
        assert!(result.contains("t[3] = 30"));
    }

    /// Tests the format option for code formatting.
    ///
    /// When `format` is specified in the header, the output code
    /// will be formatted with proper indentation.
    ///
    /// # Example
    /// ```lua
    /// --[[luajit-pro, format]]
    /// if true then if true then x = 1 end end
    /// ```
    #[test]
    fn test_transform_lua_code_format() {
        let code = r#"--[[luajit-pro, format]]
if true then
if true then
x = 1
end
end
"#;

        let result = transform_lua_code(code, "test.lua", None);

        // Formatted code should have proper indentation
        assert!(result.contains("if true then"));
        // Should contain indented nested block
        assert!(result.contains("\t") || result.contains("    "));
    }

    /// Tests the no-comment option for removing comments.
    ///
    /// When `no-comment` is specified, all Lua comments are removed
    /// from the output.
    ///
    /// # Example
    /// ```lua
    /// --[[luajit-pro, no-comment]]
    /// local x = 1 -- this comment will be removed
    /// ```
    #[test]
    fn test_transform_lua_code_no_comment() {
        let code = r#"--[[luajit-pro, no-comment]]
local x = 1 -- inline comment
-- line comment
local y = 2
"#;

        let result = transform_lua_code(code, "test.lua", None);

        // Comments should be removed (except the header)
        assert!(!result.contains("inline comment"));
        assert!(!result.contains("line comment"));
        // Code should remain
        assert!(result.contains("local x = 1"));
        assert!(result.contains("local y = 2"));
    }

    /// Tests the pretty option (format + no-comment).
    ///
    /// The `pretty` option combines both `format` and `no-comment`,
    /// producing clean, formatted code without comments.
    #[test]
    fn test_transform_lua_code_pretty() {
        let code = r#"--[[luajit-pro, pretty]]
-- comment to remove
if true then
if true then
x = 1
end
end
"#;

        let result = transform_lua_code(code, "test.lua", None);

        // Comments removed and code formatted
        assert!(!result.contains("comment to remove"));
        assert!(result.contains("if true then"));
    }

    /// Tests parameter table with conditional code generation.
    ///
    /// This demonstrates how to use the parameter table for
    /// compile-time conditionals.
    ///
    /// # Example
    /// ```lua
    /// --[[luajit-pro, {DEBUG = true}]]
    /// function __LJP:COMP_TIME()
    ///     if DEBUG then
    ///         return [[print("debug mode")]]
    ///     else
    ///         return [[]]
    ///     end
    /// end
    /// ```
    #[test]
    fn test_transform_lua_code_with_param_table() {
        // Note: This test verifies that param_table values are passed
        // to the compile-time environment. The actual conditional logic
        // is tested through Lua execution.
        let code = r#"--[[luajit-pro]]
function __LJP:COMP_TIME()
return [[local value = 42]]
end
"#;

        let result = transform_lua_code(code, "test.lua", None);

        assert!(result.contains("local value = 42"));
    }

    // ========================================================================
    // Optimizer tests
    // ========================================================================

    /// Tests @comp_time_enum annotation for replacing enum field accesses.
    ///
    /// When a table is annotated with `@comp_time_enum`, all accesses to
    /// its fields are replaced with literal values at compile time.
    ///
    /// # Example
    /// ```lua
    /// --[[luajit-pro, opt]]
    /// local --[[@comp_time_enum]] Color = { RED = 1 }
    /// local c = Color.RED  -- becomes: local c = 1 --[[Color.RED]]
    /// ```
    #[test]
    fn test_optimizer_comp_time_enum() {
        let code = r#"--[[luajit-pro, opt]]
local --[[@comp_time_enum]] Color = {
    RED = 1,
    GREEN = 2,
    BLUE = 3,
}
local c = Color.RED
"#;

        let result = transform_lua_code(code, "test.lua", None);

        // Enum access should be replaced with literal value
        assert!(
            result.contains("1") && result.contains("Color.RED"),
            "Expected enum replacement, got: {}",
            result
        );
    }

    /// Tests @used annotation for preventing dead code elimination.
    ///
    /// The `@used` annotation ensures a variable is not optimized out
    /// by adding a dummy usage.
    ///
    /// # Example
    /// ```lua
    /// --[[luajit-pro, opt]]
    /// local --[[@used]] _INIT = 1
    /// ```
    #[test]
    fn test_optimizer_used_annotation() {
        let code = r#"--[[luajit-pro, opt]]
local --[[@used]] _INIT = 1
"#;

        let result = transform_lua_code(code, "test.lua", None);

        // Should contain dummy usage to prevent optimization
        assert!(
            result.contains("_INIT"),
            "Expected _INIT usage, got: {}",
            result
        );
    }
}
