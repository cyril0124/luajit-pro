--[[luajit-pro, no-cache, pretty]]
-- Test: Pretty mode (no-comment + format combined)
-- This comment should be removed in pretty mode

local result = {}

--[[
    Multi-line comment that should be removed
    in pretty mode
]]

function __LJP:COMP_TIME()
return [[
result.pretty_mode = true
]]
end

if true then
if true then
result.nested = true
end
end

assert(result.pretty_mode == true, "Pretty mode test failed")
assert(result.nested == true, "Nested blocks test failed")

print("[TEST PASSED] test_pretty.lua")
return true
