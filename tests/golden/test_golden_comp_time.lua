--[[luajit-pro, format]]
-- Golden file test: COMP_TIME basic usage

local test = {}

function __LJP:COMP_TIME()
local values = {}
for i = 1, 3 do
values[#values + 1] = string.format("test.value%d = %d", i, i * 10)
end
return table.concat(values, "\n")
end

return test
