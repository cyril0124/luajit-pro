--[[luajit-pro, no-cache]]
-- Test: Include functionality (Golden Test)

local included_module1 = false
local included_module2 = false

   included_module1 = true                  --[=====[ __ljp:include("tests/golden/include_helper1") --]=====]
   included_module2 = true                  --[=====[ __LJP:INCLUDE("tests/golden/include_helper2") --]=====]

assert(included_module1 == true, "Include test 1 failed")
assert(included_module2 == true, "Include test 2 failed")

print("[TEST PASSED] test_golden_include.lua")
return true
