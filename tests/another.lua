--[[luajit-pro]]

do

    print("some thing")
end

function _G.__LJP:comp_time()
    local code = ""
    for i  = 1, 10 do
        code = code .. "print(\"hello\")\n"
    end
    return [[
        print("from another.lua comp_time")
    ]] .. code
end

_G.__LJP:Include("tests/another1")