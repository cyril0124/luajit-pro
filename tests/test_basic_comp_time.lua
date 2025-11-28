--[[luajit-pro, no-cache]]
-- Test: Basic comp_time functionality

local result = {}

function __LJP:COMP_TIME()
    return [[
        result.basic_comp_time = "passed"
    ]]
end

assert(result.basic_comp_time == "passed", "Basic comp_time test failed")

-- Test: Multiple comp_time blocks
function __LJP:COMP_TIME()
    local code = ""
    for i = 1, 3 do
        code = code .. string.format("result.test_%d = %d\n", i, i * 10)
    end
    return code
end

assert(result.test_1 == 10, "Multiple comp_time test 1 failed")
assert(result.test_2 == 20, "Multiple comp_time test 2 failed")
assert(result.test_3 == 30, "Multiple comp_time test 3 failed")

-- Test: String concatenation in comp_time
function __LJP:COMP_TIME()
    return "result.concat_test = " .. "\"" .. "hello" .. "_" .. "world" .. "\""
end

assert(result.concat_test == "hello_world", "String concatenation test failed")

print("[TEST PASSED] test_basic_comp_time.lua")
return true
