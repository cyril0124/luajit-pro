local purple = "\27[35m"
local reset = "\27[0m"
local old_print = print
local f = string.format

_G.__code_name__ = "[Anonymous]"

package.path = package.path .. ";?.lua"

function print(...)
	old_print(purple .. "[comp_time] " .. _G.__code_name__ .. reset, ...)
end

function printf(...)
	io.write(purple .. "[comp_time] " .. _G.__code_name__ .. reset .. "\t" .. f(...))
end

local output_content = ""
_G.output = function(...)
	local args = { ... }
	local str = args[1]
	local values = args[2]

	-- Get upvalues from the caller
	local level = 2
	local i = 1
	local upvalues = {}
	while true do
		-- Try get the local variables, if it fails, break the loop
		local ok, _ = pcall(debug.getlocal, level + 1, i)
		if not ok then
			break
		end

		local name, value = debug.getlocal(level, i)
		if not name then
			break
		end
		upvalues[name] = value
		i = i + 1
	end

	-- Replace {{key}} with the value of the key in the upvalues
	local str = str:gsub("{{(.-)}}", function(key)
		local v
		if type(values) == "table" then
			v = values[key]
		end

		if not v then
			v = upvalues[key]
		end

		assert(v, f("[output] key not found: %s\n\ttemplate_str is: %s\n", key, str))
		return tostring(v)
	end)

	output_content = output_content .. " " .. str .. " "
end
_G.outputf = function(...)
	local args = { ... }
	local str = args[1]
	local values = args[2]

	-- Get upvalues from the caller
	local level = 2
	local i = 1
	local upvalues = {}
	while true do
		-- Try get the local variables, if it fails, break the loop
		local ok, _ = pcall(debug.getlocal, level + 1, i)
		if not ok then
			break
		end

		local name, value = debug.getlocal(level, i)
		if not name then
			break
		end
		upvalues[name] = value
		i = i + 1
	end

	-- Replace {{key}} with the value of the key in the upvalues
	local str = str:gsub("{{(.-)}}", function(key)
		local v
		if type(values) == "table" then
			v = values[key]
		end

		if not v then
			v = upvalues[key]
		end

		assert(v, f("[output] key not found: %s\n\ttemplate_str is: %s\n", key, str))
		return tostring(v)
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

_G.KEEP_LINE = false
_G.keep_line = function()
	_G.KEEP_LINE = true
end
_G._check_keep_line = function()
	local kl = _G.KEEP_LINE
	_G.KEEP_LINE = false
	return kl
end

_G.render = function(str)
	-- Get upvalues from the caller
	local level = 2
	local i = 1
	local upvalues = {}
	while true do
		local name, value = debug.getlocal(level, i)
		if not name then
			break
		end
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
	end,
})

getmetatable("").__index.render = function(template, vars)
	assert(type(template) == "string", "template must be a string")
	assert(type(vars) == "table", "vars must be a table")
	return (
		template:gsub("{{(.-)}}", function(key)
			assert(vars[key], string.format("[render] key not found: %s\n\ttemplate_str is: %s\n", key, template))
			return tostring(vars[key] or "")
		end)
	)
end

getmetatable("").__index.strip = function(str, suffix)
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
	end,
})
