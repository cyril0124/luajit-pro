--[[luajit-pro]]

do
    print("hello from another1")
end

function _G.__LJP:comp_time()
    return [[
        print("from another1.lua comp_time")
    ]]
end