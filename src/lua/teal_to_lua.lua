--
-- For debug purposes
--
-- local inspect = require("inspect")
-- local pp = function(...)
-- 	print(inspect(...))
-- end

local tl = require("tl")

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

	tlconfig._init_env_modules = tlconfig._init_env_modules or {}
	if tlconfig.global_env_def then
		table.insert(tlconfig._init_env_modules, 1, tlconfig.global_env_def)
	end

	local opts = {
		defaults = {
			feat_lax = lax_mode and "on" or "off",
			feat_arity = tlconfig["feat_arity"],
			gen_compat = tlconfig["gen_compat"],
			gen_target = tlconfig["gen_target"],
		},
		predefined_modules = tlconfig._init_env_modules,
	}

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

		for _, pat in ipairs({ tl_pat, d_tl_pat, lua_pat }) do
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
	local tlconfig = {
		include_dir = {},
		disable_warnings = {},
		warning_error = {},
		gen_target = "5.1",
		quiet = false,
		_disabled_warnings_set = {},
		_warning_errors_set = {},
	}
	local err
	local env
	local pp_opts

	if not env then
		env = setup_env(tlconfig, input_file_name)
		pp_opts = {
			preserve_indent = true,
			preserve_newlines = true,
			preserve_hashbang = false,
		}
	end

	local res = {
		input_file = input_file_name,
		output_file = input_file_name .. ".lua",
	}

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
