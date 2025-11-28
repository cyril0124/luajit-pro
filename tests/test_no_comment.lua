--[[luajit-pro, no-cache, no-comment]]
-- Test: Comment removal functionality
-- This comment should be removed

local result = {}

--[[ 
    This is a multi-line comment
    It should also be removed
]]

result.code_works = true

-- Another comment to remove
function __LJP:COMP_TIME()
    return [[
        result.comp_time = true
    ]]
end

assert(result.code_works == true, "Code works test failed")
assert(result.comp_time == true, "Comp time test failed")

print("[TEST PASSED] test_no_comment.lua")
return true
