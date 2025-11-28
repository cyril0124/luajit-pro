--[[luajit-pro, no-cache]]
-- Test: LuaJIT extensions - ULL numbers and bit operations (Golden Test)

local result = {}

-- Test 64-bit unsigned integer literals
local big_num = 0xFFFFFFFFFFFFFFFFULL
result.ull_literal = true

-- Test bit operations
local a = 0
a = bit.lshift(1, 4)
result.lshift = (a == 16)

a = bit.rshift(16, 2)
result.rshift = (a == 4)

a = bit.band(0xFF, 0x0F)
result.band = (a == 0x0F)

a = bit.bor(0xF0, 0x0F)
result.bor = (a == 0xFF)

a = bit.bxor(0xFF, 0x0F)
result.bxor = (a == 0xF0)

a = bit.bnot(0)
result.bnot = true  -- Just test it doesn't error

assert(result.ull_literal == true, "ULL literal test failed")
assert(result.lshift == true, "lshift test failed")
assert(result.rshift == true, "rshift test failed")
assert(result.band == true, "band test failed")
assert(result.bor == true, "bor test failed")
assert(result.bxor == true, "bxor test failed")
assert(result.bnot == true, "bnot test failed")

print("[TEST PASSED] test_golden_luajit_extensions.lua")
return true
