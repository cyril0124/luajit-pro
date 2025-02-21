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
use std::time::{Duration, Instant, SystemTime};

use full_moon::visitors::VisitorMut;
use lua_optimizer::LuaOptimizer;
use lua_transformer::LuaTransformer;

const OUTPUT_DIR: &'static str = ".luajit_pro";

fn get_mtime(file_path: &str) -> SystemTime {
    match std::fs::metadata(file_path) {
        Ok(metadata) => match metadata.modified() {
            Ok(mtime) => mtime,
            Err(e) => panic!("Failed to get modified time => {}", e.to_string()),
        },
        Err(e) => panic!("Failed to get metadata => {}", e.to_string()),
    }
}

#[inline]
fn parse_param_table(line: &str) -> (Option<HashMap<&str, String>>, bool) {
    let start = line.find('{');
    let end = line.rfind('}');
    if !(start.is_some() && end.is_some()) {
        return (None, false);
    }

    let content = &line[start.unwrap() + 1..end.unwrap()];
    let mut map = HashMap::new();
    let mut need_rebuild = false;
    for kv in content.split(',') {
        if let Some((k, v)) = kv.split_once('=') {
            let key = k.trim();
            let default_value = v.trim();
            let current_value = std::env::var(key).unwrap_or(default_value.to_string());
            if !need_rebuild {
                need_rebuild = default_value != current_value;
            }
            assert!(
                matches!(current_value.as_str(), "true" | "false" | "0" | "1"),
                "Invalid value, key: {}, value: {}",
                key,
                current_value
            );
            map.insert(key, current_value);
        }
    }
    (Some(map), need_rebuild)
}

#[inline]
fn serialize_param_table(param_table: Option<HashMap<&str, String>>) -> String {
    let mut result = String::from("{");

    for (key, value) in param_table.unwrap() {
        result.push_str(&format!("{} = {}, ", key, value));
    }

    result.pop();
    result.pop();
    result.push_str("}");

    result
}

pub fn transform_lua_code(
    code: &str,
    lua_file_path: &str,
    param_table: Option<HashMap<&str, String>>,
) -> String {
    let first_line = code.lines().next().unwrap_or("");

    let final_code = if first_line.contains("teal") {
        assert!(
            !first_line.contains("luau"),
            "Cannot use both luau and teal"
        );
        let lua_code =
            lang_utils::convert_teal_to_lua(lua_file_path, first_line.contains("syntax-only"))
                .replace("bit32", "bit");
        lua_code
    } else {
        code.to_string()
    };

    let ast = full_moon::parse(&final_code).unwrap();

    let mut transformer = LuaTransformer::new();
    transformer.file_path = Some((lua_file_path.to_string()).to_string());
    transformer.input_param_list = {
        let mut input_param_list = Vec::new();
        if let Some(param_table) = param_table.clone() {
            for (key, value) in param_table {
                input_param_list.push((key.to_owned(), value));
            }
            if input_param_list.len() == 0 {
                None
            } else {
                Some(input_param_list)
            }
        } else {
            None
        }
    };
    let mut new_ast = transformer.visit_ast(ast);

    if first_line.contains("opt") && std::env::var("LJP_NO_OPT").unwrap_or(String::from("0")) == "0"
    {
        let mut optimizer = LuaOptimizer::new();
        let neww_ast = full_moon::parse(&new_ast.to_string())
            .expect(&format!("Failed to parse: <<<{}>>>", new_ast.to_string()));
        new_ast = optimizer.visit_ast(neww_ast);
    }

    let mut new_content = new_ast.to_string();

    if let Some(param_table) = param_table {
        new_content = lang_utils::inject_global_vals(&new_content, param_table);
    }

    if first_line.contains("luau") {
        assert!(
            !first_line.contains("teal"),
            "Cannot use both luau and teal"
        );
        new_content = lang_utils::convert_luau_to_lua(&new_content);
    }

    if first_line.contains("pretty") {
        // pretty == no-comment + format
        new_content = lang_utils::format_lua_code(&lang_utils::remove_lua_comments(&new_content));
    } else {
        if first_line.contains("no-comment") {
            new_content = lang_utils::remove_lua_comments(&new_content);
        }

        if first_line.contains("format") {
            new_content = lang_utils::format_lua_code(&new_content);
        }
    }

    new_content
}

#[no_mangle]
pub fn transform_lua(file_path: *const c_char) -> *const c_char {
    #[cfg(feature = "print-time")]
    let start = Instant::now();

    let c_str = unsafe { CStr::from_ptr(file_path) };
    let lua_file_path = c_str.to_str().unwrap();

    let first_line = {
        let file =
            File::open(lua_file_path).expect(&format!("Failed to open file => {}", lua_file_path));
        let mut reader = std::io::BufReader::new(file);
        let mut first_line = String::new();
        reader.read_line(&mut first_line).unwrap();

        first_line
    };

    let no_cache = first_line.contains("no-cache")
        || std::env::var("LJP_NO_CACHE").map_or_else(|_| false, |v| v == "1");

    let build_cache_dir = format!("{}/build_cache", OUTPUT_DIR);
    let cached_file = format!(
        "{}/{}",
        build_cache_dir,
        Path::new(lua_file_path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
    );

    if !std::fs::exists(&build_cache_dir).unwrap_or(false) {
        std::fs::create_dir_all(&build_cache_dir).expect("Failed to create directory");
    } else {
        if !no_cache {
            if std::fs::exists(&cached_file).unwrap_or(false) {
                if get_mtime(&lua_file_path) <= get_mtime(&cached_file) {
                    let cached_first_line = {
                        let file = File::open(&cached_file)
                            .expect(&format!("Failed to open file => {}", cached_file));
                        let mut reader = std::io::BufReader::new(file);
                        let mut first_line = String::new();
                        reader.read_line(&mut first_line).unwrap();

                        first_line
                    };

                    let (_, need_rebuild) = parse_param_table(&cached_first_line);

                    if !need_rebuild {
                        let code = std::fs::read_to_string(&cached_file).unwrap();
                        let c_str = CString::new(code).unwrap();

                        #[cfg(feature = "print-time")]
                        {
                            let duration = start.elapsed();
                            println!(
                                "[luajit_pro_heler] Time elapsed(cached) in transform_lua() is: {:?}, file: {}",
                                duration, lua_file_path
                            );
                            std::io::stdout().flush().unwrap();
                        }

                        return c_str.into_raw();
                    }
                }
            }
        }
    }

    let (param_table, _) = parse_param_table(&first_line);
    let content = std::fs::read_to_string(lua_file_path).unwrap();
    let new_content = transform_lua_code(&content, lua_file_path, param_table.clone());

    let new_content = if let Some(first_newline_pos) = new_content.find('\n') {
        let old_first_line = first_line.clone();
        let start = first_line.find("{");
        let end = first_line.rfind('}');
        let mut result = if let (Some(start), Some(end)) = (start, end) {
            let before = &first_line[..start];
            let after = &first_line[end + 1..];
            format!("{}{}{}", before, serialize_param_table(param_table), after)
                .strip_suffix("\n")
                .unwrap_or_default()
                .to_string()
        } else {
            first_line
        };

        let new_first_line = &new_content[..first_newline_pos];
        if new_first_line.contains("luajit-pro") {
            result.push_str(&new_content[first_newline_pos..]);
        } else {
            if old_first_line.contains("luajit-pro") {
                if new_first_line.contains("tl_compat") || new_first_line.contains("bit") {
                    result = result.strip_suffix("\n").unwrap_or_default().to_string() + " ";
                    result.push_str(&new_content);
                } else {
                    result = format!(
                        "{} {}",
                        result.strip_suffix("\n").unwrap_or_default(),
                        &new_content
                    );
                }
            } else {
                result.push_str(&new_content);
            }
        }
        result
    } else {
        new_content
    };

    std::fs::write(cached_file, &new_content).expect("Failed to write to file");

    let c_str = CString::new(new_content).unwrap();

    #[cfg(feature = "print-time")]
    {
        let duration = start.elapsed();
        println!(
            "[luajit_pro_heler] Time elapsed in transform_lua() is: {:?}, file: {}",
            duration, lua_file_path
        );
        std::io::stdout().flush().unwrap();
    }

    c_str.into_raw()
}
