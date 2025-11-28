--[[luajit-pro, no-cache]]
-- Test: Compile-time enum functionality (Golden Test)

local --[[@comp_time_enum]] test_enum = {
    VALUE_A = 100,
    VALUE_B = 200,
    VALUE_C = "string_value"
}

local result = {}

-- Test numeric enum values
if test_enum.VALUE_A == 100 then
    result.enum_a = true
end

if test_enum.VALUE_B == 200 then
    result.enum_b = true
end

-- Test string enum value
if test_enum.VALUE_C == "string_value" then
    result.enum_c = true
end

assert(result.enum_a == true, "Enum VALUE_A test failed")
assert(result.enum_b == true, "Enum VALUE_B test failed")
assert(result.enum_c == true, "Enum VALUE_C test failed")

print("[TEST PASSED] test_golden_comp_time_enum.lua")
return true
