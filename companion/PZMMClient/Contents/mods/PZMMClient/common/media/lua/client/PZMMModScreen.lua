-- PZMM - Client: PZMM's My mods screen in the game (Build 42). The main menu's Mods button opens this
-- instead of the game's mod screen. It's drawn from scratch to look like the app; underneath it uses
-- the game's own mod data and saving (ModSelector.Model), so what it saves is what the game's screen
-- would. "Game's mod screen" opens the original.
-- ponytail: mouse only, English labels, the main menu's Mods button only (new game and load game keep the game's screen).
require "OptionScreens/MainScreen"
require "OptionScreens/ModSelector/ModSelector"
require "OptionScreens/ModSelector/ModSelectorModel"

if not (MainScreen and ModSelector and ModSelector.Model) then return end

-- --- the app's look (app/src/app.css, dark theme)

local function hex(h, a)
    return { r = tonumber(h:sub(1, 2), 16) / 255, g = tonumber(h:sub(3, 4), 16) / 255, b = tonumber(h:sub(5, 6), 16) / 255, a = a or 1 }
end
local C = {
    bg = hex("121212"), panel = hex("1a1a1a"), raised = hex("242424"), line = hex("2e2e2e"), edge = hex("5a5a56"),
    text = hex("e8e8e6"), muted = hex("9b9b97"), accent = hex("6e080c"), accentHover = hex("8a1418"), accentText = hex("d0484e"),
    hover = hex("3a1214"), ok = hex("3fb950"), warn = hex("f0883e"), danger = hex("e5604f"),
}
local function copy(c, a) return { r = c.r, g = c.g, b = c.b, a = a or c.a } end

local TM = getTextManager()
local SMALL, MEDIUM = UIFont.Small, UIFont.Medium
local H_SMALL, H_MEDIUM = TM:getFontHeight(SMALL), TM:getFontHeight(MEDIUM)

local PAD = 16
local BAR = H_SMALL + 14 -- toolbar controls (32 px, as the app's)
local ROW1 = 56 -- the app's top row: logo, profile
local ROW2 = H_SMALL + 19 -- its tab bar
local TOP = ROW1 + ROW2
local FILTERS = BAR + 16 -- the filters bar under the tabs (8 px above and below its controls)
local BOTTOM = 56 -- the footer with Back and Accept
local SIDE = 380 -- the right panel
local ROW = math.max(40, H_SMALL + 16)
local HEAD = 46
local GAP = 12
local BOX = 16

local function tex(name) return getTexture("media/ui/PZMM/" .. name .. ".png") end
local LOGO = tex("logo")
-- the app's links (app/src/lib/Links.svelte), each brand's own logo
local LINKS = {
    { tex("link_github"), "https://github.com/SEVNC3/pz-mod-manager", "GitHub", "GitHub\nThe PZMM app: download it, read the source, report a bug." },
    { tex("link_discord"), "https://discord.com/invite/6URUy6mNzu", "Discord", "Discord\nHelp, ideas and the PZMM community." },
    { tex("link_kofi"), "https://ko-fi.com/sevnc3", "Ko-fi", "Ko-fi\nBuy the developer a coffee: PZMM is free." },
}
-- the app's top bar actions the game can do too: { id, icon, tooltip }
local ACTIONS = {
    { "share", tex("icon_share"), "Copy mod list\nYour enabled mods as a preset: paste it into the game's Presets > Add, or send it to a friend." },
    { "refresh", tex("icon_refresh"), "Rescan mods\nPicks up mods Steam downloaded, or you added, since this screen opened." },
    { "folder", tex("icon_folder"), "Open mods folder\nZomboid/mods, where mods you made or downloaded by hand go." },
    { "check", tex("icon_check"), "Check setup in PZMM\nOpens PZMM on Check setup: it checks your mods, load order and game files, and offers fixes.", app = true },
}
-- the PZMM section labels, rendered in the app's style; any other name gets a tape drawn here
local TAPE_L, TAPE_M, TAPE_R = tex("tape_l"), tex("tape_m"), tex("tape_r")
local MARKER = (function()
    local ok, f = pcall(function() return UIFont.Handwritten end)
    return ok and f or UIFont.Medium
end)()
local H_MARKER = TM:getFontHeight(MARKER)
local TAPE_CACHE = {}
local function tapeImage(name)
    if TAPE_CACHE[name] == nil then
        local slug = (name:lower():gsub("[^a-z0-9]+", "_"):gsub("^_+", ""):gsub("_+$", ""))
        TAPE_CACHE[name] = tex("tape_" .. slug) or false
    end
    return TAPE_CACHE[name]
end
--- A section's tape label at `x`, centred on `cy`; returns its right edge.
local function tape(ui, name, x, cy)
    local t = tapeImage(name)
    if t then
        ui:drawTextureScaled(t, x, cy - t:getHeight() / 2, t:getWidth(), t:getHeight(), 1, 1, 1, 1)
        return x + t:getWidth()
    end
    local h = math.max(24, TM:getFontHeight(MARKER) + 2)
    local w = TM:MeasureStringX(MARKER, name) + 6
    local y = cy - h / 2
    ui:drawTextureScaled(TAPE_L, x, y, 14, h, 1, 1, 1, 1)
    ui:drawTextureScaled(TAPE_M, x + 14, y, w, h, 1, 1, 1, 1)
    ui:drawTextureScaled(TAPE_R, x + 14 + w, y, 84, h, 1, 1, 1, 1)
    ui:drawText(name, x + 16, cy - TM:getFontHeight(MARKER) / 2, 0.165, 0.157, 0.137, 1, MARKER)
    return x + 98 + w
end
local STAR_ON, STAR_OFF = getTexture("media/ui/inventoryPanes/FavouriteYes.png"), getTexture("media/ui/inventoryPanes/FavouriteNo.png")
-- the sections, by where each mod comes from: 1 Workshop, 2 this PC, 3 can't load on this build

local function rect(ui, x, y, w, h, c, a) ui:drawRect(x, y, w, h, a or c.a, c.r, c.g, c.b) end
local function text(ui, s, x, y, c, font) ui:drawText(s, x, y, c.r, c.g, c.b, c.a, font or SMALL) end
local function bold(ui, s, x, y, c, font) text(ui, s, x, y, c, font); text(ui, s, x + 1, y, c, font) end
local function width(s, font) return TM:MeasureStringX(font or SMALL, s) end
local function image(ui, t, x, y, w, h) if t then ui:drawTextureScaled(t, x, y, w or t:getWidth(), h or t:getHeight(), 1, 1, 1, 1) end end
--- `s` cut to fit `w` px, with an ellipsis.
local function fit(s, w, font)
    if width(s, font) <= w then return s end
    while #s > 1 and width(s .. "...", font) > w do s = s:sub(1, -2) end
    return s .. "..."
end
--- `s` as lines of at most `w` px.
local function wrap(s, w, font)
    local lines = {}
    for para in (s .. "\n"):gmatch("(.-)\n") do
        local line = ""
        for word in para:gmatch("%S+") do
            local next = line == "" and word or (line .. " " .. word)
            if width(next, font) > w and line ~= "" then
                lines[#lines + 1] = line
                line = word
            else
                line = next
            end
        end
        lines[#lines + 1] = line
    end
    return lines
end

-- rounded corners: the game only draws plain rectangles, so the corners are small anti-aliased
-- quarter-circle images the mod ships (radius 3, 6 or 10), tinted like any rectangle
local CORNERS = {}
local function corners(r, kind)
    local key = r .. kind
    if not CORNERS[key] then
        local t = {}
        for _, p in ipairs({ "tl", "tr", "bl", "br" }) do t[p] = tex("c" .. r .. "_" .. kind .. "_" .. p) end
        CORNERS[key] = t
    end
    return CORNERS[key]
end
--- A filled rectangle with rounded corners; `top`/`bottom` false keeps those corners square.
local function round(ui, x, y, w, h, r, c, a, top, bottom)
    a = a or c.a
    local t = corners(r, "fill")
    local ty, by = top ~= false and r or 0, bottom ~= false and r or 0
    ui:drawRect(x + r, y, w - 2 * r, h, a, c.r, c.g, c.b)
    ui:drawRect(x, y + ty, r, h - ty - by, a, c.r, c.g, c.b)
    ui:drawRect(x + w - r, y + ty, r, h - ty - by, a, c.r, c.g, c.b)
    if ty > 0 then
        ui:drawTextureScaled(t.tl, x, y, r, r, a, c.r, c.g, c.b)
        ui:drawTextureScaled(t.tr, x + w - r, y, r, r, a, c.r, c.g, c.b)
    end
    if by > 0 then
        ui:drawTextureScaled(t.bl, x, y + h - r, r, r, a, c.r, c.g, c.b)
        ui:drawTextureScaled(t.br, x + w - r, y + h - r, r, r, a, c.r, c.g, c.b)
    end
end
--- Rounded only at the `left` and/or `right` end (a segmented control's first and last option).
local function roundEnds(ui, x, y, w, h, r, c, a, left, right)
    a = a or c.a
    local t = corners(r, "fill")
    local lw, rw = left and r or 0, right and r or 0
    ui:drawRect(x + lw, y, w - lw - rw, h, a, c.r, c.g, c.b)
    if left then
        ui:drawRect(x, y + r, r, h - 2 * r, a, c.r, c.g, c.b)
        ui:drawTextureScaled(t.tl, x, y, r, r, a, c.r, c.g, c.b)
        ui:drawTextureScaled(t.bl, x, y + h - r, r, r, a, c.r, c.g, c.b)
    end
    if right then
        ui:drawRect(x + w - r, y + r, r, h - 2 * r, a, c.r, c.g, c.b)
        ui:drawTextureScaled(t.tr, x + w - r, y, r, r, a, c.r, c.g, c.b)
        ui:drawTextureScaled(t.br, x + w - r, y + h - r, r, r, a, c.r, c.g, c.b)
    end
end
--- Its 1 px outline.
local function outline(ui, x, y, w, h, r, c, a)
    a = a or c.a
    local t = corners(r, "ring")
    ui:drawRect(x + r, y, w - 2 * r, 1, a, c.r, c.g, c.b)
    ui:drawRect(x + r, y + h - 1, w - 2 * r, 1, a, c.r, c.g, c.b)
    ui:drawRect(x, y + r, 1, h - 2 * r, a, c.r, c.g, c.b)
    ui:drawRect(x + w - 1, y + r, 1, h - 2 * r, a, c.r, c.g, c.b)
    ui:drawTextureScaled(t.tl, x, y, r, r, a, c.r, c.g, c.b)
    ui:drawTextureScaled(t.tr, x + w - r, y, r, r, a, c.r, c.g, c.b)
    ui:drawTextureScaled(t.bl, x, y + h - r, r, r, a, c.r, c.g, c.b)
    ui:drawTextureScaled(t.br, x + w - r, y + h - r, r, r, a, c.r, c.g, c.b)
end
--- This mod's own settings (the Settings tab), in Zomboid/Lua/pzmm/client.txt as key=1 or key=0.
local SETTINGS = { logo = true, watermark = true, news = true, tips = true }
pcall(function()
    local r = getFileReader("pzmm/client.txt", false)
    if not r then return end
    local line = r:readLine()
    while line do
        local k, v = line:match("^(%w+)=(%d)$")
        if k and SETTINGS[k] ~= nil then SETTINGS[k] = v == "1" end
        line = r:readLine()
    end
    r:close()
end)
local function saveSettings()
    pcall(function()
        local w = getFileWriter("pzmm/client.txt", true, false)
        for k, v in pairs(SETTINGS) do w:write(k .. "=" .. (v and "1" or "0") .. "\n") end
        w:close()
    end)
end

--- A hover tooltip, as the app's: set while drawing whatever is hovered, drawn last (in render) so
--- nothing covers it. Lines split on "\n"; the first is the bold title.
local function tip(ui, s) if SETTINGS.tips then ui.tipText = s end end
local function drawTip(ui)
    local s = ui.tipText
    ui.tipText = nil
    if not s then return end
    local rows, first = {}, true
    for para in (s .. "\n"):gmatch("(.-)\n") do
        for _, l in ipairs(wrap(para, 320)) do rows[#rows + 1] = { l, first } end
        first = false
    end
    local w = 0
    for _, r in ipairs(rows) do w = math.max(w, width(r[1]) + 1) end
    w = w + 20
    local h = #rows * (H_SMALL + 2) + 12
    -- by the cursor, kept on screen (the banner is small: it may draw past its own edges)
    local mx, my = ui:getMouseX(), ui:getMouseY()
    local x = math.min(mx + 14, getCore():getScreenWidth() - ui:getAbsoluteX() - w - 4)
    local y = my + 20
    if ui:getAbsoluteY() + y + h > getCore():getScreenHeight() - 4 then y = my - h - 8 end
    round(ui, x, y, w, h, 6, C.raised, 0.97)
    outline(ui, x, y, w, h, 6, C.edge, 1)
    for i, r in ipairs(rows) do
        local f = r[2] and bold or text
        f(ui, r[1], x + 10, y + 6 + (i - 1) * (H_SMALL + 2), r[2] and C.text or C.muted)
    end
end
local function card(ui, x, y, w, h)
    round(ui, x, y, w, h, 10, C.panel)
    outline(ui, x, y, w, h, 10, C.line)
end

--- A button drawn in place (for lists that scroll), in our buttons' colours: "primary" red, else raised.
local function flatButton(ui, label, x, y, mx, my, kind)
    local w, h = width(label) + 28, BAR
    local over = mx >= x and mx < x + w and my >= y and my < y + h
    if kind == "quiet" then
        if over then round(ui, x, y, w, h, 6, C.hover) end
    else
        round(ui, x, y, w, h, 6, kind == "primary" and (over and C.accentHover or C.accent) or (over and C.hover or C.raised))
        outline(ui, x, y, w, h, 6, kind == "primary" and C.accent or C.line)
    end
    text(ui, label, x + 14, y + (h - H_SMALL) / 2, C.text)
    return w, h, over
end

--- A chip ("Needed by 3"); `c` outlined in that colour, else a dark chip. Returns its right edge.
local function chip(ui, label, x, y, c)
    local w, h = width(label) + 12, H_SMALL + 2
    if c then
        round(ui, x, y, w, h, 3, c, 0.1)
        outline(ui, x, y, w, h, 3, c, 0.7)
    else
        round(ui, x, y, w, h, 3, C.raised)
        outline(ui, x, y, w, h, 3, C.line)
    end
    text(ui, label, x + 6, y + 1, c or C.text)
    return x + w
end
local function tick(ui, x, y, c)
    for i, d in ipairs({ { 0, 0 }, { 2, 2 }, { 4, 0 }, { 6, -2 }, { 8, -4 } }) do rect(ui, x + d[1], y + d[2], 2, 2, c) end
end
--- A source pill ending at `right` ("Subscribed" in green with a tick, as in the app); returns its left edge.
local function pill(ui, label, right, y, c, ticked)
    local w, h = width(label) + (ticked and 30 or 18), H_SMALL + 4
    local x = right - w
    round(ui, x, y, w, h, 6, c, 0.1)
    outline(ui, x, y, w, h, 6, c, 0.6)
    local tx = x + 9
    if ticked then
        tick(ui, tx, y + h / 2, c)
        tx = tx + 14
    end
    text(ui, label, tx, y + 2, c)
    return x
end
local function handle(ui, x, y)
    for i = 0, 2 do
        rect(ui, x, y + i * 5, 2, 2, C.muted, 0.6)
        rect(ui, x + 5, y + i * 5, 2, 2, C.muted, 0.6)
    end
end
local function checkbox(ui, x, y, on, bad)
    if on then
        round(ui, x, y, BOX, BOX, 3, C.accent)
        tick(ui, x + 3, y + 8, C.text)
    else
        round(ui, x, y, BOX, BOX, 3, C.raised)
        outline(ui, x, y, BOX, BOX, 3, bad and C.danger or C.edge)
    end
end

-- --- buttons: rounded, in the app's colours (only ours: the game's other buttons keep theirs)

local buttonPrerender = ISButton.prerender
function ISButton:prerender()
    if not self.pzmmRound or self.isJoypad then return buttonPrerender(self) end
    self.fade:setFadeIn((self.mouseOver and self:isMouseOver()) and self.enable or false)
    self.fade:update()
    local f = self.pressed and 1 or self.fade:fraction()
    local a, b = self.backgroundColor, self.backgroundColorMouseOver
    local c = { r = b.r * f + a.r * (1 - f), g = b.g * f + a.g * (1 - f), b = b.b * f + a.b * (1 - f), a = (b.a * f + a.a * (1 - f)) * (self.enable and 1 or 0.5) }
    round(self, 0, 0, self.width, self.height, 6, c)
    if self.borderColor.a > 0 then outline(self, 0, 0, self.width, self.height, 6, self.borderColor) end
    self:updateTooltip()
end

--- `kind`: "primary" (blood red), "quiet" (no fill until hovered), else a dark button.
local function button(parent, x, y, label, kind, onClick, w)
    local b = ISButton:new(x, y, w or (width(label) + 28), BAR, label, parent, onClick)
    b:initialise()
    b:instantiate()
    b.pzmmRound = true
    b.backgroundColor = kind == "primary" and copy(C.accent) or kind == "quiet" and copy(C.panel, 0) or copy(C.raised)
    b.backgroundColorMouseOver = kind == "primary" and copy(C.accentHover) or copy(C.hover)
    b.borderColor = kind == "primary" and copy(C.accent) or kind == "quiet" and copy(C.line, 0) or copy(C.line)
    b.textColor = copy(C.text)
    parent:addChild(b)
    return b
end

-- --- what's shown: the game's mod data, in sections, filtered by the tab and the search

--- 1 Workshop, 2 on this PC, 3 can't load on this build.
local function source(d)
    if not d.isAvailable then return 3 end
    local ws = d.modInfo:getWorkshopID()
    return (ws and ws ~= "" and ws ~= "0") and 1 or 2
end
local SOURCES = { "Subscribed", "On this PC", "Can't load" }

--- The active PZMM profile's library (Zomboid/Lua/pzmm/library.txt, which the app writes): its
--- name, its sections in order, and each mod's section and place. nil without the app.
local function readLibrary()
    local ok, lib = pcall(function()
        local r = getFileReader("pzmm/library.txt", false)
        if not r then return nil end
        local lib = { sections = {}, of = {}, pos = {} }
        local section, n = nil, 0
        local line = r:readLine()
        while line do
            local profile = line:match("^# PZMM profile: (.*)$")
            local name = line:match("^# %-%-%-%s*(.*)$")
            if profile then
                lib.profile = profile
            elseif name then
                section = name ~= "" and name or "Other"
                lib.sections[#lib.sections + 1] = section
            else
                local id = line:match("^[+-](.+)$")
                if id then
                    n = n + 1
                    lib.of[id], lib.pos[id] = section or "Other", n
                end
            end
            line = r:readLine()
        end
        r:close()
        return lib
    end)
    return ok and lib or nil
end

--- The PZMM app, when it's installed: it writes Zomboid/Lua/pzmm/app.txt (version=..., started=...)
--- each time it starts. nil when it never has.
local function readApp()
    local ok, app = pcall(function()
        local r = getFileReader("pzmm/app.txt", false)
        if not r then return nil end
        local app = {}
        local line = r:readLine()
        while line do
            local k, v = line:match("^(%w+)=(.*)$")
            if k then app[k] = v end
            line = r:readLine()
        end
        r:close()
        return app.version and app or nil
    end)
    return ok and app or nil
end

local PAGES = {
    { "home", "Home", "Home\nYour mods, problems and updates at a glance, and Continue.", "drawHome" },
    { "mods", "My mods", "My mods\nTurn mods on and off, in your PZMM profile's sections." },
    { "feed", "What's new", "What's new\nChange notes for your Workshop mods updated since you last played." },
    { "workshop", "Browse Workshop", "Browse Workshop\nOnly in the PZMM app.", "drawWorkshop" },
    { "maps", "Maps", "Maps\nYour maps' load order and clashes.", "drawMaps" },
    { "servers", "Servers", "Servers\nYour saved servers, and the game's server browser.", "drawServers" },
    { "saves", "Saves", "Saves\nPlay, open or delete a save, and see its mods.", "drawSaves" },
    { "java", "Java mods", "Java mods\nZombieBuddy, and the mods that ship Java.", "drawJava" },
    { "make", "Make a mod", "Make a mod\nOnly in the PZMM app.", "drawMake" },
    { "settings", "PZMM Settings", "PZMM Settings\nThis in-game mod manager's options.", "drawSettings" },
}
local PAGE = {}
for _, pg in ipairs(PAGES) do PAGE[pg[1]] = pg end
local TABS = { "All", "Enabled", "Disabled", "Problems" }
local TAB_TIPS = {
    All = "All\nEvery mod the game found.",
    Enabled = "Enabled\nMods that load with your next game.",
    Disabled = "Disabled\nMods you have but aren't loading.",
    Problems = "Problems\nMods that can't load on this build of the game, or clash with another mod.",
}
--- Required mods that aren't installed at all (set by updateView: id -> true).
local MISSING = {}
local function missingReqs(d)
    local out = {}
    for id, _ in pairs(d.requireMods or {}) do if MISSING[id] then out[#out + 1] = id end end
    return out
end
local SHOWN = {
    Enabled = function(d) return d.isActive end,
    Disabled = function(d) return not d.isActive and d.isAvailable end,
    Problems = function(d) return not d.isAvailable or d.isIncompatible or (d.isActive and #missingReqs(d) > 0) end,
}

--- Steam's word for an item's download state, as we show it: { chip, colour, tooltip } (nil: nothing to say).
local STEAM_STATES = {
    Downloading = { "Downloading", "warn", "Downloading\nSteam is still downloading it. Wait for it to finish before you load a save." },
    DownloadPending = { "Download waiting", "warn", "Download waiting\nSteam hasn't started downloading it yet." },
    NeedsUpdate = { "Update waiting", "warn", "Update waiting\nSteam has a newer version it hasn't downloaded yet. Restart Steam or the game to get it." },
    Error = { "Steam error", "danger", "Steam error\nSteam couldn't download it. Try Unsubscribe and Subscribe again in Steam." },
}

PZMMMods = ISPanelJoypad:derive("PZMMMods")

function PZMMMods:new(x, y, w, h)
    local o = ISPanelJoypad:new(x, y, w, h)
    setmetatable(o, self)
    self.__index = self
    o.background = false
    o.model = ModSelector.Model:new(o)
    o.tab = "All"
    o.collapsed = {}
    o.entries = {}
    o.scroll, o.feedScroll = 0, 0
    o.feedOpen, o.feedMore = {}, {}
    o.selected = nil
    o.needed, o.place = {}, {}
    return o
end

function PZMMMods:createChildren()
    -- the box is drawn behind it (rounded); the field itself is clear, inset from the edges
    self.search = ISTextEntryBox:new("", PAD + 8, TOP + 8 + 2, 180 - 12, BAR - 4)
    self.search.font = SMALL
    self.search:initialise()
    self.search:instantiate()
    self.search.backgroundColor = copy(C.raised, 0)
    self.search.borderColor = copy(C.line, 0)
    self.search.onTextChange = function() self.scroll = 0; self:updateView() end
    pcall(function() self.search.javaObject:setPlaceholderText("Search mods") end)
    self:addChild(self.search)

    local y = self.height - BOTTOM + (BOTTOM - BAR) / 2
    self.back = button(self, PAD, y, getText("UI_btn_back"), nil, PZMMMods.onBack)
    self.vanilla = button(self, self.back:getRight() + 10, y, "Game's mod screen", "quiet", PZMMMods.onVanilla)
    local accept = getText("UI_btn_accept")
    self.accept = button(self, self.width - PAD - width(accept) - 40, y, accept, "primary", PZMMMods.onAccept, width(accept) + 40)
end

-- the model calls this after every change
function PZMMMods:updateView()
    local q = self.search and self.search:getText():lower() or ""
    local keep = SHOWN[self.tab]
    -- sections in the library's order (the app's), else by source; a name used twice is one section
    local names, index, byGroup = {}, {}, {}
    local function section(name)
        if not index[name] then
            names[#names + 1] = name
            index[name] = #names
            byGroup[#names] = {}
        end
        return index[name]
    end
    local lib = self.lib
    if lib then for _, name in ipairs(lib.sections) do section(name) end end
    -- required mods that aren't installed at all: before the tabs count Problems with them
    for k in pairs(MISSING) do MISSING[k] = nil end
    for _, d in pairs(self.model.mods) do
        for id, _ in pairs(d.requireMods or {}) do
            if not self.model.mods[id] then MISSING[id] = true end
        end
    end
    -- mods may have changed: Maps reads the order again when it's next drawn
    self.mapOrder = nil
    local counts = { All = 0, Enabled = 0, Disabled = 0, Problems = 0 }
    self.needed = {}
    for _, d in pairs(self.model.mods) do
        counts.All = counts.All + 1
        for name, f in pairs(SHOWN) do if f(d) then counts[name] = counts[name] + 1 end end
        if d.isActive and d.requireMods then
            for id, _ in pairs(d.requireMods) do self.needed[id] = (self.needed[id] or 0) + 1 end
        end
        local match = q == "" or d.name:lower():find(q, 1, true) or d.modId:lower():find(q, 1, true)
        if match and (not keep or keep(d)) then
            local name = lib and (lib.of[d.modId] or "Other") or SOURCES[source(d)]
            table.insert(byGroup[section(name)], d)
        end
    end
    self.counts = counts
    self.entries = {}
    local pos = lib and lib.pos or {}
    for g, name in ipairs(names) do
        local mods = byGroup[g]
        if #mods > 0 then
            -- the library's order, then by name for mods it doesn't list yet
            table.sort(mods, function(a, b)
                local pa, pb = pos[a.modId] or 1e9, pos[b.modId] or 1e9
                if pa ~= pb then return pa < pb end
                return not string.sort(a.name, b.name)
            end)
            local on = 0
            for _, d in ipairs(mods) do if d.isActive then on = on + 1 end end
            table.insert(self.entries, { head = name, on = on, n = #mods })
            if not self.collapsed[name] then
                for i, d in ipairs(mods) do table.insert(self.entries, { mod = d, last = i == #mods }) end
            end
        end
    end
    -- the load order, as the game will read it
    self.place = {}
    pcall(function()
        local list = self.model:getActiveMods():getMods()
        for i = 0, list:size() - 1 do
            local id = list:get(i)
            self.place[id] = i + 1
        end
    end)
end

-- --- the layout: header, toolbar, the list (left), the load order or a mod's details (right), footer

function PZMMMods:listBox()
    local y = TOP + FILTERS + 12
    return PAD, y, self.width - PAD * 3 - SIDE, self.height - BOTTOM - y
end
function PZMMMods:sideBox()
    local y = TOP + FILTERS + 12
    return self.width - PAD - SIDE, y, SIDE, self.height - BOTTOM - y
end
local function entryHeight(e, i) return e.head and (HEAD + (i > 1 and GAP or 0)) or ROW end

function PZMMMods:prerender()
    rect(self, 0, 0, self.width, self.height, C.bg, 1)
    for x = 0, self.width, 24 do rect(self, x, 0, 1, self.height, C.text, x % 120 == 0 and 0.035 or 0.012) end
    for y = 0, self.height, 24 do rect(self, 0, y, self.width, 1, C.text, y % 120 == 0 and 0.035 or 0.012) end

    -- the app's top row: the skull, then the profile as a button
    rect(self, 0, 0, self.width, ROW1, C.panel, 1)
    rect(self, 0, ROW1 - 1, self.width, 1, C.line, 1)
    image(self, LOGO, PAD, (ROW1 - 44) / 2, 44, 44)
    local px = PAD + 44 + 14
    local py = (ROW1 - BAR) / 2
    local ty = py + (BAR - H_SMALL) / 2
    if self.lib and self.lib.profile then
        local name = self.lib.profile
        local w = width("Profile:") + 6 + width(name) + 1 + 24
        round(self, px, py, w, BAR, 6, C.raised)
        outline(self, px, py, w, BAR, 6, C.line)
        text(self, "Profile:", px + 12, ty, C.muted)
        bold(self, name, px + 12 + width("Profile:") + 6, ty, C.text)
        local mxp, myp = self:getMouseX(), self:getMouseY()
        if mxp >= px and mxp < px + w and myp >= py and myp < py + BAR then tip(self, "Your PZMM profile\nIts sections sort My mods. Switch profiles in the app.") end
    else
        text(self, self.app and "Open PZMM to see your profile's sections here" or "Get the PZMM app: github.com/SEVNC3/pz-mod-manager", px, ty, C.muted)
    end
    -- the app's top bar actions the game can do too, at the right: icon buttons, explained on hover
    local shown = {}
    for _, a in ipairs(ACTIONS) do if not a.app or self.app then shown[#shown + 1] = a end end
    local ax = self.width - PAD - #shown * (BAR + 4) + 4
    local mxa, mya = self:getMouseX(), self:getMouseY()
    self.actionBoxes = {}
    for _, a in ipairs(shown) do
        local over = mxa >= ax and mxa < ax + BAR and mya >= py and mya < py + BAR
        if over then
            round(self, ax, py, BAR, BAR, 6, C.hover)
            outline(self, ax, py, BAR, BAR, 6, C.accent, 0.7)
            tip(self, a[3])
        end
        if a[2] then self:drawTextureScaled(a[2], ax + 8, py + 8, BAR - 16, BAR - 16, over and 1 or 0.7, 1, 1, 1) end
        self.actionBoxes[#self.actionBoxes + 1] = { a[1], ax, py }
        ax = ax + BAR + 4
    end

    -- its tab bar: My mods, the active tab (soft red fill, light edges, white underline)
    rect(self, 0, ROW1, self.width, ROW2, C.panel, 1)
    rect(self, 0, TOP - 1, self.width, 1, C.line, 1)
    local tx = 12
    local mx0, my0 = self:getMouseX(), self:getMouseY()
    self.pageBoxes = {}
    for _, pg in ipairs(PAGES) do
        local feed = self.feed or (not self.app and self:liteFeed())
        -- What's new: the updates you haven't seen there yet
        local n = 0
        if pg[1] == "feed" and feed then
            for _, e in ipairs(feed.entries) do if e.updated > (self.feedSeen or 0) then n = n + 1 end end
        end
        local badge = n > 0 and tostring(n) or nil
        local tw = width(pg[2]) + 28 + (badge and width(badge) + 18 or 0)
        local on = self.page == pg[1]
        local over = mx0 >= tx and mx0 < tx + tw and my0 >= ROW1 and my0 < TOP
        if over then tip(self, pg[3]) end
        if on or over then
            round(self, tx, ROW1, tw, ROW2 - 1, 3, C.accent, 0.32, true, false)
            rect(self, tx + 3, ROW1, tw - 6, 1, C.accent, 0.7)
            rect(self, tx, ROW1 + 3, 1, ROW2 - 4, C.accent, 0.7)
            rect(self, tx + tw - 1, ROW1 + 3, 1, ROW2 - 4, C.accent, 0.7)
            rect(self, tx, TOP - 3, tw, 2, C.text, 1)
        end
        local ty = ROW1 + (ROW2 - H_SMALL) / 2
        text(self, pg[2], tx + 14, ty, (on or over) and hex("ffffff") or C.muted)
        if badge then
            local bxp = tx + 14 + width(pg[2]) + 6
            round(self, bxp, ty, width(badge) + 12, H_SMALL, 6, C.accent)
            text(self, badge, bxp + 6, ty, hex("ffffff"))
        end
        self.pageBoxes[#self.pageBoxes + 1] = { pg[1], tx, tw }
        tx = tx + tw + 2
    end
    local lx = self.width - PAD - #LINKS * 34 + 4
    local ly = ROW1 + (ROW2 - 30) / 2
    local mx0, my0 = self:getMouseX(), self:getMouseY()
    self.linkBoxes = {}
    for _, l in ipairs(LINKS) do
        local over = mx0 >= lx and mx0 < lx + 30 and my0 >= ly and my0 < ly + 30
        if over then
            tip(self, l[4])
            round(self, lx, ly, 30, 30, 6, C.hover)
            outline(self, lx, ly, 30, 30, 6, C.accent, 0.7)
        end
        if l[1] then self:drawTextureScaled(l[1], lx + 5, ly + 5, 20, 20, over and 1 or 0.85, 1, 1, 1) end
        self.linkBoxes[#self.linkBoxes + 1] = { l[2], lx, ly }
        lx = lx + 34
    end

    self.search:setVisible(self.page == "mods")
    if self.page == "feed" then
        self:drawFeed()
    elseif self.page ~= "mods" then
        self:drawPage()
    else
    -- the filters bar: search, then All / Enabled / Disabled / Problems as one segmented control
    rect(self, 0, TOP + FILTERS - 1, self.width, 1, C.line, 1)
    local y = TOP + 8
    round(self, PAD, y, 180, BAR, 6, C.raised)
    outline(self, PAD, y, 180, BAR, 6, C.line)
    local x = PAD + 180 + 8
    self.tabBoxes = {}
    local labels, segW = {}, 0
    for i, t in ipairs(TABS) do
        local n = self.counts and self.counts[t] or 0
        labels[i] = (t == "Problems") and (t .. " (" .. n .. ")") or t
        segW = segW + width(labels[i]) + 24
    end
    local mx, my = self:getMouseX(), self:getMouseY()
    for i, t in ipairs(TABS) do
        local w = width(labels[i]) + 24
        local on = self.tab == t
        if on then roundEnds(self, x, y, w, BAR, 6, C.accent, 0.32, i == 1, i == #TABS) end
        local over = mx >= x and mx < x + w and my >= y and my < y + BAR
        if over then tip(self, TAB_TIPS[t]) end
        local s = on and bold or text
        s(self, labels[i], x + 12, y + (BAR - H_SMALL) / 2, (on or over) and C.text or C.muted)
        self.tabBoxes[#self.tabBoxes + 1] = { t, x, y, w, BAR }
        x = x + w
    end
    outline(self, PAD + 188, y, segW, BAR, 6, C.line)

    self:drawList()
    self:drawSide()
    end

    -- footer
    rect(self, 0, self.height - BOTTOM, self.width, BOTTOM, C.panel, 1)
    rect(self, 0, self.height - BOTTOM, self.width, 1, C.line, 1)
    local changed = 0
    for _, d in pairs(self.model.mods) do if d.isActive ~= d.defaultActive then changed = changed + 1 end end
    if self.mapsChanged then changed = changed + 1 end
    if changed > 0 then
        local s = changed .. " change" .. (changed == 1 and "" or "s") .. " not saved yet"
        text(self, s, self.accept:getX() - width(s) - 16, self.height - BOTTOM + (BOTTOM - H_SMALL) / 2, C.muted)
    end
    if self.back:isMouseOver() then tip(self, "Back\nTo the main menu. Changes not saved yet: you're asked first.") end
    if self.vanilla:isMouseOver() then tip(self, "Game's mod screen\nThe game's own mod screen, for presets and load order.") end
    if self.accept:isMouseOver() then tip(self, "Accept\nSave your mods. The game restarts its Lua when the mod set needs it.") end
end

function PZMMMods:render()
    ISPanelJoypad.render(self)
    self:launchIfNeeded()
    -- the status line, left in the footer after Game's mod screen
    if self.status and getTimestampMs() < self.status[2] then
        local x = self.vanilla:getRight() + 24
        text(self, fit(self.status[1], self.accept:getX() - 300 - x), x, self.height - BOTTOM + (BOTTOM - H_SMALL) / 2, C.ok)
    end
    drawTip(self)
end
--- Say what just happened, in the footer's status line, for five seconds.
function PZMMMods:say(msg) self.status = { msg, getTimestampMs() + 5000 } end

function PZMMMods:drawList()
    local lx, ly, lw, lh = self:listBox()
    self:setStencilRect(lx, ly, lw, lh)
    local mx, my = self:getMouseX(), self:getMouseY()
    local y = ly - self.scroll
    local w = lw - 14
    self.hits = {}
    for i, e in ipairs(self.entries) do
        local h = entryHeight(e, i)
        if y + h >= ly and y <= ly + lh then
            if e.head then
                local top = y + (i > 1 and GAP or 0)
                round(self, lx, top, w, HEAD, 10, C.panel, nil, true, false)
                handle(self, lx + 12, top + HEAD / 2 - 6)
                text(self, self.collapsed[e.head] and ">" or "v", lx + 30, top + (HEAD - H_SMALL) / 2, C.muted)
                local right = tape(self, e.head, lx + 48, top + HEAD / 2)
                text(self, e.on .. " of " .. e.n .. " enabled", right + 12, top + (HEAD - H_SMALL) / 2, C.muted)
                if self.collapsed[e.head] then
                    round(self, lx, top + HEAD - 10, w, 10, 10, C.panel, nil, false, true)
                end
                self.hits[#self.hits + 1] = { kind = "head", g = e.head, x = lx, y = top, w = w, h = HEAD }
            else
                self:drawRow(e, lx, y, w, mx, my)
            end
        end
        y = y + h
    end
    if #self.entries == 0 then
        local q = self.search and self.search:getText() or ""
        local msg = (q ~= "" and ("No mods match \"" .. q .. "\".")) or (self.tab ~= "All" and ("No mods in " .. self.tab .. ".")) or "No mods found. Subscribe to some on the Workshop, then Rescan."
        text(self, msg, lx + math.floor((w - width(msg)) / 2), ly + 40, C.muted)
    end
    self:clearStencilRect()
    self.listHeight = y + self.scroll - ly
    -- a thin scrollbar when it doesn't all fit
    if self.listHeight > lh then
        local th = math.max(30, lh * lh / self.listHeight)
        local ty = ly + (lh - th) * self.scroll / (self.listHeight - lh)
        round(self, lx + lw - 8, ty, 6, th, 3, C.edge, 0.8)
    end
end

function PZMMMods:drawRow(e, x, y, w, mx, my)
    local d = e.mod
    round(self, x, y, w, ROW, 10, C.panel, nil, false, e.last)
    rect(self, x + 12, y, w - 24, 1, C.line, 1)
    local over = mx >= x and mx < x + w and my >= y and my < y + ROW
    local h = ROW - (e.last and 10 or 0)
    if self.selected == d then
        rect(self, x, y, w, h, C.accent, 0.35)
        rect(self, x, y, 3, h, C.accentText, 1)
    elseif over then
        rect(self, x, y, w, h, C.accent, 0.14)
    end
    handle(self, x + 12, y + ROW / 2 - 6)
    local bx, by = x + 32, y + (ROW - BOX) / 2
    checkbox(self, bx, by, d.isActive, not d.isAvailable)
    local mid = y + (ROW - H_SMALL) / 2
    local n = d.isActive and self.place[d.modId]
    if n then text(self, tostring(n), x + 84 - width(tostring(n)), mid, C.accentText) end

    -- right: where it's from, then the favourite star
    local sx = x + w - 30
    local st = d.favorite and STAR_ON or STAR_OFF
    if st then
        local c = d.favorite and hex("e6d8ad") or C.muted
        self:drawTextureScaled(st, sx, y + (ROW - H_SMALL) / 2, H_SMALL, H_SMALL, d.favorite and 1 or 0.35, c.r, c.g, c.b)
    end
    local right = sx - 10
    local py = y + (ROW - H_SMALL - 4) / 2
    local src = source(d)
    if src == 1 then
        right = pill(self, "Subscribed", right, py, C.ok, true)
    elseif src == 2 then
        right = pill(self, "Local", right, py, C.muted)
    end

    -- name (bold), mod ID, then chips; cut short before the pill
    local tx = x + 98
    local c = (not d.isAvailable and C.danger) or (d.isIncompatible and C.warn) or (d.isActive and C.text) or C.muted
    local room = right - 16 - tx
    local name = fit(d.name, room)
    bold(self, name, tx, mid, c)
    tx = tx + width(name) + 12
    if tx + width(d.modId) < right - 16 then
        text(self, d.modId, tx, mid, copy(C.muted, 0.75))
        tx = tx + width(d.modId) + 12
    end
    local cy = y + (ROW - H_SMALL - 2) / 2
    local need = self.needed[d.modId]
    if need and tx + 90 < right then tx = chip(self, "Needed by " .. need, tx, cy) + 6 end
    if not d.isAvailable and tx + 150 < right then
        tx = chip(self, "Can't load on this build", tx, cy, C.danger) + 6
    elseif d.isIncompatible and tx + 90 < right then
        tx = chip(self, "Incompatible", tx, cy, C.warn) + 6
    end
    local miss = d.isActive and missingReqs(d) or {}
    if #miss > 0 and tx + 130 < right then
        local cx = tx
        tx = chip(self, "Missing " .. #miss .. " required", tx, cy, C.danger) + 6
        if mx >= cx and mx < tx and my >= y and my < y + ROW then tip(self, "Missing requirements\n" .. table.concat(miss, ", ") .. "\nPick the mod to subscribe to what it needs.") end
    end
    local st = self:steamOf(d)
    local look = st and STEAM_STATES[st.state]
    if look and tx + 110 < right then
        local cx = tx
        tx = chip(self, look[1], tx, cy, C[look[2]]) + 6
        if mx >= cx and mx < tx and my >= y and my < y + ROW then tip(self, look[3]) end
    end
    if mx >= bx - 6 and mx < bx + BOX + 6 and my >= by - 6 and my < by + BOX + 6 then
        tip(self, not d.isAvailable and "Can't load\nThis mod doesn't load on this build of the game." or d.isActive and "Turn off" or "Turn on")
    elseif mx >= sx - 4 and mx < sx + 26 and my >= y and my < y + ROW then
        tip(self, d.favorite and "Unfavourite" or "Favourite\nFavourites sort first in the game's own mod screen.")
    end
    self.hits[#self.hits + 1] = { kind = "box", mod = d, x = bx - 6, y = by - 6, w = BOX + 12, h = BOX + 12 }
    self.hits[#self.hits + 1] = { kind = "star", mod = d, x = sx - 4, y = y, w = 30, h = ROW }
    self.hits[#self.hits + 1] = { kind = "row", mod = d, x = x, y = y, w = w, h = ROW }
end

-- the right panel: the picked mod's details
function PZMMMods:drawSide()
    local x, y, w, h = self:sideBox()
    card(self, x, y, w, h)
    if self.selected then
        self:setStencilRect(x, y, w, h)
        self:drawDetails(self.selected, x + 18, y + 16, w - 36, y + h)
        self:clearStencilRect()
        return
    end
    local hint = "Pick a mod to see its details."
    text(self, hint, x + (w - width(hint)) / 2, y + h / 2 - H_SMALL / 2, C.muted)
end

function PZMMMods:drawDetails(d, x, y, w, bottom)
    local info = d.modInfo
    if not d.poster then
        d.poster = false
        pcall(function() if info:getPosterCount() > 0 then d.poster = getTexture(info:getPoster(0)) or false end end)
    end
    if d.poster then
        local pw = math.min(w, 220)
        self:drawTextureScaledAspect(d.poster, x, y, pw, pw, 1, 1, 1, 1)
        y = y + pw + 14
    end
    -- the close X at the top right, as the app's details
    local mx, my = self:getMouseX(), self:getMouseY()
    local cxw = width("Close") + 28
    local _, _, cover = flatButton(self, "Close", x + w - cxw, y, mx, my, "quiet")
    if cover then tip(self, "Close\nBack to the load order.") end
    self.hits[#self.hits + 1] = { kind = "fn", fn = function() self.selected = nil end, x = x + w - cxw, y = y, w = cxw, h = BAR }
    for _, l in ipairs(wrap(d.name, w - cxw - 8, MEDIUM)) do
        bold(self, l, x, y, C.text, MEDIUM)
        y = y + H_MEDIUM + 2
    end
    y = y + 8
    -- its actions: on or off (red when off), its Workshop page, its files
    local ws
    pcall(function() ws = d.modInfo:getWorkshopID() end)
    local acts = { { d.isActive and "Turn off" or "Turn on", not d.isActive and "primary" or nil, function() self:toggle(d) end } }
    if ws and ws ~= "" and ws ~= "0" then acts[#acts + 1] = { "Workshop page", nil, function() pcall(function() activateSteamOverlayToWorkshopItem(ws) end) end } end
    acts[#acts + 1] = { "Show files", nil, function() pcall(function() showFolderInDesktop(d.modInfo:getDir()) end) end }
    local ax = x
    for _, a in ipairs(acts) do
        local aw = width(a[1]) + 28
        if ax + aw > x + w then break end
        flatButton(self, a[1], ax, y, mx, my, a[2])
        self.hits[#self.hits + 1] = { kind = "fn", fn = a[3], x = ax, y = y, w = aw, h = BAR }
        ax = ax + aw + 8
    end
    y = y + BAR + 14
    local function field(label, value)
        if not value or value == "" then return end
        text(self, label, x, y, C.muted)
        text(self, fit(tostring(value), w - 110), x + 110, y, C.text)
        y = y + H_SMALL + 6
    end
    field("Mod ID", d.modId)
    pcall(function() field("Author", info:getAuthor()) end)
    pcall(function() field("Workshop", info:getWorkshopID()) end)
    pcall(function() field("Game version", (info:getVersionMin() and info:getVersionMin():toString() or "") .. (info:getVersionMax() and (" to " .. info:getVersionMax():toString()) or "")) end)
    local ids = {}
    for id, _ in pairs(d.requireMods or {}) do ids[#ids + 1] = id end
    table.sort(ids)
    if #ids > 0 then
        -- each required mod and where it stands: on, off, or not installed
        text(self, "Requires", x, y, C.muted)
        for _, id in ipairs(ids) do
            local r = self.model.mods[id]
            local state, c = "not installed", C.danger
            if r then state, c = r.isActive and "on" or "off", r.isActive and C.ok or C.warn end
            text(self, fit(id, w - 110 - width(state) - 12), x + 110, y, C.text)
            text(self, state, x + w - width(state), y, c)
            y = y + H_SMALL + 6
        end
    end
    -- Workshop items it needs that you aren't subscribed to: Subscribe opens them in Steam
    local st = self:steamOf(d)
    for _, item in ipairs(st and st.missing or {}) do
        local title = (self.steam[item] and self.steam[item].title) or ("Workshop item " .. item)
        text(self, fit(title, w - 130), x, y + (BAR - H_SMALL) / 2, C.text)
        local label = self.app and "Install with PZMM" or "Subscribe"
        local bw = width(label) + 28
        local bx = x + w - bw
        local _, _, over = flatButton(self, label, bx, y, self:getMouseX(), self:getMouseY(), "primary")
        if over then tip(self, self.app and (label .. "\nOpens PZMM, which installs " .. title .. " once you say yes.") or ("Subscribe\nOpens " .. title .. " in Steam: subscribe there and it downloads.")) end
        local act = self.app and function() self:request("install\t" .. item, title .. " asked for.") end or function() pcall(function() activateSteamOverlayToWorkshopItem(item) end) end
        self.hits[#self.hits + 1] = { kind = "fn", fn = act, x = bx, y = y, w = bw, h = BAR }
        y = y + BAR + 6
    end
    local need = self.needed[d.modId]
    if need then field("Needed by", need .. " enabled mod" .. (need == 1 and "" or "s")) end
    y = y + 8
    local desc = ""
    pcall(function() desc = info:getDescription() or "" end)
    desc = desc:gsub("<[^>]*>", ""):gsub("%[[^%]]*%]", "")
    for _, l in ipairs(wrap(desc, w)) do
        if y + H_SMALL > bottom - 12 then break end
        text(self, l, x, y, C.muted)
        y = y + H_SMALL + 2
    end
end

-- --- What's new: the app's feed (Zomboid/Lua/pzmm/feed.txt, written when it loads change notes),
-- drawn as the app's What's new tab: scraps of paper for the latest, then every change note by day

local INK, INK_SOFT, BLOOD = hex("24211c"), hex("5d574c"), hex("6a0a0d")
local SCRAPS = {}
for i = 1, 8 do SCRAPS[i] = tex("scrap_" .. i) end
local function inside(b, x, y) return b and x >= b.x and x < b.x + b.w and y >= b.y and y < b.y + b.h end
local ROWS = 2 -- rows of scraps on the board; the rest are in the history
local SHOWN_NOTES = 3 -- notes an open entry shows before "Show older"
local FINE = "\"May break saves\" is a hint: PZMM flags map mods, new major versions and notes that mention a new game or saves. Back up a save before you load it with these."

--- The game opens only its own sites (LuaManager.isIndieStoneUrl: steamcommunity.com, projectzomboid.com,
--- theindiestone.com, pzwiki.net) and drops the rest without a word. Any other link goes through Steam's
--- link filter, which is on that list and passes you on to the site.
local OWN_SITES = { "https://steamcommunity.com", "https://projectzomboid.com", "https://theindiestone.com", "https://pzwiki.net" }
local function allowed(url)
    for _, site in ipairs(OWN_SITES) do
        if url == site or url:sub(1, #site + 1) == site .. "/" then return true end
    end
    return false
end
--- The browser, or the Steam overlay where the game can't open one.
local function openLink(url)
    if not allowed(url) then
        url = "https://steamcommunity.com/linkfilter/?u=" .. url:gsub("[^%w%-_%.~]", function(c) return string.format("%%%02X", c:byte()) end)
    end
    if not pcall(function() openUrl(url) end) then pcall(function() activateSteamOverlayToWebPage(url) end) end
end

local function split(str, sep)
    local out, start = {}, 1
    while true do
        local i = str:find(sep, start, true)
        if not i then
            out[#out + 1] = str:sub(start)
            return out
        end
        out[#out + 1] = str:sub(start, i - 1)
        start = i + #sep
    end
end
-- the app writes newlines as \n and backslashes as \\
local function unflat(str) return (str:gsub("\\(.)", function(c) return c == "n" and "\n" or c end)) end

--- The newest update you've seen in What's new (Zomboid/Lua/pzmm/feedseen.txt): its badge counts newer ones.
local function readFeedSeen()
    local ok, t = pcall(function()
        local r = getFileReader("pzmm/feedseen.txt", false)
        if not r then return nil end
        local line = r:readLine()
        r:close()
        return tonumber(line)
    end)
    return ok and t or 0
end

local function readPlayed()
    local ok, t = pcall(function()
        local r = getFileReader("pzmm/played.txt", false)
        if not r then return nil end
        local line = r:readLine()
        r:close()
        return tonumber(line)
    end)
    return ok and t or nil
end

local function readFeed()
    local ok, f = pcall(function()
        local r = getFileReader("pzmm/feed.txt", false)
        if not r then return nil end
        local f, e = { entries = {} }, nil
        local line = r:readLine()
        while line do
            local p = split(line, "\t")
            if p[1] == "entry" then
                e = { item = p[2], updated = tonumber(p[3]) or 0, pending = p[4] == "1", risks = (p[5] or "") ~= "" and split(p[5], ",") or {},
                    from = unflat(p[6] or ""), to = unflat(p[7] or ""), title = unflat(p[8] or ""), loaded = p[9] ~= "0", mods = {}, notes = {}, logs = {} }
                f.entries[#f.entries + 1] = e
            elseif p[1] == "mods" and e then
                e.mods = (p[2] or "") ~= "" and split(p[2], ",") or {}
            elseif p[1] == "note" and e then
                e.notes[#e.notes + 1] = { time = tonumber(p[2]) or 0, text = unflat(p[3] or "") }
            elseif p[1] == "log" and e then
                e.logs[#e.logs + 1] = { title = unflat(p[2] or ""), text = unflat(p[3] or "") }
            else
                local k, v = line:match("^(%a+)=(.*)$")
                if k then f[k] = v end
            end
            line = r:readLine()
        end
        r:close()
        return f
    end)
    return ok and f or nil
end

--- A note as plain text: BBCode tags, Markdown marks and runs of blank lines out.
local function plain(str)
    str = str:gsub("%[/?[%a%*][^%]]*%]", ""):gsub("\n#+%s*", "\n"):gsub("^#+%s*", ""):gsub("[%*_`]+", ""):gsub("\n\n\n+", "\n\n")
    return (str:gsub("^%s+", ""):gsub("%s+$", ""))
end
--- Something to read, or notes not loaded yet (they may have some).
local function hasNotes(e)
    if not e.loaded then return true end
    for _, n in ipairs(e.notes) do
        if n.text:find("%S") then return true end
    end
    return #e.logs > 0
end
local function excerpt(e)
    local raw = (e.notes[1] and e.notes[1].text) or (e.logs[1] and e.logs[1].text) or ""
    return plain(raw)
end
local function date(t, long)
    local ok, out = pcall(function() return os.date(long and "%A %d %B %Y" or "%d %B %Y", t) end)
    return ok and out or ""
end
local RISKS = { map = "It adds a map.", version = "A major version jump.", notes = "Its notes mention a new game or wiped saves." }
local function riskTip(e)
    local why = {}
    for _, r in ipairs(e.risks) do why[#why + 1] = RISKS[r] or r end
    return "May break saves\n" .. table.concat(why, " ") .. "\nBack up a save before you load it with this."
end
local function scrapOf(item)
    local n = 0
    for i = 1, #item do n = n + item:byte(i) end
    return n % 8 + 1
end

--- Opening What's new clears its badge, until a newer update comes (called while it's drawn: writes only on news).
function PZMMMods:sawFeed()
    local f = self.feed or (not self.app and self:liteFeed())
    local newest = self.feedSeen or 0
    for _, e in ipairs(f and f.entries or {}) do newest = math.max(newest, e.updated) end
    if newest == self.feedSeen then return end
    self.feedSeen = newest
    pcall(function()
        local w = getFileWriter("pzmm/feedseen.txt", true, false)
        w:write(tostring(newest))
        w:close()
    end)
end

function PZMMMods:feedBox() return PAD, TOP + 12, self.width - PAD * 2, self.height - BOTTOM - TOP - 12 end

function PZMMMods:entryOn(e)
    for _, id in ipairs(e.mods) do
        local d = self.model.mods[id]
        if d and d.isActive then return true end
    end
    return false
end

--- A link-like button on paper (the app's .ink): white wash, a thin ink line, blood when hovered.
local function inkButton(ui, label, x, y, mx, my)
    local w, h = width(label) + 20, H_SMALL + 10
    local over = mx >= x and mx < x + w and my >= y and my < y + h
    round(ui, x, y, w, h, 6, over and BLOOD or hex("ffffff"), over and 1 or 0.25)
    outline(ui, x, y, w, h, 6, over and BLOOD or INK, over and 1 or 0.35)
    text(ui, label, x + 10, y + 5, over and hex("ffffff") or INK)
    return w, h
end

local UH_OH = tex("uh_oh")
local SORRY = "Because of limits on what mods can do inside the game, What's new only works with the PZMM Mod Manager app installed. The app fetches your mods' change notes from Steam and brings them here."
local SORRY_LINKS = {
    { icon = LINKS[1][1], url = LINKS[1][2], title = "Download on GitHub", sub = "Get the PZMM Mod Manager app" },
    { icon = LINKS[2][1], url = LINKS[2][2], title = "Join the Discord", sub = "Help, ideas and the community" },
}

--- What can't work here: the raccoon's sign, why (`body`, What's new's by default), and its `links` (the
--- app's GitHub and Discord by default), filling `w` by `h`. Returns its height.
function PZMMMods:drawSorry(x, y, w, h, mx, my, body, links, title)
    body, links, title = body or SORRY, links or SORRY_LINKS, title or "We're sorry"
    local LARGE = UIFont.Large
    local H_LARGE = TM:getFontHeight(LARGE)
    -- whole pixels throughout: a fractional edge leaves a seam between round()'s strips
    x, y, w, h = math.floor(x), math.floor(y), math.floor(w), math.floor(h)
    local pad, gap = 40, 48
    local img = math.floor(math.max(160, math.min(h - pad * 2, w * 0.4)))
    local tw = math.min(600, w - pad * 2 - gap - img) -- a readable measure, the pair centred in the card
    local gx = x + math.floor((w - img - gap - tw) / 2)
    local tx = gx + img + gap
    local lines = wrap(body, tw, MEDIUM)
    local stacked = tw < 520 or #links > 2
    local bw = stacked and tw or math.floor((tw - 16) / 2)
    local bh = H_MEDIUM + H_SMALL + 30
    local banners = stacked and #links * (bh + 16) - 16 or bh
    local body = H_LARGE + 16 + #lines * (H_MEDIUM + 4) + 28 + banners
    h = math.max(h, img + pad * 2, body + pad * 2)
    round(self, x, y, w, h, 10, C.panel)
    outline(self, x, y, w, h, 10, C.line, 1)
    image(self, UH_OH, gx, y + math.floor((h - img) / 2), img, img)
    local ty = y + math.floor((h - body) / 2)
    bold(self, title, tx, ty, C.text, LARGE)
    ty = ty + H_LARGE + 16
    for _, l in ipairs(lines) do
        text(self, l, tx, ty, C.muted, MEDIUM)
        ty = ty + H_MEDIUM + 4
    end
    ty = ty + 28
    for i, s in ipairs(links) do
        local bx = stacked and tx or tx + (i - 1) * (bw + 16)
        local by = stacked and ty + (i - 1) * (bh + 16) or ty
        local over = mx >= bx and mx < bx + bw and my >= by and my < by + bh
        if over then tip(self, s.title .. "\n" .. (s.url or s.sub)) end
        round(self, bx, by, bw, bh, 10, over and C.hover or C.raised)
        outline(self, bx, by, bw, bh, 10, over and C.accent or C.line, over and 0.9 or 1)
        local lx = s.icon and bx + 66 or bx + 18
        if s.icon then image(self, s.icon, bx + 18, by + (bh - 32) / 2, 32, 32) end
        bold(self, s.title, lx, by + 13, C.text, MEDIUM)
        text(self, s.sub, lx, by + 17 + H_MEDIUM, C.muted)
        self.feedHits[#self.feedHits + 1] = { kind = s.fn and "fn" or "url", url = s.url, fn = s.fn, x = bx, y = by, w = bw, h = bh }
    end
    return h
end

function PZMMMods:drawScrap(e, x, y, w, h, k, mx, my)
    local on = self:entryOn(e)
    local tint = on and 1 or 0.92
    local t = SCRAPS[scrapOf(e.item)]
    if t then self:drawTextureScaled(t, x - 16 * k, y - 16 * k, 332 * k, 262 * k, 1, tint, tint, tint) end
    local px, pw = x + 20, w - 40
    local ty = y + 22
    local title = e.title ~= "" and e.title or e.item
    for i, l in ipairs(wrap(title, pw, MARKER)) do
        if i > 2 then break end
        text(self, l, px, ty, INK, MARKER)
        ty = ty + H_MARKER
    end
    -- the title shows the mod in My mods
    self.feedHits[#self.feedHits + 1] = { kind = "show", e = e, x = px, y = y + 22, w = pw, h = ty - y - 22 }
    if mx >= px and mx < px + pw and my >= y + 22 and my < ty then tip(self, "Show it in My mods") end
    local meta = date(e.updated)
    if e.from ~= "" then meta = meta .. "    " .. e.from .. " -> " .. e.to end
    if not on then meta = meta .. "    Disabled" end
    text(self, meta, px, ty + 2, INK_SOFT)
    ty = ty + H_SMALL + 10
    local actionsY = y + h - 16 - (H_SMALL + 10)
    local body = excerpt(e)
    local italic = body == ""
    if italic then body = ((self.feed == nil or not e.loaded) and "Its change notes are on the Workshop: Read all notes opens them.") or ((#e.notes == 0 and #e.logs == 0) and "The author didn't write change notes for this update." or "(no text)") end
    for _, l in ipairs(wrap(body, pw)) do
        if ty + H_SMALL > actionsY - 6 then break end
        text(self, l, px, ty, italic and INK_SOFT or INK)
        ty = ty + H_SMALL + 3
    end
    local bw, bh = inkButton(self, "Read all notes", px, actionsY, mx, my)
    self.feedHits[#self.feedHits + 1] = { kind = "read", e = e, x = px, y = actionsY, w = bw, h = bh }
    if mx >= px and mx < px + bw and my >= actionsY and my < actionsY + bh then tip(self, "Read all notes\nOpens its full change notes in the history below.") end
    if #e.risks > 0 then
        -- the stamp: blood-red letters in a blood-red frame
        local label = "MAY BREAK SAVES"
        local sw, sh = width(label) + 14, H_SMALL + 6
        local sx, sy = x + w - 20 - sw, actionsY + 2
        outline(self, sx, sy, sw, sh, 3, BLOOD, 0.85)
        outline(self, sx + 1, sy + 1, sw - 2, sh - 2, 3, BLOOD, 0.85)
        bold(self, label, sx + 7, sy + 3, BLOOD)
        if mx >= sx and mx < sx + sw and my >= sy and my < sy + sh then tip(self, riskTip(e)) end
    end
end

--- An entry's open body as lines: { text, colour, bold, gap after, hit kind }.
function PZMMMods:entryLines(e, w)
    local more = self.feedMore[e.item]
    local c = e.cache
    if c and c.w == w and c.more == more then return c.lines end
    local lines = {}
    local function add(str, col, b, gap, kind) lines[#lines + 1] = { str, col, b, gap or 2, kind } end
    add(table.concat(e.mods, ", "), C.muted, false, 8)
    if #e.notes > 0 then
        for i, n in ipairs(e.notes) do
            if i > SHOWN_NOTES and not more then break end
            add(date(n.time), C.text, true, 4)
            local body = plain(n.text)
            for _, l in ipairs(wrap(body ~= "" and body or "(no text)", w)) do add(l, C.text, false) end
            lines[#lines][4] = 10
        end
        if #e.notes > SHOWN_NOTES then
            local rest = #e.notes - SHOWN_NOTES
            add(more and "Show fewer" or ("Show " .. rest .. " older note" .. (rest == 1 and "" or "s")), C.accentText, false, 2, "more")
        end
    elseif #e.logs > 0 then
        add("No Workshop change notes. The mod's own changelog, newest first:", C.muted, false, 8)
        for i, l in ipairs(e.logs) do
            if i > SHOWN_NOTES and not more then break end
            add(l.title, C.text, true, 4)
            for _, part in ipairs(wrap(plain(l.text), w)) do add(part, C.text, false) end
            lines[#lines][4] = 10
        end
        if #e.logs > SHOWN_NOTES then
            local rest = #e.logs - SHOWN_NOTES
            add(more and "Show fewer" or ("Show " .. rest .. " older note" .. (rest == 1 and "" or "s")), C.accentText, false, 2, "more")
        end
    elseif self.feed == nil or not e.loaded then
        add("Its change notes are on the Workshop: Change notes, at the right, opens them.", C.muted, false)
    else
        add("The author didn't write change notes for this update.", C.muted, false)
    end
    e.cache = { w = w, more = more, lines = lines }
    return lines
end

--- One entry of the history: a card with its head (fold, title, versions, chips, Change notes) and,
--- open, its notes; closed, a peek at the newest. Returns the y under it.
function PZMMMods:drawEntry(e, x, y, w, mx, my, top, bottom)
    local open = self.feedOpen[e.item]
    local HEADH = 40
    local lines = open and self:entryLines(e, w - 60) or nil
    local bodyH = H_SMALL + 10
    if lines then
        bodyH = 0
        for _, l in ipairs(lines) do bodyH = bodyH + H_SMALL + l[4] end
        bodyH = bodyH + 12
    end
    local h = HEADH + bodyH
    if self.feedJump == e.item then
        self.feedScroll = math.max(0, self.feedScroll + (y - top) - 12)
        self.feedJump = nil
    end
    if y + h < top or y > bottom then return y + h end
    local on = self:entryOn(e)
    round(self, x, y, w, h, 10, C.panel)
    outline(self, x, y, w, h, 10, C.line)
    local ty = y + (HEADH - H_SMALL) / 2
    -- right: the Workshop's change notes page
    local link = "Change notes"
    local lx = x + w - 16 - width(link)
    local overLink = mx >= lx - 6 and mx < x + w - 10 and my >= y and my < y + HEADH
    text(self, link, lx, ty, overLink and C.text or C.accentText)
    if overLink then
        rect(self, lx, ty + H_SMALL, width(link), 1, C.text, 1)
        tip(self, "Change notes\nIts change notes page on the Workshop, in your browser or the Steam overlay.")
    end
    self.feedHits[#self.feedHits + 1] = { kind = "url", url = "https://steamcommunity.com/sharedfiles/filedetails/changelog/" .. e.item, x = lx - 6, y = y, w = width(link) + 16, h = HEADH }
    -- left: the fold, the title, versions, chips
    text(self, open and "v" or ">", x + 14, ty, C.muted)
    local tx = x + 30
    local title = fit(e.title ~= "" and e.title or e.item, lx - 24 - tx - 200)
    bold(self, title, tx, ty, on and C.text or C.muted)
    tx = tx + width(title) + 12
    if e.from ~= "" then
        local v = e.from .. " -> " .. e.to
        text(self, v, tx, ty, C.muted)
        tx = tx + width(v) + 12
    end
    if #e.risks > 0 then
        local cx = tx
        tx = chip(self, "May break saves", tx, ty - 1, C.warn) + 6
        if mx >= cx and mx < tx and my >= y and my < y + HEADH then tip(self, riskTip(e)) end
    end
    if not on then chip(self, "Disabled", tx, ty - 1) end
    self.feedHits[#self.feedHits + 1] = { kind = "fold", e = e, x = x, y = y, w = w, h = HEADH }
    -- the body
    local by = y + HEADH
    if lines then
        for _, l in ipairs(lines) do
            local col = l[2]
            if l[5] == "more" then
                local over = mx >= x + 30 and mx < x + 30 + width(l[1]) and my >= by and my < by + H_SMALL
                if over then col = C.text end
                self.feedHits[#self.feedHits + 1] = { kind = "more", e = e, x = x + 30, y = by, w = width(l[1]), h = H_SMALL + 2 }
            end
            local f = l[3] and bold or text
            f(self, l[1], x + 30, by, col)
            by = by + H_SMALL + l[4]
        end
    else
        local peek = excerpt(e):match("[^\n]*%S[^\n]*") or ""
        text(self, fit(peek, w - 60), x + 30, by, C.muted)
    end
    return y + h
end

function PZMMMods:drawFeed()
    local bx, by, bw, bh = self:feedBox()
    local cw = math.min(bw, 1240)
    local x0 = bx + (bw - cw) / 2
    local mx, my = self:getMouseX(), self:getMouseY()
    self.feedHits = {}
    self:setStencilRect(bx, by, bw, bh)
    local y = by + 8 - self.feedScroll
    local f = self.feed or (not self.app and self:liteFeed())
    -- seen while it's open, however you got here (and updates Steam reports while you're on it)
    self:sawFeed()
    bold(self, "What's new", x0, y, C.text, MEDIUM)
    if f then
        -- Enabled only, at the right of the heading
        local label = "Enabled only"
        local cx = x0 + cw - width(label) - BOX - 8
        local cy = y + (H_MEDIUM - BOX) / 2
        checkbox(self, cx, cy, self.enabledOnly)
        text(self, label, cx + BOX + 8, y + (H_MEDIUM - H_SMALL) / 2, C.text)
        self.feedHits[#self.feedHits + 1] = { kind = "only", x = cx - 4, y = cy - 4, w = BOX + 16 + width(label), h = BOX + 8 }
        if mx >= cx - 4 and mx < cx + BOX + 12 + width(label) and my >= cy - 4 and my < cy + BOX + 4 then tip(self, "Enabled only\nHide updates to mods you have turned off.") end
    end
    y = y + H_MEDIUM + 4
    if not f then
        self.feedHeight = 0
        if self.app then
            text(self, "Open PZMM once and it brings What's new here.", x0, y, C.muted)
        else
            -- scrolls only when the window is too short for it
            local h = self:drawSorry(x0, y + 8, cw, by + bh - y - 32, mx, my)
            self.feedHeight = y + 8 + h + self.feedScroll - by + 24
        end
        self:clearStencilRect()
        return
    end
    local since = tonumber(f.since) or 0
    local sub = (f.basis == "launch" and ("Your Workshop mods updated since you last played (" .. date(since) .. ")."))
        or (f.basis == "save" and ("Your Workshop mods updated since your last save (" .. date(since) .. ")."))
        or "Your Workshop mods updated in the last week (no sign of play yet)."
    text(self, sub, x0, y, C.muted)
    y = y + H_SMALL + 16
    if f.lite then
        y = self:drawNotice(x0, y, cw, "Change notes: the app fetches each update's notes from Steam. Here, Read all notes opens them on the Workshop.", mx, my)
    end

    local shown, risky = {}, 0
    for _, e in ipairs(f.entries) do
        local on = self:entryOn(e)
        if on or not self.enabledOnly then shown[#shown + 1] = e end
        if on and #e.risks > 0 then risky = risky + 1 end
    end
    if risky > 0 then
        local s = risky == 1 and "1 enabled mod changed in a way that may break a save in progress. Read its notes before you load a save."
            or (risky .. " enabled mods changed in ways that may break a save in progress. Read their notes before you load a save.")
        text(self, "!  " .. s, x0, y, C.warn)
        y = y + H_SMALL + 14
    end
    if #shown == 0 then
        text(self, (self.enabledOnly and #f.entries > 0) and "None of your enabled mods were updated since you last played." or "None of your mods were updated since you last played.", x0, y, C.muted)
        y = y + H_SMALL + 14
    end

    -- the board: the latest, as scraps of paper taped on the blueprint
    local cols = math.max(1, math.floor((cw + 16) / 316))
    local cardW = (cw - 16 * (cols - 1)) / cols
    local k = cardW / 300
    local cardH = 230 * k
    local pinned = math.min(#shown, cols * ROWS)
    for i = 1, pinned do
        local col, row = (i - 1) % cols, math.floor((i - 1) / cols)
        local cx, cy = x0 + col * (cardW + 16), y + row * (cardH + 24)
        if cy + cardH >= by and cy <= by + bh then self:drawScrap(shown[i], cx, cy, cardW, cardH, k, mx, my) end
    end
    if pinned > 0 then y = y + math.ceil(pinned / cols) * (cardH + 24) + 16 end

    -- the history: every change note, newest day first
    if #shown > 0 then
        bold(self, "All change notes", x0, y, C.text, MEDIUM)
        y = y + H_MEDIUM + 8
        local lastDay
        for _, e in ipairs(shown) do
            -- the app puts updates with nothing to read last: one heading for them, not their days
            local day = hasNotes(e) and date(e.updated, true) or "No change notes"
            if day ~= lastDay then
                lastDay = day
                y = y + 6
                bold(self, day, x0, y, C.muted)
                y = y + H_SMALL + 8
            end
            y = self:drawEntry(e, x0, y, cw, mx, my, by, by + bh) + 8
        end
        y = y + 8
        for _, l in ipairs(wrap(FINE, cw)) do
            text(self, l, x0, y, C.muted)
            y = y + H_SMALL + 2
        end
    end
    self:clearStencilRect()
    self.feedHeight = y + self.feedScroll - by + 24
    if self.feedHeight > bh then
        local th = math.max(30, bh * bh / self.feedHeight)
        local thy = by + (bh - th) * self.feedScroll / (self.feedHeight - bh)
        round(self, bx + bw - 6, thy, 6, th, 3, C.edge, 0.8)
    end
end

function PZMMMods:feedClick(x, y)
    local _, by, _, bh = self:feedBox()
    if self.page == "home" then by, bh = TOP, self.height - BOTTOM - TOP end
    if y < by or y > by + bh then return end
    for _, h in ipairs(self.feedHits or {}) do
        if inside(h, x, y) then
            getSoundManager():playUISound("UISelectListItem")
            if h.kind == "only" then
                self.enabledOnly = not self.enabledOnly
                self.feedScroll = 0
            elseif h.kind == "read" and (self.feed == nil or not h.e.loaded) then
                openLink("https://steamcommunity.com/sharedfiles/filedetails/changelog/" .. h.e.item)
            elseif h.kind == "read" then
                self.feedOpen[h.e.item] = true
                self.feedJump = h.e.item
            elseif h.kind == "fold" then
                self.feedOpen[h.e.item] = not self.feedOpen[h.e.item]
            elseif h.kind == "more" then
                self.feedMore[h.e.item] = not self.feedMore[h.e.item]
            elseif h.kind == "url" then
                openLink(h.url)
            elseif h.kind == "fn" then
                h.fn()
            elseif h.kind == "show" then
                -- the mod in My mods, picked
                local d = self.model.mods[h.e.mods[1] or ""]
                if d then
                    self.page, self.selected, self.tab = "mods", d, "All"
                    self.search:setText("")
                    self:updateView()
                end
            end
            return
        end
    end
end

-- --- input


function PZMMMods:onMouseDown(x, y)
    for _, a in ipairs(self.actionBoxes or {}) do
        if x >= a[2] and x < a[2] + BAR and y >= a[3] and y < a[3] + BAR then
            getSoundManager():playUISound("UISelectListItem")
            self:onAction(a[1])
            return true
        end
    end
    for _, l in ipairs(self.linkBoxes or {}) do
        if x >= l[2] and x < l[2] + 30 and y >= l[3] and y < l[3] + 30 then
            openLink(l[1])
            return true
        end
    end
    for _, pg in ipairs(self.pageBoxes or {}) do
        if x >= pg[2] and x < pg[2] + pg[3] and y >= ROW1 and y < TOP then
            getSoundManager():playUISound("UISelectListItem")
            self.page, self.feedScroll = pg[1], 0
            return true
        end
    end
    if self.page ~= "mods" then
        self:feedClick(x, y)
        return true
    end
    for _, t in ipairs(self.tabBoxes or {}) do
        if x >= t[2] and x < t[2] + t[4] and y >= t[3] and y < t[3] + t[5] then
            self.tab = t[1]
            self.scroll = 0
            self:updateView()
            return true
        end
    end
    local lx, ly, lw, lh = self:listBox()
    if y < ly or y > ly + lh then return true end
    for _, h in ipairs(self.hits or {}) do
        if inside(h, x, y) then
            getSoundManager():playUISound("UISelectListItem")
            if h.kind == "head" then
                self.collapsed[h.g] = not self.collapsed[h.g]
                self:updateView()
            elseif h.kind == "box" then
                self:toggle(h.mod)
            elseif h.kind == "star" then
                self.model:setFavorite(h.mod.modId, not h.mod.favorite)
                self.model:refreshMods()
            elseif h.kind == "fn" then
                h.fn()
            else
                self.selected = self.selected ~= h.mod and h.mod or nil
            end
            return true
        end
    end
    return true
end

function PZMMMods:onRightMouseDown(x, y)
    if self.page ~= "mods" then return true end
    for _, h in ipairs(self.hits or {}) do
        if h.kind == "row" and x >= h.x and x < h.x + h.w and y >= h.y and y < h.y + h.h then
            self:modMenu(h.mod)
            return true
        end
    end
    return true
end
--- A mod's menu: on or off, favourite, its ID, its Workshop page, its files.
function PZMMMods:modMenu(d)
    local ws
    pcall(function() ws = d.modInfo:getWorkshopID() end)
    local items = {
        { d.isActive and "Turn off" or "Turn on", function() self:toggle(d) end },
        { d.favorite and "Unfavourite" or "Favourite", function()
            self.model:setFavorite(d.modId, not d.favorite)
            self.model:refreshMods()
        end },
        false,
        { "Copy mod ID", function()
            Clipboard.setClipboard(d.modId)
            self:say(d.modId .. " copied.")
        end },
    }
    if ws and ws ~= "" and ws ~= "0" then
        items[#items + 1] = { "Workshop page", function() pcall(function() activateSteamOverlayToWorkshopItem(ws) end) end }
    end
    items[#items + 1] = { "Show files", function() pcall(function() showFolderInDesktop(d.modInfo:getDir()) end) end }
    self:menu(items)
end

function PZMMMods:onMouseWheel(del)
    if self.page ~= "mods" then
        local _, _, _, bh = self:feedBox()
        if self.page == "home" then bh = self.height - BOTTOM - TOP end
        self.feedScroll = math.max(0, math.min(self.feedScroll + del * 48, (self.feedHeight or 0) - bh))
        return true
    end
    local _, _, _, lh = self:listBox()
    self.scroll = math.max(0, math.min(self.scroll + del * 48, (self.listHeight or 0) - lh))
    return true
end

-- --- actions

function PZMMMods:close()
    self:setVisible(false)
    self:removeFromUIManager()
    if MainScreen.instance and MainScreen.instance.bottomPanel then MainScreen.instance.bottomPanel:setVisible(true) end
end

function PZMMMods:onAccept()
    self:close()
    self.model:acceptChanges() -- saves (and restarts the game's Lua when the mod set needs it)
end

--- Back: changes not saved yet get the game's own question (save them or not).
function PZMMMods:onBack()
    for _, d in pairs(self.model.mods) do
        if d.isActive ~= d.defaultActive or self.mapsChanged then
            self:ask({ title = "Save your changes to your mods before you go back?", choices = {
                { "Save", function() self:onBackAnswer({ internal = "YES" }) end },
                { "Don't save", function() self:onBackAnswer({ internal = "NO" }) end },
            } })
            return
        end
    end
    self:close()
end
function PZMMMods:onBackAnswer(b)
    if b.internal ~= "YES" then
        if self.modsOrig then
            pcall(function() self.model:getActiveMods():copyFrom(self.modsOrig) end)
            self.model:refreshMods()
        end
        if self.mapsChanged and self.mapOrig then self:setMapOrder(self.mapOrig) end
        for id, d in pairs(self.model.mods) do
            if d.isActive ~= d.defaultActive then self.model:forceActivateMods(d.modInfo, d.defaultActive) end
            if self.model.favs[id] ~= d.defaultFav then self.model.favs[id] = d.defaultFav end
        end
    end
    self:onAccept()
end

local menuClick = MainScreen.onMenuItemMouseDownMainMenu

PZMMOverlay = ISPanel:derive("PZMMOverlay")
local ITEM_H = H_SMALL + 12

--- Ask, as the app does: `q` = { title, lines = {...}, choices = { { label, fn }, ... } }. The first
--- choice is red; Cancel (or a click outside) closes it.
function PZMMMods:ask(q)
    PZMMOverlay.show({ ask = q })
end
--- A popup menu at the mouse: `items` = { { label, fn } or false (a line), ... }.
function PZMMMods:menu(items)
    PZMMOverlay.show({ menu = items, x0 = getMouseX(), y0 = getMouseY() })
end

function PZMMOverlay.show(o)
    local p = PZMMOverlay:new(0, 0, getCore():getScreenWidth(), getCore():getScreenHeight())
    setmetatable(p, PZMMOverlay)
    PZMMOverlay.__index = PZMMOverlay
    for k, v in pairs(o) do p[k] = v end
    p.background = false
    p:initialise()
    p:addToUIManager()
    p:setAlwaysOnTop(true)
    p:bringToTop()
end
function PZMMOverlay:closeIt()
    self:setVisible(false)
    self:removeFromUIManager()
end

function PZMMOverlay:prerender()
    local mx, my = self:getMouseX(), self:getMouseY()
    self.hits = {}
    if self.ask then
        local q = self.ask
        rect(self, 0, 0, self.width, self.height, hex("000000"), 0.45)
        local w = math.min(560, self.width - 32)
        local tw = w - 44
        local title = wrap(q.title, tw)
        local lines = {}
        for _, l in ipairs(q.lines or {}) do
            for i, r in ipairs(wrap(l, tw - 18)) do lines[#lines + 1] = { r, i == 1 } end
        end
        local h = 20 + #title * (H_SMALL + 2) + 10 + (#lines > 0 and #lines * (H_SMALL + 2) + 8 or 0) + BAR + 20
        local x, y = math.floor((self.width - w) / 2), math.floor((self.height - h) / 2)
        self.box = { x = x, y = y, w = w, h = h }
        round(self, x, y, w, h, 10, C.panel)
        outline(self, x, y, w, h, 10, C.line)
        local ty = y + 20
        for _, l in ipairs(title) do
            bold(self, l, x + 22, ty, C.text)
            ty = ty + H_SMALL + 2
        end
        ty = ty + 10
        for _, l in ipairs(lines) do
            if l[2] then rect(self, x + 26, ty + H_SMALL / 2 - 1, 3, 3, C.muted, 1) end
            text(self, l[1], x + 40, ty, C.muted)
            ty = ty + H_SMALL + 2
        end
        if #lines > 0 then ty = ty + 8 end
        -- the choices at the right: the first red, the rest raised, Cancel quiet and last
        local row = {}
        for i, c in ipairs(q.choices or {}) do row[#row + 1] = { c[1], i == 1 and "primary" or nil, c[2] } end
        row[#row + 1] = { "Cancel", "quiet" }
        local bx = x + w - 22
        for i = #row, 1, -1 do
            local b = row[i]
            local bw = width(b[1]) + 28
            bx = bx - bw
            flatButton(self, b[1], bx, ty, mx, my, b[2])
            self.hits[#self.hits + 1] = { x = bx, y = ty, w = bw, h = BAR, fn = b[3] or false }
            bx = bx - 8
        end
    elseif self.menu then
        local w = 0
        for _, it in ipairs(self.menu) do if it then w = math.max(w, width(it[1])) end end
        w = w + 28
        local h = 8
        for _, it in ipairs(self.menu) do h = h + (it and ITEM_H or 9) end
        local x = math.min(self.x0, self.width - w - 4)
        local y = math.min(self.y0, self.height - h - 4)
        self.box = { x = x, y = y, w = w, h = h }
        round(self, x, y, w, h, 8, C.panel)
        outline(self, x, y, w, h, 8, C.line)
        local iy = y + 4
        for _, it in ipairs(self.menu) do
            if it then
                local over = mx >= x and mx < x + w and my >= iy and my < iy + ITEM_H
                if over then round(self, x + 4, iy, w - 8, ITEM_H, 6, C.hover) end
                text(self, it[1], x + 14, iy + 6, C.text)
                self.hits[#self.hits + 1] = { x = x, y = iy, w = w, h = ITEM_H, fn = it[2] }
                iy = iy + ITEM_H
            else
                rect(self, x + 10, iy + 4, w - 20, 1, C.line, 1)
                iy = iy + 9
            end
        end
    end
end

function PZMMOverlay:onMouseDown(x, y)
    for _, h in ipairs(self.hits or {}) do
        if x >= h.x and x < h.x + h.w and y >= h.y and y < h.y + h.h then
            getSoundManager():playUISound("UISelectListItem")
            self:closeIt()
            if h.fn then h.fn() end
            return true
        end
    end
    -- a click outside the box cancels; inside it does nothing
    local b = self.box
    if not (b and x >= b.x and x < b.x + b.w and y >= b.y and y < b.y + b.h) then self:closeIt() end
    return true
end
function PZMMOverlay:onRightMouseDown(x, y)
    self:closeIt()
    return true
end

--- Turn a mod on or off. Off, with enabled mods that need it: ask whether they go off too (the app's
--- "Disable them too").
function PZMMMods:toggle(d)
    if not d.isActive then
        self.model:forceActivateMods(d.modInfo, true)
        return
    end
    local users = {}
    for _, m in pairs(self.model.mods) do
        if m.isActive and m.requireMods and m.requireMods[d.modId] then users[#users + 1] = m end
    end
    if #users == 0 then
        self.model:forceActivateMods(d.modInfo, false)
        return
    end
    local list = {}
    for _, m in ipairs(users) do list[#list + 1] = m.name end
    self:ask({ title = (#users == 1 and "This enabled mod needs " or "These enabled mods need ") .. d.name .. ":", lines = list, choices = {
        { "Turn " .. (#users == 1 and "it" or "them") .. " off too", function()
            self.model:forceActivateMods(d.modInfo, false)
            for _, m in ipairs(users) do self.model:forceActivateMods(m.modInfo, false) end
        end },
        { "Only " .. d.name, function() self.model:forceActivateMods(d.modInfo, false) end },
    } })
end

--- Steam's details for a mod's Workshop item (nil until Steam answers, or for local mods).
function PZMMMods:steamOf(d)
    if not self.steam then return nil end
    local ws
    pcall(function() ws = d.modInfo:getWorkshopID() end)
    return ws and self.steam[ws]
end

--- Ask Steam about every item you're subscribed to (once per visit): title, update time, download state
--- and the items it requires. Required items you aren't subscribed to are asked about next, for titles.
function PZMMMods:querySteam()
    self.steam = self.steam or {}
    pcall(function()
        local ids = getSteamWorkshopItemIDs()
        if ids and not ids:isEmpty() then querySteamWorkshopItemDetails(ids, PZMMMods.onSteamDetails, self) end
    end)
end
function PZMMMods:onSteamDetails(status, info)
    if status ~= "Completed" then return end
    local subscribed, wanted = {}, {}
    pcall(function()
        local ids = getSteamWorkshopItemIDs()
        for i = 0, ids:size() - 1 do subscribed[tostring(ids:get(i))] = true end
    end)
    for i = 0, info:size() - 1 do
        pcall(function()
            local det = info:get(i)
            local id = det:getIDString()
            local e = { title = det:getTitle(), updated = tonumber(tostring(det:getTimeUpdated())) or 0, missing = {} }
            pcall(function() e.state = tostring(det:getState()) end)
            -- its required items: getChildren() is a Java long[] (null without any), so count and index instead
            pcall(function()
                for j = 0, det:getNumChildren() - 1 do
                    local c = tostring(det:getChildID(j))
                    if not subscribed[c] then
                        e.missing[#e.missing + 1] = c
                        if not self.steam[c] then wanted[#wanted + 1] = c end
                    end
                end
            end)
            self.steam[id] = e
        end)
    end
    self.liteFeedCache = nil
    if #wanted > 0 then
        pcall(function()
            local list = ArrayList.new()
            for _, c in ipairs(wanted) do
                self.steam[c] = self.steam[c] or { title = nil, updated = 0, missing = {} }
                list:add(c)
            end
            querySteamWorkshopItemDetails(list, PZMMMods.onSteamDetails, self)
        end)
    end
end

--- What's new without the app: your Workshop items Steam says were updated since you last played (or
--- in the last week), with no change notes (only the app can fetch them).
function PZMMMods:liteFeed()
    if self.liteFeedCache then return self.liteFeedCache end
    -- (the game's Lua has no next(): any entry at all, by pairs)
    local any = false
    for _ in pairs(self.steam or {}) do any = true break end
    if not any then return nil end
    local since, basis = self.lastPlayed, "launch"
    if not since then since, basis = os.time() - 7 * 86400, "" end
    local mods = {}
    for id, d in pairs(self.model.mods) do
        local ws
        pcall(function() ws = d.modInfo:getWorkshopID() end)
        if ws and ws ~= "" then
            mods[ws] = mods[ws] or {}
            table.insert(mods[ws], id)
        end
    end
    local f = { since = since, basis = basis, entries = {}, lite = true }
    for id, e in pairs(self.steam) do
        if mods[id] and e.updated > since then
            table.insert(f.entries, { item = id, title = e.title or id, updated = e.updated, mods = mods[id], notes = {}, logs = {}, loaded = false, risks = {}, from = "", to = "", pending = e.state == "NeedsUpdate" })
        end
    end
    table.sort(f.entries, function(a, b) return a.updated > b.updated end)
    self.liteFeedCache = f
    return f
end

function PZMMMods:onAction(id)
    if id == "share" then
        -- the game's own preset text (its mod screen's Presets > Add takes it): name, then ids, ";" after each
        local ids = {}
        for mid, d in pairs(self.model.mods) do
            if d.isActive then ids[#ids + 1] = mid .. ";" end
        end
        table.sort(ids)
        Clipboard.setClipboard(((self.lib and self.lib.profile) or "PZMM") .. ":" .. table.concat(ids) .. "\n")
        self:say(#ids .. " enabled mods copied: paste them into the game's Presets > Add, or send them to a friend.")
    elseif id == "refresh" then
        self.lib, self.app, self.feed = readLibrary(), readApp(), readFeed()
        self.mapOrder, self.saves, self.java, self.selected, self.liteFeedCache = nil, nil, nil, nil, nil
        self.model:reloadMods()
        self:updateView()
        self:say("Mods rescanned.")
    elseif id == "check" then
        self:request("check", "Check setup asked for.")
    elseif id == "folder" then
        pcall(function() showFolderInDesktop(Core.getMyDocumentFolder() .. getFileSeparator() .. "mods") end)
    end
end

function PZMMMods:onVanilla()
    self:close()
    menuClick({ internal = "MODS" }, 0, 0)
end

function PZMMMods.open()
    local s = PZMMMods.instance
    local w, h = getCore():getScreenWidth(), getCore():getScreenHeight()
    if not s or s.width ~= w or s.height ~= h then
        if s then s:removeFromUIManager() end
        s = PZMMMods:new(0, 0, w, h)
        s:initialise()
        s:instantiate()
        PZMMMods.instance = s
    end
    s.selected, s.scroll = nil, 0
    s:setVisible(true)
    s:addToUIManager()
    s:bringToTop()
    if MainScreen.instance and MainScreen.instance.bottomPanel then MainScreen.instance.bottomPanel:setVisible(false) end
    s.model.isNewGame = false
    s.lib, s.app, s.feed = readLibrary(), readApp(), readFeed()
    s.mapOrder, s.mapOrig, s.mapsChanged, s.modsOrig, s.saves, s.java = nil, nil, false, nil, nil, nil
    s.steam, s.liteFeedCache, s.lastPlayed, s.feedSeen = nil, nil, readPlayed(), readFeedSeen()
    s:querySteam()
    s.page, s.feedScroll = s.page or "mods", 0
    s.model:reloadMods()
end

-- when you last started a game (Zomboid/Lua/pzmm/played.txt), for What's new without the app
local function savePlayed()
    pcall(function()
        local w = getFileWriter("pzmm/played.txt", true, false)
        w:write(tostring(os.time()))
        w:close()
    end)
end
if Events.OnGameStart then Events.OnGameStart.Add(savePlayed) end

-- the main menu's Mods button opens this screen
MainScreen.onMenuItemMouseDownMainMenu = function(item, x, y)
    if item.internal == "MODS" then
        getSoundManager():playUISound("UIActivateMainMenuItem")
        return PZMMMods.open()
    end
    return menuClick(item, x, y)
end
-- --- the main menu's PZMM banner: the logo in a box at the top right, opening into the latest patch notes

--- The latest patch notes, shipped in the mod (42/media/pzmm/) like Knox UI's news, so a Workshop update
--- brings them, with or without the app: one file for this in-game mod manager, one for the desktop app.
--- Each is `# <version>`, then that release's notes in Markdown, pasted as is; bullets `- **Lead.** text`
--- become notes.
local function readNotes(kind, label, file)
    local notes = { kind = kind, label = label, version = "" }
    pcall(function()
        local r = getModFileReader("PZMMClient", "media/pzmm/" .. file, false)
        if not r then return end
        local line = r:readLine()
        while line do
            local v = line:match("^#%s+(%S+)")
            local lead, rest = line:match("^%-%s+%*%*(.-)%*%*%s*(.*)$")
            if v and notes.version == "" then
                notes.version = v
            elseif lead then
                notes[#notes + 1] = { lead, rest }
            elseif line:match("^%-%s+") then
                notes[#notes + 1] = { line:gsub("^%-%s+", ""), "" }
            end
            line = r:readLine()
        end
        r:close()
    end)
    return notes
end
local NOTES = {}
for _, n in ipairs({ readNotes("game", "In-game mod manager", "notes_game.txt"), readNotes("app", "Desktop app", "notes_app.txt") }) do
    if n.version ~= "" then NOTES[#NOTES + 1] = n end
end
local RELEASES = "https://github.com/SEVNC3/pz-mod-manager/releases/latest"

--- The patches whose notes you've opened (Zomboid/Lua/pzmm/seen.txt): lines of `<kind> <version>`.
local function seenNotes()
    local seen = {}
    pcall(function()
        local r = getFileReader("pzmm/seen.txt", false)
        if not r then return end
        local line = r:readLine()
        while line do
            seen[line] = true
            line = r:readLine()
        end
        r:close()
    end)
    return seen
end
local function saveSeen(seen)
    pcall(function()
        local w = getFileWriter("pzmm/seen.txt", true, false)
        for k in pairs(seen) do w:write(k .. "\n") end
        w:close()
    end)
end

--- The new-patch mark: a blood-red disc with a white "!" drawn from two bars, 20 px.
local function bang(ui, x, y)
    round(ui, x, y, 20, 20, 10, hex("c42020"))
    rect(ui, x + 8, y + 4, 4, 8, hex("ffffff"), 1)
    rect(ui, x + 8, y + 13, 4, 3, hex("ffffff"), 1)
end

PZMMNews = ISPanel:derive("PZMMNews")
local NEWS_W, NEWS_OPEN_W, NEWS_HEAD, NEWS_TABS = 280, 460, 60, H_SMALL + 19
local BLANK = tex("blank")

function PZMMNews:new()
    local o = ISPanel:new(0, 0, NEWS_W, NEWS_HEAD)
    setmetatable(o, self)
    self.__index = self
    o.background = false
    o.seen = seenNotes()
    return o
end

function PZMMNews:fresh(n) return not self.seen[n.kind .. " " .. n.version] end

--- Show a tab's notes: they count as read.
function PZMMNews:show(n)
    if self.tab ~= n then self.newsScroll = 0 end
    self.tab = n
    if self:fresh(n) then
        self.seen[n.kind .. " " .. n.version] = true
        saveSeen(self.seen)
    end
end

function PZMMNews:prerender()
    local main = MainScreen.instance
    -- the pause menu is the main screen too: off screen there
    if not main or main.inGame then
        self:setX(-10000)
        return
    end
    -- the main menu without the game's logo and ZombieBuddy's "loaded" watermark over the top of the screen
    main.pzmmLogo = main.pzmmLogo or main.logoTexture
    main.logoTexture = (SETTINGS.logo and BLANK) or main.pzmmLogo
    pcall(function()
        local a = 0
        if not SETTINGS.watermark then
            a = 0.4
            pcall(function() a = PZAPI.ModOptions:getOptions("ZombieBuddy"):getOption("watermarkOpacity"):getValue() end)
        end
        ZombieBuddy.Watermark.setAlpha(a)
    end)
    if not SETTINGS.news then
        self:setX(-10000)
        return
    end

    local w = self.open and NEWS_OPEN_W or NEWS_W
    self:setWidth(w)
    self:setX(main.width - w - 24)
    self:setY(24)
    local mx, my = self:getMouseX(), self:getMouseY()
    local lines
    local h = NEWS_HEAD
    if self.open then
        local c = self.cache
        if not (c and c.tab == self.tab and c.w == w) then
            c = { tab = self.tab, w = w, lines = {} }
            for _, note in ipairs(self.tab) do
                for _, l in ipairs(wrap(note[1], w - 48)) do c.lines[#c.lines + 1] = { l, true } end
                for _, l in ipairs(wrap(note[2], w - 48)) do c.lines[#c.lines + 1] = { l, false } end
                c.lines[#c.lines + 1] = { "", false }
            end
            self.cache = c
        end
        lines = c.lines
        -- the notes scroll inside a capped body, so a long patch never runs off the screen
        self.bodyFull = #lines * (H_SMALL + 2)
        local room = getCore():getScreenHeight() - 24 - 120 - (NEWS_HEAD + NEWS_TABS + 14 + H_SMALL + 28)
        self.bodyH = math.min(self.bodyFull, math.max(120, math.min(420, room)))
        self.newsScroll = math.max(0, math.min(self.newsScroll or 0, self.bodyFull - self.bodyH))
        h = h + NEWS_TABS + 14 + self.bodyH + H_SMALL + 28
    end
    self:setHeight(h)
    round(self, 0, 0, w, h, 10, C.panel, 0.94)
    outline(self, 0, 0, w, h, 10, C.line, 1)
    if my >= 0 and my < NEWS_HEAD and mx >= 0 and mx < w then
        round(self, 1, 1, w - 2, NEWS_HEAD - 2, 10, C.hover, 0.6)
        tip(self, self.open and "Hide the patch notes" or "PZMM patch notes\nWhat changed in the in-game mod manager and the desktop app. The red ! marks a patch you haven't read.")
    end
    image(self, LOGO, 10, 10, 40, 40)
    bold(self, "PZMM patch notes", 60, 10, C.text, MEDIUM)
    local versions, fresh = {}, false
    for _, n in ipairs(NOTES) do
        versions[#versions + 1] = (n.kind == "game" and "In game " or "App ") .. n.version
        fresh = fresh or self:fresh(n)
    end
    text(self, table.concat(versions, "   "), 60, 12 + H_MEDIUM, C.muted)
    local rx = w - 14 - width("+", MEDIUM)
    text(self, self.open and "-" or "+", rx, (NEWS_HEAD - H_MEDIUM) / 2, C.muted, MEDIUM)
    if fresh then bang(self, rx - 32, math.floor((NEWS_HEAD - 20) / 2)) end
    if not self.open then return end

    -- the tabs, as the app's: a red wash and a white underline on the open or hovered one
    local tx, top = 12, NEWS_HEAD
    self.tabBoxes = {}
    for _, n in ipairs(NOTES) do
        local label = n.label .. "  " .. n.version
        local mark = self:fresh(n)
        local tw = width(label) + 28 + (mark and 28 or 0)
        local on = self.tab == n
        local over = mx >= tx and mx < tx + tw and my >= top and my < top + NEWS_TABS
        if over then
            tip(self, (n.kind == "game" and "In-game mod manager\nThis screen, PZMM - Client, in the game." or "Desktop app\nThe PZMM app for Windows and Linux.")
                .. (mark and "\nNew: you haven't read this patch yet." or ""))
        end
        if on or over then
            round(self, tx, top, tw, NEWS_TABS - 1, 3, C.accent, 0.32, true, false)
            rect(self, tx + 3, top, tw - 6, 1, C.accent, 0.7)
            rect(self, tx, top + 3, 1, NEWS_TABS - 4, C.accent, 0.7)
            rect(self, tx + tw - 1, top + 3, 1, NEWS_TABS - 4, C.accent, 0.7)
            rect(self, tx, top + NEWS_TABS - 3, tw, 2, C.text, 1)
        end
        text(self, label, tx + 14, top + (NEWS_TABS - H_SMALL) / 2, (on or over) and hex("ffffff") or C.muted)
        if mark then bang(self, tx + 14 + width(label) + 8, top + math.floor((NEWS_TABS - 20) / 2)) end
        self.tabBoxes[#self.tabBoxes + 1] = { n, tx, tw }
        tx = tx + tw + 2
    end
    rect(self, 0, top + NEWS_TABS - 1, w, 1, C.line, 1)

    local by = top + NEWS_TABS + 14
    self:setStencilRect(0, top + NEWS_TABS, w, self.bodyH + 14)
    local y = by - self.newsScroll
    for _, l in ipairs(lines) do
        if y + H_SMALL > top + NEWS_TABS and y < by + self.bodyH then
            if l[2] then bold(self, l[1], 24, y, C.text) else text(self, l[1], 24, y, C.muted) end
        end
        y = y + H_SMALL + 2
    end
    self:clearStencilRect()
    if self.bodyFull > self.bodyH then
        -- the scroll bar (What's new's), and a line where the notes are cut off
        local th = math.max(30, self.bodyH * self.bodyH / self.bodyFull)
        local thy = by + (self.bodyH - th) * self.newsScroll / (self.bodyFull - self.bodyH)
        round(self, w - 12, thy, 6, th, 3, C.edge, 0.8)
        rect(self, 12, by + self.bodyH + 3, w - 24, 1, C.line, 1)
    end
    y = by + self.bodyH + 10
    local link = "Full notes on GitHub"
    local over = my >= y and my < y + H_SMALL + 4 and mx >= 24 and mx < 24 + width(link)
    text(self, link, 24, y, over and C.text or C.accentText)
    if over then tip(self, "Full notes on GitHub\nThe latest release, with downloads.") end
    self.linkY = y
end

function PZMMNews:render() drawTip(self) end

function PZMMNews:onMouseWheel(del)
    if not (self.open and self.bodyFull and self.bodyFull > self.bodyH) then return false end
    self.newsScroll = math.max(0, math.min((self.newsScroll or 0) + del * 48, self.bodyFull - self.bodyH))
    return true
end

function PZMMNews:onMouseDown(x, y)
    if y < NEWS_HEAD then
        getSoundManager():playUISound("UISelectListItem")
        self.open = not self.open
        if self.open then
            -- the first tab with something new, else the one last open
            local pick = self.tab or NOTES[1]
            for _, n in ipairs(NOTES) do
                if self:fresh(n) then pick = n break end
            end
            self:show(pick)
        end
        return true
    end
    if not self.open then return true end
    if y < NEWS_HEAD + NEWS_TABS then
        for _, t in ipairs(self.tabBoxes or {}) do
            if x >= t[2] and x < t[2] + t[3] then
                getSoundManager():playUISound("UISelectListItem")
                self:show(t[1])
            end
        end
    elseif self.linkY and y >= self.linkY and y < self.linkY + H_SMALL + 4 and x >= 24 and x < 24 + width("Full notes on GitHub") then
        openLink(RELEASES)
    end
    return true
end

-- the app's community note (Home.svelte), drawn into scrap_note.png by .temp/noteart.py: { label, url, its
-- box on the paper before the tilt }. scrap_note_<i>.png has link i in blood, underlined (hovered).
local COMMUNITY = {
    { "Project Zomboid website", "https://projectzomboid.com/blog/", { 30, 68, 276, 21 } },
    { "The Indie Stone forums", "https://theindiestone.com/forums/", { 30, 89, 276, 21 } },
    { "Project Zomboid Discord", "https://discord.gg/theindiestone", { 30, 110, 276, 21 } },
    { "PZwiki", "https://pzwiki.net/wiki/Project_Zomboid_Wiki", { 30, 131, 276, 21 } },
    { "PZwiki: Modding", "https://pzwiki.net/wiki/Modding", { 30, 152, 276, 21 } },
    { "PZ Modding Community Discord (unofficial)", "https://discord.gg/ghRTHUTc94", { 30, 173, 276, 38 } },
    { "r/projectzomboid on Reddit", "https://www.reddit.com/r/projectzomboid/", { 30, 211, 276, 21 } },
    { "Steam Workshop", "https://steamcommunity.com/app/108600/workshop/", { 30, 232, 276, 21 } },
    { "PZMM Discord", "https://discord.com/invite/6URUy6mNzu", { 30, 253, 276, 21 } },
}
local NOTE_W, NOTE_H, NOTE_TILT = 332, 340, math.rad(-2) -- the paper before the tilt, and the tilt
local NOTE_HOVER = {}
for i = 1, #COMMUNITY do NOTE_HOVER[i] = tex("scrap_note_" .. i) end
-- the hero per profile, as the app's: Project Zomboid's Steam art and logo, or Project Viewpoint's
local HERO_PZ, HERO_VP, LOGO_PZ, VIEWPOINT_LOGO = tex("hero_pz"), tex("hero_viewpoint"), tex("logo_pz"), tex("viewpoint_logo")
local NOTE = tex("scrap_note")

-- --- the app's other tabs, as far as the game lets a mod go: each draws into the What's new frame (feedBox,
-- feedScroll, feedHits) and returns the y under it. Tabs the game can't do get the We're sorry card; tabs it
-- can do in part start with a notice of what's only in the app.

--- The game opens only https pages, so PZMM starts through this page: it opens pzmm://game.
local LAUNCH = "https://sevnc3.github.io/pz-mod-manager/open/"

--- Ask the PZMM app to do something (Zomboid/Lua/pzmm/requests.txt, a line per request: install <ids>,
--- backup <mode/name>, check, open <tab>). A running PZMM takes them within a second and comes to the
--- front; if they're still there a few seconds on, it isn't running, and the launch page starts it.
--- It asks before it installs or backs anything up.
function PZMMMods:request(line, said)
    local ok = pcall(function()
        local w = getFileWriter("pzmm/requests.txt", true, true)
        w:write(line .. "\n")
        w:close()
    end)
    if ok then self.launchAt = getTimestampMs() + 2500 end
    self:say(ok and (said .. " Opening PZMM...") or "Couldn't reach PZMM: Zomboid/Lua/pzmm/requests.txt can't be written.")
end

--- PZMM didn't take the requests: it isn't running, so start it.
function PZMMMods:launchIfNeeded()
    if not (self.launchAt and getTimestampMs() >= self.launchAt) then return end
    self.launchAt = nil
    local waiting = false
    pcall(function()
        local r = getFileReader("pzmm/requests.txt", false)
        if r then waiting = true; r:close() end
    end)
    if waiting then
        openLink(LAUNCH)
        self:say("Starting PZMM: if your browser asks, let it open PZMM.")
    end
end

--- The main menu's own screens: this one closes, then the game opens its own (Load, Multiplayer, Workshop...).
function PZMMMods:gameScreen(internal)
    self:close()
    menuClick({ internal = internal }, 0, 0)
end

--- The notice at the top of a tab the game can do in part: what's only in the app, and where to get it.
function PZMMMods:drawNotice(x, y, w, missing, mx, my)
    local img, pad = 56, 14
    local tx = x + pad + img + 14
    local get = not self.app and "Get the app" or nil
    local gw = get and width(get) + 54 or 0
    local lines = wrap(missing, w - (tx - x) - pad - (get and gw + 16 or 0))
    local h = math.max(img + 16, 12 + H_SMALL + 4 + #lines * (H_SMALL + 2) + 10)
    card(self, x, y, w, h)
    image(self, UH_OH, x + pad, y + math.floor((h - img) / 2), img, img)
    local ty = y + 12
    bold(self, self.app and "More of this tab is in the PZMM app" or "Some of this tab needs the PZMM app", tx, ty, C.text)
    ty = ty + H_SMALL + 4
    for _, l in ipairs(lines) do
        text(self, l, tx, ty, C.muted)
        ty = ty + H_SMALL + 2
    end
    if get then
        local gx, gy = x + w - pad - gw, y + math.floor((h - BAR) / 2)
        local over = mx >= gx and mx < gx + gw and my >= gy and my < gy + BAR
        round(self, gx, gy, gw, BAR, 6, over and C.hover or C.raised)
        outline(self, gx, gy, gw, BAR, 6, over and C.accent or C.line, over and 0.9 or 1)
        image(self, LINKS[1][1], gx + 12, gy + (BAR - 18) / 2, 18, 18)
        text(self, get, gx + 40, gy + (BAR - H_SMALL) / 2, C.text)
        if over then tip(self, "Get the app\n" .. LINKS[1][2]) end
        self.feedHits[#self.feedHits + 1] = { kind = "url", url = LINKS[1][2], x = gx, y = gy, w = gw, h = BAR }
    end
    return y + h + 16
end

--- A row inside a list card: a line above all but the first. Returns whether it's hovered.
local function listRow(ui, x, y, w, h, first, mx, my)
    if not first then rect(ui, x + 12, y, w - 24, 1, C.line, 1) end
    local over = mx >= x and mx < x + w and my >= y and my < y + h
    if over then rect(ui, x + 1, y + 1, w - 2, h - 2, C.accent, 0.14) end
    return over
end

-- Home -------------------------------------------------------------------------------------------------

function PZMMMods:drawHome(x, y, w, mx, my, top, fh)
    -- the hero and the play bar run the screen's width, joined to the tabs, as in the app
    y = self:drawHero(0, y, self.width, 400, mx, my, top, fh)
    y = self:drawPlayBar(y, mx, my) + 16
    local body = "Everything on this page works in the game. The PZMM Mod Manager app adds the rest of Home: Play and Join with a server picked, news from The Indie Stone, your mods' Workshop pictures and Check setup's fixes."
    if self.app then body = "The rest of Home is in the PZMM app: Play and Join with a server picked, news from The Indie Stone, your mods' Workshop pictures and Check setup's fixes." end
    return y + self:drawSorry(x, y, w, math.max(260, top + fh - y - 16), mx, my, body, nil, "More in the PZMM app") + 16
end

--- The play bar under the hero, as the app's: the main button and what it starts on the left, what
--- matters at a glance on the right (each opens where it counts). Returns the y under it.
function PZMMMods:drawPlayBar(y, mx, my)
    local h = 84
    rect(self, 0, y, self.width, h, C.panel, 1)
    rect(self, 0, y + h - 1, self.width, 1, C.line, 1)
    -- the main button: Continue the game's latest save, else start a new game
    local latest = getLatestSave()
    local has = latest and latest[1] and latest[2]
    local label = has and "Continue" or "New game"
    local bw, bh = width(label, MEDIUM) + 56, 46
    local bx, by = 32, y + (h - bh) / 2
    local over = mx >= bx and mx < bx + bw and my >= by and my < by + bh
    round(self, bx, by, bw, bh, 6, over and C.accentHover or C.accent)
    outline(self, bx, by, bw, bh, 6, C.accent)
    bold(self, label, bx + 28, by + (bh - H_MEDIUM) / 2, C.text, MEDIUM)
    if over then tip(self, has and ("Continue\nLoad " .. latest[1] .. " with its own mods.") or "New game\nThe game's new game screen.") end
    self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:gameScreen(has and "LATESTSAVE" or "SOLO") end, x = bx, y = by, w = bw, h = bh }
    -- what it starts
    local lx = bx + bw + 28
    bold(self, "LATEST SAVE", lx, y + h / 2 - H_SMALL - 2, C.muted)
    local sname = has and latest[1] or "None yet"
    local sw = width(sname)
    local sover = mx >= lx and mx < lx + sw and my >= y + h / 2 and my < y + h / 2 + H_SMALL + 2
    text(self, sname, lx, y + h / 2 + 2, sover and C.text or C.accentText)
    if sover then
        rect(self, lx, y + h / 2 + 2 + H_SMALL, sw, 1, C.text, 1)
        tip(self, "Saves\nEvery save, with Play, Folder and Delete.")
    end
    self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self.page, self.feedScroll = "saves", 0 end, x = lx, y = y + h / 2, w = sw, h = H_SMALL + 4 }

    -- at a glance, right-aligned: profile, mods, problems, updates
    local on, bad = 0, 0
    for _, d in pairs(self.model.mods) do
        if d.isActive then on = on + 1 end
        if SHOWN.Problems(d) then bad = bad + 1 end
    end
    local feed = self.feed or (not self.app and self:liteFeed())
    local updates = feed and #feed.entries
    local stats = {
        { "PROFILE", self.lib and self.lib.profile or "None", "Profile\nYour PZMM profile. Switch profiles in the app.", function() self.page = "mods" end },
        { "MODS", tostring(on), "Mods\nYour enabled mods.", function() self.page, self.tab = "mods", "Enabled"; self:updateView() end },
        { "PROBLEMS", tostring(bad), "Problems\nMods that can't load on this build or clash with another.", function() self.page, self.tab = "mods", "Problems"; self:updateView() end, bad > 0 and C.accentText },
        { "UPDATES", updates and tostring(updates) or "App only", "Updates\nYour Workshop mods updated since you last played." .. (updates and "" or " Needs the PZMM app."), function() self.page = "feed" end },
    }
    local sx = self.width - 32
    for i = #stats, 1, -1 do
        local st = stats[i]
        local cw = math.max(width(st[1]) + 1, width(st[2]))
        sx = sx - cw
        local sy = y + h / 2 - H_SMALL - 2
        local o = mx >= sx - 6 and mx < sx + cw + 6 and my >= y + 8 and my < y + h - 8
        bold(self, st[1], sx, sy, C.muted)
        text(self, st[2], sx, y + h / 2 + 2, st[5] or C.text)
        if o then
            rect(self, sx, y + h / 2 + 2 + H_SMALL, width(st[2]), 1, C.text, 1)
            tip(self, st[3])
        end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = st[4], x = sx - 6, y = y + 8, w = cw + 12, h = h - 16 }
        sx = sx - 40
    end
    return y + h
end

--- Home's hero, as the app's: the game's menu art, its logo (Project Viewpoint's with Viewpoint on) and
--- what Play starts, with the community note taped on the right. Returns the y under it.
function PZMMMods:drawHero(x, y, w, h, mx, my, fy, fh)
    -- the art covers the hero, cut to it (and to the frame): its own stencil, then the frame's again
    local top, bottom = math.max(y, fy), math.min(y + h, fy + fh)
    if bottom > top then
        self:clearStencilRect()
        self:setStencilRect(x, top, w, bottom - top)
        local vp = self.model.mods["Viewpoint"]
        local art = (vp and vp.isActive and HERO_VP) or HERO_PZ
        if art then
            -- covering the hero, as the app's object-fit: cover, 30% down
            local aw = math.max(w, h * art:getWidth() / art:getHeight())
            local ah = aw * art:getHeight() / art:getWidth()
            self:drawTextureScaled(art, x + (w - aw) / 2, y - (ah - h) * 0.3, aw, ah, 1, 1, 1, 1)
        else
            rect(self, x, y, w, h, C.raised, 1)
        end
        -- fading into the page at the bottom
        for i = 0, 19 do
            local f = i / 19
            rect(self, x, y + h * (0.45 + 0.55 * i / 20), w, h * 0.55 / 20 + 1, hex("000000"), 0.75 * f)
        end
        self:clearStencilRect()
        self:setStencilRect(0, fy, self.width, fh)
    end

    -- the mark: the logo, then what Play starts
    local main = MainScreen.instance
    local vp = self.model.mods["Viewpoint"]
    local logo = (vp and vp.isActive and VIEWPOINT_LOGO) or LOGO_PZ or (main and (main.pzmmLogo or main.logoTexture))
    local title = "Singleplayer"
    local ty = y + h - 22 - H_MARKER
    if logo then
        local lw = math.min(380, w * 0.5)
        local lh = lw * logo:getHeight() / logo:getWidth()
        self:drawTextureScaled(logo, x + 32, ty - 6 - lh, lw, lh, 1, 1, 1, 1)
    end
    text(self, title, x + 34, ty + 2, hex("000000", 0.6), MARKER)
    text(self, title, x + 32, ty, hex("ffffff"), MARKER)

    -- the community note, tilted, its words in the texture: the hovered link swaps in its blood version
    local nw, nh = NOTE:getWidth(), NOTE:getHeight()
    local nx, ny = x + w - 40 - nw, y + 22
    -- the mouse on the flat paper: undo the tilt about the paper's centre
    local cx, cy = nx + nw / 2, ny + nh / 2
    local dx, dy = mx - cx, my - cy
    local px = NOTE_W / 2 + dx * math.cos(NOTE_TILT) - dy * math.sin(NOTE_TILT)
    local py = NOTE_H / 2 + dx * math.sin(NOTE_TILT) + dy * math.cos(NOTE_TILT)
    local hover
    for i, l in ipairs(COMMUNITY) do
        local b = l[3]
        if px >= b[1] and px < b[1] + b[3] and py >= b[2] and py < b[2] + b[4] then hover = i end
    end
    image(self, hover and NOTE_HOVER[hover] or NOTE, nx, ny, nw, nh)
    if hover then
        local l = COMMUNITY[hover]
        tip(self, l[1] .. "\n" .. l[2])
        self.feedHits[#self.feedHits + 1] = { kind = "url", url = l[2], x = mx - 1, y = my - 1, w = 2, h = 2 }
    end
    return y + h
end

-- Maps ------------------------------------------------------------------------------------------------

--- The map order, as the game keeps it: higher wins where maps overlap.
function PZMMMods:maps()
    if not self.mapOrder then
        self.mapOrder = {}
        pcall(function()
            local a = self.model:getAllMapsInOrder()
            for i = 0, a:size() - 1 do self.mapOrder[#self.mapOrder + 1] = a:get(i) end
        end)
        if not self.mapOrig then
            self.mapOrig = {}
            for i, m in ipairs(self.mapOrder) do self.mapOrig[i] = m end
        end
    end
    return self.mapOrder
end
--- Write `order` to the game's map order (saved with Accept, as the game's own Map order screen does).
function PZMMMods:setMapOrder(order)
    pcall(function()
        local mo = self.model:getActiveMods():getMapOrder()
        mo:clear()
        for _, m in ipairs(order) do mo:add(m) end
    end)
end
function PZMMMods:moveMap(i, d)
    local o = self.mapOrder
    o[i], o[i + d] = o[i + d], o[i]
    self:setMapOrder(o)
    self.mapsChanged = true
end

function PZMMMods:drawMaps(x, y, w, mx, my)
    y = self:drawNotice(x, y, w, "The cell grid and world map, Workshop maps you haven't installed, previews, Most maps that fit, presets, Compare with a server and Autosort.", mx, my)
    local maps = self:maps()
    text(self, "Your enabled maps, in load order: where two maps cover the same cells, the higher one wins. Accept saves it.", x, y, C.muted)
    y = y + H_SMALL + 12
    if #maps == 0 then
        text(self, "No maps: turn on a map mod in My mods.", x, y, C.muted)
        return y + H_SMALL
    end
    local h = #maps * ROW
    card(self, x, y, w, h)
    for i, m in ipairs(maps) do
        local ry = y + (i - 1) * ROW
        listRow(self, x, ry, w, ROW, i == 1, mx, my)
        local mid = ry + (ROW - H_SMALL) / 2
        text(self, tostring(i), x + 36 - width(tostring(i)), mid, C.accentText)
        bold(self, m, x + 52, mid, C.text)
        local clash = {}
        pcall(function()
            local c = self.model:getMapConflicts(m)
            for j = 0, c:size() - 1 do clash[#clash + 1] = c:get(j) end
        end)
        if #clash > 0 then
            local cx = x + 52 + width(m) + 14
            local cy = ry + (ROW - H_SMALL - 2) / 2
            local right = chip(self, "Clashes with " .. #clash, cx, cy, C.warn)
            if mx >= cx and mx < right and my >= cy and my < cy + H_SMALL + 2 then tip(self, "Clashes with\n" .. table.concat(clash, ", ") .. "\nThe one higher in the list wins.") end
        end
        -- Up and Down, at the right
        local bx = x + w - 16
        for _, b in ipairs({ { "Down", 1, i < #maps }, { "Up", -1, i > 1 } }) do
            if b[3] then
                local bw = width(b[1]) + 28
                bx = bx - bw
                local by = ry + (ROW - BAR) / 2
                local _, _, over = flatButton(self, b[1], bx, by, mx, my)
                if over then tip(self, b[1] == "Up" and "Move up\nLoads before the maps under it." or "Move down") end
                local idx, d = i, b[2]
                self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:moveMap(idx, d) end, x = bx, y = by, w = bw, h = BAR }
                bx = bx - 8
            end
        end
    end
    return y + h
end

-- Saves -----------------------------------------------------------------------------------------------

function PZMMMods:savesList()
    if not self.saves then
        self.saves = {}
        pcall(function()
            for _, k in ipairs(getFullSaveDirectoryTable()) do
                local info = getSaveInfo(k)
                local s = { folder = k, active = info.activeMods, name = info.saveName or k, mode = info.gameMode or "", map = info.mapName or "Muldraugh, KY", played = getLastPlayedDate(k) or "", mods = {}, missing = {} }
                if info.activeMods then
                    local ids = info.activeMods:getMods()
                    for i = 0, ids:size() - 1 do
                        local id = ids:get(i)
                        s.mods[#s.mods + 1] = id
                        if not self.model.mods[id] then s.missing[#s.missing + 1] = id end
                    end
                end
                self.saves[#self.saves + 1] = s
            end
        end)
    end
    return self.saves
end

--- My mods become this save's: each mod on exactly when the save uses it (Accept keeps it, Back undoes it).
function PZMMMods:useSaveMods(s)
    -- the map order and mod list as they were, for Back's "Don't save"
    self:maps()
    if s.active and not self.modsOrig then
        pcall(function()
            local orig = ActiveMods.getById("pzmmUndo")
            orig:copyFrom(self.model:getActiveMods())
            self.modsOrig = orig
        end)
    end
    if s.active then
        -- the game's own copy: its mods in their order, and its map order
        pcall(function() self.model:getActiveMods():copyFrom(s.active) end)
        self.model:refreshMods()
        self.mapsChanged = true
    else
        local want = {}
        for _, id in ipairs(s.mods) do want[id] = true end
        for id, d in pairs(self.model.mods) do
            if d.isActive ~= (want[id] == true) then self.model:forceActivateMods(d.modInfo, want[id] == true) end
        end
    end
    self.page, self.tab = "mods", "Enabled"
    self:updateView()
    self:say("My mods now has " .. s.name .. "'s mods. Accept keeps them.")
end

--- The save's mod list becomes My mods as they are now (the game's own way, as its Load screen does).
function PZMMMods:giveMods(s)
    self:ask({ title = "Give " .. s.name .. " your mods as they are in My mods now? It loads with them from then on.", choices = {
        { "Give it my mods", function()
            pcall(function()
                ActiveMods.getById("currentGame"):copyFrom(self.model:getActiveMods())
                manipulateSavefile(s.folder, "WriteModsDotTxt")
                ActiveMods.getById("currentGame"):copyFrom(ActiveMods.getById("default"))
            end)
            self.saves = nil
            self:say(s.name .. " now loads with your mods.")
        end },
    } })
end

function PZMMMods:deleteSave(s)
    self:ask({ title = "Delete " .. s.name .. "? This can't be undone.", choices = {
        { "Delete", function()
            deleteSave(s.folder)
            self.saves = nil
            self:say(s.name .. " deleted.")
        end },
    } })
end

function PZMMMods:drawSaves(x, y, w, mx, my)
    y = self:drawNotice(x, y, w, "Backups (automatic, Back up now and slots), Restore, Give it my mods, Copy with my mods, sandbox editing, Recovery and Characters.", mx, my)
    local saves = self:savesList()
    if #saves == 0 then
        text(self, "No saves yet.", x, y, C.muted)
        return y + H_SMALL
    end
    local rh, th = 88, 64
    local h = #saves * rh
    card(self, x, y, w, h)
    for i, s in ipairs(saves) do
        local ry = y + (i - 1) * rh
        listRow(self, x, ry, w, rh, i == 1, mx, my)
        if s.thumb == nil then
            s.thumb = false
            pcall(function() s.thumb = getTextureFromSaveDir("thumb.png", s.folder) or false end)
        end
        if s.thumb then
            self:drawTextureScaledAspect(s.thumb, x + 16, ry + (rh - th) / 2, th, th, 1, 1, 1, 1)
        else
            round(self, x + 16, ry + (rh - th) / 2, th, th, 6, C.raised)
        end
        local tx = x + 16 + th + 16
        local ty = ry + 14
        local buttons = {
            { "Delete", nil, "Delete\nRemoves the save for good.", function() self:deleteSave(s) end },
            { "Give it my mods", nil, "Give it my mods\nThe save loads with your mods as they are in My mods now.", function() self:giveMods(s) end },
            { "Use its mods", nil, "Use its mods\nTurns on exactly this save's mods in My mods. Accept keeps them.", function() self:useSaveMods(s) end },
            { "Folder", nil, "Folder\nOpen the save's folder.", function() pcall(function() showFolderInDesktop(getAbsoluteSaveFolderName(s.folder)) end) end },
            { "Play", "primary", "Play\nLoad this save with its own mods.", function() self:close(); MainScreen.continueLatestSave(s.mode, s.name) end },
        }
        if self.app then
            table.insert(buttons, 4, { "Back up in PZMM", nil, "Back up in PZMM\nThe app keeps a backup of this save you can restore.", function() self:request("backup\t" .. s.folder:gsub("\\", "/"), "Backup of " .. s.name .. " asked for.") end })
        end
        -- the text stops before the buttons (8 px apart)
        local bwAll = 0
        for _, b in ipairs(buttons) do bwAll = bwAll + width(b[1]) + 28 + 8 end
        local room = x + w - 16 - bwAll - 24 - tx
        bold(self, fit(s.name, room, MEDIUM), tx, ty, C.text, MEDIUM)
        ty = ty + H_MEDIUM + 4
        local mode = getTextOrNull("IGUI_Gametime_" .. s.mode) or s.mode
        local meta = mode .. "    " .. s.map:gsub(";", ", ") .. "    " .. s.played
        local shown = fit(meta, room)
        text(self, shown, tx, ty, C.muted)
        if shown ~= meta and mx >= tx and mx < tx + room and my >= ty and my < ty + H_SMALL then tip(self, s.name .. "\n" .. meta) end
        -- its mods: how many, and any you don't have
        local cy = ty + H_SMALL + 6
        local cx = chip(self, #s.mods .. " mods", tx, cy)
        if mx >= tx and mx < cx and my >= cy and my < cy + H_SMALL + 2 and #s.mods > 0 then tip(self, "Its mods\n" .. table.concat(s.mods, ", ")) end
        if #s.missing > 0 then
            local mx0 = cx + 6
            local right = chip(self, #s.missing .. " missing", mx0, cy, C.danger)
            if mx >= mx0 and mx < right and my >= cy and my < cy + H_SMALL + 2 then tip(self, "Mods you don't have\n" .. table.concat(s.missing, ", ") .. "\nGet them before you load this save.") end
        end
        -- Play, Folder, Delete at the right
        local bx = x + w - 16
        local by = ry + (rh - BAR) / 2
        for _, b in ipairs(buttons) do
            local bw = width(b[1]) + 28
            bx = bx - bw
            local _, _, over = flatButton(self, b[1], bx, by, mx, my, b[2])
            if over then tip(self, b[3]) end
            self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = b[4], x = bx, y = by, w = bw, h = BAR }
            bx = bx - 8
        end
    end
    return y + h
end

-- Servers ---------------------------------------------------------------------------------------------

function PZMMMods:drawServers(x, y, w, mx, my)
    y = self:drawNotice(x, y, w, "The Internet list with filters, each server's mods checked against yours, Set up and join (downloads the server's mods into a profile of its own), History and hosting a server.", mx, my)
    local label = "Game's server browser"
    local bw = width(label) + 28
    local _, _, over = flatButton(self, label, x, y, mx, my, "primary")
    if over then tip(self, "Game's server browser\nThe Internet list, favourites and joining, in the game's own screen.") end
    self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:gameScreen("MULTIPLAYER") end, x = x, y = y, w = bw, h = BAR }
    y = y + BAR + 20
    bold(self, "Your servers", x, y, C.text)
    y = y + H_SMALL + 10
    local servers = {}
    pcall(function() servers = getServerList() or {} end)
    if #servers == 0 then
        text(self, "No saved servers yet: add one in the game's server browser.", x, y, C.muted)
        return y + H_SMALL
    end
    local rh = H_SMALL * 2 + 28
    local h = #servers * rh
    card(self, x, y, w, h)
    for i, s in ipairs(servers) do
        local ry = y + (i - 1) * rh
        listRow(self, x, ry, w, rh, i == 1, mx, my)
        local name, addr, accounts = "", "", 0
        pcall(function()
            name = s:getName() or ""
            addr = (s:getIp() or "") .. ":" .. tostring(s:getPort() or "")
            accounts = s:getAccounts():size()
        end)
        bold(self, name ~= "" and name or addr, x + 16, ry + 12, C.text)
        text(self, addr .. "    " .. accounts .. (accounts == 1 and " account" or " accounts"), x + 16, ry + 16 + H_SMALL, C.muted)
        local jw = width("Join") + 28
        local jx, jy = x + w - 16 - jw, ry + (rh - BAR) / 2
        local _, _, o = flatButton(self, "Join", jx, jy, mx, my)
        if o then tip(self, "Join\nOpens the game's server browser: pick your account there and join.") end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function() self:gameScreen("MULTIPLAYER") end, x = jx, y = jy, w = jw, h = BAR }
    end
    return y + h
end

-- Java mods -------------------------------------------------------------------------------------------

--- Mods that ship Java (javaJarFile in their mod.info), read once per visit, a few mod.info files a frame
--- so a big mod list doesn't stall the menu. Returns the list and whether it's done.
function PZMMMods:javaMods()
    if not self.java then
        self.java, self.javaTodo = {}, {}
        for id, d in pairs(self.model.mods) do self.javaTodo[#self.javaTodo + 1] = d end
    end
    for _ = 1, 25 do
        local d = table.remove(self.javaTodo)
        if not d then break end
        pcall(function()
            local r = getModFileReader(d.modId, "mod.info", false)
            if not r then return end
            local jar, needsZB
            local line = r:readLine()
            while line do
                local k, v = line:match("^%s*([%w_]+)%s*=%s*(.-)%s*$")
                if k == "javaJarFile" then jar = v end
                if k == "require" and v:find("ZombieBuddy", 1, true) then needsZB = true end
                line = r:readLine()
            end
            r:close()
            if jar and d.modId ~= "ZombieBuddy" then self.java[#self.java + 1] = { d = d, jar = jar, zb = needsZB } end
        end)
    end
    table.sort(self.java, function(a, b) return (a.d.name or "") < (b.d.name or "") end)
    return self.java, #self.javaTodo == 0
end

function PZMMMods:drawJava(x, y, w, mx, my)
    y = self:drawNotice(x, y, w, "Set up and Repair ZombieBuddy, approving Java mods before you play, the Java scan of every jar, and setting up Project Viewpoint.", mx, my)
    -- ZombieBuddy: running, or why not
    local zb = self.model.mods["ZombieBuddy"]
    local version
    pcall(function() version = ZombieBuddy.getVersion() end)
    local h = H_SMALL * 2 + 36
    card(self, x, y, w, h)
    bold(self, "ZombieBuddy", x + 16, y + 14, C.text)
    local status, c
    if version then
        status, c = "Running, v" .. tostring(version) .. ". Java mods can load.", C.ok
    elseif zb and zb.isActive then
        status, c = "Turned on, but not running: it needs its launch setup (the app's Set up ZombieBuddy).", C.warn
    elseif zb then
        status, c = "Installed but turned off: Java mods won't load.", C.warn
    else
        status, c = "Not installed: Java mods won't load. It's on the Workshop.", C.danger
    end
    text(self, status, x + 16, y + 18 + H_SMALL, c)
    y = y + h + 20
    bold(self, "Mods with Java", x, y, C.text)
    y = y + H_SMALL + 10
    local list, done = self:javaMods()
    if #list == 0 then
        text(self, done and "None of your mods ship Java." or "Looking through your mods...", x, y, C.muted)
        return y + H_SMALL
    end
    local lh = #list * ROW
    card(self, x, y, w, lh)
    for i, j in ipairs(list) do
        local ry = y + (i - 1) * ROW
        local over = listRow(self, x, ry, w, ROW, i == 1, mx, my)
        local d = j.d
        local mid = ry + (ROW - H_SMALL) / 2
        local right = pill(self, d.isActive and "Enabled" or "Off", x + w - 16, ry + (ROW - H_SMALL - 4) / 2, d.isActive and C.ok or C.muted, d.isActive)
        local tx = x + 16
        local name = fit(d.name or d.modId, right - 16 - tx - 200)
        bold(self, name, tx, mid, d.isActive and C.text or C.muted)
        tx = tx + width(name) + 12
        text(self, fit(j.jar, right - 16 - tx - 150), tx, mid, copy(C.muted, 0.75))
        if j.zb and not version then
            local jw = width(fit(j.jar, right - 16 - tx - 150))
            chip(self, "Needs ZombieBuddy", tx + jw + 12, ry + (ROW - H_SMALL - 2) / 2, C.danger)
        end
        if over then tip(self, (d.name or d.modId) .. "\n" .. j.jar .. "\nClick to show it in My mods.") end
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function()
            self.page, self.selected, self.tab = "mods", d, "All"
            self.search:setText("")
            self:updateView()
        end, x = x, y = ry, w = w, h = ROW }
    end
    return y + lh
end

-- Settings --------------------------------------------------------------------------------------------

local SETTING_ROWS = {
    { "logo", "Hide the Project Zomboid logo on the main menu" },
    { "watermark", "Hide ZombieBuddy's watermark on the main menu", "Its \"loaded\" line and Java mods list over the top of the screen." },
    { "news", "PZMM patch notes box on the main menu", "The box at the top right, with a red ! for a new patch." },
    { "tips", "Tooltips", "What a button, tab or chip does, when you hover it." },
}

function PZMMMods:drawSettings(x, y, w, mx, my)
    y = self:drawNotice(x, y, w, "Folders, downloads and updates, accounts and keys, backups, appearance and language, launch options, notifications and performance.", mx, my)
    bold(self, "In-game mod manager", x, y, C.text)
    y = y + H_SMALL + 10
    local rh = H_SMALL * 2 + 22
    local h = #SETTING_ROWS * rh
    card(self, x, y, w, h)
    for i, s in ipairs(SETTING_ROWS) do
        local ry = y + (i - 1) * rh
        listRow(self, x, ry, w, rh, i == 1, mx, my)
        checkbox(self, x + 16, ry + (rh - BOX) / 2, SETTINGS[s[1]])
        local tx = x + 16 + BOX + 14
        if s[3] then
            text(self, s[2], tx, ry + 10, C.text)
            text(self, s[3], tx, ry + 12 + H_SMALL, C.muted)
        else
            text(self, s[2], tx, ry + (rh - H_SMALL) / 2, C.text)
        end
        local key = s[1]
        self.feedHits[#self.feedHits + 1] = { kind = "fn", fn = function()
            SETTINGS[key] = not SETTINGS[key]
            saveSettings()
        end, x = x, y = ry, w = w, h = rh }
    end
    y = y + h + 20
    bold(self, "About", x, y, C.text)
    y = y + H_SMALL + 10
    local game, app = "", self.app and self.app.version or "not installed"
    for _, n in ipairs(NOTES or {}) do if n.kind == "game" then game = n.version end end
    text(self, "In-game mod manager " .. (game ~= "" and game or "?") .. "    PZMM app " .. app, x, y, C.muted)
    return y + H_SMALL
end

-- Browse Workshop and Make a mod: the game can't --------------------------------------------------------

local WORKSHOP_SORRY = "The game doesn't let mods search the Steam Workshop, so Browse Workshop only works in the PZMM Mod Manager app: search, filters, collections and one-click installs. You can still browse the Workshop in Steam."
local MAKE_SORRY = "Making a mod needs folders and files the game won't let mods create. The PZMM Mod Manager app makes a ready-to-edit mod for you, for Build 41 and 42. To upload a mod you've made, the game has its own Workshop screen."

function PZMMMods:drawWorkshop(x, y, w, mx, my, top, bh)
    local links = { SORRY_LINKS[1], SORRY_LINKS[2],
        { icon = nil, title = "Open the Workshop", sub = "Project Zomboid's Workshop in Steam", url = "https://steamcommunity.com/app/108600/workshop/" } }
    if self.app then links[1] = { icon = LOGO, title = "Open it in PZMM", sub = "Opens the app on Browse Workshop", fn = function() self:request("open\tworkshop", "Browse Workshop asked for.") end } end
    return y + self:drawSorry(x, y, w, top + bh - y - 32, mx, my, WORKSHOP_SORRY, links)
end

function PZMMMods:drawMake(x, y, w, mx, my, top, bh)
    local links = { SORRY_LINKS[1], SORRY_LINKS[2],
        { icon = nil, title = "Game's Workshop screen", sub = "Upload a mod you've made", fn = function() self:gameScreen("WORKSHOP") end } }
    if self.app then links[1] = { icon = LOGO, title = "Open it in PZMM", sub = "Opens the app on Make a mod", fn = function() self:request("open\tauthoring", "Make a mod asked for.") end } end
    return y + self:drawSorry(x, y, w, top + bh - y - 32, mx, my, MAKE_SORRY, links)
end

--- The frame every other tab draws in: What's new's (heading, scrolling, scroll bar).
function PZMMMods:drawPage()
    local bx, by, bw, bh = self:feedBox()
    local cw = math.min(bw, 1240)
    local x0 = math.floor(bx + (bw - cw) / 2)
    local mx, my = self:getMouseX(), self:getMouseY()
    self.feedHits = {}
    local pg = PAGE[self.page]
    local y
    if self.page == "home" then
        -- Home's hero meets the tabs: its frame is the whole width from under them
        bx, by, bw, bh = 0, TOP, self.width, self.height - BOTTOM - TOP
        self:setStencilRect(bx, by, bw, bh)
        y = math.floor(by - self.feedScroll)
    else
        self:setStencilRect(bx, by, bw, bh)
        y = math.floor(by + 8 - self.feedScroll)
        bold(self, pg[2], x0, y, C.text, MEDIUM)
        y = y + H_MEDIUM + 12
    end
    y = self[pg[4]](self, x0, y, cw, mx, my, by, bh)
    self:clearStencilRect()
    self.feedHeight = y + self.feedScroll - by + 24
    if self.feedHeight > bh then
        local th = math.max(30, bh * bh / self.feedHeight)
        local thy = by + (bh - th) * self.feedScroll / (self.feedHeight - bh)
        round(self, bx + bw - 6, thy, 6, th, 3, C.edge, 0.8)
    end
end

-- the menu built before this file loaded holds the old function: point its Mods item here too
local function hook()
    local main = MainScreen.instance
    local m = main and main.modsOption
    if m then m.onMouseDown = MainScreen.onMenuItemMouseDownMainMenu end
    if main and #NOTES > 0 and not (PZMMNews.instance and PZMMNews.instance.parent == main) then
        PZMMNews.instance = PZMMNews:new()
        PZMMNews.instance:initialise()
        main:addChild(PZMMNews.instance)
    end
end
if Events.OnMainMenuEnter then Events.OnMainMenuEnter.Add(hook) end
hook()
