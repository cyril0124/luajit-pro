--[[luajit-pro, teal]]

local a: number = 123
print(a)

a = "123"

local record B
    c: number
    hello: function(B)
    get: function(B): number
end

function _G.__LJP:comp_time()
    print("hello from comp time(teal)")
    return [[
        print("hello from generated code(teal)")
    ]]
end

-- local another = require("another")
-- assert(another.a == 123)