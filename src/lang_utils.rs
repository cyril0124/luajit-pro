use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::{cell::RefCell, env, str, vec};

use darklua_core::generator::LuaGenerator;
use darklua_core::rules::{Rule, RuleConfiguration, RulePropertyValue};
use mlua::prelude::*;

const CARGO_PATH: &'static str = env!("CARGO_MANIFEST_DIR");
pub fn lua_dostring(code_name: &str, code: &str) -> String {
    thread_local! {
        static LUA: UnsafeCell<Lua> = UnsafeCell::new(unsafe {
            let lua = Lua::unsafe_new();
            lua.load(r#"
    _G.__code_name__ = "[Anonymous]"
    local purple = "\27[35m"
    local reset = "\27[0m"
    local old_print = print
    local f = string.format
    package.path = package.path .. ";?.lua"
    
    function print(...)
        old_print(purple .. "[comp_time] " .. _G.__code_name__ .. reset, ...)
    end
    
    function printf(...)
        io.write(purple .. "[comp_time] " .. _G.__code_name__ .. reset .. "\t" .. f(...))
    end

    local output_content = ""
    _G.output = function(...)
        local args = {...}
        local str = args[1]

        -- Get upvalues from the caller
        local level = 2
        local i = 1
        local upvalues = {}
        while true do
            local name, value = debug.getlocal(level, i)
            if not name then break end
            upvalues[name] = value
            i = i + 1
        end

        -- Replace {{key}} with the value of the key in the upvalues
        local str = str:gsub("{{(.-)}}", function(key)
            assert(upvalues[key], f("[output] key not found: %s\n\ttemplate_str is: %s\n", key, str))
            return tostring(upvalues[key] or "")
        end)

        output_content = output_content .. " " .. str .. " "
    end
    _G.outputf = function(...)
        local args = {...}
        local str = args[1]

        -- Get upvalues from the caller
        local level = 2
        local i = 1
        local upvalues = {}
        while true do
            local name, value = debug.getlocal(level, i)
            if not name then break end
            upvalues[name] = value
            i = i + 1
        end

        -- Replace {{key}} with the value of the key in the upvalues
        local str = str:gsub("{{(.-)}}", function(key)
            assert(upvalues[key], f("[output] key not found: %s\n\ttemplate_str is: %s\n", key, str))
            return tostring(upvalues[key] or "")
        end)

        -- Call the function with the formatted string and the rest of the arguments
        local ret = f(str, table.unpack(args, 2))
        output_content = output_content .. " " .. ret .. " "
    end
    _G.out = _G.output
    _G.outf = _G.outputf
    _G.o = _G.output
    _G.of = _G.outputf

    _G.get_output = function()
        local out = output_content
        output_content = ""
        return out
    end

    _G.render = function(str)
        -- Get upvalues from the caller
        local level = 2
        local i = 1
        local upvalues = {}
        while true do
            local name, value = debug.getlocal(level, i)
            if not name then break end
            upvalues[name] = value
            i = i + 1
        end

        local str = str:gsub("{{(.-)}}", function(key)
            assert(upvalues[key], f("[render] key not found: %s\n\ttemplate_str is: %s\n", key, str))
            return tostring(upvalues[key] or "")
        end)

        return str
    end
    
    _G.env_vars = {}
    setmetatable(_G.env_vars, {
        __index = function(table, key)
            local value = os.getenv(key)
            if value == nil then
                printf("[warn] env_vars[%s] is nil!\n", key)
            end
            return os.getenv(key)
        end
    })
    
    getmetatable('').__index.render = function(template, vars)
        assert(type(template) == "string", "template must be a string")
        assert(type(vars) == "table", "vars must be a table")
        return (template:gsub("{{(.-)}}", function(key)
            assert(vars[key], string.format("[render] key not found: %s\n\ttemplate_str is: %s\n", key, template))
            return tostring(vars[key] or "")
        end))
    end
    
    getmetatable('').__index.strip = function(str, suffix)
        assert(type(suffix) == "string", "suffix must be a string")
        if str:sub(-#suffix) == suffix then
            return str:sub(1, -#suffix - 1)
        else
            return str
        end
    end

    _G.__LJP = setmetatable({}, {
        __index = function(t, key)
            return function() end
        end
    })
                "#).exec().unwrap();
            lua
        });
    }
    LUA.with(|lua| {
        let lua = unsafe { &mut *lua.get() };
        lua.globals()
            .set("__code_name__", code_name)
            .expect("Failed to set __code_name__");
        let ret_val = lua.load(code).eval::<mlua::Value>();
        let get_output: LuaFunction = lua.globals().get("get_output").unwrap();
        let output_str: String = get_output.call::<String>(()).unwrap();
        output_str
            + (match ret_val {
                Ok(value) => match value {
                    mlua::Value::String(s) => s.to_str().unwrap().to_owned(),
                    mlua::Value::Nil => "".to_owned(),
                    _ => panic!(
                        "Expected string bug got {:?}\n----------\n{}\n----------",
                        value, code
                    ),
                },
                Err(err) => panic!(
                    "Error evaluating lua code, {}\n----------\n{}\n----------",
                    err, code
                ),
            })
            .as_str()
    })
}

pub fn convert_teal_to_lua(input_file_name: &str, syntax_only: bool) -> String {
    thread_local! {
        static LUA: RefCell<Option<Lua>> = RefCell::new(None);
    }
    LUA.with(|lua| {
        let mut lua_ref = lua.borrow_mut();
        if lua_ref.is_none() {
            *lua_ref = unsafe {
                let new_lua = Lua::unsafe_new();
                new_lua
                    .load(&format!(
                        r#"
_G.package.path = package.path .. ";{CARGO_PATH}/tl/?.lua"
local tl = require "tl"

local turbo
local is_turbo_on
do
   local tl_lex = tl.lex
   local turbo_is_on = false

   turbo = function(on)
      if on then
         if jit then
            jit.off()
            tl.lex = function(input, filename)
               jit.on()
               local r1, r2 = tl_lex(input, filename)
               jit.off()
               return r1, r2
            end
         end
         collectgarbage("stop")
      else
         if jit then
            jit.on()
            tl.lex = tl_lex
         end
         collectgarbage("restart")
      end
      turbo_is_on = on
   end

   is_turbo_on = function()
      return turbo_is_on
   end
end

local function die(msg)
    assert(false, msg)
end

local function setup_env(tlconfig, filename)
    local _, extension = filename:match("(.*)%.([a-z]+)$")
    extension = extension and extension:lower()
 
    local lax_mode
    if extension == "tl" then
       lax_mode = false
    elseif extension == "lua" then
       lax_mode = true
    else
       -- if we can't decide based on the file extension, default to strict mode
       lax_mode = false
    end
 
    tlconfig._init_env_modules = tlconfig._init_env_modules or {{}}
    if tlconfig.global_env_def then
       table.insert(tlconfig._init_env_modules, 1, tlconfig.global_env_def)
    end
 
    local opts = {{
       defaults = {{
          feat_lax = lax_mode and "on" or "off",
          feat_arity = tlconfig["feat_arity"],
          gen_compat = tlconfig["gen_compat"],
          gen_target = tlconfig["gen_target"],
        }},
       predefined_modules = tlconfig._init_env_modules,
    }}
 
    local env, err = tl.new_env(opts)
    if not env then
       die(err)
    end
 
    return env
end

local function filename_to_module_name(filename)
    local path = os.getenv("TL_PATH") or _G.package.path
    for entry in path:gmatch("[^;]+") do
       entry = entry:gsub("%.", "%%.")
       local lua_pat = "^" .. entry:gsub("%?", ".+") .. "$"
       local d_tl_pat = lua_pat:gsub("%%.lua%$", "%%.d%%.tl$")
       local tl_pat = lua_pat:gsub("%%.lua%$", "%%.tl$")
 
       for _, pat in ipairs({{ tl_pat, d_tl_pat, lua_pat }}) do
          local cap = filename:match(pat)
          if cap then
             return (cap:gsub("[/\\]", "."))
          end
       end
    end
 
    -- fallback:
    return (filename:gsub("%.lua$", ""):gsub("%.d%.tl$", ""):gsub("%.tl$", ""):gsub("[/\\]", "."))
end
 
local function process_module(filename, env)
    local module_name = filename_to_module_name(filename)
    local result, err = tl.process(filename, env)
    if result then
        env.modules[module_name] = result.type
    end
    return result, err
end

local function printerr(s)
    io.stderr:write(s .. "\n")
end

local function filter_warnings(tlconfig, result)
    if not result.warnings then
       return
    end
    for i = #result.warnings, 1, -1 do
       local w = result.warnings[i]
       if tlconfig._disabled_warnings_set[w.tag] then
          table.remove(result.warnings, i)
       elseif tlconfig._warning_errors_set[w.tag] then
          local err = table.remove(result.warnings, i)
          table.insert(result.type_errors, err)
       end
    end
end

local report_all_errors
do
   local function report_errors(category, errors)
      if not errors then
         return false
      end
      if #errors > 0 then
         local n = #errors
         printerr("========================================")
         printerr(n .. " " .. category .. (n ~= 1 and "s" or "") .. ":")
         for _, err in ipairs(errors) do
            printerr(err.filename .. ":" .. err.y .. ":" .. err.x .. ": " .. (err.msg or ""))
         end
         printerr("----------------------------------------")
         printerr(n .. " " .. category .. (n ~= 1 and "s" or ""))
         return true
      end
      return false
   end

   report_all_errors = function(tlconfig, env, syntax_only)
      local any_syntax_err, any_type_err, any_warning
      for _, name in ipairs(env.loaded_order) do
         local result = env.loaded[name]

         local syntax_err = report_errors("syntax error", result.syntax_errors)
         if syntax_err then
            any_syntax_err = true
         elseif not syntax_only then
            filter_warnings(tlconfig, result)
            any_warning = report_errors("warning", result.warnings) or any_warning
            any_type_err = report_errors("error", result.type_errors) or any_type_err
         end
      end
      local ok = not (any_syntax_err or any_type_err)
      return ok, any_syntax_err, any_type_err, any_warning
   end
end

local function teal_to_lua(input_file_name, syntax_only)
    turbo(true)
    local tlconfig = {{
        include_dir = {{}},
        disable_warnings = {{}},
        warning_error = {{}},
        gen_target = "5.1",
        quiet = false,
        _disabled_warnings_set = {{}},
        _warning_errors_set = {{}},
    }}
    local err
    local env
    local pp_opts

    if not env then
        env = setup_env(tlconfig, input_file_name)
        pp_opts = {{
           preserve_indent = true,
           preserve_newlines = true,
           preserve_hashbang = false
        }}
    end

    local res = {{
        input_file = input_file_name,
        output_file = input_file_name .. ".lua"
    }}

    res.tl_result, err = process_module(input_file_name, env)

    if err then
        die(err)
    end

    local ret_code = ""
    if #res.tl_result.syntax_errors == 0 then
        ret_code = tl.generate(res.tl_result.ast, tlconfig.gen_target, pp_opts)
    end

    local _, any_syntax_err, _, _ = report_all_errors(tlconfig, env, syntax_only or false)
    assert(not any_syntax_err, "Failed to generate Lua code(syntax error found!)")

    turbo(false)
    return ret_code
end

_G.teal_to_lua = teal_to_lua
                "#
                    ))
                    .exec()
                    .unwrap();
                Some(new_lua)
            };
        }

        let func: mlua::Function = lua_ref
            .as_ref()
            .unwrap()
            .globals()
            .get("teal_to_lua")
            .unwrap();
        let lua_code = func
            .call::<String>((input_file_name, syntax_only))
            .expect("Failed to call teal_to_lua function");
        lua_code
    })
}

pub fn convert_luau_to_lua(input: &str) -> String {
    let resources: darklua_core::Resources = darklua_core::Resources::from_memory();
    let context = darklua_core::rules::ContextBuilder::new(".", &resources, input).build();
    let mut block = darklua_core::Parser::default()
        .preserve_tokens()
        .parse(input)
        .unwrap_or_else(|error| {
            panic!(
                "[convert_luau_to_lua] darklua_core could not parse content: {:?}\ncontent:\n{}",
                error, input
            );
        });

    darklua_core::rules::RemoveCompoundAssignment::default()
        .process(&mut block, &context)
        .expect("Failed to remove compound assignment");
    darklua_core::rules::RemoveFloorDivision::default()
        .process(&mut block, &context)
        .expect("Failed to remove floor division");
    darklua_core::rules::RemoveTypes::default()
        .process(&mut block, &context)
        .expect("Failed to remove types");
    darklua_core::rules::RemoveIfExpression::default()
        .process(&mut block, &context)
        .expect("Failed to remove if expression");
    darklua_core::rules::RemoveContinue::default()
        .process(&mut block, &context)
        .expect("Failed to remove continue");
    darklua_core::rules::RemoveInterpolatedString::default()
        .process(&mut block, &context)
        .expect("Failed to remove interpolated string");
    darklua_core::rules::RemoveUnusedIfBranch::default()
        .process(&mut block, &context)
        .expect("Failed to remove unused if branches");
    darklua_core::rules::RemoveEmptyDo::default()
        .process(&mut block, &context)
        .expect("Failed to remove empty do");
    darklua_core::rules::RemoveUnusedVariable::default()
        .process(&mut block, &context)
        .expect("Failed to remove unused variables");

    let mut generator = darklua_core::generator::TokenBasedLuaGenerator::new(input);
    generator.write_block(&block);
    let lua_code = generator.into_string();

    lua_code
}

pub fn inject_global_vals(input: &str, input_param_table: HashMap<&str, String>) -> String {
    let resources: darklua_core::Resources = darklua_core::Resources::from_memory();
    let context = darklua_core::rules::ContextBuilder::new(".", &resources, input).build();
    let mut block = darklua_core::Parser::default()
        .preserve_tokens()
        .parse(input)
        .unwrap_or_else(|error| {
            panic!(
                "[inject_global_vals] darklua_core could not parse content: {:?}\ncontent:\n{}",
                error, input
            );
        });

    for (key, value) in input_param_table {
        darklua_core::rules::InjectGlobalValue::boolean(key, value == "true" || value == "1")
            .process(&mut block, &context)
            .expect("Failed to inject global value");
    }
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
    let lua_code = generator.into_string();

    lua_code
}

pub fn remove_lua_comments(input: &str) -> String {
    let resources = darklua_core::Resources::from_memory();
    let context = darklua_core::rules::ContextBuilder::new(".", &resources, input).build();
    let mut block = darklua_core::Parser::default()
        .preserve_tokens()
        .parse(input)
        .unwrap_or_else(|error| {
            panic!("could not parse content: {:?}\ncontent:\n{}\norigin_code:\n----------------------\n{input}\n----------------------", error, input);
        });

    let mut rule = darklua_core::rules::RemoveComments::default();
    rule.configure({
        let mut properties = HashMap::new();
        properties.insert(
            "except".to_string(),
            RulePropertyValue::StringList(vec!["--\\[\\[@comp_time_enum\\]\\]".to_string()]),
        );
        properties
    })
    .expect("Failed to configure rule");

    rule.process(&mut block, &context)
        .expect("rule should suceed");
    let mut generator = darklua_core::generator::TokenBasedLuaGenerator::new(input);
    generator.write_block(&block);
    let lua_code = generator.into_string();

    lua_code
}

pub fn format_lua_code(input: &str) -> String {
    let ast = full_moon::parse(&input).expect("Failed to parse generated AST");
    let ret_ast = stylua_lib::format_ast(
        ast,
        stylua_lib::Config::new(),
        None,
        stylua_lib::OutputVerification::None,
    )
    .unwrap();
    ret_ast.to_string()
}
