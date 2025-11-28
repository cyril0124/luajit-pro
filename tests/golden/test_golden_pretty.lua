--[[luajit-pro, pretty]]
-- Golden file test: pretty option (format + no-comment)

local app = {}
app.name = "test"

function __LJP:COMP_TIME()
return [[
app.built_at = "compile_time"
app.features = {"a", "b", "c"}
]]
end

if true then
if true then
app.nested = true
end
end

return app
