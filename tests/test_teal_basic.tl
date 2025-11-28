--[[luajit-pro, teal]]
-- Test: Teal language support

local a: number = 42
local b: string = "hello"

local record TestRecord
    x: number
    y: number
end

local t: TestRecord = {x = 10, y = 20}

function __LJP:COMP_TIME()
    return [[
        print("[TEST PASSED] test_teal_basic.tl")
    ]]
end

assert(a == 42, "Teal number test failed")
assert(b == "hello", "Teal string test failed")
assert(t.x == 10, "Teal record x test failed")
assert(t.y == 20, "Teal record y test failed")

return true
