--[[luajit-pro, no-cache, format]]
-- Test: Code formatting

local result = {}

function __LJP:COMP_TIME()
return [[
result.formatted = true
]]
end

-- Test nested blocks formatting
if true then
if true then
result.nested = true
end
end

-- Test function formatting
local function test_func()
return "test"
end

result.func_result = test_func()

assert(result.formatted == true, "Formatted test failed")
assert(result.nested == true, "Nested blocks test failed")
assert(result.func_result == "test", "Function test failed")

print("[TEST PASSED] test_format.lua")
return true
