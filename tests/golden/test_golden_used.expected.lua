--[[luajit-pro, opt, format]]
-- Golden file test: @used annotation to prevent dead code elimination

local M = {}

---@used
local _KEEPME = (function()
	M.initialized = true
end)()

function M.get_status()
	return M.initialized
end

return M
