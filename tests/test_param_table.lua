--[[luajit-pro, no-cache, {ENABLE_FEATURE = 1}]]
-- Test: Parameter table and feature flags

local result = {}

function __LJP:COMP_TIME()
    return [[
        result.comp_time_executed = true
    ]]
end

if _G.ENABLE_FEATURE then
    result.feature_enabled = true
end

assert(result.comp_time_executed == true, "Comp time execution test failed")

print("[TEST PASSED] test_param_table.lua")
return true
