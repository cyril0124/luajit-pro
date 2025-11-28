--[[luajit-pro, format]]
-- Golden file test: format option

local result = {}

function __LJP:COMP_TIME()
return [[
result.value1 = 100
result.value2 = 200
]]
end

if true then
if true then
result.nested = true
end
end

return result
