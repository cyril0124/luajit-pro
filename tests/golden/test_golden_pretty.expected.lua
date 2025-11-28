--[[luajit-pro, pretty]] local app = {}
app.name = "test"

app.built_at = "compile_time"
app.features = { "a", "b", "c" }

if true then
	if true then
		app.nested = true
	end
end

return app
