--[[luajit-pro, no-cache, opt]]
-- Test: Optimizer functionality - @comp_time_enum and @used annotations

local result = {}

-- Test @comp_time_enum: compile-time enum value inlining
local --[[@comp_time_enum]] OpCode = {
    NOP = 0,
    LOAD = 1,
    STORE = 2,
    ADD = 0x10,
    SUB = 0x11,
}

-- These enum accesses should be replaced with literal values at compile time
local op1 = OpCode.NOP
local op2 = OpCode.ADD
local op3 = OpCode.STORE

assert(op1 == 0, "@comp_time_enum NOP value failed")
assert(op2 == 0x10, "@comp_time_enum ADD value failed")
assert(op3 == 2, "@comp_time_enum STORE value failed")

result.enum_nop = op1
result.enum_add = op2
result.enum_store = op3

-- Test @used: prevent variable from being optimized out
local --[[@used]] debug_marker = 12345

-- Verify the marker exists and is accessible
result.used_marker = debug_marker

assert(result.enum_nop == 0, "Enum NOP test failed")
assert(result.enum_add == 16, "Enum ADD test failed")
assert(result.enum_store == 2, "Enum STORE test failed")
assert(result.used_marker == 12345, "@used marker test failed")

print("[TEST PASSED] test_optimizer.lua")
return true
