-- PZMM - Client: tells the PZMM app what the game is doing.
-- Game Lua can only write files under Zomboid/Lua, so the app reads Zomboid/Lua/pzmm/game.json
-- (message format v1; the app reads it on start and when you switch to it).
local VERSION = "0.2.1"

local function json(s)
    -- control characters, quotes and backslashes as \u escapes (doubled: Lua reads "\\" as one backslash)
    return '"' .. tostring(s):gsub('[%c"\\]', function(c) return string.format("\\u%04x", c:byte()) end) .. '"'
end

-- optional facts: a function the game doesn't have on this build just leaves its field out
local function try(f)
    local ok, v = pcall(f)
    if ok and v ~= nil then return v end
end

local function write(state)
    local ok, err = pcall(function()
        local ids, mods = {}, getActivatedMods()
        for i = 0, mods:size() - 1 do ids[#ids + 1] = json(mods:get(i)) end
        local fields = {
            '"v":1',
            '"client":' .. json(VERSION),
            '"game":' .. json(getCore():getVersion()),
            '"state":' .. json(state),
            '"at":' .. tostring(math.floor(os.time())),
            '"zomboid":' .. json(Core.getMyDocumentFolder()),
            '"mods":[' .. table.concat(ids, ",") .. ']',
        }
        if state ~= "menu" then
            local mp = try(function() return isClient() end)
            fields[#fields + 1] = '"multiplayer":' .. tostring(mp == true)
            local world = try(function() return getWorld():getWorld() end)
            if world then fields[#fields + 1] = '"save":' .. json(world) end
            if mp then
                local ip = try(function() return getServerIP() end)
                local port = try(function() return getServerPort() end)
                if ip then fields[#fields + 1] = '"server":' .. json(ip .. (port and (":" .. tostring(port)) or "")) end
            end
        end
        local w = getFileWriter("pzmm/game.json", true, false)
        if not w then return end
        w:write("{" .. table.concat(fields, ",") .. "}")
        w:close()
    end)
    if not ok then print("PZMM - Client: couldn't write pzmm/game.json: " .. tostring(err)) end
end

Events.OnMainMenuEnter.Add(function() write("menu") end)
-- a save loading is not the menu (game.json says "loading" until the game starts)
if Events.OnInitWorld then Events.OnInitWorld.Add(function() write("loading") end) end
Events.OnGameStart.Add(function() write("game") end)
