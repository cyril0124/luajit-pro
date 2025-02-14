--[[luajit-pro]] --[[no-cache]] --[[format, no-comment]] --[[{FEAT = 1}]]

function __LJP:COMP_TIME()
    print("hjjj")
    local a = 123

    local b = require "tests/another"

    local b = 
    124

    for i = 1, 10 do
        print(i)
    end

    for i, v in ipairs({1, 2, 3}) do
        print(1)
    end

    do

        local a = 123
    end

    if a == 1 then
        print("a is 1")
    elseif a == 2 then
        print("a is 2")
    else
        print("a is not 1 or 2")
    end

    a = 4

    if not _G.FEAT then
        assert(false)
    end

    return "do " .. "end"
end

function __LJP:COMP_TIME()
    return "do " ..
    "end"
end


function __LJP:COMP_TIME()
    return "--" ..
    123 + 
    455
end

function __LJP:COMP_TIME()
    local hello = 123
    return "--" ..
    123 + 
    hello
end

function __LJP:COMP_TIME()
    local hello = {123}
    return "--" ..
    123 + 
    hello[1]
end

function __LJP:COMP_TIME()
    local hello = 123
    return "--" ..
    123 + 
    hello
end

function __LJP:COMP_TIME()
    local hello = {y = 123}
    return "--" ..
    123 + 
    hello.y
end

function _G.__ljp:comp_time(hello_world)
    local hello = {y = 123}
    return ("--" ..
    123 + 
    hello.y)
end

function _G.__LJP:comp_time(aa)
    local cfg = { a = 123 }
    cfg.a = 12


    ("hello"):format("hello")

    return [[
        -- any comment will be removed!!!
        print("hello from comp time(aa)")
    ]]
end


-- function __LJP:COMP_TIME()
--     local hello = 123
--     return "hello" ..
--     123 + 
--     hello()
-- end

__ljp:include("tests/another")
__LJP:INCLUDE("tests/another")
_G.__ljp:include("tests/another")

_G.__LJP:include "tests/another"

local b = 0xFFFFFFFFFFFFFFFFULL
local a = 0
a = bit.lshift(a, 12345678ULL)

if _G.FEAT then
    print("hello from inject FEAT")
end