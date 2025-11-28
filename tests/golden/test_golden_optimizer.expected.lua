--[[luajit-pro, opt, format]]
-- Golden file test: optimizer with @comp_time_enum

local M = {}

local  --[[@comp_time_enum]]Colors = {
	RED = 1,
	GREEN = 2,
	BLUE = 3,
}

function M.get_color()
	return 1 --[[Colors.RED]]
end

function M.get_all_colors()
	return {
		1 --[[Colors.RED]],
		2 --[[Colors.GREEN]],
		3 --[[Colors.BLUE]],
	}
end

return M
