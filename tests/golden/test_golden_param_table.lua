--[[luajit-pro, format, {DEBUG_MODE = false}]]
-- Golden file test: parameter table with conditional code generation

local config = {}

function __LJP:COMP_TIME()
if DEBUG_MODE then
return [[
config.log_level = "debug"
config.verbose = true
]]
else
return [[
config.log_level = "info"
config.verbose = false
]]
end
end

return config
