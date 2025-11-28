--[[luajit-pro, no-comment]]
-- Golden file test: no-comment option removes comments

local config = {}

config.debug = false
config.version = "1.0.0"

function __LJP:COMP_TIME()
return [[
config.generated = true
]]
end

return config
