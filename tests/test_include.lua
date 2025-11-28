--[[luajit-pro, no-cache]]
-- Test: Include functionality

local included_module1 = false
local included_module2 = false

__ljp:include("tests/include_helper1")
__LJP:INCLUDE("tests/include_helper2")

assert(included_module1 == true, "Include test 1 failed")
assert(included_module2 == true, "Include test 2 failed")

print("[TEST PASSED] test_include.lua")
return true
